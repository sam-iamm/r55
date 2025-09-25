//! # ETHBridge Tests (R55/RISC-V)
//!
//! Contract behavior under test:
//! - processed(bytes32)
//! - getDepositId(ETHDeposit)
//! - deposit(address,bytes,bytes,address)
//! - claimDeposit(ETHDeposit,bytes)
//!
//! Note: Proof validity and detailed trie verification are covered in
//! `r55/tests/hydra-l2-signal-service.rs`. Here we integrate just enough
//! to exercise ETHBridge flows and reverts.
use alloy_primitives::{keccak256, Address, Bytes, FixedBytes, U256, B256, hex};
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{add_balance_to_db, initialize_logger, ALICE, BOB, CAROL},
};
use alloy_sol_types::{sol, SolCall, SolValue};
use revm::{InMemoryDB, Database};
use tracing::info;
use alloy_trie::HashBuilder;
use alloy_trie::proof::ProofRetainer;
use nybbles::Nibbles;

sol! {
    struct ETHDeposit {
        uint256 nonce;
        address from;
        address to;
        uint256 amount;
        bytes data;
        bytes context;
        address canceler;
    }

    function processed(bytes32 id) returns (bool);
    function getDepositId(ETHDeposit ethDeposit) returns (bytes32);
    function deposit(address to, bytes data, bytes context, address canceler) returns (bytes32);
    function claimDeposit(ETHDeposit ethDeposit, bytes proof);
    function cancelDeposit(ETHDeposit ethDeposit, address claimee, bytes proof);
}

/// Minimal fixture for tests that deploy a bridge wired to a dummy
/// SignalService and counterpart.
struct ETHBridgeSetup {
    db: InMemoryDB,
    contract: Address,
    _signal_service: Address,
    _counterpart: Address,
}

fn eth_bridge_setup() -> ETHBridgeSetup {
    initialize_logger();
    let mut db = InMemoryDB::default();

    // Fund user accounts with some ETH
    for user in [ALICE, BOB, CAROL] {
        add_balance_to_db(&mut db, user, 1e18 as u64);
    }

    // Dummy addresses for constructor wiring
    let signal_service = Address::from([0x11; 20]);
    let counterpart = Address::from([0x22; 20]);

    // Deploy ETHBridge contract with constructor parameters
    let bytecode = get_bytecode("hydra_l2_bridge");
    let constructor_args = (signal_service, counterpart).abi_encode();
    let contract = deploy_contract(&mut db, bytecode, Some(constructor_args)).unwrap();

    ETHBridgeSetup {
        db,
        contract,
        _signal_service: signal_service,
        _counterpart: counterpart,
    }
}

/// Constructor must revert if either signal_service or counterpart is zero address
#[test]
fn test_eth_bridge_constructor_reverts_on_zero_addresses() {
    initialize_logger();
    let mut db = InMemoryDB::default();
    for user in [ALICE] { add_balance_to_db(&mut db, user, 1e18 as u64); }

    let bytecode = get_bytecode("hydra_l2_bridge");

    // Zero signal_service
    let args1 = (Address::ZERO, Address::from([0x22; 20])).abi_encode();
    let err1 = deploy_contract(&mut db, bytecode.clone(), Some(args1)).expect_err("constructor should revert on zero signal_service");
    assert!(err1.matches_string_error(""));

    // Zero counterpart
    let args2 = (Address::from([0x11; 20]), Address::ZERO).abi_encode();
    let err2 = deploy_contract(&mut db, bytecode.clone(), Some(args2)).expect_err("constructor should revert on zero counterpart");
    assert!(err2.matches_string_error(""));
}

/// processed(bytes32) returns false for ids not seen/claimed.
#[test]
fn test_eth_bridge_processed() {
    let setup = eth_bridge_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Test processed() with a random ID - should return false
    let test_id = FixedBytes::<32>::from([0x42; 32]);
    let call = processedCall { id: test_id };
    let result = run_tx(&mut db, &contract, call.abi_encode(), &ALICE).unwrap();
    
    assert!(result.status);
    let decoded: bool = bool::abi_decode(&result.output, true).unwrap();
    assert!(!decoded, "New deposit should not be processed");
}

/// getDepositId encodes the struct as a tuple and hashes it (deterministic).
#[test]
fn test_eth_bridge_get_deposit_id() {
    let setup = eth_bridge_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Create a test ETHDeposit
    let eth_deposit = ETHDeposit {
        nonce: U256::from(1u64),
        from: ALICE,
        to: BOB,
        amount: U256::from(1000u64),
        data: Bytes::from(b"test data"),
        context: Bytes::from(b"context"),
        canceler: CAROL,
    };

    let call = getDepositIdCall { ethDeposit: eth_deposit.clone() };
    println!("call.abi_encode.hex = 0x{}", hex::encode(call.abi_encode()));
    let result = run_tx(&mut db, &contract, call.abi_encode(), &ALICE).unwrap();
    
    assert!(result.status);
    let deposit_id: FixedBytes<32> = FixedBytes::<32>::abi_decode(&result.output, true).unwrap();
    
    // Verify the ID is deterministic (same input should produce same output)
    let call2 = getDepositIdCall { ethDeposit: eth_deposit };
    let result2 = run_tx(&mut db, &contract, call2.abi_encode(), &ALICE).unwrap();
    let deposit_id2: FixedBytes<32> = FixedBytes::<32>::abi_decode(&result2.output, true).unwrap();
    
    assert_eq!(deposit_id, deposit_id2, "Deposit ID should be deterministic");
}

