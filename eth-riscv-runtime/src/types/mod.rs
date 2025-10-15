use core::default::Default;
use core::marker::PhantomData;

use crate::*;

use alloy_sol_types::{SolType, SolValue};

extern crate alloc;
use alloc::vec::Vec;

mod mapping;
pub use mapping::Mapping;

mod slot;
pub use slot::Slot;

mod dynamic_slot;
pub use dynamic_slot::DynamicSlot;

/// Dynamic storage semantics (v1):
/// - Each dynamic field is anchored at a unique base slot `B` (macro-assigned).
/// - The length (in bytes) is stored at `B`.
/// - Payload words are stored at `keccak256(B || i_be)` per index `i` (0-based), where `i_be` is the u64 index encoded as big-endian (8 bytes).
/// - Reads honor the stored length and ignore surplus words.
/// - For mappings, the base `B` is `keccak256(key || id)`; dynamic payload derives from that base,
///   ensuring namespaces are disjoint per (contract, field, key, index).

///  STORAGE TYPES:
///  > Must implement the following traits:
///     - `StorageLayout`: Allows the `storage` macro to allocate a storage slot.
///  > Must implement one of the following traits, for external consumption:
///     - `DirectStorage`:  Exposes read and write capabilities of values that are directly accessed.
///     - `IndirectStorage`:  Exposes read and write capabilities of values that are gated by a guard.
///  > Unless it is a wrapper type (like `Mapping`) it must implement the following traits:
///     - `StorageStorable`: Allows db storage reads and writes with abi de/encoding.


// TODO: enhance `storage` macro to handle complex types (like tuples or custom structs)
/// A trait for storage types that require a dedicated slot in the storage layout
pub trait StorageLayout {
    fn allocate(limb0: u64, limb1: u64, limb2: u64, limb3: u64) -> Self;
}

/// Internal trait, for low-level storage operations.
pub trait StorageStorable {
    type Value: SolValue
        + core::convert::From<<<Self::Value as SolValue>::SolType as SolType>::RustType>;

    fn __read(key: U256) -> Self::Value;
    fn __write(key: U256, value: Self::Value);
}

/// Public interface for interacting with direct storage types (like `Slot`)
pub trait DirectStorage<V>
where
    Self: StorageStorable<Value = V>,
{
    fn read(&self) -> V;
    fn write(&mut self, value: V);
}

/// Public interface for interacting with indirect storage types (like `MappingGuard`)
pub trait IndirectStorage<V>
where
    V: StorageStorable,
    V::Value: SolValue + core::convert::From<<<V::Value as SolValue>::SolType as SolType>::RustType>,
{
    fn read(&self) -> V::Value;
    fn write(&mut self, value: V::Value);
}
