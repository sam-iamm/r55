//! # PendleYieldContractFactoryHydra (Hydra N=2) — R55 Implementation
//!
//! Minimal R55 implementation of the Hydra-focused Pendle yield-contract factory.
//! Deploys a (PT, YT) pair via **CREATE**, initializes PT with YT, and registers the pair.
//!
//! ## Solidity References
//! - Factory (target): `pendle-core-v2-public/contracts/core/YieldContracts/PendleYieldContractFactoryHydra.sol`
//! - Original Pendle factory (what we diverged from): `pendle-core-v2-public/contracts/core/YieldContracts/PendleYieldContractFactoryUpg.sol`
//! - PT: `pendle-core-v2-public/contracts/core/YieldContracts/PendlePrincipalToken.sol`
//! - YT: `pendle-core-v2-public/contracts/core/YieldContracts/PendleYieldToken.sol`
//!
//! ## Hydra / Parity Notes
//! - ABI parity required for `createYieldContractWithCreate(address,uint32,bool) -> (address,address)` and getters.
//! - Events are emitted for observability; Hydra equivalence does not require log parity in this setup.
//! - Constructor does not need to match Solidity (Hydra N=2 initcode can use different constructor args).
//!
//! ## ABI integer widths (Solidity vs R55)
//! Solidity exposes smaller ints (`uint96 expiryDivisor()`, `uint128 interestFeeRate()`, `uint128 rewardFeeRate()`).
//! This R55 implementation stores/returns those values as `U256`.
//! This is ABI-safe because return values are encoded as **one 32-byte word** either way; the bytes match as long as
//! the value fits the Solidity bit width (\(< 2^96\) / \(< 2^128\)).
//!
//! Additional note: `SY.assetInfo()` in Solidity returns `(AssetType, address, uint8)`.
//! The local interface decodes it as `(U256, Address, U256)` and only uses `asset_decimals` (must fit \(< 2^8\));
//! `asset_type` is treated as an opaque numeric enum and is unused.
//!
//! ## What was removed vs `PendleYieldContractFactoryUpg.sol` (by design)
//! - **CREATE2 determinism**: no `Create2.deploy`, no chainid salt.
//! - **Split-code factory**: no `BaseSplitCodeFactory` path for YT creation code.
//! - **Metadata heavy logic**: no `ExpiryUtilsLib` RFC2822 formatting and no `StringLib.stripPrefix`.
//!   - Metadata here matches the minimized Hydra Solidity factory:
//!     - PT name: `"PT <SY.name()>"`, PT symbol: `"PT-<SY.symbol()>"`
//!     - YT name: `"YT <SY.name()>"`, YT symbol: `"YT-<SY.symbol()>"`
//!   - Breakdown:
//!     - Original Pendle factory strips `"SY "` / `"SY-"` prefixes from `SY.name()` / `SY.symbol()` to avoid redundancy.
//!     - Original Pendle factory appends a formatted expiry string (RFC2822-style `DDMMMYYYY`) derived from `expiry`.
//!     - Hydra factory omits both to stay small (<24KB) and relies on `(SY, expiry)` mappings for uniqueness.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;

use alloy_core::primitives::{Address, U256};
use contract_derive::{contract, interface, storage, Event};
use eth_riscv_runtime::{block, revert};
use eth_riscv_runtime::types::*;

mod deployable;
use deployable::{PendlePrincipalToken, PendleYieldToken};

const VERSION: U256 = U256::from_limbs([6, 0, 0, 0]);

const PT_PREFIX: &str = "PT";
const YT_PREFIX: &str = "YT";

// -----------------------------------------------------------------------------
// Events (parity with Solidity)
// -----------------------------------------------------------------------------

/// Matches Solidity:
/// `event CreateYieldContract(address indexed SY, uint256 indexed expiry, address PT, address YT);`
#[derive(Event)]
struct CreateYieldContract {
    #[indexed]
    sy: Address,
    #[indexed]
    expiry: U256,
    pt: Address,
    yt: Address,
}

// -----------------------------------------------------------------------------
// External interfaces (minimal)
// -----------------------------------------------------------------------------

#[interface("camelCase")]
trait IStandardizedYield {
    fn assetInfo(&self) -> (U256, Address, U256);
    fn name(&self) -> String;
    fn symbol(&self) -> String;
}

#[interface("camelCase")]
trait IPPrincipalToken {
    fn initialize(&mut self, yt: Address);
}

// -----------------------------------------------------------------------------
// Storage
// -----------------------------------------------------------------------------

#[storage]
pub struct PendleYieldContractFactory {
    // config
    expiry_divisor: Slot<U256>,
    interest_fee_rate: Slot<U256>,
    reward_fee_rate: Slot<U256>,
    treasury: Slot<Address>,

