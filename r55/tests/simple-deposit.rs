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
    function depositEmptyBytes(bytes data);
    function depositLongBytes(bytes data);
    function validateAddress(address addr);
    function complexParams(address addr1, bytes data1, address addr2, bytes data2, address addr3);
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

// --- Scaffolding tests for standard calldata ---

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

    let data = Bytes::from("Test");
    let to = ALICE;
    let call = depositBytesAddressCall {
        data: data.clone(),
        to,
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositBytesAddress(bytes,address) call failed");

    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected = keccak256(&(data, to).abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected keccak256(abi.encode(data, to))"
    );
}

#[test]
fn test_calldata_deposit_bytes_bytes_address() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let data = Bytes::from("Test");
    let data2 = Bytes::from("Test2");
    let to = ALICE;
    let call = depositBytesBytesAddressCall {
        data: data.clone(),
        data2: data2.clone(),
        to,
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositBytesBytesAddress(bytes,bytes,address) call failed");

    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected = keccak256(&(data, data2, to).abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected keccak256(abi.encode(data, data2, to))"
    );
}

#[test]
fn test_calldata_deposit_address_bytes() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let to = ALICE;
    let data = Bytes::from("Test");
    let call = depositAddressBytesCall {
        to,
        data: data.clone(),
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositAddressBytes(address,bytes) call failed");

    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected = keccak256(&(to, data).abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected keccak256(abi.encode(to, data))"
    );
}

