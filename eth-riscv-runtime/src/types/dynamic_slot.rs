use super::*;

use core::marker::PhantomData;

extern crate alloc;
use alloc::vec::Vec;

use alloy_core::primitives::Bytes;

/// Generic dynamic storage slot that anchors a variable-length value at a base slot
/// and stores the payload across derived slots computed as `keccak256(base || i_be)`.
///
/// Where `i_be` is the u64 index encoded as big-endian (8 bytes).
///
/// Semantics (v1):
/// - Length (bytes) is stored at base slot `B`.
/// - Payload words are stored at `keccak256(B || i_be)` for `i = 0..`, padded to 32 bytes on write.
/// - Reads return exactly the first `len` bytes and ignore any surplus words from prior larger writes.
/// - When used under `Mapping`, the mapping guard provides `B = keccak256(key || id)`,
///   so dynamic payload keys remain disjoint per `(contract, field, key, index)`.
///
/// Example (key derivation):
/// - Let `B = 0x[32 bytes]` and `i` be the 64-bit chunk index.
/// - For `i = 0`, `i_be = 0x0000000000000000`, key₀ = keccak256(`B || 0000000000000000`).
/// - For `i = 1`, `i_be = 0x0000000000000001`, key₁ = keccak256(`B || 0000000000000001`).
/// - For `i = 257`, `i_be = 0x0000000000000101`, key₂₅₇ = keccak256(`B || 0000000000000101`).
/// Each chunk stores 32 bytes of payload at its corresponding key.
///
/// Safety:
/// - Length is bounded to prevent excessive allocations.
/// - Reads honor the stored length and ignore surplus words.
///
/// Notes on trait bounds:
/// - `StorageStorable` requires `Value: SolValue + From<...>` across the runtime. The dynamic path
///   does not use ABI encode/decode for payload; it converts via `DynamicValue`. The `SolValue`
///   bound is retained for type coherence with other storage types.
/// Upper bound for dynamic payload size in bytes.
/// Rationale: conservative DoS guard for v1 to cap allocation and loop work.
/// This is an internal runtime policy, not part of the ABI. Contracts that accept
/// untrusted dynamic inputs should still validate lengths at the application layer.
const MAX_DYNAMIC_BYTES: usize = 131_072; // 128 KiB upper bound for v1
/// Layout handle for a dynamic value of type `T`.
///
/// `id` holds the base slot `B` that anchors the value. The macro `#[storage]`
/// assigns `B` uniquely per field (or, under mappings, per `(field, key)`), and
/// the dynamic payload is spread across derived keys `keccak256(B || i_be)`.
pub struct DynamicSlot<T> {
    id: U256,
    _pd: PhantomData<T>,
}

impl<T> StorageLayout for DynamicSlot<T> {
    /// Assign the base slot id `B` for this field. The `#[storage]` macro passes the slot id
    /// as four limbs (U256). No hashing is performed here; hashing is deferred to per-chunk
    /// addressing for dynamic payload words.
    fn allocate(first: u64, second: u64, third: u64, fourth: u64) -> Self {
        Self {
            id: U256::from_limbs([first, second, third, fourth]),
            _pd: PhantomData::default(),
        }
    }
}

/// Conversion layer between runtime values and their storage byte representation.
/// Not ABI encoding; this is strictly for persistence in `DynamicSlot<T>`.
/// Implementations must be inverses: `from_storage_bytes(to_storage_bytes(v)) == v`.
pub trait DynamicValue {
    fn to_storage_bytes(v: &Self) -> Vec<u8>;
    fn from_storage_bytes(b: Vec<u8>) -> Self;
}

impl DynamicValue for alloc::string::String {
    /// Persist strings as raw UTF-8 bytes. Invalid UTF-8 on read triggers a revert.
    fn to_storage_bytes(v: &Self) -> Vec<u8> {
        v.as_bytes().to_vec()
    }

