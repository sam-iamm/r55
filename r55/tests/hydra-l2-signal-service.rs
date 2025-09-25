//! # SignalService Tests (R55/RISC-V)
//!
//! Purpose: Validate the standalone semantics of the `SignalService` contract.
//!
//! Covered behavior:
//! - `sendSignal(bytes32)` stores a sender-specific flag and emits `SignalSent`.
//! - `isSignalStored(bytes32,address)` returns whether `(sender,value)` was signaled.
//! - `verifySignal(address,bytes32,bytes)` gates by publisher-signed state root and
//!   verifies Merkle-Patricia-Trie (MPT) proofs for the storage slot derived from
//!   `(value,sender)` (ERC-7201 namespacing).
//!
//! Proofs built in tests:
//! - Negative gates: missing publisher root, non-publisher root, empty accountProof,
//!   ABI decode failure, and invalid account proof.
//! - Positive path: we construct the storage trie (slot -> RLP(1)) and the account
//!   trie (account key -> RLP(nonce,balance,storageRoot,codeHash)), compute the
//!   state root, and ABI-encode the proofs expected by the contract.
//!
//! Structure:
//! - Minimal RLP helpers (just enough to encode account/storage leaf values)
//! - Setup fixture that deploys `SignalService`
//! - sendSignal, isSignalStored tests
//! - verifySignal negative matrix + positive MPT proof
//! - Cross-function checks
//!
//! Integration note:
//! These tests focus on SignalService correctness. Bridge integration tests live
//! in `r55/tests/hydra-l2-bridge.rs` and only rely on `verifySignal` behavior
//! at the API surface.

use alloy_primitives::{Address, FixedBytes, B256, U256, keccak256, Bytes, hex};
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{add_balance_to_db, initialize_logger, ALICE, BOB, CAROL},
};
use alloy_sol_types::{sol, SolCall, SolValue};
use revm::InMemoryDB;
use tracing::info;
use alloy_trie::HashBuilder;
use alloy_trie::proof::ProofRetainer;
use nybbles::Nibbles;
// --- RLP helpers (minimal) ---
// We only encode the subset needed for account/storage leaf values.
fn rlp_bytes(input: &[u8]) -> Vec<u8> {
    if input.len() == 1 && input[0] < 0x80 { return vec![input[0]]; }
    let mut out = Vec::with_capacity(1 + input.len());
    out.push(0x80 + (input.len() as u8));
    out.extend_from_slice(input);
    out
}

fn rlp_uint_zero() -> Vec<u8> { vec![0x80] }

fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len: usize = items.iter().map(|v| v.len()).sum();
    let mut out = Vec::with_capacity(2 + payload_len);
    if payload_len <= 55 {
        out.push(0xc0 + (payload_len as u8));
    } else {
        // long form with 1-byte length (payload < 256)
        out.push(0xf7 + 1);
        out.push(payload_len as u8);
    }
    for v in items { out.extend_from_slice(v); }
    out
}

// Test suite for SignalService (examples/hydra-l2-signal-service/src/lib.rs)
// Organization:
// - Setup helpers
// - sendSignal tests
// - isSignalStored tests
// - verifySignal tests (negative boundary + happy path)
// - Cross-function behavior

sol! {
    function sendSignal(bytes32 value) returns (bytes32);
    function isSignalStored(bytes32 value, address sender) returns (bool);
    function verifySignal(address sender, bytes32 value, bytes proof);
}

struct SignalServiceSetup {
    db: InMemoryDB,
    contract: Address,
    state_root_publisher: Address,
}

fn signal_service_setup() -> SignalServiceSetup {
    initialize_logger();
    let mut db = InMemoryDB::default();

    // Fund user accounts with some ETH
    for user in [ALICE, BOB, CAROL] {
        add_balance_to_db(&mut db, user, 1e18 as u64);
    }

    // Create mock state root publisher
    let state_root_publisher = Address::from([0x33; 20]);
    // Fund publisher as well (calls may originate from it)
    add_balance_to_db(&mut db, state_root_publisher, 1e18 as u64);

    // Deploy SignalService contract with constructor parameters
    let bytecode = get_bytecode("hydra_l2_signal_service");
    // Pass both required constructor args: (state_root_publisher, this_address)
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let constructor_args = (state_root_publisher, this_address).abi_encode();
    let contract = deploy_contract(&mut db, bytecode, Some(constructor_args)).unwrap();

    SignalServiceSetup {
        db,
        contract,
        state_root_publisher,
    }
}

