use alloy_core::hex;
use alloy_primitives::{Address, Bytes, FixedBytes};
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{add_balance_to_db, get_selector_from_sig, initialize_logger, ALICE, BOB, CAROL},
};
// keep imports local in tests to avoid unused warnings
use alloy_sol_types::{sol, SolCall, SolValue};
use revm::InMemoryDB;
use tracing::info;

sol! {
    function depositBytes(bytes data);
    function depositBytesAddress(bytes data, address to);
    function depositBytesBytesAddress(bytes data, bytes data2, address to);
    function depositAddressBytes(address to, bytes data);
    function depositAddressBytesBytesAddress(address to, bytes data, bytes data2, address to2);
    function deposit(address to, bytes data, bytes data2, address to2);
}

struct SimpleDepositSetup {
    db: InMemoryDB,
    contract: Address,
}

fn simple_deposit_setup() -> SimpleDepositSetup {
    initialize_logger();
    let mut db = InMemoryDB::default();

    // Fund user accounts with some ETH
    for user in [ALICE, BOB, CAROL] {
        add_balance_to_db(&mut db, user, 1e18 as u64);
    }

    // Deploy contract (no constructor parameters needed)
    let bytecode = get_bytecode("simple_deposit");
    let contract = deploy_contract(&mut db, bytecode, None).unwrap();

    SimpleDepositSetup { db, contract }
}

// --- Scaffolding helpers ---
fn decode_calldata_from_hex(calldata_hex: &[u8]) -> Vec<u8> {
    let calldata_str = core::str::from_utf8(calldata_hex).unwrap();
    hex::decode(&calldata_str[2..]).unwrap() // Remove the 0x prefix
}

#[test]
fn test_deposit_arbitrary_calldata_alloy_1_3_1() {
    info!("Testing arbitrary calldata from tuple encoding using alloy 1.3.1");
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    // this is the correctly generated calldata using standard, ethereum abi encoding
    let proper_calldata_hex = b"0x486d0d84000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000";

    // this is the incorrectly generated calldata using r55's methodology
    let _r55_calldata_hex = b"0x486d0d840000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000";

    let calldata_hex = proper_calldata_hex;

    let calldata = decode_calldata_from_hex(calldata_hex);

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositAddressBytesBytesAddress with test data");

    info!(
        " depositAddressBytesBytesAddress with test data returned expected result: {:?}",
        Bytes::from(result.output)
    );
}

// --- Scaffolding tests for arbitrary calldata ---

#[test]
fn test_calldata_deposit_bytes() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    let call = depositBytesCall {
        data: Bytes::from("Test"),
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result =
        run_tx(&mut db, &contract, calldata, &ALICE).expect("depositBytes(bytes) call failed");

    use alloy_core::{hex, primitives::keccak256};
    let expected = keccak256(&Bytes::from("Test").abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected keccak256(abi.encode(\"Test\"))"
    );
}

#[test]
fn test_calldata_deposit_bytes_address() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    let call = depositBytesAddressCall {
        data: Bytes::from("Test"),
        to: ALICE,
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result =
        run_tx(&mut db, &contract, calldata, &ALICE).expect("depositBytes(bytes) call failed");

    use alloy_core::{hex, primitives::keccak256};
    let expected = FixedBytes::<32>::from([0x42u8; 32]);

    info!("Expected: {:?}", expected); // FixedBytes::<32>::from([0x42u8; 32])
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected bytes32(0x42424242...)"
    );
}

#[test]
fn test_calldata_deposit_bytes_bytes_address() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let call = depositBytesBytesAddressCall {
        data: Bytes::from("Test"),
        data2: Bytes::from("Test2"),
        to: ALICE,
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositBytesBytesAddress(bytes,bytes,address) call failed");

    use alloy_core::{hex, primitives::keccak256};
    let expected = FixedBytes::<32>::from([0x42u8; 32]);

    info!("Expected: {:?}", expected); // FixedBytes::<32>::from([0x42u8; 32])
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected bytes32(0x42424242...)"
    );
}

#[test]
fn test_calldata_deposit_address_bytes() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let call = depositAddressBytesCall {
        to: ALICE,
        data: Bytes::from("Test"),
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositAddressBytes(address,bytes) call failed");

    use alloy_core::{hex, primitives::keccak256};
    let expected = FixedBytes::<32>::from([0x42u8; 32]);

    info!("Expected: {:?}", expected); // FixedBytes::<32>::from([0x42u8; 32])
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected bytes32(0x42424242...)"
    );
}

// Test depositAddressBytesBytesAddress(address,bytes,bytes,address)
#[test]
fn test_calldata_deposit_address_bytes_bytes_address() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let call = depositAddressBytesBytesAddressCall {
        to: ALICE,
        data: Bytes::from("Test"),
        data2: Bytes::from("Test2"),
        to2: BOB,
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositAddressBytesBytesAddress(address,bytes,bytes,address) call failed");

    use alloy_core::{hex, primitives::keccak256};
    let expected = FixedBytes::<32>::from([0x42u8; 32]);

    info!("Expected: {:?}", expected); // FixedBytes::<32>::from([0x42u8; 32])
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected bytes32(0x42424242...)"
    );
}

