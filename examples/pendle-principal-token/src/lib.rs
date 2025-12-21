//! # PendlePrincipalToken (PT) — Hydra N=2 Implementation
//!
//! ## Overview
//! This R55 contract implements a Hydra-compatible N=2 version of Pendle's Principal Token (PT).
//! PT represents the principal component of a tokenized yield position, paired with Yield Token (YT).
//! Provides ABI parity with Solidity PT contracts for decorrelated Hydra execution. While simpler than YT,
//! PT is critical for maintaining supply invariants in PY pairs.
//!
//! ## Solidity Reference
//! - **Main Contract**: `pendle-core-v2-public/contracts/core/YieldContracts/PendlePrincipalToken.sol`
//! - **Inheritance**: `PendleERC20` for ERC20 base, with PT-specific mint/burn.
//!
//! ## Key Features
//! - **Mint by YT**: Only YT contract can mint PT tokens (during PY minting; 1:1 with YT).
//! - **Burn by YT**: Only YT contract can burn PT tokens (during PY redeeming; pre-expiry only).
//! - **ERC20 Interface**: Standard ERC20 functions for PT tokens (transfer, approve, etc.).
//! - **Expiry**: Immutable expiry timestamp; PT value matures at expiry (no further minting).
//! - **Access Control**: Factory initializes; only YT can modify supply (critical invariant).
//! - **Reentrancy Protection**: Guards on transfers.
//!
//! ## Execution Flow (high level)
//! - Factory deploys PT with `(SY, name, symbol, decimals, expiry)`; PT records `factory = msg.sender`.
//! - Factory calls `initialize(YT)` exactly once to set the paired YT address.
//! - YT calls `mintByYT(user, amount)` / `burnByYT(user, amount)` to keep PT supply paired with YT.
//! - Regular ERC20 operations (`transfer`, `transferFrom`, `approve`) work with reentrancy guard + checks.
//!
//! ## Hydra N=2 Specifics
//! - **Storage Mode**: HydraStorage (separate N=2 storage for decorrelation).
//! - **Immutables**: SY, factory, expiry, decimals stored in slots (cannot change post-deployment).
//! - **Mint/Burn**: Restricted to YT caller via `onlyYT` modifier; ensures paired operations.
//! - **ABI Parity**: Public/external functions and events match Solidity exactly for N=1/N=2 equivalence.
//!
//! ## Security Considerations
//! - **Reentrancy**: Guards prevent reentrant calls on transfers.
//! - **Access Control**: Only YT can mint/burn; no owner (relies on external governance).
//! - **Supply Invariants**: Mint/burn must be paired with YT operations; expiry prevents post-expiry minting.
//! - **ERC20 Assumptions**: Standard ERC20 behavior; no custom transfer hooks.
//!
//! ## Edge Cases & Assumptions
//! - **Expiry Effects**: Post-expiry, no new minting; PT holders redeem via YT flows.
//! - **Zero Transfers**: Allowed but guarded against invalid operations.
//! - **Caller Validation**: `onlyYT` ensures only paired YT can modify supply; invalid callers revert.
//! - **Decimals/Expiry**: Must be set correctly at deployment; immutable post-init.
//! - **Balance Checks**: Transfer guards prevent insufficient balances.
//!
//! ## Differences from Solidity
//! - Single-file implementation (no inheritance; inline ERC20 logic).
//! - Storage uses slots/mappings instead of Solidity packed storage (HydraStorage allows differences).
//! - Expiry checked via R55 block timestamp syscall (not Solidity block.timestamp).
//! - Access control via modifiers (not OZ Ownable; simpler for N=2).
//! - No Pausable (PT doesn't override; add if needed).
//!
//! ## Complexities
//! - Supply synchronization with YT: Critical for PY pair integrity.
//! - Expiry handling: Immutable timestamp affects minting permissions.
//! - ERC20 base logic: Inline to avoid inheritance complexities in R55.
//!
//! ## Performance Notes
//! - Simple ERC20 operations: Low overhead.
//! - R55 syscalls: Efficient for transfers and timestamp checks.
//!
//! ## Extensibility
//! - Add pausing: Implement emergency stops if needed.
//! - Governance: Add owner controls for factory updates.
//! - Custom Logic: Extend for non-standard PT behaviors.
//!
//! ## Testing Notes
//! - **Parity Tests**: Compare mint/burn/transfer outputs with Solidity N=1.
//! - **Invariants**: Verify supply changes only via YT; expiry blocks minting.
//! - **Edge Cases**: Invalid callers, post-expiry attempts, zero amounts.
//! - **Integration**: Test with YT in PY mint/redeem flows; ensure paired operations.
//!
//! ## Usage
//! Deploy Solidity N=1 PT first, then attach this R55 N=2 in Hydra for parallel execution.
//!
//! ## References
//! - Pendle PT Docs: PT as principal-bearing tokens.
//! - Paired with YT for full PY (Principal + Yield) positions.
//! - DeFi Risks: Supply mismatches between PT/YT can break invariants; strict access control required.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use contract_derive::{contract, storage, Error, Event};
use eth_riscv_runtime::{block, types::*};
use alloy_core::primitives::{Address, U256};

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------
const VERSION: U256 = U256::from_limbs([6, 0, 0, 0]);
const NOT_ENTERED: U256 = U256::from_limbs([1, 0, 0, 0]);
const ENTERED: U256 = U256::from_limbs([2, 0, 0, 0]);

