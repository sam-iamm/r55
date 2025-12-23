//! # StandardizedYield ERC20 (SY) — Hydra N=2 Implementation
//!
//! ## Overview
//! This R55 contract implements a Hydra-compatible N=2 version of Pendle's StandardizedYield (SY) for ERC20 assets.
//! It provides ABI parity with Solidity SY contracts, enabling decorrelated execution in Hydra while maintaining
//! identical inputs/outputs for parity testing. This is a simple ERC20 wrapper SY with no yield accrual.
//!
//! ## Solidity Reference
//! - **Base**: `Pendle-SY-Public/contracts/core/StandardizedYield/SYBase.sol`
//! - **Implementation**: `Pendle-SY-Public/contracts/core/StandardizedYield/implementations/PendleERC20SY.sol`
//!
//! ## Key Features
//! - **Deposit**: Pull underlying ERC20 tokens and mint SY shares 1:1.
//! - **Redeem**: Burn SY shares and return underlying ERC20 tokens 1:1.
//! - **Exchange Rate**: Static 1e18 (1 SY = 1 underlying token; no dynamic yields).
//! - **ERC20 Interface**: SY tokens support standard ERC20 functions (transfer, approve, etc.).
//! - **Reentrancy Protection**: Non-reentrant guards on deposit/redeem.
//! - **No Rewards**: This simple SY has no yield accrual or reward mechanisms (contrast with advanced SYs).
//!
//! ## Execution Flow (high level)
//! - `deposit(receiver, tokenIn, amount, minSharesOut)`: `transferFrom` underlying, mint SY shares 1:1, emit `Deposit`.
//! - `redeem(receiver, shares, tokenOut, minTokenOut, burnFromInternalBalance)`: burn shares, transfer out underlying, emit `Redeem`.
//! - `exchangeRate()`: constant `1e18` (no yield mechanics).
//!
//! ## Hydra N=2 Specifics
//! - **Storage Mode**: HydraStorage (separate N=2 storage for decorrelation).
//! - **Underlying**: This SY wraps a single ERC20 `yieldToken` (the "asset") provided at deployment.
//! - **Decimals**: Passed at deployment (constructor arg) instead of calling out to `yieldToken.decimals()`.
//! - **ABI Parity**: Public/external functions and events match Solidity exactly for N=1/N=2 equivalence.
//!
//! ## Hydra install constraint (important)
//! Hydra installs N=2 code by running the initcode via an **operator CALL**, not a contract `CREATE`.
//! Any constructor logic that relies on "I am being created" (e.g. the R55 exception syscall
//! `Syscall::ReturnCreateAddress`) will fail.
//!
//! This implementation uses the normal EVM `ADDRESS (0x30)` opcode via `eth_riscv_runtime::tx::address()`,
//! which works in both CREATE and CALL contexts (including Hydra operator installs).
//!
//! ## Security Considerations
//! - **Reentrancy**: Guards prevent reentrant calls on deposit/redeem.
//! - **ERC20 Assumptions**: Assumes underlying ERC20 is standard (no fee-on-transfer or rebasing tokens).
//! - **Overflow/Underflow**: U256 arithmetic; potential underflow in redeem if SY supply < redeem amount.
//! - **Access Control**: No owner/pause; relies on external governance.
//!
//! ## Edge Cases & Assumptions
//! - **Zero Deposits/Redeems**: Reverts as per Solidity.
//! - **Invalid Tokens**: Only accepts the configured underlying ERC20.
//! - **Decimals Mismatch**: Caller must pass correct decimals; mismatches could break ERC20 compatibility.
//! - **Balance Queries**: Uses `this_address` for SY self-balances; incorrect setting breaks functionality.
//!
//! ## Differences from Solidity
//! - Constructor takes decimals as parameter (R55 cannot read ERC20 metadata directly).
//! - No Pausable (inherited from SYBase but not implemented in simple ERC20 SY).
//! - Storage uses slots/mappings instead of Solidity structs (HydraStorage allows differences).
//! - No direct ERC20 metadata reads (use syscalls or parameters).
//!
//! ## Performance Notes
//! - Simple 1:1 math: Low gas overhead.
//! - R55 syscalls for balances/transfers: Efficient but dependent on runtime.
//!
//! ## Extensibility
//! - Add rewards: Extend with RewardManager-like logic for staking rewards.
//! - Dynamic rates: Implement variable exchange rates based on underlying yields.
//! - Pausing: Add pause/unpause for emergency stops.
//!
//! ## Testing Notes
//! - **Parity Tests**: Compare deposit/redeem outputs with Solidity N=1.
//! - **Edge Cases**: Zero amounts, invalid tokens, decimals errors.
//! - **Integration**: Test with Hydra ERC20 bridge for cross-chain flows.
//!
//! ## Usage
//! Deploy Solidity N=1 SY first, then attach this R55 N=2 in Hydra for parallel execution.
//!
//! ## References
//! - Pendle Docs: SY as tokenized yield positions.
//! - Hydra: N=2 for fault-tolerant, decorrelated program execution.
//! - ERC20 Standard: Underlying token assumptions.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use alloy_core::primitives::{Address, U256};
use contract_derive::{contract, storage, Error, Event, interface};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::tx;