/// deposit returns a nonzero id and emits DepositMade; nonce increases.
#[test]
fn test_eth_bridge_deposit() {
    let setup = eth_bridge_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    // Test deposit function
    let to = BOB;
    let data = Bytes::from(b"deposit data");
    let context = Bytes::from(b"deposit context");
    let canceler = CAROL;

    let call = depositCall {
        to,
        data: data.clone(),
        context: context.clone(),
        canceler,
    };
    
    let result = run_tx(&mut db, &contract, call.abi_encode(), &ALICE).unwrap();
    
    assert!(result.status);
    let deposit_id: FixedBytes<32> = FixedBytes::<32>::abi_decode(&result.output, true).unwrap();
    
    // Verify the deposit ID is not zero
    assert_ne!(deposit_id, FixedBytes::<32>::ZERO, "Deposit ID should not be zero");
    
    info!("Deposit created with ID: {:?}", deposit_id);

    // Event check: DepositMade(bytes32 indexed id, ETHDeposit deposit)
    // - topic0: keccak256("DepositMade(bytes32,(uint256,address,address,uint256,bytes,bytes,address))")
    // - topic1: id (indexed)
    // - data: abi.encode(ETHDeposit tuple) in Solidity order
    let topic0 = B256::from(keccak256(b"DepositMade(bytes32,(uint256,address,address,uint256,bytes,bytes,address))"));
    let lg = result
        .logs
        .iter()
        .find(|l| l.topics()[0] == topic0)
        .expect("missing DepositMade event");
    assert_eq!(lg.topics()[1], B256::from_slice(deposit_id.as_slice()));
    let expected_tuple = (
        U256::from(0u64), // initial nonce before ++ in contract
        ALICE,
        to,
        U256::from(0u64), // msg_value() is zero in tests
        data.clone(),
        context.clone(),
        canceler,
    );
    let expected_encoded = expected_tuple.abi_encode();
    assert_eq!(lg.data.data.as_ref(), expected_encoded.as_slice(), "DepositMade data must equal abi.encode(ETHDeposit)");
}

// --- Minimal MPT helpers to build a proof matching SignalService expectations ---
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
        out.push(0xf7 + 1);
        out.push(payload_len as u8);
    }
    for v in items { out.extend_from_slice(v); }
    out
}

fn derive_key(value: FixedBytes<32>, account: Address) -> FixedBytes<32> {
    // namespace = abi.encodePacked(value, account)
    let namespace = (value, account).abi_encode_packed();
    // slot = keccak256( (keccak256(namespace) - 1) ) & ~0xff
    let namespace_hash = keccak256(&namespace);
    let word = U256::from_be_bytes(namespace_hash.0);
    let (minus_one, _) = word.overflowing_sub(U256::from(1u8));
    let buf = minus_one.to_be_bytes::<32>();
    let slot_bytes = keccak256(&buf);
    let mut arr = [0u8; 32];
    arr.copy_from_slice(slot_bytes.as_slice());
    arr[31] = 0u8;
    FixedBytes::<32>::from(arr)
}

fn build_signal_proof(
    this_address: Address,
    sender: Address,
    value: FixedBytes<32>,
) -> (Vec<Bytes>, Vec<Bytes>, FixedBytes<32>) {
    // Build storage trie proving slot -> RLP(1)
    let slot = derive_key(value, sender);
    let storage_key_hashed = B256::from(keccak256(slot.as_slice()));
    let mut storage_builder = HashBuilder::default()
        .with_proof_retainer(ProofRetainer::from_iter([Nibbles::unpack(storage_key_hashed.as_slice())]));
    let rlp_one = alloy_rlp::encode(U256::from(1u64));
    storage_builder.add_leaf(Nibbles::unpack(storage_key_hashed.as_slice()), &rlp_one);
    let storage_root = storage_builder.root();
    let storage_nodes = storage_builder.take_proof_nodes().into_nodes_sorted();
    let storage_proof: Vec<Bytes> = storage_nodes.into_iter().map(|(_, n)| Bytes::from(n.to_vec())).collect();

    // Build account trie for `this_address` with the storage_root above
    let account_key = B256::from(keccak256(this_address.as_slice()));
    let account_value_rlp = rlp_list(&[
        rlp_uint_zero(),
        rlp_uint_zero(),
        rlp_bytes(storage_root.as_slice()),
        rlp_bytes([0u8; 32].as_slice()),
    ]);
    let mut account_builder = HashBuilder::default()
        .with_proof_retainer(ProofRetainer::from_iter([Nibbles::unpack(account_key.as_slice())]));
    account_builder.add_leaf(Nibbles::unpack(account_key.as_slice()), &account_value_rlp);
    let state_root = account_builder.root();
    let account_nodes = account_builder.take_proof_nodes().into_nodes_sorted();
    let account_proof: Vec<Bytes> = account_nodes.into_iter().map(|(_, n)| Bytes::from(n.to_vec())).collect();

    (account_proof, storage_proof, FixedBytes::<32>::from(state_root.0))
}

