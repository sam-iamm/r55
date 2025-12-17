//! SignalService (R55/RISC-V)
//!
//! Bridge signaling + proof verification.
//! - sendSignal: mark `(value,sender)` under ERC-7201 namespacing and emit `SignalSent`.
//! - isSignalStored: check if `(value,sender)` was marked.
//! - verifySignal: require a publisher-signed state root, verify account + storage proofs for
//!   `(value,sender)`, and emit `SignalVerified` on success.
//!
//! Parity notes (R55):
//! - Constructor: must pass `this_address` explicitly, representing the deployed
//!   contract’s address in account proofs.

#![no_std]
#![no_main]

use alloy_core::primitives::{
    keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, B256, U256,
};
use contract_derive::{contract, storage, Event};
use eth_riscv_runtime::msg_sender;
use eth_riscv_runtime::types::*;

type B32 = FixedBytes<32>;

extern crate alloc;
use alloc::vec::Vec;
use alloy_rlp::Decodable;
use alloy_trie::proof::{verify_proof, ProofVerificationError};
use eth_riscv_runtime::{revert, revert_with_error};
use nybbles::Nibbles;

// Event: SignalSent(sender,value)
#[derive(Event)]
struct SignalSent {
    #[indexed]
    sender: Address,
    value: B32,
}

// Event: SignalVerified(sender,value)
#[derive(Event)]
struct SignalVerified {
    #[indexed]
    sender: Address,
    value: B32,
}

#[storage]
pub struct SignalService {
    state_root_publisher: Slot<Address>,
    this_address: Slot<Address>,
    signals: Mapping<B32, Slot<bool>>,
}

#[contract]
impl SignalService {
    /// Initialize with the state root publisher and this contract’s address
    pub fn new(state_root_publisher: Address, _this_address: Address) -> Self {
        let mut storage = SignalService::default();
        storage.state_root_publisher.write(state_root_publisher);
        storage.this_address.write(_this_address);
        storage
    }

    /// sendSignal(bytes32 value) → bytes32
    /// Derive slot from (value,sender), mark it, and emit SignalSent.
    pub fn sendSignal(&mut self, value: B32) -> B32 {
        let sender = msg_sender();
        let slot = derive_key(value, sender);

        self.signals[slot].write(true);
        log::emit(SignalSent::new(sender, value));

        slot
    }

    /// isSignalStored(bytes32 value, address sender) → bool
    /// True if (value,sender) has been marked.
    pub fn isSignalStored(&self, value: B32, sender: Address) -> bool {
        let slot = derive_key(value, sender);
        self.signals[slot].read()
    }