// === Function: sendSignal(bytes32) ===
// Happy-path: returns derived slot and emits SignalSent
#[test]
fn test_send_signal_basic() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Test sendSignal function
    let value = FixedBytes::<32>::from([0x42; 32]);
    let call = sendSignalCall { value };
    let result = run_tx(&mut db, &contract, call.abi_encode(), &ALICE).unwrap();
    
    assert!(result.status);
    let slot: FixedBytes<32> = FixedBytes::<32>::abi_decode(&result.output, true).unwrap();
    
    // Verify the slot is not zero
    assert_ne!(slot, FixedBytes::<32>::ZERO, "Signal slot should not be zero");
    
    info!("Signal sent with slot: {:?}", slot);
}

// Compute the same storage slot derivation used by the contract
fn derive_key_test(value: FixedBytes<32>, account: Address) -> FixedBytes<32> {
    use alloy_sol_types::SolValue;
    // namespace = abi.encodePacked(value, account)
    let namespace = (value, account).abi_encode_packed();

    // slot = keccak256( (keccak256(namespace) - 1) ) & ~0xff
    let namespace_hash = keccak256(&namespace);
    let word = U256::from_be_bytes(namespace_hash.0);
    let (minus_one, _) = word.overflowing_sub(U256::from(1u8));
    let buf = minus_one.to_be_bytes::<32>();
    let slot_bytes = keccak256(&buf);

    // mask out the lowest 8 bits (ERC-7201 namespace alignment)
    let mut arr = [0u8; 32];
    arr.copy_from_slice(slot_bytes.as_slice());
    let last = arr.len() - 1;
    arr[last] = 0u8;
    FixedBytes::<32>::from(arr)
}

// Returns slot parity with contract's derive_key; validates SignalSent topics/data
#[test]
fn test_send_signal_event_and_slot() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Arrange
    let value = FixedBytes::<32>::from([0xAA; 32]);
    let sender = ALICE;

    // Act
    let call = sendSignalCall { value };
    let result = run_tx(&mut db, &contract, call.abi_encode(), &sender).unwrap();

    // Assert returned slot equals derived slot formula
    let returned_slot: FixedBytes<32> = FixedBytes::<32>::abi_decode(&result.output, true).unwrap();
    let expected_slot = derive_key_test(value, sender);
    assert_eq!(returned_slot, expected_slot, "slot derivation must match");

    // Assert event SignalSent(address,bytes32)
    // topic0 = keccak256("SignalSent(address,bytes32)")
    let topic0 = B256::from(keccak256(b"SignalSent(address,bytes32)"));
    let logs = result.logs;
    assert_eq!(logs.len(), 1, "expected a single SignalSent log");
    assert_eq!(logs[0].topics()[0], topic0, "event signature mismatch");
    // indexed sender in topic1
    let mut padded = [0u8; 32];
    padded[12..].copy_from_slice(sender.as_slice());
    assert_eq!(logs[0].topics()[1], B256::from_slice(&padded), "indexed sender mismatch");
    // value (non-indexed) in data
    assert_eq!(logs[0].data.data.as_ref(), value.as_slice(), "value in data mismatch");
}