// (debug helpers removed; eth bridge now tested via assertions only)

// Happy path: claimDeposit sends value to receiver after proof verification
/// Happy path: verifySignal succeeds, processed flag set, DepositClaimed emitted, value transferred.
#[test]
fn test_eth_bridge_claim_deposit_happy_path() {
    initialize_logger();
    let mut db = InMemoryDB::default();

    // Fund actors
    for user in [ALICE, BOB, CAROL] { add_balance_to_db(&mut db, user, 1e18 as u64); }

    // Deploy SignalService with (publisher, this_address)
    let publisher = Address::from([0x44; 20]);
    let bytecode_ss = get_bytecode("hydra_l2_signal_service");
    // r55 deploys contracts deterministically to 0xf6a1...; use same constant as other tests
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let ctor_ss = (publisher, this_address).abi_encode();
    add_balance_to_db(&mut db, publisher, 1e18 as u64);
    let signal_service = deploy_contract(&mut db, bytecode_ss, Some(ctor_ss)).unwrap();
    // Verify deployed SignalService runtime is R55
    {
        let acct = db.basic(signal_service).unwrap().expect("signal service missing");
        let code = acct.code.expect("signal service has no code");
        let bytes = code.bytecode().as_ref();
        println!("signal_service code head: 0x{}", hex::encode(&bytes[..bytes.len().min(16)]));
        assert_eq!(bytes[0], 0xff, "signal service runtime is not R55 (0xff)");
    }

    // Bridge counterpart (remote sender used in proof derivation)
    let counterpart = Address::from([0x22; 20]);

    // Prepare deposit tuple and its id
    let amount = U256::from(10_000u64);
    let eth_deposit = ETHDeposit {
        nonce: U256::from(1u64),
        from: ALICE,
        // Avoid precompile range 0x01..0x13; choose non-precompile EOA address
        to: Address::from([0x33; 20]),
        amount,
        data: Bytes::from_static(b"hello"),
        context: Bytes::from_static(b"ctx"),
        canceler: CAROL,
    };
    // Compute expected id via contract helper to avoid drift
    let bytecode_bridge = get_bytecode("hydra_l2_bridge");
    let ctor_bridge = (signal_service, counterpart).abi_encode();
    let bridge = deploy_contract(&mut db, bytecode_bridge, Some(ctor_bridge)).unwrap();
    // Verify deployed Bridge runtime is R55
    {
        let acct = db.basic(bridge).unwrap().expect("bridge missing");
        let code = acct.code.expect("bridge has no code");
        let bytes = code.bytecode().as_ref();
        println!("bridge code head: 0x{}", hex::encode(&bytes[..bytes.len().min(16)]));
        assert_eq!(bytes[0], 0xff, "bridge runtime is not R55 (0xff)");
    }

    // Fund bridge so it can forward ETH without overwriting code
    use r55::test_utils::add_balance_preserve_code;
    add_balance_preserve_code(&mut db, bridge, 1_000_000);

    // (No debug calls)

    // Compute deposit id locally (keccak256(abi.encode(ETHDeposit)))
    let deposit_id: FixedBytes<32> = {
        let t = (
            eth_deposit.nonce,
            eth_deposit.from,
            eth_deposit.to,
            eth_deposit.amount,
            eth_deposit.data.clone(),
            eth_deposit.context.clone(),
            eth_deposit.canceler,
        );
        FixedBytes::<32>::from(keccak256(&t.abi_encode()))
    };

    // Build a valid proof for (sender=counterpart, value=deposit_id) against a synthetic state_root
    let (account_proof, storage_proof, state_root) = build_signal_proof(this_address, counterpart, deposit_id);

    // Publisher signals the state root on the SignalService (gate requirement)
    // Use sol! to encode sendSignal call
    sol! { function sendSignal(bytes32 value) returns (bytes32); }
    let send_signal_calldata = sendSignalCall { value: state_root }.abi_encode();
    println!("sendSignalCalldata.hex = 0x{}", hex::encode(send_signal_calldata.clone()));
    let send_signal = run_tx(&mut db, &signal_service, send_signal_calldata.clone(), &publisher).unwrap();
    println!("sendSignal: {:?}", send_signal);

    // Encode proof blob as (bytes[],bytes[],bytes32)
    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();

    // Cross-check via getDepositId
    let via_contract_id = {
        let call = getDepositIdCall { ethDeposit: eth_deposit.clone() };
        let res = run_tx(&mut db, &bridge, call.abi_encode(), &ALICE).unwrap();
        FixedBytes::<32>::abi_decode(&res.output, true).unwrap()
    };
    assert_eq!(via_contract_id, deposit_id, "getDepositId mismatch");

    // Record balances pre-claim
    use r55::test_utils::AccountInfo;
    let to_pre_balance = db
        .basic(eth_deposit.to)
        .unwrap()
        .unwrap_or(AccountInfo::from_balance(U256::from(0)))
        .balance;
    println!("to_pre_balance: {}", to_pre_balance);

    // Call claimDeposit
    let calldata = claimDepositCall { ethDeposit: eth_deposit.clone(), proof: proof_bytes.into() }.abi_encode();
    let result = run_tx(&mut db, &bridge, calldata, &ALICE).unwrap();
    assert!(result.status);
    println!("result: {:?}", result);
    // Event check: DepositClaimed(bytes32 indexed id, ETHDeposit deposit)
    // - topic0: keccak256("DepositClaimed(bytes32,(uint256,address,address,uint256,bytes,bytes,address))")
    // - topic1: id
    // - data: abi.encode(ETHDeposit)
    let claim_sig = B256::from(keccak256(b"DepositClaimed(bytes32,(uint256,address,address,uint256,bytes,bytes,address))"));
    let lg = result
        .logs
        .iter()
        .find(|l| l.topics()[0] == claim_sig)
        .expect("missing DepositClaimed event");
    assert_eq!(lg.topics()[1], B256::from_slice(deposit_id.as_slice()));
    let expected_claim_tuple = (
        eth_deposit.nonce,
        eth_deposit.from,
        eth_deposit.to,
        eth_deposit.amount,
        eth_deposit.data.clone(),
        eth_deposit.context.clone(),
        eth_deposit.canceler,
    );
    let expected_claim_encoded = expected_claim_tuple.abi_encode();
    assert_eq!(lg.data.data.as_ref(), expected_claim_encoded.as_slice(), "DepositClaimed data must equal abi.encode(ETHDeposit)");

    // Processed flag is set
    let processed_call = processedCall { id: deposit_id };
    let processed_res = run_tx(&mut db, &bridge, processed_call.abi_encode(), &ALICE).unwrap();
    let processed_flag: bool = bool::abi_decode(&processed_res.output, true).unwrap();
    assert!(processed_flag, "deposit must be marked processed");

    // Value transferred to `to`
    let to_post_balance = db
        .basic(eth_deposit.to)
        .unwrap()
        .unwrap_or(AccountInfo::from_balance(U256::from(0)))
        .balance;
    println!("to_post_balance: {}", to_post_balance);
    assert!(to_post_balance > to_pre_balance, "recipient balance must increase");
}

