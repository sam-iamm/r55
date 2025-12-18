//! Minimal implementation contract for testing DELEGATECALL through a proxy.
//!
//! Intended checks:
//! - Storage writes: `setX` / `getX`
//! - msg.sender: `who`
//! - Revert bubbling: `revertWith(bytes)`
//! - Proxy fallback/receive routing: `#[fallback]` (echo/marker)
//! - Dynamic returndata passthrough: `returnBytes(n)`

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use alloy_core::primitives::{Address, Bytes, U256};
use contract_derive::{contract, fallback, payable, storage};
use eth_riscv_runtime::{call_contract, delegatecall_contract, msg_sender, revert_with_error};
use eth_riscv_runtime::types::*;

#[storage]
pub struct DelegatecallImpl {
    x: Slot<U256>,
    last_sender: Slot<Address>,
    last_value: Slot<U256>,
}

#[contract]
impl DelegatecallImpl {
    pub fn new() -> Self {
        DelegatecallImpl::default()
    }

    pub fn setX(&mut self, x: U256) {
        self.x.write(x);
        self.last_sender.write(msg_sender());
        // Non-payable method: in Solidity this would be auto-guarded, so msg.value is always 0.
        self.last_value.write(U256::ZERO);
    }

    pub fn getX(&self) -> U256 {
        self.x.read()
    }

    pub fn lastSender(&self) -> Address {
        self.last_sender.read()
    }

    pub fn lastValue(&self) -> U256 {
        self.last_value.read()
    }

    pub fn who(&self) -> Address {
        msg_sender()
    }

    #[payable]
    pub fn payablePing(&self) -> U256 {
        // returns msg.value (allows checking apparent value under DELEGATECALL)
        eth_riscv_runtime::msg_value()
    }

    pub fn revertWith(&self, data: Bytes) {
        // bubble arbitrary revert payload so the proxy can be tested for revert propagation.
        revert_with_error(data.as_ref());
    }

    pub fn revertWith4(&self, a: u8, b: u8, c: u8, d: u8) {
        let buf: [u8; 4] = [a, b, c, d];
        revert_with_error(&buf);
    }

    pub fn revertWithLong(&self, n: U256) {
        // produce a deterministic long revert payload to test returndata sizing/copying
        let mut v = Vec::new();
        let count: usize = core::cmp::min(512usize, n.to::<usize>());
        v.resize(count, 0xAB);
        revert_with_error(&v);
    }

    /// Return a dynamic bytes payload of length `n` filled with 0xCD.
    /// Used to test success-path dynamic returndata passthrough through proxy fallback+delegatecall.
    pub fn returnBytes(&self, n: U256) -> Bytes {
        let count: usize = core::cmp::min(512usize, n.to::<usize>());
        let mut v = Vec::new();
        v.resize(count, 0xCD);
        Bytes::from(v)
    }

    /// Make an external CALL to another contract.
    /// Used to test that delegated code can make external calls.
    /// Returns the returndata from the call.
    pub fn callOther(&self, target: Address, calldata: Bytes) -> Bytes {
        // Make external CALL with 0 value (no ETH transfer)
        // This tests that delegated code can make external calls to other contracts
        match call_contract(target, 0, calldata.as_ref(), None) {
            Ok(ret) => ret,
            Err(revert_data) => revert_with_error(&revert_data),
        }
    }

    /// Make a nested DELEGATECALL to another contract.
    /// Used to test nested delegatecall scenarios (proxy -> impl1 -> impl2).
    /// Returns the returndata from the nested delegatecall.
    pub fn delegateTo(&self, target: Address, calldata: Bytes) -> Bytes {
        // Make nested DELEGATECALL
        // This tests that delegated code can make further delegatecalls
        // Storage context should remain the original proxy's storage
        match delegatecall_contract(target, calldata.as_ref(), None) {
            Ok(ret) => ret,
            Err(revert_data) => revert_with_error(&revert_data),
        }
    }

    /// Fallback handler for testing proxy `fallback()` / `receive()` routing.
    ///
    /// - If calldata is empty, return a 1-byte marker so tests can assert the path executed.
    /// - Otherwise, echo calldata unchanged.
    #[fallback]
    #[payable]
    pub fn fallback(&mut self, calldata: Bytes) -> Bytes {
        if calldata.is_empty() {
            let mut v = Vec::new();
            v.push(0xF0);
            return Bytes::from(v);
        }
        calldata
    }
}