// Test deposit(address,bytes,bytes,address)
#[test]
fn test_calldata_deposit() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let call = depositCall {
        to: ALICE,
        data: Bytes::from("Test"),
        data2: Bytes::from("Test2"),
        to2: BOB,
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("deposit(address,bytes,bytes,address) call failed");

    use alloy_core::{hex, primitives::keccak256};
    let expected = FixedBytes::<32>::from([0x42u8; 32]);

    info!("Expected: {:?}", expected); // FixedBytes::<32>::from([0x42u8; 32])
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected bytes32(0x42424242...)"
    );
}

// --- R55 tuple-encoded tests for all functions ---

#[test]
fn test_calldata_deposit_bytes_r55() {
    use alloy_sol_types::SolValue;

    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    // Build calldata the R55 way: selector + abi-encoded tuple of args
    let selector = get_selector_from_sig("depositBytes(bytes)");
    let data = Bytes::from("Test");

    // Encode params the canonical tuple way expected by R55
    let mut encoded_params = (data.clone(),).abi_encode();
    let mut calldata = selector.to_vec();
    calldata.append(&mut encoded_params);

    info!("calldata: 0x{}", hex::encode(&calldata));

    let result =
        run_tx(&mut db, &contract, calldata, &ALICE).expect("depositBytes(bytes) call failed");

    // Expected: keccak256(abi.encode(data))
    use alloy_core::primitives::keccak256;
    let expected = keccak256(&data.abi_encode());
    info!("Expected: 0x{}", hex::encode(expected));
    info!("raw output: 0x{}", hex::encode(&result.output));

    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected keccak256(abi.encode(\"Test\"))"
    );
}

#[test]
fn test_calldata_deposit_bytes32_r55() {
    use alloy_core::primitives::{keccak256, FixedBytes};
    use alloy_sol_types::SolValue;

    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytes32(bytes32)");
    let data32 = FixedBytes::<32>::from([0x11u8; 32]);

    let mut params = (data32,).abi_encode();
    let mut calldata = selector.to_vec();
    calldata.append(&mut params);

    info!("calldata: 0x{}", hex::encode(&calldata));

    let result =
        run_tx(&mut db, &contract, calldata, &ALICE).expect("depositBytes32(bytes32) failed");
    let expected = keccak256(&data32);
}

#[test]
fn test_calldata_deposit_bytes_address_r55() {
    use alloy_sol_types::SolValue;

    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let selector = get_selector_from_sig("depositBytesAddress(bytes,address)");
    let data = Bytes::from("Hello");
    let to = ALICE;

    let mut params = (data, to).abi_encode();
    let mut calldata = selector.to_vec();
    calldata.append(&mut params);

    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositBytesAddress(bytes,address) failed");
}

#[test]
fn test_calldata_deposit_bytes_bytes_address_r55() {
    use alloy_sol_types::SolValue;

    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let selector = get_selector_from_sig("depositBytesBytesAddress(bytes,bytes,address)");
    let d1 = Bytes::from("First");
    let d2 = Bytes::from("Second");
    let to = ALICE;

    let mut params = (d1, d2, to).abi_encode();
    let mut calldata = selector.to_vec();
    calldata.append(&mut params);

    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositBytesBytesAddress(bytes,bytes,address) failed");
}

#[test]
fn test_calldata_deposit_address_bytes_r55() {
    use alloy_sol_types::SolValue;

    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let selector = get_selector_from_sig("depositAddressBytes(address,bytes)");
    let to = ALICE;
    let data = Bytes::from("World");

    let mut params = (to, data).abi_encode();
    let mut calldata = selector.to_vec();
    calldata.append(&mut params);

    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositAddressBytes(address,bytes) failed");
}

#[test]
fn test_calldata_deposit_address_bytes_bytes_address_r55() {
    use alloy_sol_types::SolValue;

    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let selector =
        get_selector_from_sig("depositAddressBytesBytesAddress(address,bytes,bytes,address)");
    let to = ALICE;
    let d1 = Bytes::from("First");
    let d2 = Bytes::from("Second");
    let to2 = BOB;

    let mut params = (to, d1, d2, to2).abi_encode();
    let mut calldata = selector.to_vec();
    calldata.append(&mut params);

    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositAddressBytesBytesAddress(address,bytes,bytes,address) failed");
}

#[test]
fn test_calldata_deposit_r55() {
    use alloy_sol_types::SolValue;

    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let selector = get_selector_from_sig("deposit(address,bytes,bytes,address)");
    let to = ALICE;
    let d1 = Bytes::from("First");
    let d2 = Bytes::from("Second");
    let to2 = CAROL;

    let mut params = (to, d1, d2, to2).abi_encode();
    let mut calldata = selector.to_vec();
    calldata.append(&mut params);

    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("deposit(address,bytes,bytes,address) failed");
}