// === Function: isSignalStored(bytes32,address) ===
// Happy-path and negative for different sender
#[test]
fn test_is_signal_stored_basic() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // First send a signal
    let value = FixedBytes::<32>::from([0x42; 32]);
    let send_call = sendSignalCall { value };
    let send_result = run_tx(&mut db, &contract, send_call.abi_encode(), &ALICE).unwrap();
    assert!(send_result.status);
    
    // Now check if the signal is stored
    let check_call = isSignalStoredCall {
        value,
        sender: ALICE,
    };
    let check_result = run_tx(&mut db, &contract, check_call.abi_encode(), &BOB).unwrap();
    
    assert!(check_result.status);
    let is_stored: bool = bool::abi_decode(&check_result.output, true).unwrap();
    assert!(is_stored, "Signal should be stored after sending");
    
    // Check with different sender - should return false
    let check_call2 = isSignalStoredCall {
        value,
        sender: BOB,
    };
    let check_result2 = run_tx(&mut db, &contract, check_call2.abi_encode(), &CAROL).unwrap();
    
    assert!(check_result2.status);
    let is_stored2: bool = bool::abi_decode(&check_result2.output, true).unwrap();
    assert!(!is_stored2, "Signal should not be stored for different sender");
}

// === Function: verifySignal(address,bytes32,bytes) ===
// Negative: ABI-decodable proof with non-empty accountProof, but the state root was
// not signaled by the publisher -> must revert with StateRootNotFound.
#[test]
fn test_verify_signal_missing_publisher_gates() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Prepare decodable proof with non-empty accountProof
    let state_root = FixedBytes::<32>::from([0xAA; 32]);
    let account_proof: Vec<Bytes> = vec![Bytes::from_static(b"x")];
    let storage_proof: Vec<Bytes> = vec![];
    use alloy_sol_types::SolValue;
    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();

    let call = verifySignalCall { sender: ALICE, value: FixedBytes::<32>::from([0x42; 32]), proof: proof_bytes.into() };
    let err = run_tx(&mut db, &contract, call.abi_encode(), &BOB).expect_err("should revert: publisher did not signal state root");
    assert!(err.matches_custom_error("StateRootNotFound()"));
}

// === Cross-function behavior ===
// Multiple independent signals across different senders
#[test]
fn test_cross_multiple_signals() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Send multiple signals from different senders
    let values = [
        FixedBytes::<32>::from([0x01; 32]),
        FixedBytes::<32>::from([0x02; 32]),
        FixedBytes::<32>::from([0x03; 32]),
    ];
    
    let senders = [ALICE, BOB, CAROL];
    
    for (i, (value, sender)) in values.iter().zip(senders.iter()).enumerate() {
        let call = sendSignalCall { value: *value };
        let result = run_tx(&mut db, &contract, call.abi_encode(), sender).unwrap();
        assert!(result.status);
        
        // Verify the signal is stored
        let check_call = isSignalStoredCall {
            value: *value,
            sender: *sender,
        };
        let check_result = run_tx(&mut db, &contract, check_call.abi_encode(), &ALICE).unwrap();
        assert!(check_result.status);
        
        let is_stored: bool = bool::abi_decode(&check_result.output, true).unwrap();
        assert!(is_stored, "Signal {} should be stored", i);
    }
}

// verifySignal must require the state root to have been signaled by the PUBLISHER, not any address
#[test]
fn test_verify_signal_non_publisher_state_root_fails() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Non-publisher (ALICE) signals the state root
    let state_root = FixedBytes::<32>::from([0xAB; 32]);
    run_tx(&mut db, &contract, sendSignalCall { value: state_root }.abi_encode(), &ALICE).unwrap();

    // Build a valid-looking proof referencing that state root
    use alloy_sol_types::SolValue;
    let account_proof: Vec<Bytes> = vec![Bytes::from_static(b"ok")];
    let storage_proof: Vec<Bytes> = vec![];
    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();

    // Should still revert because publisher didn't signal this state root
    let call = verifySignalCall { sender: BOB, value: FixedBytes::<32>::from([0xCD; 32]), proof: proof_bytes.into() };
    let err = run_tx(&mut db, &contract, call.abi_encode(), &CAROL).expect_err("non-publisher state root should fail");
    assert!(err.matches_custom_error("StateRootNotFound()"));
}

