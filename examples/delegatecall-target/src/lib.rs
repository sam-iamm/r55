//! Target contract for testing external CALLs from delegated code.
//!
//! This contract is called externally by the delegatecall-impl contract
//! to verify that delegated code can make external calls to other contracts.
//!
//! Functions:
//! - `echo(bytes) -> bytes`: Echoes input data back
//! - `getCaller() -> address`: Returns msg.sender (to verify caller context)
//! - `setFlag(uint256)`: Writes to storage
//! - `getFlag() -> uint256`: Reads from storage

#![no_std]
#![no_main]

extern crate alloc;

use alloy_core::primitives::{Address, Bytes, U256};
use contract_derive::{contract, storage};
use eth_riscv_runtime::msg_sender;
use eth_riscv_runtime::types::*;

#[storage]
pub struct DelegatecallTarget {
    flag: Slot<U256>,
}

#[contract]
impl DelegatecallTarget {
    pub fn new() -> Self {
        DelegatecallTarget::default()
    }

    /// Echoes the input bytes back to the caller.
    /// Used to test returndata propagation through external calls.
    pub fn echo(&self, data: Bytes) -> Bytes {
        data
    }

    /// Returns the msg.sender of this call.
    /// When called from delegated code via external CALL:
    /// - msg.sender should be the implementation contract address (not proxy, not original user)
    pub fn getCaller(&self) -> Address {
        msg_sender()
    }

    /// Sets a flag value in storage.
    /// Used to verify storage operations work correctly in the target contract.
    pub fn setFlag(&mut self, value: U256) {
        self.flag.write(value);
    }

    /// Gets the flag value from storage.
    pub fn getFlag(&self) -> U256 {
        self.flag.read()
    }
}
