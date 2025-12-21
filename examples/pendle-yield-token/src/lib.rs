//! # PendleYieldToken (YT) — Hydra N=2 Implementation
//!
//! ## Overview
//! This R55 contract implements a Hydra-compatible N=2 version of Pendle's Yield Token (YT).
//! YT represents the future yield component of a Pendle Principal Token (PT), accruing interest and rewards over time.
//! Provides ABI parity with Solidity YT contracts for decorrelated Hydra execution. This is a highly complex DeFi contract
//! handling accrual, fees, and post-expiry logic.
//!
//! ## Solidity Reference
//! - **Main Contract**: `pendle-core-v2-public/contracts/core/YieldContracts/PendleYieldToken.sol`
//! - **Inheritance**:
//!   - `RewardManagerAbstract`: Handles reward distribution from SY.
//!   - `InterestManagerYT`: Manages interest accrual based on PY index.
//!   - `PendleERC20`: ERC20 base for YT tokens.
//!
//! ## Key Features
//! - **Mint PY**: Deposit SY into PT + YT pair (PT for principal, YT for yield rights); computes PY amount via SY conversion.
//! - **Redeem PY**: Burn YT (and PT if pre-expiry) to withdraw SY; post-expiry accrues treasury interest.
//! - **Interest Accrual**: YT holders earn interest from underlying SY yields (tracked via monotonic PY index).
//! - **Reward Accrual**: YT holders receive rewards from SY (distributed via user indexes and shares).
//! - **Post-Expiry**: After expiry, treasury collects remaining interest/rewards; users redeem via PT.
//! - **Fee Handling**: Interest/reward fees paid to factory treasury on redemptions.
//! - **PY Index**: Monotonic, cached per block; determines interest accrual (non-decreasing).
//! - **Hooks**: Before-transfer hooks for accrual on any balance changes (transfers, mints, redeems).
//!
//! ## Hydra N=2 Specifics
//! - **Storage Mode**: HydraStorage (separate N=2 storage for decorrelation).
//! - **Accrual Mechanics**: Reward/interest distribution uses user-specific indexes and accruals (inline RewardManager/InterestManager).
//! - **this_address**: Used for contract-held balances (PT/YT during redeem); must be set accurately.
//! - **Post-Expiry**: Snapshot PY index, reward indexes, and owed amounts; treasury payouts include external reward redemption.
//! - **ABI Parity**: All functions, events, and return types match Solidity for N=1/N=2 equivalence.
//!
//! ## Execution Flow (high level)
//! - Factory deploys YT and sets `(SY, PT, factory, expiry, doCacheIndexSameBlock)`.
//!   - This implementation sets `this_address` during construction (used for internal reserve/balance reads).
//! - Core entrypoints:
//!   - `mintPY(...)`: pull SY, compute PY output using the SY exchange rate / PY index, mint PT+YT in lockstep.
//!   - `redeemPY(...)`: burn YT (and PT pre-expiry), return SY; post-expiry performs treasury accounting.
//!   - Transfers call into “before transfer” accrual hooks to keep indexes and user accruals consistent.
//! - External calls (critical to parity):
//!   - SY: `exchangeRate`, `rewardIndexesCurrent`, `getRewardTokens`, `claimRewards`
//!   - Factory: `treasury`, `rewardFeeRate`, `interestFeeRate`
//!   - PT: `mintByYT`, `burnByYT`
//!
//! ## Where divergence is most likely
//! - Interest/reward math (rounding/truncation) and index caching (block.number vs runtime syscall).
//! - Post-expiry snapshots + fee paths that mix external calls (SY reward claiming) with internal accounting.
//!
//! ## Security Considerations
//! - **Reentrancy**: Guards on all state-changing functions (mint, redeem, transfers).
//! - **Overflow/Underflow**: U256 arithmetic; careful with multiplication/division in accrual (e.g., interestFromYT formula).
//! - **Access Control**: Factory-only setter for `this_address`; no owner/pause (relies on external governance).
//! - **Index Monotonicity**: PY index must not decrease; enforced via max with cached value.
//! - **Treasury Assumptions**: Factory treasury address must be valid; fees sent correctly.
//!
//! ## Edge Cases & Assumptions
//! - **Expiry Handling**: Pre-expiry burns both PT/YT; post-expiry only YT; index snapshots at expiry.
//! - **Zero Balances/Amounts**: Reverts on zero mint/redeem; accrual skips zero balances.
//! - **Rounding Errors**: Interest/reward calculations use divDown-like behavior (truncation); verify against Solidity.
//! - **External Rewards**: Redeemed once per redemption if balances short; assumes SY.claimRewards succeeds.
//! - **Large Numbers**: U256 handles large amounts, but test for precision in accrual math.
//! - **Post-Expiry Owed**: Tracks user reward owed; treasury collects deltas.
//!
//! ## Differences from Solidity
//! - Single-file implementation (no inheritance; inline RewardManager/InterestManager logic).
//! - Storage uses slots/mappings (not Solidity structs; HydraStorage allows differences).
//! - Index caching via R55 block syscall (not Solidity block.number).
//! - No Pausable (YT doesn't override; add if needed for emergencies).
//! - Accrual math inlined (no separate contracts; potential for divergence if not exact).
//!
//! ## Complexities
//! - Accrual hooks before transfers/mints/redeems (critical for state consistency).
//! - Post-expiry data snapshots and treasury payouts (multi-step with external calls).
//! - Fee calculations (reward/interest rates from factory; applied on redemption).
//! - PY index monotonicity and expiry detection (via block timestamp).
//! - Reward shares based on SY-equivalent YT balance + accrued interest.
//!
//! ## Performance Notes
//! - Accrual on every transfer: Higher gas but ensures up-to-date state.
//! - R55 syscalls: Efficient for block/timestamp, but external SY calls add overhead.
//! - Post-expiry snapshots: One-time cost after expiry.
//!
//! ## Extensibility
//! - Add pausing: Implement emergency pause/unpause.
//! - Custom rewards: Extend reward logic for non-SY rewards.
//! - Governance: Add owner controls for fees or parameters.
//!
//! ## Testing Notes
//! - **Parity Tests**: Compare mint/redeem/accrual outputs with Solidity N=1 across expiry scenarios.
//! - **Math Precision**: Verify interest/reward calculations (e.g., divDown truncation).
//! - **Edge Cases**: Zero amounts, expiry transitions, large balances, treasury invalid.
//! - **Hooks**: Ensure accrual runs before all balance changes.
//! - **Integration**: Test with PT and SY in full PY flows; verify Hydra N=2 vs N=1 logs.
//!
//! ## Usage
//! Deploy Solidity N=1 YT first, then attach this R55 N=2 in Hydra for parallel execution.
//!
//! ## References
//! - Pendle YT Docs: YT as yield-bearing tokens.
//! - RewardManagerAbstract: Reward distribution logic.
//! - InterestManagerYT: Interest accrual via PY index.
//! - DeFi Risks: Complex accrual math can lead to precision errors; thorough auditing required.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use alloy_core::primitives::{Address, U256};
use contract_derive::{contract, storage, Error, Event, interface};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::tx;

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------
const VERSION: U256 = U256::from_limbs([6, 0, 0, 0]);
const NOT_ENTERED: U256 = U256::from_limbs([1, 0, 0, 0]);
const ENTERED: U256 = U256::from_limbs([2, 0, 0, 0]);
const ONE: U256 = U256::from_limbs([1_000_000_000_000_000_000u64, 0, 0, 0]);