// For the same sender, different values must map to different slots and both be stored
#[test]
fn test_send_signal_distinct_values_slots() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;
    let sender = ALICE;

    let v1 = FixedBytes::<32>::from([0x01; 32]);
    let v2 = FixedBytes::<32>::from([0x02; 32]);

    let r1 = run_tx(&mut db, &contract, sendSignalCall { value: v1 }.abi_encode(), &sender).unwrap();
    let r2 = run_tx(&mut db, &contract, sendSignalCall { value: v2 }.abi_encode(), &sender).unwrap();

    let s1: FixedBytes<32> = FixedBytes::<32>::abi_decode(&r1.output, true).unwrap();
    let s2: FixedBytes<32> = FixedBytes::<32>::abi_decode(&r2.output, true).unwrap();
    assert_ne!(s1, s2, "distinct values must yield distinct storage slots");

    // Check both stored flags are true
    let c1 = isSignalStoredCall { value: v1, sender };
    let c2 = isSignalStoredCall { value: v2, sender };
    let o1: bool = bool::abi_decode(&run_tx(&mut db, &contract, c1.abi_encode(), &sender).unwrap().output, true).unwrap();
    let o2: bool = bool::abi_decode(&run_tx(&mut db, &contract, c2.abi_encode(), &sender).unwrap().output, true).unwrap();
    assert!(o1 && o2);
}

// Zero bytes32 input should be accepted and stored
#[test]
fn test_send_signal_zero_value() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    let zero = FixedBytes::<32>::ZERO;
    run_tx(&mut db, &contract, sendSignalCall { value: zero }.abi_encode(), &ALICE).unwrap();
    let check = isSignalStoredCall { value: zero, sender: ALICE };
    let stored: bool = bool::abi_decode(&run_tx(&mut db, &contract, check.abi_encode(), &ALICE).unwrap().output, true).unwrap();
    assert!(stored);
}

// Negative: verifySignal must revert on ABI-decode failure of the proof blob
#[test]
fn test_verify_signal_decode_failure() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Garbage bytes (not ABI of (bytes[],bytes[],bytes32))
    let bad_proof = Bytes::from_static(b"not-an-abi-encoded-proof");
    let call = verifySignalCall { sender: ALICE, value: FixedBytes::<32>::from([0xEE; 32]), proof: bad_proof };
    let err = run_tx(&mut db, &contract, call.abi_encode(), &BOB).expect_err("ABI decode failure should revert");
    // Generic revert with empty data
    assert!(err.matches_string_error(""));
}
#[test]
fn test_verify_signal_state_root_not_found() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Create proof with non-empty accountProof but missing state root signal
    let state_root = FixedBytes::<32>::from([0x55; 32]);
    let account_proof: Vec<Bytes> = vec![Bytes::from(b"ap".as_slice())];
    let storage_proof: Vec<Bytes> = vec![];
    use alloy_sol_types::SolValue;
    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();

    let call = verifySignalCall { sender: ALICE, value: FixedBytes::<32>::from([0x11; 32]), proof: proof_bytes.into() };
    let err = run_tx(&mut db, &contract, call.abi_encode(), &BOB).expect_err("should revert: state root not signaled");

    assert!(err.matches_custom_error("StateRootNotFound()"));
}

#[test]
fn test_verify_signal_account_proof_empty() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;
    let publisher = setup.state_root_publisher;

    // First signal the state root from the publisher
    let state_root = FixedBytes::<32>::from([0x66; 32]);
    let send = sendSignalCall { value: state_root };
    run_tx(&mut db, &contract, send.abi_encode(), &publisher).unwrap();

    // Build proof with empty accountProof
    let account_proof: Vec<Bytes> = vec![];
    let storage_proof: Vec<Bytes> = vec![];
    use alloy_sol_types::SolValue;
    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();

    let call = verifySignalCall { sender: ALICE, value: FixedBytes::<32>::from([0x22; 32]), proof: proof_bytes.into() };
    let err = run_tx(&mut db, &contract, call.abi_encode(), &BOB).expect_err("should revert: empty accountProof");
    assert!(err.matches_custom_error("AccountProofEmpty()"));
}