    /// verifySignal(address sender, bytes32 value, bytes proof)
    /// - Require a publisher-signed state root
    /// - Verify account proof to extract storage root
    /// - Verify storage proof for `(value,sender)` → 1
    /// On success, emit SignalVerified; on failure, revert with selector.
    pub fn verifySignal(&self, sender: Address, value: B32, proof_bytes: Bytes) {
        // ABI-decode SignalProof(accountProof,storageProof,stateRoot)
        alloy_sol_types::sol! {
            struct SignalProof { bytes[] accountProof; bytes[] storageProof; bytes32 stateRoot; }
        }

        let Ok(signal_proof) = <SignalProof as alloy_sol_types::SolType>::abi_decode(&proof_bytes)
        else {
            revert_selector(selector_invalid_proof_encoding());
        };

        let account_proof: Vec<Bytes> = signal_proof.accountProof;
        let storage_proof: Vec<Bytes> = signal_proof.storageProof;
        let state_root_b32: B32 = signal_proof.stateRoot;

        // Check publisher signaled this state root
        let publisher = self.state_root_publisher.read();
        let state_root_slot = derive_key(state_root_b32, publisher);
        if !self.signals[state_root_slot].read() {
            revert_selector(selector_state_root_not_found());
        }

        if account_proof.is_empty() {
            revert_selector(selector_account_proof_empty());
        }

        // Verify account proof for this contract’s storage root
        let this_addr = self.this_address.read();
        let state_root = B256::from_slice(state_root_b32.as_slice());
        let account_key = Nibbles::unpack(alloy_keccak256(this_addr.as_slice()).as_slice());

        let account_rlp: Vec<u8> =
            match verify_proof(state_root, account_key, None, account_proof.iter()) {
                Ok(()) => revert_selector(selector_invalid_account_proof()), // non-inclusion
                Err(ProofVerificationError::ValueMismatch {
                    got: Some(bytes),
                    expected: None,
                    ..
                }) => bytes.to_vec(),
                Err(_) => revert_selector(selector_invalid_account_proof()),
            };

        // Decode account RLP: (nonce,balance,storageRoot,codeHash)
        let mut acc_buf = &account_rlp[..];
        let mut items = match alloy_rlp::Header::decode_raw(&mut acc_buf) {
            Ok(alloy_rlp::PayloadView::List(list)) => list,
            _ => revert_selector(selector_invalid_account_proof()),
        };
        if items.len() != 4 {
            revert_selector(selector_invalid_account_proof());
        }
        let _nonce_item = items.remove(0);
        let _balance_item = items.remove(0);
        let mut storage_item = items.remove(0);
        let Ok(storage_root_bytes) = alloy_rlp::Bytes::decode(&mut storage_item) else {
            revert_selector(selector_invalid_account_proof());
        };
        if storage_root_bytes.len() != 32 {
            revert_selector(selector_invalid_account_proof());
        }
        let storage_root = B256::from_slice(&storage_root_bytes);

        // Verify storage proof for (value,sender) → RLP(1)
        let slot = derive_key(value, sender);
        let storage_key = Nibbles::unpack(alloy_keccak256(slot.as_slice()).as_slice());
        let expected_rlp_one: Vec<u8> = alloy_rlp::encode(U256::from(1u8));
        if verify_proof(
            storage_root,
            storage_key,
            Some(expected_rlp_one),
            storage_proof.iter(),
        )
        .is_err()
        {
            revert_selector(selector_invalid_inclusion_proof());
        }

        log::emit(SignalVerified::new(sender, value));
    }
}

// Derive storage slot for (value,sender) per ERC-7201
fn derive_key(value: B32, account: Address) -> B32 {
    let namespace = alloy_sol_types::SolValue::abi_encode_packed(&(value, account));
    let namespace_hash = alloy_keccak256(&namespace);
    let word = U256::from_be_slice(namespace_hash.as_slice());
    let (minus_one, _) = word.overflowing_sub(U256::from(1u8));
    let buf = minus_one.to_be_bytes::<32>();
    let slot_bytes = alloy_keccak256(&buf);

    // Zero out lowest 8 bits
    let mut arr = [0u8; 32];
    arr.copy_from_slice(slot_bytes.as_slice());
    arr[arr.len() - 1] = 0u8;
    B32::from(arr)
}

// --- Internals ---

fn revert_selector(sel: [u8; 4]) -> ! {
    revert_with_error(&sel)
}

fn selector_state_root_not_found() -> [u8; 4] {
    // keccak256("StateRootNotFound()")[:4]
    let sig = b"StateRootNotFound()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

fn selector_account_proof_empty() -> [u8; 4] {
    // keccak256("AccountProofEmpty()")[:4]
    let sig = b"AccountProofEmpty()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

fn selector_invalid_account_proof() -> [u8; 4] {
    // keccak256("InvalidAccountProof()")[:4]
    let sig = b"InvalidAccountProof()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

fn selector_invalid_inclusion_proof() -> [u8; 4] {
    // keccak256("InvalidInclusionProof()")[:4]
    let sig = b"InvalidInclusionProof()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

fn selector_invalid_proof_encoding() -> [u8; 4] {
    // keccak256("InvalidProofEncoding()")[:4]
    let sig = b"InvalidProofEncoding()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}