// -----------------------------------------------------------------------------
// Interfaces
// -----------------------------------------------------------------------------
#[interface("camelCase")]
trait IStandardizedYield {
    fn exchangeRate(&mut self) -> U256;
    fn rewardIndexesCurrent(&mut self) -> Vec<U256>;
    fn getRewardTokens(&self) -> Vec<Address>;
    fn claimRewards(&mut self, receiver: Address);
}

#[interface("camelCase")]
trait IPPrincipalToken {
    fn mintByYT(&mut self, user: Address, amount: U256);
    fn burnByYT(&mut self, user: Address, amount: U256);
}

#[interface("camelCase")]
trait IPYieldContractFactory {
    fn treasury(&self) -> Address;
    fn rewardFeeRate(&self) -> U256;
    fn interestFeeRate(&self) -> U256;
}

#[interface("camelCase")]
trait IERC20Minimal {
    fn balanceOf(&self, account: Address) -> U256;
}

#[interface("camelCase")]
trait IERC20Like {
    fn balanceOf(&self, account: Address) -> U256;
    fn transfer(&mut self, to: Address, amount: U256) -> bool;
}

// -----------------------------------------------------------------------------
// Events (per ABI)
// -----------------------------------------------------------------------------
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

#[derive(Event)]
pub struct Initialized {
    pub version: u8,
}

#[derive(Event)]
pub struct Mint {
    #[indexed]
    pub caller: Address,
    #[indexed]
    pub receiver_pt: Address,
    #[indexed]
    pub receiver_yt: Address,
    pub amount_sy_in: U256,
    pub amount_py_out: U256,
}

#[derive(Event)]
pub struct Burn {
    #[indexed]
    pub caller: Address,
    #[indexed]
    pub receiver: Address,
    pub amount_py_in: U256,
    pub amount_sy_out: U256,
}

#[derive(Event)]
pub struct RedeemRewards {
    #[indexed]
    pub user: Address,
    pub rewards_out: Vec<U256>,
}

#[derive(Event)]
pub struct RedeemInterest {
    #[indexed]
    pub user: Address,
    pub interest_out: U256,
}

#[derive(Event)]
pub struct CollectRewardFee {
    pub token: Address,
    pub fee: U256,
}

#[derive(Event)]
pub struct CollectInterestFee {
    pub fee: U256,
}

#[derive(Event)]
pub struct NewInterestIndex {
    pub index: U256,
}

// -----------------------------------------------------------------------------
// Errors (per ABI)
// -----------------------------------------------------------------------------
#[derive(Error)]
pub enum YTError {
    ArrayEmpty,
    ArrayLengthMismatch,
    YCExpired,
    YCNoFloatingSy,
    YCNotExpired,
    YCNothingToRedeem,
    YCPostExpiryDataNotSet,
    YieldContractInsufficientSy(U256, U256),
    ReentrantCall,
    ZeroAddress,
    InsufficientBalance(U256),
    InsufficientAllowance(U256),
    SelfTransfer,
    SelfApproval,
    AlreadyInitialized,
    Unauthorized,
}

// -----------------------------------------------------------------------------
// Storage
// -----------------------------------------------------------------------------
#[storage]
pub struct PendleYieldToken {
    // ERC20 base
    total_supply: Slot<U256>,
    reentrancy_status: Slot<U256>,
    balance_of: Mapping<Address, Slot<U256>>,
    allowance_of: Mapping<Address, Mapping<Address, Slot<U256>>>,
    name: DynamicSlot<String>,
    symbol: DynamicSlot<String>,
    decimals: Slot<U256>,

    // Immutables / config
    sy: Slot<Address>,
    pt: Slot<Address>,
    factory: Slot<Address>,
    /// N=2 self address supplied out-of-band (constructor or explicit setter).
    /// Must be set to the contract address before reserve/balance reads; flows assume PT/YT are held here for redeem.
    this_address: Slot<Address>,
    expiry: Slot<U256>,
    do_cache_index_same_block: Slot<bool>,

    // Basic YT state (placeholders; full logic later)
    sy_reserve: Slot<U256>,
    py_index_last_updated_block: Slot<U256>,
    py_index_stored: Slot<U256>,
    post_expiry_first_py_index: Slot<U256>,
    post_expiry_total_sy_interest_for_treasury: Slot<U256>,