/// Deposit nonce monotonic: two deposits produce distinct ids
#[test]
fn test_eth_bridge_deposit_nonce_monotonic() {
    let setup = eth_bridge_setup();
    let mut db = setup.db;
    let contract = setup.contract;

    let to = BOB;
    let context = Bytes::from_static(b"ctx");
    let canceler = CAROL;

    let r1 = run_tx(&mut db, &contract, depositCall { to, data: Bytes::from_static(b"d1"), context: context.clone(), canceler }.abi_encode(), &ALICE).unwrap();
    let id1: FixedBytes<32> = FixedBytes::<32>::abi_decode(&r1.output, true).unwrap();

    let r2 = run_tx(&mut db, &contract, depositCall { to, data: Bytes::from_static(b"d2"), context, canceler }.abi_encode(), &ALICE).unwrap();
    let id2: FixedBytes<32> = FixedBytes::<32>::abi_decode(&r2.output, true).unwrap();

    assert_ne!(id1, id2, "distinct deposits must produce distinct ids");
}

/// Re-claiming the same deposit must revert with AlreadyClaimed()
#[test]
fn test_eth_bridge_claim_deposit_twice_reverts_already_claimed() {
    initialize_logger();
    let mut db = InMemoryDB::default();

    // Actors
    for user in [ALICE, BOB, CAROL] { add_balance_to_db(&mut db, user, 1e18 as u64); }
    let publisher = Address::from([0x44; 20]);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    // Deploy SignalService at deterministic address and fund
    let bytecode_ss = get_bytecode("hydra_l2_signal_service");
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let signal_service = deploy_contract(&mut db, bytecode_ss, Some((publisher, this_address).abi_encode())).unwrap();

    // Deploy Bridge configured to talk to that SignalService and a counterpart
    let counterpart = Address::from([0x22; 20]);
    let bytecode_bridge = get_bytecode("hydra_l2_bridge");
    let bridge = deploy_contract(&mut db, bytecode_bridge, Some((signal_service, counterpart).abi_encode())).unwrap();

    // Fund bridge without overwriting code so it can forward ETH
    use r55::test_utils::add_balance_preserve_code;
    add_balance_preserve_code(&mut db, bridge, 1_000_000);

    // Prepare a valid deposit tuple and its id
    let amount = U256::from(10_000u64);
    let to = Address::from([0x33; 20]); // avoid precompile range
    let eth_deposit = ETHDeposit {
        nonce: U256::from(1u64),
        from: ALICE,
        to,
        amount,
        data: Bytes::from_static(b"hello"),
        context: Bytes::from_static(b"ctx"),
        canceler: CAROL,
    };
    let deposit_id: FixedBytes<32> = {
        let t = (
            eth_deposit.nonce,
            eth_deposit.from,
            eth_deposit.to,
            eth_deposit.amount,
            eth_deposit.data.clone(),
            eth_deposit.context.clone(),
            eth_deposit.canceler,
        );
        FixedBytes::<32>::from(keccak256(&t.abi_encode()))
    };

    // Publisher signals a synthetic state root that includes (sender=counterpart, value=deposit_id)
    let (account_proof, storage_proof, state_root) = build_signal_proof(this_address, counterpart, deposit_id);
    sol! { function sendSignal(bytes32 value) returns (bytes32); }
    let _ = run_tx(&mut db, &signal_service, sendSignalCall { value: state_root }.abi_encode(), &publisher).unwrap();

    // First claim succeeds
    let proof_bytes = (account_proof.clone(), storage_proof.clone(), state_root).abi_encode();
    let calldata = claimDepositCall { ethDeposit: eth_deposit.clone(), proof: proof_bytes.clone().into() }.abi_encode();
    let res1 = run_tx(&mut db, &bridge, calldata.clone(), &ALICE).unwrap();
    assert!(res1.status);

    // Second claim must revert with AlreadyClaimed()
    let err = run_tx(&mut db, &bridge, calldata, &ALICE).unwrap_err();
    assert!(err.matches_custom_error("AlreadyClaimed()"), "expected AlreadyClaimed() revert, got: {}", err);
}

