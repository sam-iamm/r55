use alloy_primitives::{Address, Bytes};
use alloy_sol_types::SolValue;
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{
        add_balance_to_db, get_calldata, get_selector_from_sig, initialize_logger, ALICE, BOB, CAROL,
    },
};
use revm::InMemoryDB;
use std::fmt::Write;
use tracing::info;
use alloy_core::hex;

// Helper function to convert bytes to hex string
fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut hex, "{:02x}", byte).unwrap();
    }
    hex
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

#[test]
fn test_simple_deposit_deployment() {
    let SimpleDepositSetup { db: _, contract } = simple_deposit_setup();
    
    // Contract should be deployed successfully
    assert_ne!(contract, Address::ZERO);
    info!("SimpleDeposit contract deployed at: {:?}", contract);
}

#[test]
fn test_deposit_bytes() {
    info!("Testing depositBytes function with various input data");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytes(bytes)");
    info!("Function signature: depositBytes(bytes)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector));
    
    // Test with empty bytes
    let empty_data = Bytes::new();
    info!("Parameters:");
    info!("  data: 0x{} (length: {})", bytes_to_hex(&empty_data), empty_data.len());
    
    let encoded_params = empty_data.abi_encode();
    info!("Encoded parameters (single bytes): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with empty data");
    
    // Should return a 32-byte hash
    assert_eq!(result.output.len(), 32);
    info!(" depositBytes with empty data returned: {:?}", Bytes::from(result.output));
    
    // Test with some data
    let test_data = Bytes::from("Test");
    info!("Parameters:");
    info!("  data: 0x{} (length: {})", bytes_to_hex(&test_data), test_data.len());
    
    let encoded_params = test_data.abi_encode();
    info!("Encoded parameters (single bytes): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with test data");
    
    assert_eq!(result.output.len(), 32);
    info!("depositBytes with test data returned: {:?}", Bytes::from(result.output));
    
    // Test with longer data
    let long_data = Bytes::from(vec![0x42u8; 100]);
    info!("Parameters:");
    info!("  data: 0x{} (length: {})", bytes_to_hex(&long_data), long_data.len());
    
    let encoded_params = long_data.abi_encode();
    info!("Encoded parameters (single bytes): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with long data");
    
    assert_eq!(result.output.len(), 32);
    info!("depositBytes with long data returned: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_bytes_address() {
    info!("Testing depositBytesAddress function with bytes and address parameters");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytesAddress(bytes,address)");
    info!("Function signature: depositBytesAddress(bytes,address)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector));
    
    // Test with empty bytes and zero address
    let empty_data = Bytes::new();
    let zero_address = Address::ZERO;
    info!("Parameters:");
    info!("  data: 0x{} (length: {})", bytes_to_hex(&empty_data), empty_data.len());
    info!("  address: {}", zero_address);
    
    let encoded_params = (empty_data, zero_address).abi_encode();
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesAddress");
    
    assert_eq!(result.output.len(), 32);
    // Should return [0x42; 32] as per the contract implementation
    let expected = [0x42u8; 32];
    assert_eq!(result.output, expected);
    info!("depositBytesAddress returned expected result: {:?}", Bytes::from(result.output));
    
    // Test with some data and a real address
    let test_data = Bytes::from("Test data");
    info!("Parameters:");
    info!("  data: 0x{} (length: {})", bytes_to_hex(&test_data), test_data.len());
    info!("  address: {}", BOB);
    
    let encoded_params = (test_data, BOB).abi_encode();
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesAddress with test data");
    
    assert_eq!(result.output.len(), 32);
    assert_eq!(result.output, expected);
    info!("depositBytesAddress with test data returned expected result");
}

#[test]
fn test_deposit_bytes_bytes_address() {
    info!("Testing depositBytesBytesAddress function with bytes, bytes, and address parameters");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytesBytesAddress(bytes,bytes,address)");
    info!("Function signature: depositBytesBytesAddress(bytes,bytes,address)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector));
    
    // Test with empty bytes and zero address
    let empty_data1 = Bytes::new();
    let empty_data2 = Bytes::new();
    let zero_address = Address::ZERO;
    info!("Parameters:");
    info!("  data1: 0x{} (length: {})", bytes_to_hex(&empty_data1), empty_data1.len());
    info!("  data2: 0x{} (length: {})", bytes_to_hex(&empty_data2), empty_data2.len());
    info!("  address: {}", zero_address);
    
    let encoded_params = (empty_data1, empty_data2, zero_address).abi_encode();
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
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
    info!("Parameters:");
    info!("  data1: 0x{} (length: {})", bytes_to_hex(&test_data1), test_data1.len());
    info!("  data2: 0x{} (length: {})", bytes_to_hex(&test_data2), test_data2.len());
    info!("  address: {}", BOB);
    
    let encoded_params = (test_data1, test_data2, BOB).abi_encode();
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesBytesAddress with test data");
    
    assert_eq!(result.output.len(), 32);
    assert_eq!(result.output, expected);
    info!(" depositBytesBytesAddress with test data returned expected result: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_address_bytes_bytes_address() {
    info!("Testing depositAddressBytesBytesAddress function with address, bytes, bytes, and address parameters");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositAddressBytesBytesAddress(address,bytes,bytes,address)");
    info!("Function signature: depositAddressBytesBytesAddress(address,bytes,bytes,address)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector));
    
    // Test with some data and a real address
    let test_data1 = Bytes::from("First data");
    let test_data2 = Bytes::from("Second data");
    info!("Parameters:");
    info!("  address1: {}", CAROL);
    info!("  data1: 0x{} (length: {})", bytes_to_hex(&test_data1), test_data1.len());
    info!("  data2: 0x{} (length: {})", bytes_to_hex(&test_data2), test_data2.len());
    info!("  address2: {}", BOB);
    
    let encoded_params = (CAROL, test_data1, test_data2, BOB).abi_encode();
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositAddressBytesBytesAddress with test data");
    
    assert_eq!(result.output.len(), 32);
    let expected = [0x42u8; 32];
    assert_eq!(result.output, expected);
    info!(" depositAddressBytesBytesAddress with test data returned expected result: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_arbitrary_calldata_alloy_1_3_1() {
    info!("Testing arbitrary calldata from tuple encoding using alloy 1.3.1");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let calldata_hex = b"0x783c6ddc000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000568656c6c6f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005776f726c64000000000000000000000000000000000000000000000000000000";
    
    // Parse the hex string to bytes
    let calldata_str = std::str::from_utf8(calldata_hex).unwrap();
    let calldata = hex::decode(&calldata_str[2..]).unwrap(); // Remove "0x" prefix
    
    info!("Raw calldata from Alloy 1.3.1: {}", calldata_str);
    info!("Parsed calldata length: {} bytes", calldata.len());
    
    // Extract function selector (first 4 bytes)
    let selector = &calldata[0..4];
    info!("Function selector: 0x{}", bytes_to_hex(selector));
    
    // Try to identify which function this is
    let expected_selector = get_selector_from_sig("depositAddressBytesBytesAddress(address,bytes,bytes,address)");
    info!("Expected selector for depositAddressBytesBytesAddress: 0x{}", bytes_to_hex(&expected_selector));
    
    if selector == expected_selector {
        info!("✓ Selector matches depositAddressBytesBytesAddress function");
    } else {
        info!("✗ Selector does not match expected function");
    }

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositAddressBytesBytesAddress with test data");
    
    assert_eq!(result.output.len(), 32);
    let expected = [0x42u8; 32];
    assert_eq!(result.output, expected);
    info!(" depositAddressBytesBytesAddress with test data returned expected result: {:?}", Bytes::from(result.output));
}