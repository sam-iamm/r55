//! ERC-1967 implementation-slot proxy (OpenZeppelin-style *base* proxy).
//!
//! This contract mirrors the *core* mechanics from OpenZeppelin's `Proxy.sol` +
//! `ERC1967Proxy.sol`:
//! - Stores the implementation at the **EIP-1967 implementation slot**
//! - Routes unknown selectors (and empty calldata) into a **single fallback path**
//! - Uses **DELEGATECALL** to execute implementation code in the proxy's storage context
//! - Bubbles **raw returndata** and **raw revert data** unchanged
//!
//! ## What this proxy is (and is not)
//! - **Is**: an ERC-1967 *implementation-slot* proxy with an optional "init data" delegatecall
//!   performed during construction (constructor parity with OZ `ERC1967Proxy`).
//! - **Is not**: an upgradeable admin system. It intentionally does **not** implement:
//!   - transparent admin gating (`TransparentUpgradeableProxy`)
//!   - beacon resolution (`BeaconProxy` / `UpgradeableBeacon`)
//!   - upgrade functions (`ERC1967Upgrade` / `ERC1967Utils` helpers like `upgradeToAndCall`)
//!
//! Those are additional policy layers and are best modeled as **separate proxy contracts**
//! in R55, just like OpenZeppelin ships them as separate contracts.
//!
//! ## Fallback / receive semantics (Solidity parity)
//! In Solidity:
//! - If calldata is empty, `receive()` runs if present, else `fallback()` runs if present.
//! - If calldata has a selector but no function matches, `fallback()` runs.
//! - `fallback`/`receive` are not selected by selector matching; they are selected when
//!   dispatch fails (or calldata is empty).
//!
//! In R55:
//! - `contract-derive` invokes the `#[fallback]` method when selector dispatch fails
//!   **or** when calldata is empty/short (<4 bytes). This single method therefore covers
//!   the Solidity `receive()` + `fallback()` routing behavior.
//! - `#[payable]` on the fallback allows value, matching OZ `Proxy.sol`'s payable fallback.
//!
//! ## Constructor semantics
//! `new(implementation, init_data)`:
//! - Writes `implementation` into the EIP-1967 slot
//! - If `init_data` is non-empty, performs a DELEGATECALL to `implementation` with `init_data`,
//!   so initialization runs in the **proxy** context (storage, address(this), etc.).

#![no_std]
#![no_main]

extern crate alloc;

use alloy_core::primitives::{Address, Bytes, U256};
use contract_derive::{contract, fallback, payable};
use eth_riscv_runtime::{delegatecall_contract, revert_with_error, sload, sstore};

#[derive(Default)]
pub struct ERC1967Proxy {
}

/// EIP-1967 implementation slot:
/// `bytes32(uint256(keccak256("eip1967.proxy.implementation")) - 1)`
/// `0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc`
fn implementation_slot() -> U256 {
    U256::from_be_bytes([
        0x36, 0x08, 0x94, 0xa1, 0x3b, 0xa1, 0xa3, 0x21, 0x06, 0x67, 0xc8, 0x28, 0x49, 0x2d,
        0xb9, 0x8d, 0xca, 0x3e, 0x20, 0x76, 0xcc, 0x37, 0x35, 0xa9, 0x20, 0xa3, 0xca, 0x50,
        0x5d, 0x38, 0x2b, 0xbc,
    ])
}

fn load_implementation() -> Address {
    // EIP-1967 stores the implementation as a 20-byte address in the low 20 bytes of the word.
    let word = sload(implementation_slot());
    let bytes: [u8; 32] = word.to_be_bytes::<32>();
    Address::from_slice(&bytes[12..32])
}

fn store_implementation(addr: Address) {
    // EVM convention: store address in the low 20 bytes of the 32-byte word.
    let mut b = [0u8; 32];
    b[12..32].copy_from_slice(addr.as_slice());
    sstore(implementation_slot(), U256::from_be_bytes(b));
}

#[contract]
impl ERC1967Proxy {
    /// Constructor.
    ///
    /// Mirrors OZ `ERC1967Proxy(address implementation, bytes memory data)` pattern:
    /// - writes the implementation to the EIP-1967 slot
    /// - if `init_data` is non-empty, performs a delegatecall to run initialization in proxy context
    pub fn new(implementation: Address, init_data: Bytes) -> Self {
        store_implementation(implementation);

        if !init_data.is_empty() {
            match delegatecall_contract(implementation, init_data.as_ref(), None) {
                Ok(_out) => {}
                Err(revert_data) => revert_with_error(&revert_data),
            }
        }

        ERC1967Proxy {}
    }

    /// Read current implementation (debug/utility).
    ///
    /// Note: OpenZeppelin's base `Proxy.sol` does not expose this publicly; it's provided here
    /// to simplify testing and inspection in the R55 environment.
    pub fn implementation(&self) -> Address {
        load_implementation()
    }

    /// Fallback handler:
    /// - invoked when selector is unknown OR calldata is empty/too-short (receive()).
    /// - delegates to implementation using DELEGATECALL.
    /// - returns raw returndata bytes on success, reverts with raw revert bytes on failure.
    #[fallback]
    #[payable]
    pub fn fallback(&mut self, calldata: Bytes) -> Bytes {
        let implementation = load_implementation();
        match delegatecall_contract(implementation, calldata.as_ref(), None) {
            Ok(out) => out,
            Err(revert_data) => revert_with_error(&revert_data),
        }
    }
}