/// Claim must revert with FailedClaim() if amount does not fit u64 (bridge enforces runtime call ABI constraint)
#[test]
fn test_eth_bridge_claim_deposit_amount_too_large_reverts_failed_claim() {
    initialize_logger();
    let mut db = InMemoryDB::default();

    // Actors
    for user in [ALICE, BOB, CAROL] { add_balance_to_db(&mut db, user, 1e18 as u64); }
    let publisher = Address::from([0x44; 20]);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    // Deploy SignalService and Bridge
    let bytecode_ss = get_bytecode("hydra_l2_signal_service");
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let signal_service = deploy_contract(&mut db, bytecode_ss, Some((publisher, this_address).abi_encode())).unwrap();
    let counterpart = Address::from([0x22; 20]);
    let bytecode_bridge = get_bytecode("hydra_l2_bridge");
    let bridge = deploy_contract(&mut db, bytecode_bridge, Some((signal_service, counterpart).abi_encode())).unwrap();

    // Fund bridge without overwriting code
    use r55::test_utils::add_balance_preserve_code;
    add_balance_preserve_code(&mut db, bridge, 1_000_000);

    // Amount with non-zero high bytes to exceed u64
    let mut be = [0u8; 32];
    be[0] = 1; // set high byte
    let big_amount = U256::from_be_bytes(be);

    let to = Address::from([0x33; 20]); // avoid precompile range
    let eth_deposit = ETHDeposit {
        nonce: U256::from(1u64),
        from: ALICE,
        to,
        amount: big_amount,
        data: Bytes::from_static(b"hello"),
        context: Bytes::from_static(b"ctx"),
        canceler: CAROL,
    };

    // Build deposit id and a matching proof so verifySignal passes, then fail on amount check
    let deposit_id: FixedBytes<32> = {
        let t = (
            eth_deposit.nonce,
            eth_deposit.from,
            eth_deposit.to,
            eth_deposit.amount,
            eth_deposit.data.clone(),
            eth_deposit.context.clone(),
            eth_deposit.canceler,
        );
        FixedBytes::<32>::from(keccak256(&t.abi_encode()))
    };
    let (account_proof, storage_proof, state_root) = build_signal_proof(this_address, counterpart, deposit_id);
    sol! { function sendSignal(bytes32 value) returns (bytes32); }
    let _ = run_tx(&mut db, &signal_service, sendSignalCall { value: state_root }.abi_encode(), &publisher).unwrap();

    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();
    let calldata = claimDepositCall { ethDeposit: eth_deposit, proof: proof_bytes.into() }.abi_encode();

    // Expect FailedClaim()
    let err = run_tx(&mut db, &bridge, calldata, &ALICE).unwrap_err();
    assert!(err.matches_custom_error("FailedClaim()"), "expected FailedClaim() revert, got: {}", err);
}

// SignalService-specific negatives (e.g., missing state root) are covered in
// hydra-l2-signal-service.rs and are not asserted here against ETHBridge.

// SignalService-specific negatives (e.g., empty accountProof) are covered in
// hydra-l2-signal-service.rs and are not asserted here against ETHBridge.

// SignalService-specific negatives (e.g., invalid inclusion proof) are covered
// in hydra-l2-signal-service.rs and are not asserted here against ETHBridge.