// Test depositAddressBytesBytesAddress(address,bytes,bytes,address)
#[test]
fn test_calldata_deposit_address_bytes_bytes_address() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let to = ALICE;
    let data = Bytes::from("Test");
    let data2 = Bytes::from("Test2");
    let to2 = BOB;
    let call = depositAddressBytesBytesAddressCall {
        to,
        data: data.clone(),
        data2: data2.clone(),
        to2,
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositAddressBytesBytesAddress(address,bytes,bytes,address) call failed");

    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected = keccak256(&(to, data, data2, to2).abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(
        result.output.as_slice(),
        expected.as_slice(),
        "expected keccak256(abi.encode(to, data, data2, to2))"
    );
}

// Test deposit(address,bytes,bytes,address)
#[test]
fn test_calldata_deposit() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let to = ALICE;
    let data = Bytes::from("Test");
    let data2 = Bytes::from("Test2");
    let to2 = BOB;
    let call = depositCall {
        to,
        data: data.clone(),
        data2: data2.clone(),
        to2,
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("deposit(address,bytes,bytes,address) call failed");

    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected = keccak256(&(to, data, data2, to2).abi_encode());

    info!("Expected: {:?}", expected); // FixedBytes::<32>::from([0x42u8; 32])
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(result.output.as_slice(), expected.as_slice());
}

// --- R55 tuple-encoded tests for all functions ---

#[test]
fn test_calldata_deposit_bytes_r55() {
    use alloy_sol_types::SolValue;

    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    // Build calldata the R55 way: selector + abi-encoded tuple of args
    let selector = get_selector_from_sig("depositBytes(bytes)");
    let data = Bytes::from("Test");

    // Encode params the tuple way expected by R55
    let mut encoded_params = (data.clone(),).abi_encode();
    let mut calldata = selector.to_vec();
    calldata.append(&mut encoded_params);

    // the call manages to still succeed
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositBytes(bytes) tuple encoding failed");

    // Expected: keccak256(abi.encode(data))
    use alloy_core::primitives::keccak256;
    let expected = keccak256(&data.abi_encode());

    // we don't expect the output to be equal to the expected value
    assert_ne!(result.output.as_slice(), expected.as_slice());
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

    // this still succeeds
    let _ = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositBytes32(bytes32) tuple encoding failed");
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

    // this manages to still succeed
    let _ = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositBytesAddress(bytes,address) tuple encoding failed");
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

    // this manages to still succeed
    let _ = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("depositBytesBytesAddress(bytes,bytes,address) tuple encoding failed");
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

    let result = run_tx(&mut db, &contract, calldata, &ALICE);
    // we expect this to fail because the calldata is poorly formatted
    assert!(result.is_err());
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

    let result = run_tx(&mut db, &contract, calldata, &ALICE);
    // we expect this to fail because the calldata is poorly formatted
    assert!(result.is_err());
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

    let result = run_tx(&mut db, &contract, calldata, &ALICE);
    // we expect this to fail because the calldata is poorly formatted
    assert!(result.is_err());
}

// --- Comprehensive ABI Decoding Tests ---

#[test]
fn test_empty_bytes_edge_case() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    let data = Bytes::from([]); // Empty bytes
    let call = depositEmptyBytesCall { data: data.clone() };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result =
        run_tx(&mut db, &contract, calldata, &ALICE).expect("depositEmptyBytes(bytes) call failed");

    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected = keccak256(&(data,).abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(result.output.as_slice(), expected.as_slice());
}

#[test]
fn test_long_bytes_stress_test() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    // Create a 1KB string to test large data handling
    let long_data = "A".repeat(1024);
    let data = Bytes::from(long_data.into_bytes());
    let call = depositLongBytesCall { data: data.clone() };
    let calldata = call.abi_encode();
    info!("calldata length: {} bytes", calldata.len());

    let result =
        run_tx(&mut db, &contract, calldata, &ALICE).expect("depositLongBytes(bytes) call failed");

    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected = keccak256(&(data,).abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(result.output.as_slice(), expected.as_slice());
}

#[test]
fn test_address_validation() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    let addr = CAROL; // Use a different address
    let call = validateAddressCall { addr };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result =
        run_tx(&mut db, &contract, calldata, &ALICE).expect("validateAddress(address) call failed");

    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected = keccak256(&(addr,).abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(result.output.as_slice(), expected.as_slice());
}

#[test]
fn test_complex_parameters() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    let addr1 = ALICE;
    let data1 = Bytes::from("FirstData");
    let addr2 = BOB;
    let data2 = Bytes::from("SecondData");
    let addr3 = CAROL;

    let call = complexParamsCall {
        addr1,
        data1: data1.clone(),
        addr2,
        data2: data2.clone(),
        addr3,
    };
    let calldata = call.abi_encode();
    info!("calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("complexParams(address,bytes,address,bytes,address) call failed");

    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected = keccak256(&(addr1, data1, addr2, data2, addr3).abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(result.output.as_slice(), expected.as_slice());
}

#[test]
fn test_manual_calldata_verification() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    // Test with the exact calldata you provided earlier (hardcoded, standard abi encoded calldata)
    let manual_calldata_hex = b"0x0a5f93a6000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000000b0000000000000000000000000000000000000000000000000000000000000004546573740000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000055465737432000000000000000000000000000000000000000000000000000000";

    let calldata = decode_calldata_from_hex(manual_calldata_hex);
    info!("Manual calldata: 0x{}", hex::encode(&calldata));

    let result = run_tx(&mut db, &contract, calldata, &ALICE).expect("Manual calldata test failed");

    // Expected: keccak256(abi.encode(0x000000000000000000000000000000000000000A, "Test", "Test2", 0x000000000000000000000000000000000000000B))
    use alloy_core::{hex, primitives::keccak256};
    use alloy_sol_types::SolValue;
    let expected_to = Address::from([
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x0A,
    ]);
    let expected_data = Bytes::from("Test");
    let expected_data2 = Bytes::from("Test2");
    let expected_to2 = Address::from([
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x0B,
    ]);
    let expected =
        keccak256(&(expected_to, expected_data, expected_data2, expected_to2).abi_encode());

    info!("Expected: {:?}", expected);
    info!("raw output: 0x{}", hex::encode(&result.output));
    assert_eq!(result.output.as_slice(), expected.as_slice());
}

#[test]
fn test_different_address_values() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    // Test with different address combinations
    let test_cases = vec![(ALICE, BOB), (BOB, CAROL), (CAROL, ALICE)];

    for (addr1, addr2) in test_cases {
        let call = validateAddressCall { addr: addr1 };
        let calldata = call.abi_encode();

        let result = run_tx(&mut db, &contract, calldata, &ALICE)
            .expect("validateAddress(address) call failed");

        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;
        let expected = keccak256(&(addr1,).abi_encode());

        assert_eq!(
            result.output.as_slice(),
            expected.as_slice(),
            "Failed for address {:?}",
            addr1
        );
    }
}

#[test]
fn test_various_byte_lengths() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();

    // Test different byte lengths: 0, 1, 31, 32, 33, 64, 100
    let test_lengths = vec![0, 1, 31, 32, 33, 64, 100];

    for len in test_lengths {
        let data = Bytes::from(vec![0xAB; len]);
        let call = depositEmptyBytesCall { data: data.clone() };
        let calldata = call.abi_encode();

        let result = run_tx(&mut db, &contract, calldata, &ALICE)
            .expect("depositEmptyBytes(bytes) call failed");

        use alloy_core::primitives::keccak256;
        use alloy_sol_types::SolValue;
        let expected = keccak256(&(data,).abi_encode());

        assert_eq!(
            result.output.as_slice(),
            expected.as_slice(),
            "Failed for byte length {}",
            len
        );
    }
}