// With real verification enabled, a dummy proof must fail with INVALID_ACCOUNT_PROOF()
#[test]
fn test_verify_signal_invalid_account_proof() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;
    let publisher = setup.state_root_publisher;

    // State root signaled by publisher
    let state_root = FixedBytes::<32>::from([0x77; 32]);
    let _ = run_tx(&mut db, &contract, sendSignalCall { value: state_root }.abi_encode(), &publisher).unwrap();

    // Non-empty accountProof -> passes basic checks
    let account_proof: Vec<Bytes> = vec![Bytes::from_static(b"proof")];
    let storage_proof: Vec<Bytes> = vec![];
    use alloy_sol_types::SolValue;
    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();

    let sender = BOB;
    let signal_value = FixedBytes::<32>::from([0x33; 32]);
    let call = verifySignalCall { sender, value: signal_value, proof: proof_bytes.into() };
    let err = run_tx(&mut db, &contract, call.abi_encode(), &CAROL).expect_err("should fail with invalid account proof");
    assert!(err.matches_custom_error("INVALID_ACCOUNT_PROOF()"));
}

// Positive MPT path: build account/storage tries matching the contract’s expectations.
// Steps:
// 1) Compute the storage slot for (sender,value) per ERC-7201.
// 2) Build storage trie leaf: keccak(slot) -> RLP(1).
// 3) Build account trie leaf: keccak(this_address) -> RLP(nonce,balance,storageRoot,codeHash).
// 4) Publisher signals the resulting state root via sendSignal.
// 5) ABI-encode (accountProof, storageProof, stateRoot) and call verifySignal.
#[test]
fn test_verify_signal_positive_mpt_proof() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;
    let publisher = setup.state_root_publisher;

    // Sender/value pair we will prove
    let sender = ALICE;
    let value = FixedBytes::<32>::from([0x5A; 32]);

    // Compute derived slot and hashed storage key
    let slot = derive_key_test(value, sender);
    let storage_key_hashed = B256::from(keccak256(slot.as_slice()));

    // Build storage trie: key -> RLP(1)
    let mut storage_builder = HashBuilder::default().with_proof_retainer(ProofRetainer::from_iter([Nibbles::unpack(storage_key_hashed.as_slice())]));
    let rlp_one = alloy_rlp::encode(U256::from(1u64));
    storage_builder.add_leaf(Nibbles::unpack(storage_key_hashed.as_slice()), &rlp_one);
    let storage_root = storage_builder.root();
    let storage_proof_nodes = storage_builder.take_proof_nodes().into_nodes_sorted();
    let storage_proof: Vec<Bytes> = storage_proof_nodes.into_iter().map(|(_, n)| Bytes::from(n.to_vec())).collect();

    // Build account trie: key keccak(this_address) -> RLP(nonce,balance,storage_root,code_hash)
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let account_key = B256::from(keccak256(this_address.as_slice()));
    // Manually RLP-encode (nonce, balance, storageRoot, codeHash)
    let account_value_rlp = rlp_list(&[
        rlp_uint_zero(),
        rlp_uint_zero(),
        rlp_bytes(storage_root.as_slice()),
        rlp_bytes([0u8;32].as_slice()),
    ]);
    let mut account_builder = HashBuilder::default().with_proof_retainer(ProofRetainer::from_iter([Nibbles::unpack(account_key.as_slice())]));
    account_builder.add_leaf(Nibbles::unpack(account_key.as_slice()), &account_value_rlp);
    let state_root = account_builder.root();
    let account_proof_nodes = account_builder.take_proof_nodes().into_nodes_sorted();
    let account_proof: Vec<Bytes> = account_proof_nodes.into_iter().map(|(_, n)| Bytes::from(n.to_vec())).collect();

    // Publisher signals the state root
    let _ = run_tx(&mut db, &contract, sendSignalCall { value: FixedBytes::<32>::from(state_root.0) }.abi_encode(), &publisher).unwrap();

    // verifySignal expects ABI((bytes[] accountProof, bytes[] storageProof, bytes32 stateRoot))
    use alloy_sol_types::SolValue;
    let proof_bytes = (account_proof, storage_proof, FixedBytes::<32>::from(state_root.0)).abi_encode();
    let call = verifySignalCall { sender, value, proof: proof_bytes.into() };
    let res = run_tx(&mut db, &contract, call.abi_encode(), &BOB).unwrap();

    // Check SignalVerified event
    let topic0 = B256::from(keccak256(b"SignalVerified(address,bytes32)"));
    assert_eq!(res.logs.len(), 1);
    assert_eq!(res.logs[0].topics()[0], topic0);
    let mut padded = [0u8; 32];
    padded[12..].copy_from_slice(sender.as_slice());
    assert_eq!(res.logs[0].topics()[1], B256::from_slice(&padded));
    assert_eq!(res.logs[0].data.data.as_ref(), value.as_slice());
}

