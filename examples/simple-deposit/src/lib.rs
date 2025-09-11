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
        use alloc::vec::Vec;
        use alloy_core::primitives::{keccak256, U256};

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

    // depositBytes32(bytes32 data) -> bytes32
    // returns bytes32(keccak256(data))
    pub fn depositBytes32(&self, data: B32) -> B32 {
        // Compute keccak256(data) to return a deterministic 32-byte value
        use alloy_core::primitives::keccak256;
        let hash = keccak256(&data);
        FixedBytes::<32>::from(hash)
    }

    // depositBytesAddress(bytes data, address to) -> bytes32
    // returns keccak256(abi.encode(data, to))
    pub fn depositBytesAddress(&self, data: Bytes, to: Address) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        let encoded = (data, to).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // depositBytesBytesAddress(bytes data, bytes data2, address to) -> bytes32
    // returns keccak256(abi.encode(data, data2, to))
    pub fn depositBytesBytesAddress(&self, data: Bytes, data2: Bytes, to: Address) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        let encoded = (data, data2, to).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // depositBytesBytesAddres(bytes data, bytes data2, address to) -> bytes32
    // returns keccak256(abi.encode(data, data2, to))
    pub fn depositBytesBytesAddres(&self, data: Bytes, data2: Bytes, to: Address) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        let encoded = (data, data2, to).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // depositAddressBytesBytesAddress(address to, bytes data, bytes data2, address to2) -> bytes32
    // returns keccak256(abi.encode(to, data, data2, to2))
    pub fn depositAddressBytesBytesAddress(&self, to: Address, data: Bytes, data2: Bytes, to2: Address) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        let encoded = (to, data, data2, to2).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // depositAddressBytes(address to, bytes data) -> bytes32
    // returns keccak256(abi.encode(to, data))
    pub fn depositAddressBytes(&self, to: Address, data: Bytes) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        let encoded = (to, data).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // deposit(address to, bytes data, bytes data2, address to2) -> bytes32
    pub fn deposit(&self, to: Address, data: Bytes, data2: Bytes, to2: Address) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        // Encode the arguments exactly as standard ABI (tuple of params)
        let encoded = (to, data, data2, to2).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // Edge case: empty bytes
    pub fn depositEmptyBytes(&self, data: Bytes) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        let encoded = (data,).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // Edge case: very long bytes (stress test)
    pub fn depositLongBytes(&self, data: Bytes) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        let encoded = (data,).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // Parameter validation: return specific value based on decoded address
    pub fn validateAddress(&self, addr: Address) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        // Return hash of the address to prove it was decoded correctly
        let encoded = (addr,).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // Complex parameter combination test
    pub fn complexParams(&self, addr1: Address, data1: Bytes, addr2: Address, data2: Bytes, addr3: Address) -> B32 {
        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;

        let encoded = (addr1, data1, addr2, data2, addr3).abi_encode();
        let hash = keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }
}