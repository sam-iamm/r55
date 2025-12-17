#![no_std]
#![no_main]

use alloy_core::primitives::{keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, U256};
use contract_derive::contract;
use eth_riscv_runtime::{revert, revert_with_error};

type B32 = FixedBytes<32>;

extern crate alloc;

#[derive(Default)]
pub struct SimpleDeposit;

#[contract]
impl SimpleDeposit {
    pub fn new() -> Self {
        SimpleDeposit::default()
    }

    // depositStruct(ETHDeposit eth_deposit) -> bytes32
    // returns keccak256(abi.encode(eth_deposit))
    pub fn depositStruct(&self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address)) -> B32 {
        let encoded = eth_deposit.abi_encode();
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // deposit(bytes data) -> bytes32
    // returns bytes32(keccak256(abi.encode(data)))
    pub fn depositBytes(&self, data: Bytes) -> B32 {
        let encoded = data.abi_encode(); // abi.encode(data)
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // depositBytes32(bytes32 data) -> bytes32
    // returns bytes32(keccak256(data))
    pub fn depositBytes32(&self, data: B32) -> B32 {
        // Compute keccak256(data) to return a deterministic 32-byte value
        let hash = alloy_keccak256(&data);
        FixedBytes::<32>::from(hash)
    }

    // depositBytesAddress(bytes data, address to) -> bytes32
    // returns keccak256(abi.encode(data, to))
    pub fn depositBytesAddress(&self, data: Bytes, to: Address) -> B32 {
        let encoded = (data, to).abi_encode_params(); // CRITICAL: use abi_encode_params() for multi-parameter functions
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // depositBytesBytesAddress(bytes data, bytes data2, address to) -> bytes32
    // returns keccak256(abi.encode(data, data2, to))
    pub fn depositBytesBytesAddress(&self, data: Bytes, data2: Bytes, to: Address) -> B32 {
        let encoded = (data, data2, to).abi_encode_params(); // CRITICAL: use abi_encode_params() for multi-parameter functions
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // depositBytesBytesAddres(bytes data, bytes data2, address to) -> bytes32
    // returns keccak256(abi.encode(data, data2, to))
    pub fn depositBytesBytesAddres(&self, data: Bytes, data2: Bytes, to: Address) -> B32 {
        let encoded = (data, data2, to).abi_encode_params(); // CRITICAL: use abi_encode_params() for multi-parameter functions
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // depositAddressBytesBytesAddress(address to, bytes data, bytes data2, address to2) -> bytes32
    // returns keccak256(abi.encode(to, data, data2, to2))
    pub fn depositAddressBytesBytesAddress(
        &self,
        to: Address,
        data: Bytes,
        data2: Bytes,
        to2: Address,
    ) -> B32 {
        let encoded = (to, data, data2, to2).abi_encode_params(); // CRITICAL: use abi_encode_params() for multi-parameter functions
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // depositAddressBytes(address to, bytes data) -> bytes32
    // returns keccak256(abi.encode(to, data))
    pub fn depositAddressBytes(&self, to: Address, data: Bytes) -> B32 {
        let encoded = (to, data).abi_encode_params(); // CRITICAL: use abi_encode_params() for multi-parameter functions
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // deposit(address to, bytes data, bytes data2, address to2) -> bytes32
    pub fn deposit(&self, to: Address, data: Bytes, data2: Bytes, to2: Address) -> B32 {
        // Encode the arguments exactly as standard ABI (tuple of params)
        let encoded = (to, data, data2, to2).abi_encode_params(); // CRITICAL: use abi_encode_params() for multi-parameter functions
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // Edge case: empty bytes
    pub fn depositEmptyBytes(&self, data: Bytes) -> B32 {
        let encoded = data.abi_encode();
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // Edge case: very long bytes (stress test)
    pub fn depositLongBytes(&self, data: Bytes) -> B32 {
        let encoded = data.abi_encode();
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // Parameter validation: return specific value based on decoded address
    pub fn validateAddress(&self, addr: Address) -> B32 {
        // Return hash of the address to prove it was decoded correctly
        let encoded = addr.abi_encode();
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // Complex parameter combination test
    pub fn complexParams(
        &self,
        addr1: Address,
        data1: Bytes,
        addr2: Address,
        data2: Bytes,
        addr3: Address,
    ) -> B32 {
        let encoded = (addr1, data1, addr2, data2, addr3).abi_encode_params(); // CRITICAL: use abi_encode_params() for multi-parameter functions
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // --- Explicit revert entrypoints to exercise relaxed matching in parallel verifier ---
    // Revert with no data (empty payload)
    pub fn revertWithNoData(&self) {
        revert();
    }

    // Revert with a fixed 4-byte custom error selector to produce non-empty payload
    pub fn revertWithSelector(&self) {
        // Intentionally different selector than Solidity (0x12345678) to force payload mismatch.
        let sel: [u8; 4] = [0x87, 0x65, 0x43, 0x21];
        revert_with_error(&sel);
    }
}