/// Zero-amount claim: succeeds, marks processed, emits event, no balance delta.
#[test]
fn test_eth_bridge_claim_deposit_zero_amount_succeeds_no_balance_change() {
    initialize_logger();
    let mut db = InMemoryDB::default();

    for user in [ALICE, BOB, CAROL] { add_balance_to_db(&mut db, user, 1e18 as u64); }
    let publisher = Address::from([0x44; 20]);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    let bytecode_ss = get_bytecode("hydra_l2_signal_service");
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let signal_service = deploy_contract(&mut db, bytecode_ss, Some((publisher, this_address).abi_encode())).unwrap();
    let counterpart = Address::from([0x22; 20]);
    let bridge = {
        let bytecode_bridge = get_bytecode("hydra_l2_bridge");
        deploy_contract(&mut db, bytecode_bridge, Some((signal_service, counterpart).abi_encode())).unwrap()
    };

    use r55::test_utils::add_balance_preserve_code;
    add_balance_preserve_code(&mut db, bridge, 1_000_000);

    let to = Address::from([0x33; 20]);
    let eth_deposit = ETHDeposit { nonce: U256::from(1u64), from: ALICE, to, amount: U256::from(0u64),
        data: Bytes::from_static(b"hello"), context: Bytes::from_static(b"ctx"), canceler: CAROL };
    let id = {
        let t = (eth_deposit.nonce, eth_deposit.from, eth_deposit.to, eth_deposit.amount,
                 eth_deposit.data.clone(), eth_deposit.context.clone(), eth_deposit.canceler);
        FixedBytes::<32>::from(keccak256(&t.abi_encode()))
    };
    let (account_proof, storage_proof, state_root) = build_signal_proof(this_address, counterpart, id);
    sol! { function sendSignal(bytes32 value) returns (bytes32); }
    let _ = run_tx(&mut db, &signal_service, sendSignalCall { value: state_root }.abi_encode(), &publisher).unwrap();

    // Pre/post balance check
    use r55::test_utils::AccountInfo;
    let pre = db.basic(to).unwrap().unwrap_or(AccountInfo::from_balance(U256::from(0))).balance;

    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();
    let calldata = claimDepositCall { ethDeposit: eth_deposit.clone(), proof: proof_bytes.into() }.abi_encode();
    let res = run_tx(&mut db, &bridge, calldata, &ALICE).unwrap();
    assert!(res.status);
    // processed flag set
    let processed_call = processedCall { id };
    let processed_res = run_tx(&mut db, &bridge, processed_call.abi_encode(), &ALICE).unwrap();
    let flag: bool = bool::abi_decode(&processed_res.output, true).unwrap();
    assert!(flag);

    let post = db.basic(to).unwrap().unwrap_or(AccountInfo::from_balance(U256::from(0))).balance;
    assert_eq!(post, pre, "zero-amount claim must not change balance");
}

/// cancelDeposit happy path: only canceler, processed set, DepositCancelled emitted, value transferred to claimee
#[test]
fn test_eth_bridge_cancel_deposit_happy_path() {
    initialize_logger();
    let mut db = InMemoryDB::default();

    for user in [ALICE, BOB, CAROL] { add_balance_to_db(&mut db, user, 1e18 as u64); }
    let publisher = Address::from([0x44; 20]);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    // Deploy SignalService and Bridge
    let bytecode_ss = get_bytecode("hydra_l2_signal_service");
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let signal_service = deploy_contract(&mut db, bytecode_ss, Some((publisher, this_address).abi_encode())).unwrap();
    let counterpart = Address::from([0x22; 20]);
    let bytecode_bridge = get_bytecode("hydra_l2_bridge");
    let bridge = deploy_contract(&mut db, bytecode_bridge, Some((signal_service, counterpart).abi_encode())).unwrap();

    use r55::test_utils::{add_balance_preserve_code, AccountInfo};
    add_balance_preserve_code(&mut db, bridge, 1_000_000);

    // Deposit tuple
    let amount = U256::from(7u64);
    let to = Address::from([0x33; 20]); // recipient originally
    let claimee = Address::from([0x44; 20]); // will receive funds on cancel
    let eth_deposit = ETHDeposit { nonce: U256::from(1u64), from: ALICE, to, amount, data: Bytes::from_static(b"d"), context: Bytes::from_static(b"c"), canceler: CAROL };

    // Inclusion proof for id under sender=counterpart
    let id: FixedBytes<32> = {
        let t = (eth_deposit.nonce, eth_deposit.from, eth_deposit.to, eth_deposit.amount,
                 eth_deposit.data.clone(), eth_deposit.context.clone(), eth_deposit.canceler);
        FixedBytes::<32>::from(keccak256(&t.abi_encode()))
    };
    let (account_proof, storage_proof, state_root) = build_signal_proof(this_address, counterpart, id);
    sol! { function sendSignal(bytes32 value) returns (bytes32); }
    let _ = run_tx(&mut db, &signal_service, sendSignalCall { value: state_root }.abi_encode(), &publisher).unwrap();

    // Pre-balance of claimee
    let pre = db.basic(claimee).unwrap().unwrap_or(AccountInfo::from_balance(U256::from(0))).balance;

    // cancelDeposit by canceler succeeds
    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();
    let calldata = cancelDepositCall { ethDeposit: eth_deposit.clone(), claimee, proof: proof_bytes.into() }.abi_encode();
    let res = run_tx(&mut db, &bridge, calldata, &CAROL).unwrap();
    assert!(res.status);

    // processed flag set
    let processed_call = processedCall { id };
    let processed_res = run_tx(&mut db, &bridge, processed_call.abi_encode(), &ALICE).unwrap();
    let flag: bool = bool::abi_decode(&processed_res.output, true).unwrap();
    assert!(flag);

    // DepositCancelled emitted
    // topic0 = keccak256("DepositCancelled(bytes32,address)")
    // topic1 = id (indexed)
    // data   = abi(address claimee) (32-byte left-padded)
    let topic0 = B256::from(keccak256(b"DepositCancelled(bytes32,address)"));
    let lg = res.logs.iter().find(|l| l.topics()[0] == topic0).expect("missing DepositCancelled");
    assert_eq!(lg.topics()[0], topic0);
    assert_eq!(lg.topics()[1], B256::from_slice(id.as_slice()));
    let mut padded_addr = [0u8; 32];
    padded_addr[12..].copy_from_slice(claimee.as_slice());
    assert_eq!(lg.data.data.as_ref(), &padded_addr);

    // Value sent to claimee
    let post = db.basic(claimee).unwrap().unwrap_or(AccountInfo::from_balance(U256::from(0))).balance;
    assert!(post > pre);
}