    // Post-expiry per-token data (token -> index / owed)
    post_expiry_first_reward_index: Mapping<Address, Slot<U256>>,
    post_expiry_user_reward_owed: Mapping<Address, Slot<U256>>,

    // Reward state: token -> user -> RewardState
    reward_index: Mapping<Address, Slot<U256>>,
    reward_last_balance: Mapping<Address, Slot<U256>>,
    user_reward_index: Mapping<Address, Mapping<Address, Slot<U256>>>,
    user_reward_accrued: Mapping<Address, Mapping<Address, Slot<U256>>>,

    // Interest state: user -> UserInterest
    user_interest_index: Mapping<Address, Slot<U256>>,
    user_interest_accrued: Mapping<Address, Slot<U256>>,

    // Init flag (only if initialization ever added)
    initialized: Slot<U256>,
}

// -----------------------------------------------------------------------------
// Public/external implementation (ABI surface)
// -----------------------------------------------------------------------------
#[contract]
impl PendleYieldToken {
    // ---------------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------------
    pub fn new(
        sy: Address,
        pt: Address,
        name: String,
        symbol: String,
        decimals: U256,
        expiry: U256,
        do_cache_index_same_block: bool,
    ) -> Self {
        let mut yt = PendleYieldToken::default();

        yt.sy.write(sy);
        yt.pt.write(pt);
        yt.factory.write(msg_sender());
        // Populate `address(this)` using EVM `ADDRESS (0x30)`.
        yt.this_address.write(tx::address());
        yt.expiry.write(expiry);
        yt.decimals.write(decimals);
        yt.name.write(name);
        yt.symbol.write(symbol);
        yt.do_cache_index_same_block.write(do_cache_index_same_block);
        yt.reentrancy_status.write(NOT_ENTERED);
        yt.initialized.write(U256::from(1)); // constructor-only init

        yt
    }

    /// Optional setter to populate `this_address` if constructor injection is not feasible.
    /// Must be called once by the factory/operator before any reserve sync.
    pub fn setThisAddress(&mut self, addr: Address) -> Result<(), YTError> {
        if self.this_address.read() != Address::ZERO {
            return Err(YTError::AlreadyInitialized);
        }
        if addr == Address::ZERO {
            return Err(YTError::ZeroAddress);
        }
        if msg_sender() != self.factory.read() {
            return Err(YTError::Unauthorized);
        }
        self.this_address.write(addr);
        Ok(())
    }

    // ---------------------------------------------------------------------
    // ERC20
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

    pub fn transfer(&mut self, to: Address, amount: U256) -> Result<bool, YTError> {
        self.non_reentrant(|this| {
            this._transfer(msg_sender(), to, amount)?;
            Ok(true)
        })
    }

    pub fn transferFrom(&mut self, from: Address, to: Address, amount: U256) -> Result<bool, YTError> {
        self.non_reentrant(|this| {
            let spender = msg_sender();
            this._spend_allowance(from, spender, amount)?;
            this._transfer(from, to, amount)?;
            Ok(true)
        })
    }

    pub fn approve(&mut self, spender: Address, amount: U256) -> Result<bool, YTError> {
        let owner = msg_sender();
        self._approve(owner, spender, amount)?;
        Ok(true)
    }

    // ---------------------------------------------------------------------
    // Views
    // ---------------------------------------------------------------------
    pub fn SY(&self) -> Address {
        self.sy.read()
    }

    pub fn PT(&self) -> Address {
        self.pt.read()
    }

    pub fn factory(&self) -> Address {
        self.factory.read()
    }

    pub fn expiry(&self) -> U256 {
        self.expiry.read()
    }

    pub fn doCacheIndexSameBlock(&self) -> bool {
        self.do_cache_index_same_block.read()
    }

    pub fn syReserve(&self) -> U256 {
        self.sy_reserve.read()
    }

    pub fn pyIndexLastUpdatedBlock(&self) -> U256 {
        self.py_index_last_updated_block.read()
    }

    pub fn pyIndexStored(&self) -> U256 {
        self.py_index_stored.read()
    }

    pub fn postExpiry(&self) -> (U256, U256) {
        (
            self.post_expiry_first_py_index.read(),
            self.post_expiry_total_sy_interest_for_treasury.read(),
        )
    }

    pub fn getPostExpiryData(&self) -> Result<(U256, U256, Vec<U256>, Vec<U256>), YTError> {
        let first_py = self.post_expiry_first_py_index.read();
        if first_py.is_zero() {
            return Err(YTError::YCPostExpiryDataNotSet);
        }
        let reward_tokens = {
            let sy = IStandardizedYield::new(self.sy.read()).with_ctx(&*self);
            sy.getRewardTokens().expect("SY.getRewardTokens failed")
        };
        let mut first_reward_indexes = Vec::with_capacity(reward_tokens.len());
        let mut user_reward_owed = Vec::with_capacity(reward_tokens.len());
        for token in reward_tokens.into_iter() {
            first_reward_indexes.push(self.post_expiry_first_reward_index[token].read());
            user_reward_owed.push(self.post_expiry_user_reward_owed[token].read());
        }
        Ok((
            first_py,
            self.post_expiry_total_sy_interest_for_treasury.read(),
            first_reward_indexes,
            user_reward_owed,
        ))
    }

    pub fn getRewardTokens(&self) -> Vec<Address> {
        let sy = IStandardizedYield::new(self.sy.read()).with_ctx(&*self);
        sy.getRewardTokens().expect("SY.getRewardTokens failed")
    }

    pub fn isExpired(&self) -> bool {
        let now = eth_riscv_runtime::block::timestamp();
        self.expiry.read() <= now
    }

    pub fn VERSION(&self) -> U256 {
        VERSION
    }