// Constants
const ONE: U256 = U256::from_limbs([1000000000000000000, 0, 0, 0]); // 1e18
const NOT_ENTERED: U256 = U256::from_limbs([1, 0, 0, 0]);
const ENTERED: U256 = U256::from_limbs([2, 0, 0, 0]);

// Interfaces
#[interface("camelCase")]
trait IERC20Minimal {
    fn balanceOf(&self, account: Address) -> U256;
    fn transfer(&mut self, to: Address, amount: U256) -> Option<bool>;
    fn transferFrom(&mut self, from: Address, to: Address, amount: U256) -> Option<bool>;
    fn decimals(&self) -> U256;
    fn symbol(&self) -> String;
    fn name(&self) -> String;
}

#[interface("camelCase")]
trait IStandardizedYield {
    fn deposit(&mut self, receiver: Address, tokenIn: Address, amountTokenToDeposit: U256, minSharesOut: U256) -> U256;
    fn redeem(&mut self, receiver: Address, amountSharesToRedeem: U256, tokenOut: Address, minTokenOut: U256, burnFromInternalBalance: bool) -> U256;
    fn exchangeRate(&self) -> U256;
    fn getTokensIn(&self) -> Vec<Address>;
    fn getTokensOut(&self) -> Vec<Address>;
    fn isValidTokenIn(&self, token: Address) -> bool;
    fn isValidTokenOut(&self, token: Address) -> bool;
    fn previewDeposit(&self, tokenIn: Address, amountTokenToDeposit: U256) -> U256;
    fn previewRedeem(&self, tokenOut: Address, amountSharesToRedeem: U256) -> U256;
    fn claimRewards(&mut self, user: Address) -> Vec<U256>;
    fn getRewardTokens(&self) -> Vec<Address>;
    fn accruedRewards(&self, user: Address) -> Vec<U256>;
    fn rewardIndexesCurrent(&mut self) -> Vec<U256>;
    fn rewardIndexesStored(&self) -> Vec<U256>;
}

// Events
#[derive(Event)]
pub struct Deposit {
    #[indexed]
    pub sender: Address,
    #[indexed]
    pub receiver: Address,
    #[indexed]
    pub tokenIn: Address,
    pub amountDeposited: U256,
    pub amountMinted: U256,
}

#[derive(Event)]
pub struct Redeem {
    #[indexed]
    pub sender: Address,
    #[indexed]
    pub receiver: Address,
    #[indexed]
    pub tokenOut: Address,
    pub amountShares: U256,
    pub amountTokenOut: U256,
}

#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub value: U256,
}

#[derive(Event)]
pub struct Approval {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub spender: Address,
    pub value: U256,
}

// Errors
#[derive(Error)]
pub enum SYError {
    InvalidTokenIn(Address),
    InvalidTokenOut(Address),
    ZeroAddress,
    SelfTransfer,
    ZeroDeposit,
    ZeroRedeem,
    InsufficientSharesOut(U256, U256),
    InsufficientTokenOut(U256, U256),
    ReentrantCall,
    NotOwner,
    Paused,
}