/// cancelDeposit must revert if called by a non-canceler
#[test]
fn test_eth_bridge_cancel_deposit_only_canceler_reverts() {
    initialize_logger();
    let mut db = InMemoryDB::default();

    for user in [ALICE, BOB, CAROL] { add_balance_to_db(&mut db, user, 1e18 as u64); }
    let publisher = Address::from([0x44; 20]);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    let bytecode_ss = get_bytecode("hydra_l2_signal_service");
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let signal_service = deploy_contract(&mut db, bytecode_ss, Some((publisher, this_address).abi_encode())).unwrap();
    let counterpart = Address::from([0x22; 20]);
    let bridge = deploy_contract(&mut db, get_bytecode("hydra_l2_bridge"), Some((signal_service, counterpart).abi_encode())).unwrap();

    use r55::test_utils::add_balance_preserve_code;
    add_balance_preserve_code(&mut db, bridge, 1_000_000);

    let eth_deposit = ETHDeposit { nonce: U256::from(1u64), from: ALICE, to: Address::from([0x33; 20]), amount: U256::from(1u64),
        data: Bytes::from_static(b"d"), context: Bytes::from_static(b"c"), canceler: CAROL };
    let id: FixedBytes<32> = {
        let t = (eth_deposit.nonce, eth_deposit.from, eth_deposit.to, eth_deposit.amount,
                 eth_deposit.data.clone(), eth_deposit.context.clone(), eth_deposit.canceler);
        FixedBytes::<32>::from(keccak256(&t.abi_encode()))
    };
    let (account_proof, storage_proof, state_root) = build_signal_proof(this_address, counterpart, id);
    sol! { function sendSignal(bytes32 value) returns (bytes32); }
    let _ = run_tx(&mut db, &signal_service, sendSignalCall { value: state_root }.abi_encode(), &publisher).unwrap();

    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();
    let calldata = cancelDepositCall { ethDeposit: eth_deposit, claimee: BOB, proof: proof_bytes.into() }.abi_encode();
    // Called by ALICE (not the canceler)
    let err = run_tx(&mut db, &bridge, calldata, &ALICE).unwrap_err();
    assert!(err.matches_custom_error("OnlyCanceler()"));
}

/// cancelDeposit must revert with AlreadyClaimed() if id already processed
#[test]
fn test_eth_bridge_cancel_deposit_reverts_already_processed() {
    initialize_logger();
    let mut db = InMemoryDB::default();

    for user in [ALICE, BOB, CAROL] { add_balance_to_db(&mut db, user, 1e18 as u64); }
    let publisher = Address::from([0x44; 20]);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    let bytecode_ss = get_bytecode("hydra_l2_signal_service");
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let signal_service = deploy_contract(&mut db, bytecode_ss, Some((publisher, this_address).abi_encode())).unwrap();
    let counterpart = Address::from([0x22; 20]);
    let bridge = deploy_contract(&mut db, get_bytecode("hydra_l2_bridge"), Some((signal_service, counterpart).abi_encode())).unwrap();

    use r55::test_utils::add_balance_preserve_code;
    add_balance_preserve_code(&mut db, bridge, 1_000_000);

    let to = Address::from([0x33; 20]);
    let eth_deposit = ETHDeposit { nonce: U256::from(1u64), from: ALICE, to, amount: U256::from(3u64), data: Bytes::from_static(b"d"), context: Bytes::from_static(b"c"), canceler: CAROL };
    let id: FixedBytes<32> = {
        let t = (eth_deposit.nonce, eth_deposit.from, eth_deposit.to, eth_deposit.amount,
                 eth_deposit.data.clone(), eth_deposit.context.clone(), eth_deposit.canceler);
        FixedBytes::<32>::from(keccak256(&t.abi_encode()))
    };
    let (account_proof, storage_proof, state_root) = build_signal_proof(this_address, counterpart, id);
    sol! { function sendSignal(bytes32 value) returns (bytes32); }
    let _ = run_tx(&mut db, &signal_service, sendSignalCall { value: state_root }.abi_encode(), &publisher).unwrap();

    // First cancel by CAROL succeeds
    let proof_bytes = (account_proof.clone(), storage_proof.clone(), state_root).abi_encode();
    let calldata1 = cancelDepositCall { ethDeposit: eth_deposit.clone(), claimee: BOB, proof: proof_bytes.clone().into() }.abi_encode();
    let res1 = run_tx(&mut db, &bridge, calldata1, &CAROL).unwrap();
    assert!(res1.status);

    // Second cancel must revert AlreadyClaimed()
    let calldata2 = cancelDepositCall { ethDeposit: eth_deposit, claimee: BOB, proof: proof_bytes.into() }.abi_encode();
    let err = run_tx(&mut db, &bridge, calldata2, &CAROL).unwrap_err();
    assert!(err.matches_custom_error("AlreadyClaimed()"));
}