// -----------------------------------------------------------------------------
// Events
// -----------------------------------------------------------------------------
#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub amount: U256,
}

#[derive(Event)]
pub struct Approval {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub spender: Address,
    pub amount: U256,
}

/// Mirrors OZ Initializable Initialized(uint8) emission
#[derive(Event)]
pub struct Initialized {
    pub version: u8,
}

// -----------------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------------
#[derive(Error)]
pub enum PTError {
    OnlyYT,
    OnlyYCFactory,
    ReentrantCall,
    ZeroAddress,
    SelfTransfer,
    SelfApproval,
    InsufficientBalance(U256),
    InsufficientAllowance(U256),
    AlreadyInitialized,
}

// -----------------------------------------------------------------------------
// Storage
// -----------------------------------------------------------------------------
#[storage]
pub struct PendlePrincipalToken {
    /// Total token supply
    total_supply: Slot<U256>,
    /// Reentrancy status (1 = not entered, 2 = entered)
    reentrancy_status: Slot<U256>,
    /// balances[owner]
    balance_of: Mapping<Address, Slot<U256>>,
    /// allowances[owner][spender]
    allowance_of: Mapping<Address, Mapping<Address, Slot<U256>>>,
    /// ERC20 metadata
    name: DynamicSlot<String>,
    symbol: DynamicSlot<String>,
    /// Decimals (uint8 in Solidity; stored as U256)
    decimals: Slot<U256>,
    /// SY address (immutable in Solidity)
    sy: Slot<Address>,
    /// Factory address (immutable in Solidity)
    factory: Slot<Address>,
    /// Expiry timestamp
    expiry: Slot<U256>,
    /// YT address set during initialize
    yt: Slot<Address>,
    /// Initialization flag (matches OpenZeppelin Initializable pattern)
    initialized: Slot<U256>,
}

// -----------------------------------------------------------------------------
// Public/external implementation (ABI surface)
// -----------------------------------------------------------------------------
#[contract]
impl PendlePrincipalToken {
    // ---------------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------------
    /// Initialize storage: SY, factory (caller), expiry, decimals, name, symbol, guard state.
    pub fn new(sy: Address, name: String, symbol: String, decimals: U256, expiry: U256) -> Self {
        let mut pt = PendlePrincipalToken::default();

        pt.sy.write(sy);
        pt.factory.write(msg_sender());
        pt.expiry.write(expiry);
        pt.decimals.write(decimals);
        pt.name.write(name);
        pt.symbol.write(symbol);

        pt.reentrancy_status.write(NOT_ENTERED);

        pt
    }

    // ---------------------------------------------------------------------
    // Initialization
    // ---------------------------------------------------------------------
    /// Set YT once; only factory; reverts if already initialized or yt == 0; emits Initialized(1).
    /// Matches OpenZeppelin Initializable pattern: uses separate initialized flag.
    pub fn initialize(&mut self, yt: Address) -> Result<(), PTError> {
        self.check_only_factory()?;
        // Check initialized flag (matches OpenZeppelin's _initialized check)
        if self.initialized.read() != U256::ZERO {
            return Err(PTError::AlreadyInitialized);
        }
        if yt == Address::ZERO {
            return Err(PTError::ZeroAddress);
        }
        // Set initialized flag to 1 (matches OpenZeppelin's _initialized = 1)
        self.initialized.write(U256::from(1));
        self.yt.write(yt);
        log::emit(Initialized::new(1));
        Ok(())
    }

    // ---------------------------------------------------------------------
    // PT-specific functions
    // ---------------------------------------------------------------------
    /// Mint PT for `user`; only YT; updates supply; emits Transfer(0,user,amount).
    pub fn mintByYT(&mut self, user: Address, amount: U256) -> Result<(), PTError> {
        self.check_only_yt()?;
        self._mint(user, amount)
    }

    /// Burn PT from `user`; only YT; updates supply; emits Transfer(user,0,amount).
    pub fn burnByYT(&mut self, user: Address, amount: U256) -> Result<(), PTError> {
        self.check_only_yt()?;
        self._burn(user, amount)
    }

    pub fn isExpired(&self) -> bool {
        let now = block::timestamp();
        let expiry = self.expiry.read();
        expiry <= now
    }

    // ---------------------------------------------------------------------
    // ERC20 public functions
    // ---------------------------------------------------------------------
    /// Transfer with reentrancy guard; reverts on zero/self or insufficient balance.
    pub fn transfer(&mut self, to: Address, amount: U256) -> Result<bool, PTError> {
        self.non_reentrant(|this| {
            this._transfer(msg_sender(), to, amount)?;
            Ok(true)
        })
    }