    // SY => expiry => PT/YT
    get_pt: Mapping<Address, Mapping<U256, Slot<Address>>>,
    get_yt: Mapping<Address, Mapping<U256, Slot<Address>>>,

    is_pt: Mapping<Address, Slot<bool>>,
    is_yt: Mapping<Address, Slot<bool>>,
}

// -----------------------------------------------------------------------------
// Contract
// -----------------------------------------------------------------------------

#[contract]
impl PendleYieldContractFactory {
    /// R55/HydraStorage constructor.
    /// These values must be provided when installing the N=2 program.
    pub fn new(
        expiry_divisor: U256,
        interest_fee_rate: U256,
        reward_fee_rate: U256,
        treasury: Address,
    ) -> Self {
        if expiry_divisor.is_zero() || treasury == Address::ZERO {
            revert();
        }

        let mut s = PendleYieldContractFactory::default();
        s.expiry_divisor.write(expiry_divisor);
        s.interest_fee_rate.write(interest_fee_rate);
        s.reward_fee_rate.write(reward_fee_rate);
        s.treasury.write(treasury);
        s
    }

    // ---- views (ABI parity for dependencies) ----

    pub fn VERSION(&self) -> U256 {
        VERSION
    }

    pub fn expiryDivisor(&self) -> U256 {
        self.expiry_divisor.read()
    }

    pub fn interestFeeRate(&self) -> U256 {
        self.interest_fee_rate.read()
    }

    pub fn rewardFeeRate(&self) -> U256 {
        self.reward_fee_rate.read()
    }

    pub fn treasury(&self) -> Address {
        self.treasury.read()
    }

    pub fn getPT(&self, sy: Address, expiry: U256) -> Address {
        self.get_pt[sy][expiry].read()
    }

    pub fn getYT(&self, sy: Address, expiry: U256) -> Address {
        self.get_yt[sy][expiry].read()
    }

    pub fn isPT(&self, a: Address) -> bool {
        self.is_pt[a].read()
    }

    pub fn isYT(&self, a: Address) -> bool {
        self.is_yt[a].read()
    }

    // ---- stateful ----

    pub fn createYieldContractWithCreate(
        &mut self,
        sy: Address,
        expiry_u32: u32,
        do_cache_index_same_block: bool,
    ) -> (Address, Address) {
        let expiry = U256::from(expiry_u32);

        // Matches Hydra Solidity factory: expiry must be strictly in the future.
        if expiry <= block::timestamp() {
            revert();
        }

        let divisor = self.expiry_divisor.read();
        if divisor.is_zero() || (expiry % divisor) != U256::ZERO {
            revert();
        }

        if self.get_pt[sy][expiry].read() != Address::ZERO {
            revert();
        }

        // Read SY metadata
        let sy_ro = IStandardizedYield::new(sy).with_ctx(&*self);
        let (_asset_type, _asset_address, asset_decimals) = match sy_ro.assetInfo() {
            Some(v) => v,
            None => revert(),
        };

        let sy_name = match sy_ro.name() {
            Some(v) => v,
            None => revert(),
        };
        let sy_symbol = match sy_ro.symbol() {
            Some(v) => v,
            None => revert(),
        };

        // Match `PendleYieldContractFactoryHydra.sol` minimal formatting:
        // PT name: "PT <SY.name()>", PT symbol: "PT-<SY.symbol()>"
        // YT name: "YT <SY.name()>", YT symbol: "YT-<SY.symbol()>"
        let pt_name = alloc::format!("{} {}", PT_PREFIX, sy_name);
        let pt_symbol = alloc::format!("{}-{}", PT_PREFIX, sy_symbol);

        let pt_child = PendlePrincipalToken::deploy((sy, pt_name, pt_symbol, asset_decimals, expiry))
            .with_ctx(&mut *self);
        let pt = pt_child.address();

        let yt_name = alloc::format!("{} {}", YT_PREFIX, sy_name);
        let yt_symbol = alloc::format!("{}-{}", YT_PREFIX, sy_symbol);

        let yt_child = PendleYieldToken::deploy((
            sy,
            pt,
            yt_name,
            yt_symbol,
            asset_decimals,
            expiry,
            do_cache_index_same_block,
        ))
        .with_ctx(&mut *self);
        let yt = yt_child.address();

        // PT.initialize(YT)
        let mut pt_iface = IPPrincipalToken::new(pt).with_ctx(&mut *self);
        if pt_iface.initialize(yt).is_none() {
            revert();
        }

        // register
        self.get_pt[sy][expiry].write(pt);
        self.get_yt[sy][expiry].write(yt);
        self.is_pt[pt].write(true);
        self.is_yt[yt].write(true);

        eth_riscv_runtime::log::emit(CreateYieldContract::new(sy, expiry, pt, yt));

        (pt, yt)
    }
}