    pub fn reentrancyGuardEntered(&self) -> bool {
        self.reentrancy_status.read() == ENTERED
    }

    pub fn treasury(&self) -> Address {
        let factory = IPYieldContractFactory::new(self.factory.read()).with_ctx(&*self);
        factory.treasury().expect("factory.treasury failed")
    }

    pub fn rewardFeeRate(&self) -> U256 {
        let factory = IPYieldContractFactory::new(self.factory.read()).with_ctx(&*self);
        factory.rewardFeeRate().expect("factory.rewardFeeRate failed")
    }

    pub fn interestFeeRate(&self) -> U256 {
        let factory = IPYieldContractFactory::new(self.factory.read()).with_ctx(&*self);
        factory.interestFeeRate().expect("factory.interestFeeRate failed")
    }

    // Mappings views
    pub fn userInterest(&self, user: Address) -> (U256, U256) {
        (self.user_interest_index[user].read(), self.user_interest_accrued[user].read())
    }

    pub fn userReward(&self, token: Address, user: Address) -> (U256, U256) {
        (
            self.user_reward_index[token][user].read(),
            self.user_reward_accrued[token][user].read(),
        )
    }

    // ---------------------------------------------------------------------
    // Expiry guards
    // ---------------------------------------------------------------------
    pub fn check_not_expired(&self) -> Result<(), YTError> {
        if self.isExpired() {
            return Err(YTError::YCExpired);
        }
        Ok(())
    }