/// cancelDeposit amount-too-large must revert with FailedClaim()
#[test]
fn test_eth_bridge_cancel_deposit_amount_too_large_reverts_failed_claim() {
    initialize_logger();
    let mut db = InMemoryDB::default();

    for user in [ALICE, BOB, CAROL] { add_balance_to_db(&mut db, user, 1e18 as u64); }
    let publisher = Address::from([0x44; 20]);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    let bytecode_ss = get_bytecode("hydra_l2_signal_service");
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let signal_service = deploy_contract(&mut db, bytecode_ss, Some((publisher, this_address).abi_encode())).unwrap();
    let counterpart = Address::from([0x22; 20]);
    let bridge = deploy_contract(&mut db, get_bytecode("hydra_l2_bridge"), Some((signal_service, counterpart).abi_encode())).unwrap();

    use r55::test_utils::add_balance_preserve_code;
    add_balance_preserve_code(&mut db, bridge, 1_000_000);

    // amount with non-zero high bytes
    let mut be = [0u8; 32]; be[0] = 1; let big = U256::from_be_bytes(be);
    let eth_deposit = ETHDeposit { nonce: U256::from(1u64), from: ALICE, to: Address::from([0x33; 20]), amount: big, data: Bytes::from_static(b"d"), context: Bytes::from_static(b"c"), canceler: CAROL };
    let id: FixedBytes<32> = {
        let t = (eth_deposit.nonce, eth_deposit.from, eth_deposit.to, eth_deposit.amount,
                 eth_deposit.data.clone(), eth_deposit.context.clone(), eth_deposit.canceler);
        FixedBytes::<32>::from(keccak256(&t.abi_encode()))
    };
    let (account_proof, storage_proof, state_root) = build_signal_proof(this_address, counterpart, id);
    sol! { function sendSignal(bytes32 value) returns (bytes32); }
    let _ = run_tx(&mut db, &signal_service, sendSignalCall { value: state_root }.abi_encode(), &publisher).unwrap();

    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();
    let calldata = cancelDepositCall { ethDeposit: eth_deposit, claimee: BOB, proof: proof_bytes.into() }.abi_encode();
    let err = run_tx(&mut db, &bridge, calldata, &CAROL).unwrap_err();
    assert!(err.matches_custom_error("FailedClaim()"));
}

/// cancelDeposit zero-amount: succeeds, processed set, no balance delta to claimee
#[test]
fn test_eth_bridge_cancel_deposit_zero_amount_succeeds_no_balance_change() {
    initialize_logger();
    let mut db = InMemoryDB::default();

    for user in [ALICE, BOB, CAROL] { add_balance_to_db(&mut db, user, 1e18 as u64); }
    let publisher = Address::from([0x44; 20]);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    let bytecode_ss = get_bytecode("hydra_l2_signal_service");
    let this_address = Address::from_slice(&hex!("f6a171f57acac30c292e223ea8adbb28abd3e14d"));
    let signal_service = deploy_contract(&mut db, bytecode_ss, Some((publisher, this_address).abi_encode())).unwrap();
    let counterpart = Address::from([0x22; 20]);
    let bridge = deploy_contract(&mut db, get_bytecode("hydra_l2_bridge"), Some((signal_service, counterpart).abi_encode())).unwrap();

    use r55::test_utils::{add_balance_preserve_code, AccountInfo};
    add_balance_preserve_code(&mut db, bridge, 1_000_000);

    let claimee = BOB;
    let eth_deposit = ETHDeposit { nonce: U256::from(1u64), from: ALICE, to: Address::from([0x33; 20]), amount: U256::from(0u64), data: Bytes::from_static(b"d"), context: Bytes::from_static(b"c"), canceler: CAROL };
    let id: FixedBytes<32> = {
        let t = (eth_deposit.nonce, eth_deposit.from, eth_deposit.to, eth_deposit.amount,
                 eth_deposit.data.clone(), eth_deposit.context.clone(), eth_deposit.canceler);
        FixedBytes::<32>::from(keccak256(&t.abi_encode()))
    };
    let (account_proof, storage_proof, state_root) = build_signal_proof(this_address, counterpart, id);
    sol! { function sendSignal(bytes32 value) returns (bytes32); }
    let _ = run_tx(&mut db, &signal_service, sendSignalCall { value: state_root }.abi_encode(), &publisher).unwrap();

    let pre = db.basic(claimee).unwrap().unwrap_or(AccountInfo::from_balance(U256::from(0))).balance;
    let proof_bytes = (account_proof, storage_proof, state_root).abi_encode();
    let calldata = cancelDepositCall { ethDeposit: eth_deposit, claimee, proof: proof_bytes.into() }.abi_encode();
    let res = run_tx(&mut db, &bridge, calldata, &CAROL).unwrap();
    assert!(res.status);

    // processed flag set
    let processed_call = processedCall { id };
    let processed_res = run_tx(&mut db, &bridge, processed_call.abi_encode(), &ALICE).unwrap();
    let flag: bool = bool::abi_decode(&processed_res.output, true).unwrap();
    assert!(flag);

    let post = db.basic(claimee).unwrap().unwrap_or(AccountInfo::from_balance(U256::from(0))).balance;
    assert_eq!(post, pre, "zero-amount cancel must not change balance");
}
