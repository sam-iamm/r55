use alloy_primitives::{Address, Bytes};
use alloy_sol_types::SolValue;
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{
        add_balance_to_db, get_calldata, get_selector_from_sig, initialize_logger, ALICE, BOB,
    },
};
use revm::InMemoryDB;
use tracing::info;

struct SimpleDepositSetup {
    db: InMemoryDB,
    contract: Address,
}

fn simple_deposit_setup() -> SimpleDepositSetup {
    initialize_logger();
    let mut db = InMemoryDB::default();

    // Fund user accounts with some ETH
    for user in [ALICE, BOB] {
        add_balance_to_db(&mut db, user, 1e18 as u64);
    }

    // Deploy contract (no constructor parameters needed)
    let bytecode = get_bytecode("simple_deposit");
    let contract = deploy_contract(&mut db, bytecode, None).unwrap();

    SimpleDepositSetup { db, contract }
}

#[test]
fn test_simple_deposit_deployment() {
    let SimpleDepositSetup { db: _, contract } = simple_deposit_setup();
    
    // Contract should be deployed successfully
    assert_ne!(contract, Address::ZERO);
    info!("SimpleDeposit contract deployed at: {:?}", contract);
}

#[test]
fn test_deposit_bytes() {
    info!("🧪 Testing depositBytes function with various input data");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytes(bytes)");
    
    // Test with empty bytes
    let empty_data = Bytes::new();
    let calldata = get_calldata(selector, empty_data.abi_encode());
    info!("Calling depositBytes with empty data - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with empty data");
    
    // Should return a 32-byte hash
    assert_eq!(result.output.len(), 32);
    info!(" depositBytes with empty data returned: {:?}", Bytes::from(result.output));
    
    // Test with some data
    let test_data = Bytes::from("Hello, World!");
    let calldata = get_calldata(selector, test_data.abi_encode());
    info!(" Calling depositBytes with 'Hello, World!' - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with test data");
    
    assert_eq!(result.output.len(), 32);
    info!(" depositBytes with test data returned: {:?}", Bytes::from(result.output));
    
    // Test with longer data
    let long_data = Bytes::from(vec![0x42u8; 100]);
    let calldata = get_calldata(selector, long_data.abi_encode());
    info!(" Calling depositBytes with 100 bytes of 0x42 - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with long data");
    
    assert_eq!(result.output.len(), 32);
    info!(" depositBytes with long data returned: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_bytes32() {
    info!("🧪 Testing depositBytes32 function with various bytes32 inputs");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytes32(bytes32)");
    
    // Test with zero bytes32
    let zero_bytes32 = [0u8; 32];
    let calldata = get_calldata(selector, zero_bytes32.abi_encode());
    info!(" Calling depositBytes32 with zero bytes32 - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes32 with zero data");
    
    assert_eq!(result.output.len(), 32);
    info!(" depositBytes32 with zero data returned: {:?}", Bytes::from(result.output));
    
    // Test with specific bytes32
    let test_bytes32 = [0x42u8; 32];
    let calldata = get_calldata(selector, test_bytes32.abi_encode());
    info!(" Calling depositBytes32 with 0x42... bytes32 - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes32 with test data");
    
    assert_eq!(result.output.len(), 32);
    info!(" depositBytes32 with test data returned: {:?}", Bytes::from(result.output));
    
    // Test with random bytes32
    let random_bytes32 = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
        0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
    ];
    let calldata = get_calldata(selector, random_bytes32.abi_encode());
    info!(" Calling depositBytes32 with random bytes32 - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes32 with random data");
    
    assert_eq!(result.output.len(), 32);
    info!(" depositBytes32 with random data returned: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_bytes_address() {
    info!("🧪 Testing depositBytesAddress function with bytes and address parameters");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytesAddress(bytes,address)");
    
    // Test with empty bytes and zero address
    let empty_data = Bytes::new();
    let zero_address = Address::ZERO;
    let calldata = get_calldata(selector, (empty_data, zero_address).abi_encode());
    info!(" Calling depositBytesAddress with empty data and zero address - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesAddress");
    
    assert_eq!(result.output.len(), 32);
    // Should return [0x42; 32] as per the contract implementation
    let expected = [0x42u8; 32];
    assert_eq!(result.output, expected);
    info!(" depositBytesAddress returned expected result: {:?}", Bytes::from(result.output));
    
    // Test with some data and a real address
    let test_data = Bytes::from("Test data");
    let calldata = get_calldata(selector, (test_data, BOB).abi_encode());
    info!(" Calling depositBytesAddress with 'Test data' and BOB address - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesAddress with test data");
    
    assert_eq!(result.output.len(), 32);
    assert_eq!(result.output, expected);
    info!(" depositBytesAddress with test data returned expected result");
}

#[test]
fn test_deposit_bytes_bytes_address() {
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytesBytesAddress(bytes,bytes,address)");
    
    // Test with empty bytes and zero address
    let empty_data1 = Bytes::new();
    let empty_data2 = Bytes::new();
    let zero_address = Address::ZERO;
    let calldata = get_calldata(selector, (empty_data1, empty_data2, zero_address).abi_encode());
    info!(" Calling depositBytesBytesAddress with two empty data arrays and zero address - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesBytesAddress");
    
    assert_eq!(result.output.len(), 32);
    // Should return [0x42; 32] as per the contract implementation
    let expected = [0x42u8; 32];
    assert_eq!(result.output, expected);
    info!(" depositBytesBytesAddress returned expected result: {:?}", Bytes::from(result.output));
    
    // Test with some data and a real address
    let test_data1 = Bytes::from("First data");
    let test_data2 = Bytes::from("Second data");
    let calldata = get_calldata(selector, (test_data1, test_data2, BOB).abi_encode());
    info!(" Calling depositBytesBytesAddress with 'First data', 'Second data', and BOB address - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesBytesAddress with test data");
    
    assert_eq!(result.output.len(), 32);
    assert_eq!(result.output, expected);
    info!(" depositBytesBytesAddress with test data returned expected result: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_bytes_bytes_address_alloy_1_3_1() {
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    // Retrieved failing calldata generated by alloy 1.3.1 SolCall train (abi_encode)
    let calldata = b"0x486d0d84000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000";
    let calldata = calldata.to_vec();
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesBytesAddress with test data");

    let expected = [0x42u8; 32];
    
    assert_eq!(result.output.len(), 32);
    assert_eq!(result.output, expected);
    info!(" depositBytesBytesAddress with test data returned expected result: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_address_bytes() {
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositAddressBytes(address,bytes)");
    
    // Test with zero address and empty bytes
    let zero_address = Address::ZERO;
    let empty_data = Bytes::new();
    let calldata = get_calldata(selector, (zero_address, empty_data).abi_encode());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositAddressBytes");
    
    assert_eq!(result.output.len(), 32);
    // Should return [0x42; 32] as per the contract implementation
    let expected = [0x42u8; 32];
    assert_eq!(result.output, expected);
    info!("depositAddressBytes returned expected result: {:?}", Bytes::from(result.output));
    
    // Test with a real address and some data
    let test_data = Bytes::from("Test data for address bytes");
    let calldata = get_calldata(selector, (BOB, test_data).abi_encode());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositAddressBytes with test data");
    
    assert_eq!(result.output.len(), 32);
    assert_eq!(result.output, expected);
    info!("depositAddressBytes with test data returned expected result");
}

#[test]
fn test_deterministic_behavior() {
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    // Test that depositBytes32 returns the same result for the same input
    let selector = get_selector_from_sig("depositBytes32(bytes32)");
    let test_bytes32 = [0x42u8; 32];
    let calldata = get_calldata(selector, test_bytes32.abi_encode());
    
    let result1 = run_tx(&mut db, &contract, calldata.clone(), &ALICE)
        .expect("Error executing depositBytes32 first time");
    
    let result2 = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes32 second time");
    
    // Results should be identical
    assert_eq!(result1.output, result2.output);
    info!("depositBytes32 returned consistent results: {:?}", Bytes::from(result1.output));
    
    // Test that depositBytes returns the same result for the same input
    let selector = get_selector_from_sig("depositBytes(bytes)");
    let test_data = Bytes::from("Consistent test data");
    let calldata = get_calldata(selector, test_data.abi_encode());
    
    let result1 = run_tx(&mut db, &contract, calldata.clone(), &ALICE)
        .expect("Error executing depositBytes first time");
    
    let result2 = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes second time");
    
    // Results should be identical
    assert_eq!(result1.output, result2.output);
    info!("depositBytes returned consistent results: {:?}", Bytes::from(result1.output));
}

#[test]
fn test_different_caller_addresses() {
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytes32(bytes32)");
    let test_bytes32 = [0x42u8; 32];
    let calldata = get_calldata(selector, test_bytes32.abi_encode());
    
    // Test with ALICE as caller
    let result_alice = run_tx(&mut db, &contract, calldata.clone(), &ALICE)
        .expect("Error executing depositBytes32 with ALICE");
    
    // Test with BOB as caller
    let result_bob = run_tx(&mut db, &contract, calldata, &BOB)
        .expect("Error executing depositBytes32 with BOB");
    
    // Results should be identical regardless of caller (since function doesn't use msg.sender)
    assert_eq!(result_alice.output, result_bob.output);
    info!("depositBytes32 returned same result for different callers: {:?}", Bytes::from(result_alice.output));
}
