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
    info!("Testing depositBytes function with various input data");
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
    let test_data = Bytes::from("Test");
    let calldata = get_calldata(selector, test_data.abi_encode());
    info!("Calling depositBytes with 'Test' - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with test data");
    
    assert_eq!(result.output.len(), 32);
    info!("depositBytes with test data returned: {:?}", Bytes::from(result.output));
    
    // Test with longer data
    let long_data = Bytes::from(vec![0x42u8; 100]);
    let calldata = get_calldata(selector, long_data.abi_encode());
    info!("Calling depositBytes with 100 bytes of 0x42 - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with long data");
    
    assert_eq!(result.output.len(), 32);
    info!("depositBytes with long data returned: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_bytes_alloy_1_3_1() {
    info!("Testing depositBytes function with input data from alloy 1.3.1");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    // Retrieved following calldata from alloy 1.3.1 SolValue trait (abi_encode)
    // selector + abi.encode("Test")
    // depositBytes(bytes)
    let calldata_hex = "1ac32b940000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045465737400000000000000000000000000000000000000000000000000000000";
    let calldata = alloy_primitives::hex::decode(calldata_hex).expect("Invalid hex");
    
    info!("Alloy 1.3.1 depositBytes calldata: 0x{}", calldata_hex);

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with Alloy 1.3.1 test data");
    
    assert_eq!(result.output.len(), 32);
    info!("depositBytes with test data returned expected result: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_bytes_address() {
    info!("Testing depositBytesAddress function with bytes and address parameters");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytesAddress(bytes,address)");
    
    // Test with empty bytes and zero address
    let empty_data = Bytes::new();
    let zero_address = Address::ZERO;
    let calldata = get_calldata(selector, (empty_data, zero_address).abi_encode());
    info!("Calling depositBytesAddress with empty data and zero address - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesAddress");
    
    assert_eq!(result.output.len(), 32);
    // Should return [0x42; 32] as per the contract implementation
    let expected = [0x42u8; 32];
    assert_eq!(result.output, expected);
    info!("depositBytesAddress returned expected result: {:?}", Bytes::from(result.output));
    
    // Test with some data and a real address
    let test_data = Bytes::from("Test data");
    let calldata = get_calldata(selector, (test_data, BOB).abi_encode());
    info!("Calling depositBytesAddress with 'Test data' and BOB address - calldata: {:?}", Bytes::from(calldata.clone()));
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesAddress with test data");
    
    assert_eq!(result.output.len(), 32);
    assert_eq!(result.output, expected);
    info!("depositBytesAddress with test data returned expected result");
}

#[test]
fn test_deposit_bytes_address_alloy_1_3_1() {
    info!("Testing depositBytesAddress function with bytes and address parameters from alloy 1.3.1");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    // Retrieved following calldata from alloy 1.3.1 SolCall trait (abi_encode)
    // selector + abi.encode("Test", BOB) (FAILED)
    // depositBytesAddress(bytes,address)
    let calldata_hex = "8072bf6c0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000b00000000000000000000000000000000000000000000000000000000000000045465737400000000000000000000000000000000000000000000000000000000";
    let calldata = alloy_primitives::hex::decode(calldata_hex).expect("Invalid hex");
    
    info!("Alloy 1.3.1 depositBytesAddress calldata: 0x{}", calldata_hex);

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesAddress with Alloy 1.3.1 test data");
    
    assert_eq!(result.output.len(), 32);
    info!("depositBytesAddress with test data returned expected result: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_bytes_bytes_address() {
    info!("Testing depositBytesBytesAddress function with bytes, bytes, and address parameters");
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
    info!("Testing Alloy 1.3.1 depositBytesBytesAddress function with bytes, bytes, and address parameters");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    // Retrieved failing calldata generated by alloy 1.3.1 SolCall trait (abi_encode)
    // selector + abi.encode("First data", "Second data", BOB) (FAILED)
    // depositBytesBytesAddress(bytes,bytes,address)
    let calldata_hex = "486d0d84000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000";
    let calldata = alloy_primitives::hex::decode(calldata_hex).expect("Invalid hex");
    
    info!("Alloy 1.3.1 depositBytesBytesAddress calldata: 0x{}", calldata_hex);
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytesBytesAddress with Alloy 1.3.1 test data");

    let expected = [0x42u8; 32];
    
    assert_eq!(result.output.len(), 32);
    assert_eq!(result.output, expected);
    info!(" depositBytesBytesAddress with test data returned expected result: {:?}", Bytes::from(result.output));
}