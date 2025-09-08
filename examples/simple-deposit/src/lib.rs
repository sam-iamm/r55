#![no_std]
#![no_main]

use alloy_core::primitives::{Address, Bytes, FixedBytes, U256};
use contract_derive::contract;

type B32 = FixedBytes<32>;

extern crate alloc;

#[derive(Default)]
pub struct SimpleDeposit;

#[contract]
impl SimpleDeposit {
    pub fn new() -> Self {
        SimpleDeposit::default()
    }

    // deposit(bytes data) -> bytes32 (WORKS)
    // returns bytes32(keccak256(abi.encode(data)))
    pub fn depositBytes(&self, data: Bytes) -> B32 {
        use alloy_core::primitives::{keccak256, U256};
        use alloc::vec::Vec;
    
        let len = data.len() as u64;
    
        // ABI encoding for `bytes data` in abi.encode():
        // offset (32) || length (32) || padded data
        let mut encoded = Vec::new();
    
        // offset = 32 (0x20) because length starts after the first word
        encoded.extend_from_slice(&U256::from(32).to_be_bytes::<32>());
        // length
        encoded.extend_from_slice(&U256::from(len).to_be_bytes::<32>());
        // data (padded to 32)
        let mut padded = data.to_vec();
        while padded.len() % 32 != 0 {
            padded.push(0u8);
        }
        encoded.extend_from_slice(&padded);
    
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }
    

    // depositBytes32(bytes32 data) -> bytes32 (WORKS)
    // returns bytes32(keccak256(data))
    pub fn depositBytes32(&self, data: B32) -> B32 {
        // Compute keccak256(data) to return a deterministic 32-byte value
        use alloy_core::primitives::keccak256;
        let hash = keccak256(&data);
        FixedBytes::<32>::from(hash)
    }

    // depositBytesAddress(bytes data, address to) -> bytes32 (WORKS)
    // returns bytes32(0x42424242...)
    pub fn depositBytesAddress(&self, _data: Bytes, _to: Address) -> B32 {
        FixedBytes::<32>::from([0x42u8; 32])
    }

    // depositBytesBytesAddress(bytes data, bytes data2, address to) -> bytes32 (DOES NOT WORK)
    // returns bytes32(0x42424242...)
    pub fn depositBytesBytesAddress(&self, _data: Bytes, _data2: Bytes, _to: Address) -> B32 {
        FixedBytes::<32>::from([0x42u8; 32])
    }

    // depositAddressBytes(address to, bytes data) -> bytes32 (DOES NOT WORK)
    // returns bytes32(0x42424242...)
    pub fn depositAddressBytes(&self, _to: Address, _data: Bytes) -> B32 {
        FixedBytes::<32>::from([0x42u8; 32])
    }
}