// Storage
#[storage]
pub struct StandardizedYield {
    yieldToken: Slot<Address>,
    decimals: Slot<U256>,
    /// Cached self address (used for custody transfers / internal burns).
    this_address: Slot<Address>,
    owner: Slot<Address>,
    paused: Slot<bool>,
    name: DynamicSlot<String>,
    symbol: DynamicSlot<String>,
    totalSupply: Slot<U256>,
    balance_of: Mapping<Address, Slot<U256>>,
    allowance_of: Mapping<Address, Mapping<Address, Slot<U256>>>,
    reentrancy_status: Slot<U256>,
    initialized: Slot<U256>,
}

// Contract Implementation
#[contract]
impl StandardizedYield {
    // Constructor.
    //
    // NOTE: This is intentionally *not* Solidity-identical. Hydra N=2 installation is an operator
    // operation, so the N=2 constructor can take extra metadata inputs without affecting external
    // call parity. We accept `decimals` as an argument to avoid an external call in the constructor.
    //
    // IMPORTANT:
    // Do NOT use the R55 exception syscall `Syscall::ReturnCreateAddress` here.
    //
    // Hydra installs N=2 code via an operator CALL (not `CREATE`), so `ReturnCreateAddress` has no
    // created address and will REVERT in the current R55 EVM implementation. That revert causes
    // Hydra to roll back the install checkpoint, leaving the Hydra account `mode` unset (`None`).
    //
    // Use the standard EVM `ADDRESS (0x30)` opcode instead (`tx::address()`), which works in both
    // CREATE and CALL contexts.
    pub fn new(
        name: String,
        symbol: String,
        yieldToken: Address,
        decimals: U256,
        owner: Address,
    ) -> Self {
        let mut sy = StandardizedYield::default();
        sy.yieldToken.write(yieldToken);
        sy.this_address.write(tx::address());
        sy.decimals.write(decimals);
        sy.owner.write(owner);
        sy.paused.write(false);
        sy.name.write(name);
        sy.symbol.write(symbol);
        sy.reentrancy_status.write(NOT_ENTERED);
        sy.initialized.write(U256::from(1));
        sy
    }

    // Pausable (mirrors SYBase: pause/unpause are onlyOwner; token movements are blocked while paused).
    pub fn paused(&self) -> bool {
        self.paused.read()
    }

    pub fn pause(&mut self) -> Result<(), SYError> {
        if msg_sender() != self.owner.read() {
            return Err(SYError::NotOwner);
        }
        self.paused.write(true);
        Ok(())
    }

    pub fn unpause(&mut self) -> Result<(), SYError> {
        if msg_sender() != self.owner.read() {
            return Err(SYError::NotOwner);
        }
        self.paused.write(false);
        Ok(())
    }

    // ERC20-like functions
    pub fn name(&self) -> String {
        self.name.read()
    }

    pub fn symbol(&self) -> String {
        self.symbol.read()
    }

    pub fn decimals(&self) -> U256 {
        self.decimals.read()
    }

    pub fn totalSupply(&self) -> U256 {
        self.totalSupply.read()
    }

    pub fn balanceOf(&self, account: Address) -> U256 {
        self.balance_of[account].read()
    }

    pub fn allowance(&self, owner: Address, spender: Address) -> U256 {
        self.allowance_of[owner][spender].read()
    }

    pub fn transfer(&mut self, to: Address, amount: U256) -> Result<bool, SYError> {
        self.non_reentrant(|this| {
            this._transfer(msg_sender(), to, amount)?;
            Ok(true)
        })
    }

    pub fn transferFrom(&mut self, from: Address, to: Address, amount: U256) -> Result<bool, SYError> {
        self.non_reentrant(|this| {
            let spender = msg_sender();
            this._spend_allowance(from, spender, amount)?;
            this._transfer(from, to, amount)?;
            Ok(true)
        })
    }

    pub fn approve(&mut self, spender: Address, amount: U256) -> Result<bool, SYError> {
        let owner = msg_sender();
        self._approve(owner, spender, amount)?;
        Ok(true)
    }

