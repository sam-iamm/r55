#![no_std]
#![no_main]

use alloy_core::primitives::{Address, Bytes, FixedBytes, U256};
use contract_derive::{contract, storage, Event};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{keccak256, msg_sender, revert, sload, sstore};

extern crate alloc;
use alloc::vec::Vec;
use alloc::vec;

// bytes32 alias that the macro recognizes AND abi-decodes without conversions
type B32 = FixedBytes<32>;


// struct DecodedProof {
//     account_proof: alloc::vec::Vec<Bytes>,
//     storage_proof: alloc::vec::Vec<Bytes>,
//     state_root: B32,
// }

#[derive(Event)]
pub struct SignalSent {
    #[indexed]
    pub sender: Address,
    pub value: B32,
}

#[derive(Event)]
pub struct SignalVerified {
    #[indexed]
    pub sender: Address,
    pub value: B32,
}

#[storage]
pub struct SignalService {
    state_root_publisher: Slot<Address>,
    this_address: Slot<Address>,
}

#[contract]
impl SignalService {
    pub fn new(state_root_publisher: Address, this_address: Address) -> Self {
        let mut s = SignalService::default();
        s.state_root_publisher.write(state_root_publisher);
        s.this_address.write(this_address);
        s
    }

    // sendSignal(bytes32) -> bytes32
    pub fn sendSignal(&mut self, value: B32) -> B32 {
        let sender = msg_sender();
        let slot = derive_slot(value, sender);
        sstore(slot, U256::from(1u8));
        let out = slot.to_be_bytes::<32>();
        let ret = FixedBytes::<32>::from(out);
        log::emit(SignalSent::new(sender, value));
        ret
    }

    // isSignalStored(bytes32,address) -> bool
    pub fn isSignalStored(&self, value: B32, sender: Address) -> bool {
        let slot = derive_slot(value, sender);
        sload(slot) == U256::from(1u8)
    }
}

// ---- helpers: ERC-7201 slot derivation ----

// abi.encodePacked(value (32), account (20))
fn packed_value_sender(value: B32, account: Address) -> Vec<u8> {
    let mut buf = Vec::with_capacity(52);
    buf.extend_from_slice(value.as_slice()); // value is FixedBytes<32>
    buf.extend_from_slice(account.as_slice());
    buf
}

// slot = keccak256( keccak256(ns_bytes) - 1 as 32 bytes ) & ~0xff
fn erc7201_slot(ns: &[u8]) -> U256 {
    let h1 = keccak256(ns.as_ptr() as u64, ns.len() as u64);
    let h1m1 = h1 - U256::from(1u8);
    let h1m1_be = h1m1.to_be_bytes::<32>(); // specify const generic size
    let h2 = keccak256(h1m1_be.as_ptr() as u64, h1m1_be.len() as u64);
    h2 & !U256::from(0xffu8)
}

// deriveSlot(value, account)
fn derive_slot(value: B32, account: Address) -> U256 {
    let ns = packed_value_sender(value, account);
    erc7201_slot(&ns)
}