// Negative storage inclusion: wrong storage value (expect RLP(1), provide RLP(0))
#[test]
fn test_verify_signal_invalid_storage_inclusion() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;
    let publisher = setup.state_root_publisher;

    // Sender/value pair we will attempt to prove incorrectly
    let sender = ALICE;
    let value = FixedBytes::<32>::from([0x6A; 32]);

    // Compute derived slot and hashed storage key
    let slot = derive_key_test(value, sender);
    let storage_key_hashed = B256::from(keccak256(slot.as_slice()));

    // Build storage trie: key -> RLP(0) (incorrect)
    let mut storage_builder = HashBuilder::default().with_proof_retainer(ProofRetainer::from_iter([Nibbles::unpack(storage_key_hashed.as_slice())]));
    let rlp_zero = alloy_rlp::encode(U256::from(0u64));
    storage_builder.add_leaf(Nibbles::unpack(storage_key_hashed.as_slice()), &rlp_zero);
    let storage_root = storage_builder.root();
    let storage_proof_nodes = storage_builder.take_proof_nodes().into_nodes_sorted();
    let storage_proof: Vec<Bytes> = storage_proof_nodes.into_iter().map(|(_, n)| Bytes::from(n.to_vec())).collect();

    // Build account trie: key keccak(this_address) -> RLP(nonce,balance,storage_root,code_hash)
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let account_key = B256::from(keccak256(this_address.as_slice()));
    let account_value_rlp = rlp_list(&[
        rlp_uint_zero(),
        rlp_uint_zero(),
        rlp_bytes(storage_root.as_slice()),
        rlp_bytes([0u8;32].as_slice()),
    ]);
    let mut account_builder = HashBuilder::default().with_proof_retainer(ProofRetainer::from_iter([Nibbles::unpack(account_key.as_slice())]));
    account_builder.add_leaf(Nibbles::unpack(account_key.as_slice()), &account_value_rlp);
    let state_root = account_builder.root();
    let account_proof_nodes = account_builder.take_proof_nodes().into_nodes_sorted();
    let account_proof: Vec<Bytes> = account_proof_nodes.into_iter().map(|(_, n)| Bytes::from(n.to_vec())).collect();

    // Publisher signals the state root
    let _ = run_tx(&mut db, &contract, sendSignalCall { value: FixedBytes::<32>::from(state_root.0) }.abi_encode(), &publisher).unwrap();

    // Call verifySignal: must revert with INVALID_INCLUSION_PROOF()
    use alloy_sol_types::SolValue;
    let proof_bytes = (account_proof, storage_proof, FixedBytes::<32>::from(state_root.0)).abi_encode();
    let call = verifySignalCall { sender, value, proof: proof_bytes.into() };
    let err = run_tx(&mut db, &contract, call.abi_encode(), &BOB).expect_err("invalid storage inclusion should revert");
    assert!(err.matches_custom_error("INVALID_INCLUSION_PROOF()"));
}

#[test]
fn test_send_signal_idempotent() {
    let setup = signal_service_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    let value = FixedBytes::<32>::from([0x99; 32]);
    let sender = ALICE;

    // First send
    let _ = run_tx(&mut db, &contract, sendSignalCall { value }.abi_encode(), &sender).unwrap();
    // Second send with same value
    let _ = run_tx(&mut db, &contract, sendSignalCall { value }.abi_encode(), &sender).unwrap();

    // Check stored flag still true
    let check_call = isSignalStoredCall { value, sender };
    let result = run_tx(&mut db, &contract, check_call.abi_encode(), &sender).unwrap();
    let is_stored: bool = bool::abi_decode(&result.output, true).unwrap();
    assert!(is_stored);
}