    /// transferFrom with allowance spend and reentrancy guard; honors infinite allowance.
    pub fn transferFrom(&mut self, from: Address, to: Address, amount: U256) -> Result<bool, PTError> {
        self.non_reentrant(|this| {
            let spender = msg_sender();
            this._spend_allowance(from, spender, amount)?;
            this._transfer(from, to, amount)?;
            Ok(true)
        })
    }

    /// approve; self-approval allowed (selector parity); overwrites allowance.
    pub fn approve(&mut self, spender: Address, amount: U256) -> Result<bool, PTError> {
        let owner = msg_sender();
        self._approve(owner, spender, amount)?;
        Ok(true)
    }

    // ---------------------------------------------------------------------
    // Views
    // ---------------------------------------------------------------------
    pub fn totalSupply(&self) -> U256 {
        self.total_supply.read()
    }

    pub fn balanceOf(&self, account: Address) -> U256 {
        self.balance_of[account].read()
    }

    pub fn allowance(&self, owner: Address, spender: Address) -> U256 {
        self.allowance_of[owner][spender].read()
    }

    pub fn name(&self) -> String {
        self.name.read()
    }

    pub fn symbol(&self) -> String {
        self.symbol.read()
    }

    pub fn decimals(&self) -> U256 {
        self.decimals.read()
    }

    pub fn SY(&self) -> Address {
        self.sy.read()
    }

    pub fn factory(&self) -> Address {
        self.factory.read()
    }

    pub fn expiry(&self) -> U256 {
        self.expiry.read()
    }

    pub fn YT(&self) -> Address {
        self.yt.read()
    }

    pub fn VERSION(&self) -> U256 {
        VERSION
    }

    pub fn reentrancyGuardEntered(&self) -> bool {
        self.reentrancy_status.read() == ENTERED
    }
}

// -----------------------------------------------------------------------------
// Internal helpers (not exposed to ABI)
// -----------------------------------------------------------------------------
impl PendlePrincipalToken {
    /// onlyYT guard.
    fn check_only_yt(&self) -> Result<(), PTError> {
        if msg_sender() != self.yt.read() {
            return Err(PTError::OnlyYT);
        }
        Ok(())
    }

    /// onlyYieldFactory guard.
    fn check_only_factory(&self) -> Result<(), PTError> {
        if msg_sender() != self.factory.read() {
            return Err(PTError::OnlyYCFactory);
        }
        Ok(())
    }

    fn non_reentrant<F, R>(&mut self, f: F) -> Result<R, PTError>
    where
        F: FnOnce(&mut Self) -> Result<R, PTError>,
    {
        if self.reentrancy_status.read() == ENTERED {
            return Err(PTError::ReentrantCall);
        }
        self.reentrancy_status.write(ENTERED);
        let result = f(self);
        self.reentrancy_status.write(NOT_ENTERED);
        result
    }

    fn _mint(&mut self, account: Address, amount: U256) -> Result<(), PTError> {
        if account == Address::ZERO {
            return Err(PTError::ZeroAddress);
        }
        let balance = self.balance_of[account].read();
        self.balance_of[account].write(balance + amount);

        let supply = self.total_supply.read();
        self.total_supply.write(supply + amount);

        log::emit(Transfer::new(Address::ZERO, account, amount));
        Ok(())
    }

    fn _burn(&mut self, account: Address, amount: U256) -> Result<(), PTError> {
        if account == Address::ZERO {
            return Err(PTError::ZeroAddress);
        }
        let balance = self.balance_of[account].read();
        if balance < amount {
            return Err(PTError::InsufficientBalance(balance));
        }
        self.balance_of[account].write(balance - amount);

        let supply = self.total_supply.read();
        self.total_supply.write(supply - amount);

        log::emit(Transfer::new(account, Address::ZERO, amount));
        Ok(())
    }

    fn _transfer(&mut self, from: Address, to: Address, amount: U256) -> Result<(), PTError> {
        if from == Address::ZERO || to == Address::ZERO {
            return Err(PTError::ZeroAddress);
        }
        if from == to {
            return Err(PTError::SelfTransfer);
        }
        let from_balance = self.balance_of[from].read();
        if from_balance < amount {
            return Err(PTError::InsufficientBalance(from_balance));
        }
        self.balance_of[from].write(from_balance - amount);
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);

        log::emit(Transfer::new(from, to, amount));
        Ok(())
    }

    fn _approve(&mut self, owner: Address, spender: Address, amount: U256) -> Result<(), PTError> {
        if owner == Address::ZERO || spender == Address::ZERO {
            return Err(PTError::ZeroAddress);
        }
        self.allowance_of[owner][spender].write(amount);
        log::emit(Approval::new(owner, spender, amount));
        Ok(())
    }

    fn _spend_allowance(&mut self, owner: Address, spender: Address, amount: U256) -> Result<(), PTError> {
        let current = self.allowance_of[owner][spender].read();
        if current != U256::MAX {
            if current < amount {
                return Err(PTError::InsufficientAllowance(current));
            }
            self.allowance_of[owner][spender].write(current - amount);
        }
        Ok(())
    }
}