    // SY-specific functions
    pub fn deposit(&mut self, receiver: Address, tokenIn: Address, amountTokenToDeposit: U256, minSharesOut: U256) -> Result<U256, SYError> {
        if !self.isValidTokenIn(tokenIn) {
            return Err(SYError::InvalidTokenIn(tokenIn));
        }
        if amountTokenToDeposit.is_zero() {
            return Err(SYError::ZeroDeposit);
        }
        self.non_reentrant(|this| {
            let mut erc20 = IERC20Minimal::new(tokenIn).with_ctx(&mut *this);
            // Match SYBase._transferIn(tokenIn, msg.sender, amount): custody the underlying on this contract.
            let ok = erc20.transferFrom(msg_sender(), this.this_address.read(), amountTokenToDeposit);
            match ok {
                Some(true) | None => {}
                Some(false) => revert(),
            }
            let amountSharesOut = this._deposit(tokenIn, amountTokenToDeposit);
            if amountSharesOut < minSharesOut {
                return Err(SYError::InsufficientSharesOut(amountSharesOut, minSharesOut));
            }
            this._mint(receiver, amountSharesOut)?;
            log::emit(Deposit::new(msg_sender(), receiver, tokenIn, amountTokenToDeposit, amountSharesOut));
            Ok(amountSharesOut)
        })
    }

    pub fn redeem(&mut self, receiver: Address, amountSharesToRedeem: U256, tokenOut: Address, minTokenOut: U256, burnFromInternalBalance: bool) -> Result<U256, SYError> {
        if !self.isValidTokenOut(tokenOut) {
            return Err(SYError::InvalidTokenOut(tokenOut));
        }
        if amountSharesToRedeem.is_zero() {
            return Err(SYError::ZeroRedeem);
        }
        self.non_reentrant(|this| {
            if burnFromInternalBalance {
                // Match SYBase behavior: burn from address(this)
                this._burn(this.this_address.read(), amountSharesToRedeem)?;
            } else {
                this._burn(msg_sender(), amountSharesToRedeem)?;
            }
            let amountTokenOut = this._redeem(receiver, tokenOut, amountSharesToRedeem);
            if amountTokenOut < minTokenOut {
                return Err(SYError::InsufficientTokenOut(amountTokenOut, minTokenOut));
            }
            log::emit(Redeem::new(msg_sender(), receiver, tokenOut, amountSharesToRedeem, amountTokenOut));
            Ok(amountTokenOut)
        })
    }

    pub fn exchangeRate(&self) -> U256 {
        ONE // Static 1:1
    }

    pub fn getTokensIn(&self) -> Vec<Address> {
        alloc::vec![self.yieldToken.read()]
    }

    pub fn getTokensOut(&self) -> Vec<Address> {
        alloc::vec![self.yieldToken.read()]
    }

    pub fn isValidTokenIn(&self, token: Address) -> bool {
        token == self.yieldToken.read()
    }

    pub fn isValidTokenOut(&self, token: Address) -> bool {
        token == self.yieldToken.read()
    }

    // Solidity parity: `assetInfo() -> (AssetType, assetAddress, assetDecimals)`.
    //
    // `AssetType.TOKEN == 0` (see `Pendle-SY-Public/contracts/interfaces/IStandardizedYield.sol`).
    pub fn assetInfo(&mut self) -> (U256, Address, U256) {
        let yield_token = self.yieldToken.read();
        (U256::from(0u8), yield_token, self.decimals.read())
    }

    pub fn previewDeposit(&self, tokenIn: Address, amountTokenToDeposit: U256) -> U256 {
        if !self.isValidTokenIn(tokenIn) {
            return U256::ZERO;
        }
        amountTokenToDeposit // 1:1
    }

    pub fn previewRedeem(&self, tokenOut: Address, amountSharesToRedeem: U256) -> U256 {
        if !self.isValidTokenOut(tokenOut) {
            return U256::ZERO;
        }
        amountSharesToRedeem // 1:1
    }

    // Placeholder for rewards (none in simple SY)
    pub fn claimRewards(&mut self, _user: Address) -> Vec<U256> {
        Vec::new()
    }

    pub fn getRewardTokens(&self) -> Vec<Address> {
        Vec::new()
    }

    pub fn accruedRewards(&self, _user: Address) -> Vec<U256> {
        Vec::new()
    }

    pub fn rewardIndexesCurrent(&mut self) -> Vec<U256> {
        Vec::new()
    }

    pub fn rewardIndexesStored(&self) -> Vec<U256> {
        Vec::new()
    }

}

// NOTE: no `self_address_from_create()` helper here.
// Use `eth_riscv_runtime::tx::address()` (EVM `ADDRESS`, 0x30) instead.