    pub fn check_expired(&self) -> Result<(), YTError> {
        if !self.isExpired() {
            return Err(YTError::YCNotExpired);
        }
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Core YT functions (scaffold stubs)
    // ---------------------------------------------------------------------
    pub fn mintPY(&mut self, _receiver_pt: Address, _receiver_yt: Address) -> Result<U256, YTError> {
        self.check_not_expired()?;
        if _receiver_pt == Address::ZERO || _receiver_yt == Address::ZERO {
            return Err(YTError::ZeroAddress);
        }
        self.non_reentrant(|this| {
            this.update_data()?;
            let floating_sy = this.get_floating_sy_amount()?;
            let index = this.pyIndexCurrent()?; // updates cache if needed
            let amount_py_out = this.calc_py_to_mint(floating_sy, index);
            // Match Solidity order: YT first, then PT
            this._mint(_receiver_yt, amount_py_out)?;
            let mut pt = IPPrincipalToken::new(this.pt.read()).with_ctx(&mut *this);
            pt.mintByYT(_receiver_pt, amount_py_out).expect("PT.mintByYT failed");
            this.update_sy_reserve();
            log::emit(Mint::new(msg_sender(), _receiver_pt, _receiver_yt, floating_sy, amount_py_out));
            Ok(amount_py_out)
        })
    }

    pub fn mintPYMulti(
        &mut self,
        _receiver_pts: Vec<Address>,
        _receiver_yts: Vec<Address>,
        _amount_sy_to_mints: Vec<U256>,
    ) -> Result<Vec<U256>, YTError> {
        if _receiver_pts.is_empty() || _receiver_yts.is_empty() || _amount_sy_to_mints.is_empty() {
            return Err(YTError::ArrayEmpty);
        }
        if !(_receiver_pts.len() == _receiver_yts.len() && _receiver_pts.len() == _amount_sy_to_mints.len()) {
            return Err(YTError::ArrayLengthMismatch);
        }
        if _receiver_pts.iter().any(|&a| a == Address::ZERO) || _receiver_yts.iter().any(|&a| a == Address::ZERO) {
            return Err(YTError::ZeroAddress);
        }
        self.check_not_expired()?;
        self.non_reentrant(|this| {
            this.update_data()?;
            let mut out = Vec::with_capacity(_receiver_pts.len());
            let floating_sy = this.get_floating_sy_amount()?;
            let index = this.pyIndexCurrent()?;
            let total_sy_to_mint = _amount_sy_to_mints.iter().fold(U256::ZERO, |acc, v| acc + *v);
            if total_sy_to_mint > floating_sy {
                return Err(YTError::YieldContractInsufficientSy(total_sy_to_mint, floating_sy));
            }
            for ((rp, ry), sy_amt) in _receiver_pts.into_iter().zip(_receiver_yts.into_iter()).zip(_amount_sy_to_mints.into_iter()) {
                let amount_py_out = this.calc_py_to_mint(sy_amt, index);
                // Match Solidity order: YT first, then PT
                this._mint(ry, amount_py_out)?;
                let mut pt = IPPrincipalToken::new(this.pt.read()).with_ctx(&mut *this);
                pt.mintByYT(rp, amount_py_out).expect("PT.mintByYT failed");
                out.push(amount_py_out);
                log::emit(Mint::new(msg_sender(), rp, ry, sy_amt, amount_py_out));
            }
            this.update_sy_reserve();
            Ok(out)
        })
    }

    pub fn redeemPY(&mut self, _receiver: Address) -> Result<U256, YTError> {
        if _receiver == Address::ZERO {
            return Err(YTError::ZeroAddress);
        }
        self.non_reentrant(|this| {
            this.update_data()?;
            let amount_py_to_redeem = this.get_amount_py_to_redeem()?;
            let mut pt = IPPrincipalToken::new(this.pt.read()).with_ctx(&mut *this);
            pt.burnByYT(this.this_address.read(), amount_py_to_redeem).expect("PT.burnByYT failed");
            if !this.isExpired() {
                this._burn(this.this_address.read(), amount_py_to_redeem)?;
            }
            let index = this.pyIndexCurrent()?;
            let (sy_to_user, sy_interest_post_expiry) = this.calc_sy_redeemable_from_py(amount_py_to_redeem, index)?;
            this.transfer_sy_out(_receiver, sy_to_user)?;
            if !sy_interest_post_expiry.is_zero() {
                let current = this.post_expiry_total_sy_interest_for_treasury.read();
                this.post_expiry_total_sy_interest_for_treasury.write(current + sy_interest_post_expiry);
            }
            this.update_sy_reserve();
            log::emit(Burn::new(msg_sender(), _receiver, amount_py_to_redeem, sy_to_user));
            Ok(sy_to_user)
        })
    }

    pub fn redeemPYMulti(&mut self, _receivers: Vec<Address>, _amount_py_to_redeems: Vec<U256>) -> Result<Vec<U256>, YTError> {
        if _receivers.is_empty() || _amount_py_to_redeems.is_empty() {
            return Err(YTError::ArrayEmpty);
        }
        if _receivers.len() != _amount_py_to_redeems.len() {
            return Err(YTError::ArrayLengthMismatch);
        }
        if _receivers.iter().any(|&a| a == Address::ZERO) {
            return Err(YTError::ZeroAddress);
        }
        self.non_reentrant(|this| {
            this.update_data()?;
            let mut out = Vec::with_capacity(_receivers.len());
            let total_py = _amount_py_to_redeems.iter().fold(U256::ZERO, |acc, v| acc + *v);
            {
                let mut pt = IPPrincipalToken::new(this.pt.read()).with_ctx(&mut *this);
                pt.burnByYT(this.this_address.read(), total_py).expect("PT.burnByYT failed");
            }
            let index = this.pyIndexCurrent()?;
            let mut total_sy_interest_post_expiry = U256::ZERO;
            for (recv, py_amt) in _receivers.into_iter().zip(_amount_py_to_redeems.into_iter()) {
                if !this.isExpired() {
                    this._burn(this.this_address.read(), py_amt)?;
                }
                let (sy_to_user, sy_interest_post_expiry) = this.calc_sy_redeemable_from_py(py_amt, index)?;
                this.transfer_sy_out(recv, sy_to_user)?;
                total_sy_interest_post_expiry += sy_interest_post_expiry;
                out.push(sy_to_user);
                log::emit(Burn::new(msg_sender(), recv, py_amt, sy_to_user));
            }
            if !total_sy_interest_post_expiry.is_zero() {
                let current = this.post_expiry_total_sy_interest_for_treasury.read();
                this.post_expiry_total_sy_interest_for_treasury.write(current + total_sy_interest_post_expiry);
            }
            this.update_sy_reserve();
            Ok(out)
        })
    }

    pub fn pyIndexCurrent(&mut self) -> Result<U256, YTError> {
        self.non_reentrant(|this| {
            this.py_index_current_internal()
        })
    }

    pub fn rewardIndexesCurrent(&mut self) -> Result<Vec<U256>, YTError> {
        self.non_reentrant(|this| {
            let mut sy = IStandardizedYield::new(this.sy.read()).with_ctx(&mut *this);
            let indexes = sy.rewardIndexesCurrent().expect("SY.rewardIndexesCurrent failed");
            Ok(indexes)
        })
    }

    pub fn redeemDueInterestAndRewards(
        &mut self,
        _user: Address,
        _redeem_interest: bool,
        _redeem_rewards: bool,
    ) -> Result<(U256, Vec<U256>), YTError> {
        if !_redeem_interest && !_redeem_rewards {
            return Err(YTError::YCNothingToRedeem);
        }
        self.non_reentrant(|this| {
            this.update_data()?;
            // Match Solidity: only distribute rewards upfront (line 178)
            this.distribute_rewards_for_two(_user, Address::ZERO)?;
            let mut sy = IStandardizedYield::new(this.sy.read()).with_ctx(&mut *this);
            let reward_tokens = sy.getRewardTokens().expect("SY.getRewardTokens failed");
            let reward_fee_rate = this.rewardFeeRate();
            let interest_fee_rate = this.interestFeeRate();
            let mut rewards_out = Vec::new();
            if _redeem_rewards {
                let mut redeemed_external = false;
                let expired = this.expiry.read() <= eth_riscv_runtime::block::timestamp();
                for token in reward_tokens.iter() {
                    let accrued = this.user_reward_accrued[*token][_user].read();
                    this.user_reward_accrued[*token][_user].write(U256::ZERO);
                    let fee = accrued * reward_fee_rate / ONE;
                    let payout = accrued.saturating_sub(fee);
                    if (!payout.is_zero() || !fee.is_zero()) && !redeemed_external {
                        let token_reader = IERC20Minimal::new(*token).with_ctx(&*this);
                        let bal = token_reader.balanceOf(this.this_address.read()).unwrap_or(U256::ZERO);
                        if bal < payout + fee {
                            this.redeem_external_reward()?;
                            redeemed_external = true;
                            // Re-check balance once after redeem
                            let bal2 = token_reader.balanceOf(this.this_address.read()).unwrap_or(U256::ZERO);
                            if bal2 < payout + fee {
                                return Err(YTError::YieldContractInsufficientSy(payout + fee, bal2));
                            }
                        }
                    }
                    if expired {
                        let owed = this.post_expiry_user_reward_owed[*token].read();
                        let new_owed = if owed > accrued { owed - accrued } else { U256::ZERO };
                        this.post_expiry_user_reward_owed[*token].write(new_owed);
                    }
                    // Match Solidity order: treasury first, then user
                    if !fee.is_zero() {
                        this.transfer_reward_out(*token, this.treasury(), fee)?;
                        log::emit(CollectRewardFee::new(*token, fee));
                    }
                    if !payout.is_zero() {
                        this.transfer_reward_out(*token, _user, payout)?;
                    }
                    rewards_out.push(payout);
                }
                log::emit(RedeemRewards::new(_user, rewards_out.clone()));
            } else {
                rewards_out.resize(reward_tokens.len(), U256::ZERO);
            }
            let mut interest_out = U256::ZERO;
            if _redeem_interest {
                // Match Solidity: distribute interest AFTER rewards are handled (line 189)
                this.distribute_interest_for_two(_user, Address::ZERO)?;
                // Match Solidity: zero immediately after reading (line 51)
                let accrued = this.user_interest_accrued[_user].read();
                this.user_interest_accrued[_user].write(U256::ZERO);
                let fee = accrued * interest_fee_rate / ONE;
                interest_out = accrued.saturating_sub(fee);
                // Match Solidity order: treasury first, then user
                if !fee.is_zero() {
                    this.transfer_sy_out(this.treasury(), fee)?;
                    log::emit(CollectInterestFee::new(fee));
                }
                if !interest_out.is_zero() {
                    this.transfer_sy_out(_user, interest_out)?;
                }
                log::emit(RedeemInterest::new(_user, interest_out));
            }
            this.update_sy_reserve();
            Ok((interest_out, rewards_out))
        })
    }

    pub fn redeemInterestAndRewardsPostExpiryForTreasury(&mut self) -> Result<(U256, Vec<U256>), YTError> {
        self.check_expired()?;
        self.non_reentrant(|this| {
            // Match Solidity's updateData modifier: set post-expiry data if needed
            this.update_data()?;
            if this.post_expiry_first_py_index.read().is_zero() {
                return Err(YTError::YCPostExpiryDataNotSet);
            }
            this.redeem_external_reward()?;
            let mut sy = IStandardizedYield::new(this.sy.read()).with_ctx(&mut *this);
            let reward_tokens = sy.getRewardTokens().expect("SY.getRewardTokens failed");
            let treasury = this.treasury();
            // Match Solidity order: calculate all rewards first
            let mut rewards_out = Vec::with_capacity(reward_tokens.len());
            for token in reward_tokens.iter() {
                let token_reader = IERC20Minimal::new(*token).with_ctx(&*this);
                let bal = token_reader.balanceOf(this.this_address.read()).expect("reward balanceOf failed");
                let owed = this.post_expiry_user_reward_owed[*token].read();
                let mut payout = bal.saturating_sub(owed);
                if payout.is_zero() && bal < owed {
                    this.redeem_external_reward()?;
                    let bal2 = token_reader.balanceOf(this.this_address.read()).expect("reward balanceOf failed");
                    payout = bal2.saturating_sub(owed);
                }
                rewards_out.push(payout);
            }
            // Match Solidity order: emit all events, then transfer all
            for (token, payout) in reward_tokens.iter().zip(rewards_out.iter()) {
                if !payout.is_zero() {
                    log::emit(CollectRewardFee::new(*token, *payout));
                }
            }
            // Transfer all rewards
            for (token, payout) in reward_tokens.iter().zip(rewards_out.iter()) {
                if !payout.is_zero() {
                    this.transfer_reward_out(*token, treasury, *payout)?;
                }
            }
            let interest_out = this.post_expiry_total_sy_interest_for_treasury.read();
            if !interest_out.is_zero() {
                this.post_expiry_total_sy_interest_for_treasury.write(U256::ZERO);
                this.transfer_sy_out(treasury, interest_out)?;
                log::emit(CollectInterestFee::new(interest_out));
            }
            // Match Solidity's updateData modifier: update SY reserve at the end
            this.update_sy_reserve();
            Ok((interest_out, rewards_out))
        })
    }

    pub fn setPostExpiryData(&mut self) -> Result<(), YTError> {
        // Match Solidity behavior: silently does nothing if not expired
        self.non_reentrant(|this| {
            if this.isExpired() {
                this.set_post_expiry_once()?;
            }
            Ok(())
        })
    }

}

// NOTE: no `self_address_from_create()` helper.
// Use `eth_riscv_runtime::tx::address()` (EVM `ADDRESS`, 0x30) instead.

// -----------------------------------------------------------------------------
// Internal helpers (non-ABI)
// -----------------------------------------------------------------------------
impl PendleYieldToken {
    fn before_token_transfer(&mut self, from: Address, to: Address, amount: U256) -> Result<(), YTError> {
        let _ = amount; // unused but matches signature intent
        let now = eth_riscv_runtime::block::timestamp();
        if self.expiry.read() <= now {
            self.set_post_expiry_once()?;
        }
        self.distribute_rewards_for_two(from, to)?;
        self.distribute_interest_for_two(from, to)?;
        Ok(())
    }

