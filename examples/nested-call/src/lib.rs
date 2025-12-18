//! NestedCall contract for testing `#[interface]` macro calldata encoding.
//!
//! This contract demonstrates cross-contract calls using the `#[interface]` macro.
//! Each method receives an ETHDeposit struct, extracts specific fields, and calls
//! the corresponding SimpleDeposit function with those parameters.
//!
//! The `#[interface]` macro generates calldata for cross-contract calls. This contract
//! tests that the macro correctly encodes:
//! - Single-arg calls with `.abi_encode()`
//! - Multi-arg calls with `.abi_encode_params()` (NOT tuple encoding)
//!
//! Used in tests to verify Solidity and R55 generate identical interface calldata.

#![no_std]
#![no_main]

use alloy_core::primitives::{keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, U256};
use contract_derive::{contract, interface, storage};
use eth_riscv_runtime::{call_contract, revert_with_error};
use eth_riscv_runtime::types::*;

type B32 = FixedBytes<32>;

extern crate alloc;

/// Interface to SimpleDeposit contract - tests various argument patterns.
/// The `#[interface]` macro generates calldata encoding for each function.
#[interface("camelCase")]
trait ISimpleDeposit {
    fn depositBytes(&mut self, data: Bytes) -> B32;
    fn depositBytesAddress(&mut self, data: Bytes, to: Address) -> B32;
    fn depositBytesBytesAddress(&mut self, data: Bytes, data2: Bytes, to: Address) -> B32;
    fn depositAddressBytesBytesAddress(&mut self, to: Address, data: Bytes, data2: Bytes, to2: Address) -> B32;
    fn depositStruct(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) -> B32;
    fn complexParams(&mut self, addr1: Address, data1: Bytes, addr2: Address, data2: Bytes, addr3: Address) -> B32;
    fn revertWithSelector(&mut self);
    fn revertWithNoData(&mut self);
}


#[storage]
pub struct NestedCall {
    simple_deposit: Slot<Address>,
}

#[contract]
impl NestedCall {
    pub fn new(simple_deposit: Address) -> Self {
        let mut storage = NestedCall::default();
        storage.simple_deposit.write(simple_deposit);
        storage
    }

    // depositBytes(ETHDeposit eth_deposit) -> bytes32
    // returns simple_deposit.depositBytes(eth_deposit.context)
    pub fn depositBytes(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) -> B32 {
        let simple_deposit = self.simple_deposit.read();
        let id = ISimpleDeposit::new(simple_deposit).with_ctx(self).depositBytes(eth_deposit.5).expect("ERROR");
        id
    }

    // depositBytesAddress(ETHDeposit eth_deposit) -> bytes32
    // returns simple_deposit.depositBytesAddress(eth_deposit.context, eth_deposit.to)
    pub fn depositBytesAddress(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) -> B32 {
        let simple_deposit = self.simple_deposit.read();
        let id = ISimpleDeposit::new(simple_deposit).with_ctx(self).depositBytesAddress(eth_deposit.5, eth_deposit.6).expect("ERROR");
        id
    }

    // depositBytesBytesAddress(ETHDeposit eth_deposit) -> bytes32
    // returns simple_deposit.depositBytesBytesAddress(eth_deposit.data, eth_deposit.context, eth_deposit.canceler)
    pub fn depositBytesBytesAddress(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) -> B32 {
        let simple_deposit = self.simple_deposit.read();
        let id = ISimpleDeposit::new(simple_deposit).with_ctx(self).depositBytesBytesAddress(eth_deposit.4, eth_deposit.5, eth_deposit.6).expect("ERROR");
        id
    }

    // depositAddressBytesBytesAddress(ETHDeposit eth_deposit) -> bytes32
    // returns simple_deposit.depositAddressBytesBytesAddress(eth_deposit.to, eth_deposit.data, eth_deposit.context, eth_deposit.canceler)
    pub fn depositAddressBytesBytesAddress(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) -> B32 {
        let simple_deposit = self.simple_deposit.read();
        let id = ISimpleDeposit::new(simple_deposit).with_ctx(self).depositAddressBytesBytesAddress(eth_deposit.1, eth_deposit.4, eth_deposit.5, eth_deposit.6).expect("ERROR");
        id
    }

    // depositStruct(ETHDeposit eth_deposit) -> bytes32
    // returns simple_deposit.deposit(eth_deposit)
    pub fn depositStruct(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) -> B32 {
        let simple_deposit = self.simple_deposit.read();
        let id = ISimpleDeposit::new(simple_deposit).with_ctx(self).depositStruct(eth_deposit).expect("ERROR");
        id
    }

    // depositComplexParams(ETHDeposit eth_deposit) -> bytes32
    // returns simple_deposit.depositComplexParams(eth_deposit.from, eth_deposit.data, eth_deposit.to, eth_deposit.context, eth_deposit.canceler)
    pub fn depositComplexParams(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) -> B32 {
        let simple_deposit = self.simple_deposit.read();
        let id = ISimpleDeposit::new(simple_deposit).with_ctx(self).complexParams(eth_deposit.1, eth_deposit.4, eth_deposit.2, eth_deposit.5, eth_deposit.6).expect("ERROR");
        id
    }

    // Revert test with workaround: uses low-level call to bubble revert from nested call.
    // R55 frame 0 doesn't automatically bubble callee reverts, so manual check is needed.
    pub fn depositNestedRevertWorkaround(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) {
        let simple_deposit = self.simple_deposit.read();
        // ISimpleDeposit::new(simple_deposit).with_ctx(self).revertWithSelector().expect("ERROR");
        // Low-level call to bubble callee revert for parity with Solidity
        let sel = alloy_keccak256(b"revertWithSelector()");
        let selector: [u8; 4] = [sel[0], sel[1], sel[2], sel[3]];
        let calldata = Bytes::from(selector.to_vec());
        match call_contract(simple_deposit, 0u64, &calldata, None) {
            Ok(_) => {}
            Err(revert_data) => revert_with_error(&revert_data),
        }
    }

    // Revert test without workaround: demonstrates R55 frame 0 revert bubbling issue.
    // This will fail in Hydra tests because callee revert is not automatically propagated.
    pub fn depositNestedRevert(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) {
        let simple_deposit = self.simple_deposit.read();
        ISimpleDeposit::new(simple_deposit).with_ctx(self).revertWithSelector().expect("ERROR");
    }

    pub fn depositNestedRevertNoDataWorkaround(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) {
        let simple_deposit = self.simple_deposit.read();
        // Low-level call to bubble callee revert for parity with Solidity
        let sel = alloy_keccak256(b"revertWithNoData()");
        let selector: [u8; 4] = [sel[0], sel[1], sel[2], sel[3]];
        let calldata = Bytes::from(selector.to_vec());
        match call_contract(simple_deposit, 0u64, &calldata, None) {
            Ok(_) => {}
            Err(revert_data) => revert_with_error(&revert_data),
        }
    }

    pub fn depositNestedRevertNoData(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) {
        let simple_deposit = self.simple_deposit.read();
        ISimpleDeposit::new(simple_deposit).with_ctx(self).revertWithNoData().expect("ERROR");
    }
}