    fn from_storage_bytes(b: Vec<u8>) -> Self {
        match alloc::string::String::from_utf8(b) {
            Ok(s) => s,
            Err(_) => revert(),
        }
    }
}

impl DynamicValue for Bytes {
    /// Persist as raw byte vector; no additional framing.
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
        // Load the authoritative length (bytes) from the base slot `B`.
        // The length is encoded as a small U256 where only the lowest 64 bits are used in v1.
        let len_u256 = sload(base);
        let limbs = len_u256.as_limbs();
        if limbs[1] != 0 || limbs[2] != 0 || limbs[3] != 0 {
            // length too large to fit into usize on this platform
            revert();
        }
        let len = limbs[0] as usize;
        if len == 0 {
            // Empty value: return the default dynamic value for T from empty bytes.
            return T::from_storage_bytes(Vec::new());
        }

        // Defensive bound: prevent unbounded allocation and read loops.
        if len > MAX_DYNAMIC_BYTES {
            revert();
        }

        // Number of 32-byte words to read to reconstruct `len` bytes.
        let words = (len + 31) / 32;
        let mut out = Vec::with_capacity(words * 32);
        let base_bytes: [u8; 32] = base.to_be_bytes();

        // Fixed-size stack buffer to avoid per-iteration heap allocation.
        let mut buf: [u8; 40] = [0u8; 40];
        buf[..32].copy_from_slice(&base_bytes);

        for i in 0..(words as u64) {
            // Compute per-word storage key as: key = keccak256(B || i_be),
            // where i_be is the big-endian encoding of the 64-bit chunk index i.
            let index_be_8 = i.to_be_bytes();
            buf[32..40].copy_from_slice(&index_be_8);
            let key = keccak256(buf.as_ptr() as u64, 40);
            let word: [u8; 32] = sload(key).to_be_bytes();
            out.extend_from_slice(&word);
        }

        // Truncate to the exact byte length and convert back to T.
        // Any surplus words from prior longer writes are ignored by design in v1.
        out.truncate(len);
        T::from_storage_bytes(out)
    }

    fn __write(base: U256, value: Self::Value) {
        // Convert runtime value into its storage byte representation.
        let data = T::to_storage_bytes(&value);
        if data.len() > MAX_DYNAMIC_BYTES {
            revert();
        }
        let len = data.len() as u64;

        // Store the authoritative length (bytes) at base slot `B`.
        sstore(base, U256::from(len));

        if len == 0 {
            // Empty value: no payload words to write.
            return;
        }

        // Write 32-byte chunks to keys derived as keccak256(B || i_be), zero-padding the final chunk.
        // v1 semantics: we do not clear surplus words from prior longer writes; the stored length governs reads.
        let mut offset = 0usize;
        let mut index: u64 = 0;
        let base_bytes: [u8; 32] = base.to_be_bytes();
        // Fixed-size stack buffer to avoid per-iteration heap allocation.
        let mut buf: [u8; 40] = [0u8; 40];
        buf[..32].copy_from_slice(&base_bytes);
        while offset < data.len() {
            let remaining = data.len() - offset;
            let take = core::cmp::min(32, remaining);
            let mut chunk = [0u8; 32];
            chunk[..take].copy_from_slice(&data[offset..offset + take]);
            // Key derivation: key = keccak256(B || i_be), i_be is big-endian u64
            let index_be_8 = index.to_be_bytes();
            buf[32..40].copy_from_slice(&index_be_8);
            let key = keccak256(buf.as_ptr() as u64, 40);
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
    // Expose direct read/write for fields that own a base slot `B` (non-mapping fields).
    // When used under `Mapping`, `MappingGuard<V>` implements `IndirectStorage<V>` and will
    // call `V::__read/__write` with a derived base key (e.g., `B = keccak256(key || id)`),
    // so `DynamicSlot<T>` composes naturally without a separate IndirectStorage impl.
    fn read(&self) -> T {
        Self::__read(self.id)
    }

    fn write(&mut self, value: T) {
        Self::__write(self.id, value)
    }
}