    fn update_data(&mut self) -> Result<(), YTError> {
        let now = eth_riscv_runtime::block::timestamp();
        if self.expiry.read() <= now {
            self.set_post_expiry_once()?;
        }
        Ok(())
    }

    fn update_sy_reserve(&mut self) {
        let sy_token = IERC20Minimal::new(self.sy.read()).with_ctx(&*self);
        let bal = sy_token.balanceOf(self.this_address.read()).expect("SY.balanceOf failed");
        self.sy_reserve.write(bal);
    }

    fn set_post_expiry_once(&mut self) -> Result<(), YTError> {
        if !self.post_expiry_first_py_index.read().is_zero() {
            return Ok(());
        }
        // Redeem external rewards so that future rewards belong to treasury
        self.redeem_external_reward()?;
        let mut sy = IStandardizedYield::new(self.sy.read()).with_ctx(&mut *self);
        let current_reward_indexes = sy.rewardIndexesCurrent().expect("SY.rewardIndexesCurrent failed");
        let reward_tokens = sy.getRewardTokens().expect("SY.getRewardTokens failed");
        let first_index = self.py_index_current_internal()?;
        self.post_expiry_first_py_index.write(first_index);
        for (i, token) in reward_tokens.into_iter().enumerate() {
            let idx = current_reward_indexes.get(i).cloned().unwrap_or(U256::ZERO);
            self.post_expiry_first_reward_index[token].write(idx);
            // Snapshot current reward token balance as owed to users (treasury claims deltas later)
            let token_reader = IERC20Minimal::new(token).with_ctx(&*self);
            let bal = token_reader
                .balanceOf(self.this_address.read())
                .expect("reward token balanceOf failed");
            self.post_expiry_user_reward_owed[token].write(bal);
        }
        Ok(())
    }

