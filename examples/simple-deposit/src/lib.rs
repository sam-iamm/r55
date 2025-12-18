#![no_std]
#![no_main]

use alloy_core::primitives::{keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, U256};
use contract_derive::contract;
use eth_riscv_runtime::{revert, revert_with_error};

type B32 = FixedBytes<32>;

extern crate alloc;
use alloc::vec::Vec;

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

    // =============================================================================
    // Array Input/Output Tests (for YT contract investigation)
    // =============================================================================

    // testArrayInputU256(uint256[] amounts) -> bytes32
    // Takes array of U256, returns hash of encoded array
    pub fn testArrayInputU256(&self, amounts: Vec<U256>) -> B32 {
        let encoded = amounts.abi_encode();
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // testArrayInputAddress(address[] addresses) -> bytes32
    // Takes array of addresses, returns hash of encoded array
    pub fn testArrayInputAddress(&self, addresses: Vec<Address>) -> B32 {
        let encoded = addresses.abi_encode();
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // testArrayReturnU256(uint256 length) -> uint256[]
    // Returns array of U256 [1, 2, 3, ..., length]
    pub fn testArrayReturnU256(&self, length: U256) -> Vec<U256> {
        let len = length.as_limbs()[0] as usize;
        let mut result = Vec::new();
        for i in 1..=len {
            result.push(U256::from(i));
        }
        result
    }

    // testArrayReturnAddress(uint256 length) -> address[]
    // Returns array of addresses (zero address repeated)
    pub fn testArrayReturnAddress(&self, length: U256) -> Vec<Address> {
        let len = length.as_limbs()[0] as usize;
        let mut result = Vec::new();
        for _ in 0..len {
            result.push(Address::ZERO);
        }
        result
    }

    // testMultipleArrays(address[] addresses, uint256[] amounts) -> bytes32
    // Takes multiple arrays, returns hash of both encoded
    pub fn testMultipleArrays(&self, addresses: Vec<Address>, amounts: Vec<U256>) -> B32 {
        let encoded = (addresses, amounts).abi_encode_params();
        let hash = alloy_keccak256(&encoded);
        FixedBytes::<32>::from(hash)
    }

    // testArrayReturnMultiple(uint256 length) -> (uint256, uint256[])
    // Returns tuple with single value and array (like getPostExpiryData pattern)
    pub fn testArrayReturnMultiple(&self, length: U256) -> (U256, Vec<U256>) {
        let len = length.as_limbs()[0] as usize;
        let mut array = Vec::new();
        for i in 1..=len {
            array.push(U256::from(i));
        }
        (U256::from(100), array)
    }

    // testArrayReturnTwoArrays(uint256 length) -> (uint256[], uint256[])
    // Returns tuple with two arrays (like getPostExpiryData pattern)
    pub fn testArrayReturnTwoArrays(&self, length: U256) -> (Vec<U256>, Vec<U256>) {
        let len = length.as_limbs()[0] as usize;
        let mut array1 = Vec::new();
        let mut array2 = Vec::new();
        for i in 1..=len {
            array1.push(U256::from(i));
            array2.push(U256::from(i * 2));
        }
        (array1, array2)
    }

    // testArraySum(uint256[] amounts) -> uint256
    // Takes array, returns sum (tests array iteration)
    pub fn testArraySum(&self, amounts: Vec<U256>) -> U256 {
        let mut sum = U256::ZERO;
        for amount in amounts {
            sum = sum + amount;
        }
        sum
    }

    // testArrayLength(address[] addresses) -> uint256
    // Returns array length
    pub fn testArrayLength(&self, addresses: Vec<Address>) -> U256 {
        U256::from(addresses.len())
    }
}