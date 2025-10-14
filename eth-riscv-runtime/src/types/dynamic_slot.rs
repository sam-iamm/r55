use super::*;

use core::marker::PhantomData;

extern crate alloc;
use alloc::vec::Vec;

use alloy_core::primitives::Bytes;

/// Generic dynamic storage slot that anchors a variable-length value at a base slot
/// and stores the payload across derived slots computed as `keccak256(base || i_be)`.
///
/// Safety:
/// - Length is bounded to prevent excessive allocations.
/// - Reads honor the stored length and ignore surplus words.
const MAX_DYNAMIC_BYTES: usize = 131_072; // 128 KiB upper bound for v1
pub struct DynamicSlot<T> {
    id: U256,
    _pd: PhantomData<T>,
}

impl<T> StorageLayout for DynamicSlot<T> {
    fn allocate(first: u64, second: u64, third: u64, fourth: u64) -> Self {
        Self {
            id: U256::from_limbs([first, second, third, fourth]),
            _pd: PhantomData::default(),
        }
    }
}

/// Helper trait to convert between runtime values and their storage byte representation.
pub trait DynamicValue {
    fn to_storage_bytes(v: &Self) -> Vec<u8>;
    fn from_storage_bytes(b: Vec<u8>) -> Self;
}

impl DynamicValue for alloc::string::String {
    fn to_storage_bytes(v: &Self) -> Vec<u8> {
        v.as_bytes().to_vec()
    }

    fn from_storage_bytes(b: Vec<u8>) -> Self {
        alloc::string::String::from_utf8(b).unwrap_or_default()
    }
}

impl DynamicValue for Bytes {
    fn to_storage_bytes(v: &Self) -> Vec<u8> {
        v.to_vec()
    }

    fn from_storage_bytes(b: Vec<u8>) -> Self {
        Bytes::from(b)
    }
}

impl<T> StorageStorable for DynamicSlot<T>
where
    T: DynamicValue + SolValue + core::convert::From<<<T as SolValue>::SolType as SolType>::RustType>,
{
    type Value = T;

    fn __read(base: U256) -> Self::Value {
        // load length (in bytes) from base slot
        let len_u256 = sload(base);
        let limbs = len_u256.as_limbs();
        if limbs[1] != 0 || limbs[2] != 0 || limbs[3] != 0 {
            // length too large to fit into usize on this platform
            revert();
        }
        let len = limbs[0] as usize;
        if len == 0 {
            return T::from_storage_bytes(Vec::new());
        }

        // Prevent excessive allocations
        if len > MAX_DYNAMIC_BYTES {
            revert();
        }

        // number of 32-byte words to read
        let words = (len + 31) / 32;
        let mut out = Vec::with_capacity(words * 32);

        for i in 0..(words as u64) {
            // key = keccak256(base || i_be)
            let base_bytes: [u8; 32] = base.to_be_bytes();
            let idx_bytes = i.to_be_bytes();
            let mut buf = Vec::with_capacity(32 + 8);
            buf.extend_from_slice(&base_bytes);
            buf.extend_from_slice(&idx_bytes);
            let key = keccak256(buf.as_ptr() as u64, buf.len() as u64);
            let word: [u8; 32] = sload(key).to_be_bytes();
            out.extend_from_slice(&word);
        }

        out.truncate(len);
        T::from_storage_bytes(out)
    }

    fn __write(base: U256, value: Self::Value) {
        let mut data = T::to_storage_bytes(&value);
        if data.len() > MAX_DYNAMIC_BYTES {
            revert();
        }
        let len = data.len() as u64;

        // store length at base slot
        sstore(base, U256::from(len));

        if len == 0 {
            return;
        }

        // write 32-byte chunks, padding final chunk with zeros
        let mut offset = 0usize;
        let mut index: u64 = 0;
        while offset < data.len() {
            let remaining = data.len() - offset;
            let take = core::cmp::min(32, remaining);
            let mut chunk = [0u8; 32];
            chunk[..take].copy_from_slice(&data[offset..offset + take]);
            // key = keccak256(base || index_be)
            let base_bytes: [u8; 32] = base.to_be_bytes();
            let idx_bytes = index.to_be_bytes();
            let mut buf = Vec::with_capacity(32 + 8);
            buf.extend_from_slice(&base_bytes);
            buf.extend_from_slice(&idx_bytes);
            let key = keccak256(buf.as_ptr() as u64, buf.len() as u64);
            sstore(key, U256::from_be_bytes(chunk));
            offset += take;
            index += 1;
        }
    }
}

impl<T> DirectStorage<T> for DynamicSlot<T>
where
    Self: StorageStorable<Value = T>,
{
    fn read(&self) -> T {
        Self::__read(self.id)
    }

    fn write(&mut self, value: T) {
        Self::__write(self.id, value)
    }
}


