#![no_std]
#![no_main]
//! SignalService (R55/RISC-V)
//!
//! Secure signaling and verification utility used by the bridge.
//! - sendSignal: marks a namespaced storage slot `(value,sender)` per ERC‑7201
//!   derivation and emits `SignalSent`.
//! - isSignalStored: reads the same namespaced slot.
//! - verifySignal: requires the state root to be signaled by a trusted publisher,
//!   verifies account/storage proofs against that root, then emits `SignalVerified`.

use alloy_core::primitives::{keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, U256, B256};
use contract_derive::{contract, storage, Event};
use eth_riscv_runtime::msg_sender;
use eth_riscv_runtime::types::*;

type B32 = FixedBytes<32>;

extern crate alloc;
use alloc::vec::Vec;
use eth_riscv_runtime::{revert_with_error, revert};
use nybbles::Nibbles;
use alloy_trie::proof::{verify_proof, ProofVerificationError};
use alloy_rlp::Decodable;
// RLP helpers via derive decoding for specific proof node shapes

// SignalSent(address indexed sender, bytes32 value)
#[derive(Event)]
struct SignalSent {
    #[indexed]
    sender: Address,
    value: B32,
}

// SignalVerified(address indexed sender, bytes32 value)
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
    pub fn new(state_root_publisher: Address, _this_address: Address) -> Self {
        let mut s = SignalService::default();
        s.state_root_publisher.write(state_root_publisher);
        s.this_address.write(_this_address);
        s
    }

    pub fn sendSignal(&mut self, value: B32) -> B32 {
        let sender = msg_sender();
        let slot = derive_key(value, sender);

        self.signals[slot].write(true);
        log::emit(SignalSent::new(sender, value));

        slot
    }

    pub fn isSignalStored(&self, value: B32, sender: Address) -> bool {
        let slot = derive_key(value, sender);
        self.signals[slot].read()
    }

    pub fn verifySignal(&self, sender: Address, value: B32, proof_bytes: Bytes) {
        // Decode SignalProof(accountProof, storageProof, stateRoot)
        // Only perform state-root gating and non-empty accountProof check for now.
        alloy_sol_types::sol! {
            struct SignalProof { bytes[] accountProof; bytes[] storageProof; bytes32 stateRoot; }
        }

        let Ok(signal_proof) = <SignalProof as alloy_sol_types::SolType>::abi_decode(&proof_bytes) else {
            // If decoding fails, mirror Solidity's behavior: generic revert
            revert();
        };

        let account_proof: Vec<Bytes> = signal_proof.accountProof;
        let storage_proof: Vec<Bytes> = signal_proof.storageProof;
        let state_root_b32: B32 = signal_proof.stateRoot;

        // State root must be signaled by the publisher
        let publisher = self.state_root_publisher.read();
        let state_root_slot = derive_key(state_root_b32, publisher);
        if !self.signals[state_root_slot].read() {
            revert_selector(selector_state_root_not_found());
        }

        // accountProof must be non-empty
        if account_proof.is_empty() {
            revert_selector(selector_account_proof_empty());
        }
        // Retrieve the SignalService account's storage root from the proven account against state root.
        // We assume cross-chain deployments at the same address; the constructor provided `_this_address`.
        let this_addr = self.this_address.read();

        // Verify account proof against the state root, retrieve the RLP-encoded account value.
        // Strategy: call verify_proof with expected_value None and recover the proven value from the
        // ValueMismatch error variant which includes the actual leaf value when present.
        let state_root = B256::from_slice(state_root_b32.as_slice());
        let account_key = Nibbles::unpack(alloy_keccak256(this_addr.as_slice()).as_slice());
        let account_proof_iter = account_proof.iter();

        let account_rlp: Vec<u8> = match verify_proof(state_root, account_key, None, account_proof_iter) {
            Ok(()) => {
                // Account does not exist (non-inclusion). Treat as invalid account proof.
                revert_selector(selector_invalid_account_proof());
            }
            Err(ProofVerificationError::ValueMismatch { got: Some(bytes), expected: None, .. }) => {
                bytes.to_vec()
            }
            Err(_) => {
                // Any structural/root mismatch is an invalid account proof.
                revert_selector(selector_invalid_account_proof());
            }
        };

        // Decode account RLP and extract storage root (nonce, balance, storageRoot, codeHash)
        // Without alloy_trie::account (requires `ethereum` feature), decode minimally via alloy_rlp list API.
        let mut acc_buf = &account_rlp[..];
        let mut items = match alloy_rlp::Header::decode_raw(&mut acc_buf) {
            Ok(alloy_rlp::PayloadView::List(list)) => list,
            _ => revert_selector(selector_invalid_account_proof()),
        };
        if items.len() != 4 { revert_selector(selector_invalid_account_proof()); }
        // storageRoot is index 2
        let _nonce_item = items.remove(0);
        let _balance_item = items.remove(0);
        let mut storage_item = items.remove(0);
        let Ok(storage_root_bytes) = alloy_rlp::Bytes::decode(&mut storage_item) else { revert_selector(selector_invalid_account_proof()); };
        if storage_root_bytes.len() != 32 { revert_selector(selector_invalid_account_proof()); }
        let storage_root = B256::from_slice(&storage_root_bytes);

        // Verify storage inclusion for the derived slot equals RLP(1).
        let slot = derive_key(value, sender);
        let storage_key = Nibbles::unpack(alloy_keccak256(slot.as_slice()).as_slice());
        let expected_rlp_one: Vec<u8> = alloy_rlp::encode(U256::from(1u8));
        let storage_proof_iter = storage_proof.iter();
        if verify_proof(storage_root, storage_key, Some(expected_rlp_one), storage_proof_iter).is_err() {
            revert_selector(selector_invalid_inclusion_proof());
        }

        // Success
        log::emit(SignalVerified::new(sender, value));
    }
}

fn derive_key(value: B32, account: Address) -> B32 {
    // namespace = abi.encodePacked(value, account)
    let namespace = alloy_sol_types::SolValue::abi_encode_packed(&(value, account));

    // slot = keccak256( (keccak256(namespace) - 1) ) & ~0xff
    let namespace_hash = alloy_keccak256(&namespace);
    let word = U256::from_be_slice(namespace_hash.as_slice());
    let (minus_one, _) = word.overflowing_sub(U256::from(1u8));
    let buf = minus_one.to_be_bytes::<32>();
    let slot_bytes = alloy_keccak256(&buf);

    // mask out the lowest 8 bits (ERC-7201)
    let mut arr = [0u8; 32];
    arr.copy_from_slice(slot_bytes.as_slice());
    let last = arr.len() - 1;
    arr[last] = 0u8;
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
    // keccak256("INVALID_ACCOUNT_PROOF()")[:4]
    let sig = b"INVALID_ACCOUNT_PROOF()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

fn selector_invalid_inclusion_proof() -> [u8; 4] {
    // keccak256("INVALID_INCLUSION_PROOF()")[:4]
    let sig = b"INVALID_INCLUSION_PROOF()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}