// Internal helpers (plain impl for methods accessible within contract)
impl StandardizedYield {
    fn _deposit(&self, _tokenIn: Address, amountDeposited: U256) -> U256 {
        amountDeposited // 1:1
    }

    fn _redeem(&mut self, receiver: Address, _tokenOut: Address, amountSharesToRedeem: U256) -> U256 {
        let mut erc20 = IERC20Minimal::new(self.yieldToken.read()).with_ctx(&mut *self);
        // Best-effort ERC20 transfer; accept tokens that return no bool.
        match erc20.transfer(receiver, amountSharesToRedeem) {
            Some(true) | None => {}
            Some(false) => revert(),
        }
        amountSharesToRedeem // 1:1
    }

    fn non_reentrant<F, R>(&mut self, f: F) -> Result<R, SYError>
    where
        F: FnOnce(&mut Self) -> Result<R, SYError>,
    {
        if self.reentrancy_status.read() == ENTERED {
            return Err(SYError::ReentrantCall);
        }
        self.reentrancy_status.write(ENTERED);
        let result = f(self);
        self.reentrancy_status.write(NOT_ENTERED);
        result
    }

    // PendleERC20 parity:
    // - `_transfer` forbids zero-address and self-transfer.
    // - `_mint`/`_burn` are separate and also forbid zero-address.
    // - Pause gating is applied to all token movements (transfer/mint/burn), matching SYBase
    //   `_beforeTokenTransfer(...) whenNotPaused`.
    fn _transfer(&mut self, from: Address, to: Address, amount: U256) -> Result<(), SYError> {
        if self.paused.read() {
            return Err(SYError::Paused);
        }
        if from.is_zero() || to.is_zero() {
            return Err(SYError::ZeroAddress);
        }
        if from == to {
            return Err(SYError::SelfTransfer);
        }
        let from_balance = self.balance_of[from].read();
        if from_balance < amount {
            return Err(SYError::InsufficientTokenOut(from_balance, amount));
        }
        self.balance_of[from].write(from_balance - amount);
        let to_balance = self.balance_of[to].read() + amount;
        self.balance_of[to].write(to_balance);
        log::emit(Transfer::new(from, to, amount));
        Ok(())
    }

    fn _mint(&mut self, account: Address, amount: U256) -> Result<(), SYError> {
        if self.paused.read() {
            return Err(SYError::Paused);
        }
        if account.is_zero() {
            return Err(SYError::ZeroAddress);
        }
        let new_supply = self.totalSupply.read() + amount;
        self.totalSupply.write(new_supply);
        let to_balance = self.balance_of[account].read() + amount;
        self.balance_of[account].write(to_balance);
        log::emit(Transfer::new(Address::ZERO, account, amount));
        Ok(())
    }

    fn _burn(&mut self, account: Address, amount: U256) -> Result<(), SYError> {
        if self.paused.read() {
            return Err(SYError::Paused);
        }
        if account.is_zero() {
            return Err(SYError::ZeroAddress);
        }
        let from_balance = self.balance_of[account].read();
        if from_balance < amount {
            return Err(SYError::InsufficientTokenOut(from_balance, amount));
        }
        self.balance_of[account].write(from_balance - amount);
        let new_supply = self.totalSupply.read() - amount;
        self.totalSupply.write(new_supply);
        log::emit(Transfer::new(account, Address::ZERO, amount));
        Ok(())
    }

    fn _approve(&mut self, owner: Address, spender: Address, amount: U256) -> Result<(), SYError> {
        // PendleERC20: approve from/to zero address is forbidden.
        if owner.is_zero() || spender.is_zero() {
            return Err(SYError::ZeroAddress);
        }
        self.allowance_of[owner][spender].write(amount);
        log::emit(Approval::new(owner, spender, amount));
        Ok(())
    }

    fn _spend_allowance(&mut self, owner: Address, spender: Address, amount: U256) -> Result<(), SYError> {
        let current = self.allowance_of[owner][spender].read();
        if current != U256::MAX {
            if current < amount {
                return Err(SYError::InsufficientTokenOut(current, amount));
            }
            self.allowance_of[owner][spender].write(current - amount);
        }
        Ok(())
    }
}