    fn get_floating_sy_amount(&self) -> Result<U256, YTError> {
        let sy_token = IERC20Minimal::new(self.sy.read()).with_ctx(&*self);
        let bal = sy_token.balanceOf(self.this_address.read()).expect("SY.balanceOf failed");
        let reserve = self.sy_reserve.read();
        if bal < reserve {
            return Err(YTError::YieldContractInsufficientSy(reserve - bal, reserve));
        }
        let floating = bal - reserve;
        if floating.is_zero() {
            return Err(YTError::YCNoFloatingSy);
        }
        Ok(floating)
    }

    fn calc_py_to_mint(&self, amount_sy: U256, index: U256) -> U256 {
        // SYUtils.syToAsset: (sy * index) / 1e18
        if index.is_zero() {
            return U256::ZERO;
        }
        amount_sy * index / ONE
    }

    fn get_amount_py_to_redeem(&self) -> Result<U256, YTError> {
        let pt_token = IERC20Minimal::new(self.pt.read()).with_ctx(&*self);
        let pt_bal = pt_token.balanceOf(self.this_address.read()).expect("PT.balanceOf failed");
        let yt_bal = self.balance_of[self.this_address.read()].read();
        let now = eth_riscv_runtime::block::timestamp();
        let expired = self.expiry.read() <= now;
        let amt = if expired { pt_bal } else { core::cmp::min(pt_bal, yt_bal) };
        if amt.is_zero() {
            return Err(YTError::YCNothingToRedeem);
        }
        Ok(amt)
    }

    fn calc_sy_redeemable_from_py(&self, amount_py: U256, index_current: U256) -> Result<(U256, U256), YTError> {
        // SYUtils.assetToSy: (asset * 1e18) / index
        if index_current.is_zero() {
            return Err(YTError::YCExpired); // reuse to signal invalid index
        }
        let sy_to_user = amount_py * ONE / index_current;
        let now = eth_riscv_runtime::block::timestamp();
        if self.expiry.read() > now {
            return Ok((sy_to_user, U256::ZERO));
        }
        let first_idx = self.post_expiry_first_py_index.read();
        if first_idx.is_zero() {
            return Err(YTError::YCPostExpiryDataNotSet);
        }
        let total_sy_redeemable = amount_py * ONE / first_idx;
        let sy_interest_post_expiry = total_sy_redeemable.saturating_sub(sy_to_user);
        Ok((sy_to_user, sy_interest_post_expiry))
    }

    fn transfer_sy_out(&mut self, to: Address, amount: U256) -> Result<(), YTError> {
        if amount.is_zero() {
            return Ok(());
        }
        let mut sy_token = IERC20Like::new(self.sy.read()).with_ctx(&mut *self);
        let ok = sy_token.transfer(to, amount);
        match ok {
            Some(true) => Ok(()),
            _ => Err(YTError::YieldContractInsufficientSy(amount, amount)),
        }
    }

    fn transfer_reward_out(&mut self, token: Address, to: Address, amount: U256) -> Result<(), YTError> {
        if amount.is_zero() {
            return Ok(());
        }
        let mut erc20 = IERC20Like::new(token).with_ctx(&mut *self);
        let ok = erc20.transfer(to, amount);
        match ok {
            Some(true) => Ok(()),
            _ => Err(YTError::YieldContractInsufficientSy(amount, amount)),
        }
    }

    fn redeem_external_reward(&mut self) -> Result<(), YTError> {
        let mut sy = IStandardizedYield::new(self.sy.read()).with_ctx(&mut *self);
        // Claim to the contract address for parity with Solidity
        let contract_addr = self.this_address.read();
        sy.claimRewards(contract_addr);
        Ok(())
    }
    fn reward_shares_user(&self, user: Address) -> U256 {
        let index = self.user_interest_index[user].read();
        if index.is_zero() {
            return U256::ZERO;
        }
        let bal = self.balance_of[user].read();
        let sy_equiv = bal * ONE / index;
        sy_equiv + self.user_interest_accrued[user].read()
    }

    fn update_reward_index(&mut self) -> Result<(Vec<Address>, Vec<U256>), YTError> {
        let mut sy = IStandardizedYield::new(self.sy.read()).with_ctx(&mut *self);
        let tokens = sy.getRewardTokens().expect("SY.getRewardTokens failed");
        let expired = self.expiry.read() <= eth_riscv_runtime::block::timestamp();
        let indexes = if expired {
            tokens.iter().map(|t| self.post_expiry_first_reward_index[*t].read()).collect()
        } else {
            sy.rewardIndexesCurrent().expect("SY.rewardIndexesCurrent failed")
        };
        Ok((tokens, indexes))
    }

    fn distribute_rewards_for_two(&mut self, user1: Address, user2: Address) -> Result<(), YTError> {
        let (tokens, indexes) = self.update_reward_index()?;
        if tokens.is_empty() {
            return Ok(());
        }
        if user1 != Address::ZERO && user1 != self.this_address.read() {
            self.distribute_rewards_private(user1, &tokens, &indexes)?;
        }
        if user2 != Address::ZERO && user2 != self.this_address.read() && user2 != user1 {
            self.distribute_rewards_private(user2, &tokens, &indexes)?;
        }
        Ok(())
    }

    fn distribute_rewards_private(
        &mut self,
        user: Address,
        tokens: &[Address],
        indexes: &[U256],
    ) -> Result<(), YTError> {
        let shares = self.reward_shares_user(user);
        if shares.is_zero() {
            return Ok(());
        }
        for (i, token) in tokens.iter().enumerate() {
            let idx = indexes.get(i).cloned().unwrap_or(U256::ZERO);
            if idx.is_zero() {
                continue;
            }
            let user_idx = self.user_reward_index[*token][user].read();
            let effective_user_idx = if user_idx.is_zero() { U256::from(1u64) } else { user_idx };
            if idx == effective_user_idx {
                continue;
            }
            let delta = idx - effective_user_idx;
            let delta_reward = shares * delta / ONE;
            let accrued = self.user_reward_accrued[*token][user].read() + delta_reward;
            self.user_reward_accrued[*token][user].write(accrued);
            self.user_reward_index[*token][user].write(idx);
        }
        Ok(())
    }

    fn distribute_interest_for_two(&mut self, user1: Address, user2: Address) -> Result<(), YTError> {
        let idx = self.get_interest_index()?; // expired uses post-expiry first index
        if user1 != Address::ZERO && user1 != self.this_address.read() {
            self.distribute_interest_private(user1, idx);
        }
        if user2 != Address::ZERO && user2 != self.this_address.read() && user2 != user1 {
            self.distribute_interest_private(user2, idx);
        }
        Ok(())
    }

    fn distribute_interest_private(&mut self, user: Address, current_index: U256) {
        let prev = self.user_interest_index[user].read();
        if prev == current_index {
            return;
        }
        if prev.is_zero() {
            self.user_interest_index[user].write(current_index);
            return;
        }
        if current_index.is_zero() || prev.is_zero() {
            return;
        }
        let principal = self.balance_of[user].read();
        if principal.is_zero() {
            self.user_interest_index[user].write(current_index);
            return;
        }
        let num = principal * (current_index - prev) * ONE;
        let denom = prev * current_index;
        if denom.is_zero() {
            return;
        }
        let interest = num / denom;
        let accrued = self.user_interest_accrued[user].read() + interest;
        self.user_interest_accrued[user].write(accrued);
        self.user_interest_index[user].write(current_index);
    }

    fn py_index_current_internal(&mut self) -> Result<U256, YTError> {
        let block_no = eth_riscv_runtime::block::number();
        if self.do_cache_index_same_block.read() && self.py_index_last_updated_block.read() == U256::from(block_no) {
            return Ok(self.py_index_stored.read());
        }
        let mut sy = IStandardizedYield::new(self.sy.read()).with_ctx(&mut *self);
        let idx = sy.exchangeRate().expect("SY.exchangeRate failed");
        let prev = self.py_index_stored.read();
        let new_idx = if idx > prev { idx } else { prev };
        self.py_index_stored.write(new_idx);
        self.py_index_last_updated_block.write(U256::from(block_no));
        log::emit(NewInterestIndex::new(new_idx));
        Ok(new_idx)
    }

    fn get_interest_index(&mut self) -> Result<U256, YTError> {
        let now = eth_riscv_runtime::block::timestamp();
        if self.expiry.read() <= now {
            let idx = self.post_expiry_first_py_index.read();
            if idx.is_zero() {
                return Err(YTError::YCPostExpiryDataNotSet);
            }
            return Ok(idx);
        }
        self.py_index_current_internal()
    }

    fn non_reentrant<F, R>(&mut self, f: F) -> Result<R, YTError>
    where
        F: FnOnce(&mut Self) -> Result<R, YTError>,
    {
        if self.reentrancy_status.read() == ENTERED {
            return Err(YTError::ReentrantCall);
        }
        self.reentrancy_status.write(ENTERED);
        let result = f(self);
        self.reentrancy_status.write(NOT_ENTERED);
        result
    }

    fn _transfer(&mut self, from: Address, to: Address, amount: U256) -> Result<(), YTError> {
        if from == Address::ZERO || to == Address::ZERO {
            return Err(YTError::ZeroAddress);
        }
        if from == to {
            return Err(YTError::SelfTransfer);
        }
        self.before_token_transfer(from, to, amount)?;
        let from_balance = self.balance_of[from].read();
        if from_balance < amount {
            return Err(YTError::InsufficientBalance(from_balance));
        }
        self.balance_of[from].write(from_balance - amount);
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);
        // Hook placeholder (rewards/interest) to be added later
        log::emit(Transfer::new(from, to, amount));
        Ok(())
    }

    fn _approve(&mut self, owner: Address, spender: Address, amount: U256) -> Result<(), YTError> {
        if owner == Address::ZERO || spender == Address::ZERO {
            return Err(YTError::ZeroAddress);
        }
        self.allowance_of[owner][spender].write(amount);
        log::emit(Approval::new(owner, spender, amount));
        Ok(())
    }

    fn _mint(&mut self, account: Address, amount: U256) -> Result<(), YTError> {
        if account == Address::ZERO {
            return Err(YTError::ZeroAddress);
        }
        self.before_token_transfer(Address::ZERO, account, amount)?;
        let supply = self.total_supply.read();
        self.total_supply.write(supply + amount);
        let bal = self.balance_of[account].read();
        self.balance_of[account].write(bal + amount);
        log::emit(Transfer::new(Address::ZERO, account, amount));
        Ok(())
    }

    fn _burn(&mut self, account: Address, amount: U256) -> Result<(), YTError> {
        if account == Address::ZERO {
            return Err(YTError::ZeroAddress);
        }
        self.before_token_transfer(account, Address::ZERO, amount)?;
        let bal = self.balance_of[account].read();
        if bal < amount {
            return Err(YTError::InsufficientBalance(bal));
        }
        self.balance_of[account].write(bal - amount);
        let supply = self.total_supply.read();
        self.total_supply.write(supply - amount);
        log::emit(Transfer::new(account, Address::ZERO, amount));
        Ok(())
    }

    fn _spend_allowance(&mut self, owner: Address, spender: Address, amount: U256) -> Result<(), YTError> {
        let current = self.allowance_of[owner][spender].read();
        if current != U256::MAX {
            if current < amount {
                return Err(YTError::InsufficientAllowance(current));
            }
            self.allowance_of[owner][spender].write(current - amount);
        }
        Ok(())
    }
}
