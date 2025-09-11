use alloy_primitives::{Address, Bytes, keccak256, U256};
use alloy_sol_types::{sol, SolValue};
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{
        add_balance_to_db, initialize_logger, ALICE, BOB, CAROL,
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
    
    let calldata = get_calldata(selector, encoded_params.clone());
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with empty data");
    
    assert_eq!(result.output.len(), 32);
    let expected_hash = keccak256(&encoded_params);
    assert_eq!(result.output.as_slice(), expected_hash.as_slice());
    info!(" depositBytes with empty data returned: {:?}", Bytes::from(result.output));
    
    // Test with some data
    let test_data = Bytes::from("Test");
    info!("Parameters:");
    info!("  data: 0x{} (length: {})", bytes_to_hex(&test_data), test_data.len());
    
    let encoded_params = test_data.abi_encode();
    info!("Encoded parameters (single bytes): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params.clone());
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with test data");
    
    assert_eq!(result.output.len(), 32);
    let expected_hash = keccak256(&encoded_params);
    assert_eq!(result.output.as_slice(), expected_hash.as_slice());
    info!("depositBytes with test data returned: {:?}", Bytes::from(result.output));
    
    // Test with longer data
    let long_data = Bytes::from(vec![0x42u8; 100]);
    info!("Parameters:");
    info!("  data: 0x{} (length: {})", bytes_to_hex(&long_data), long_data.len());
    
    let encoded_params = long_data.abi_encode();
    info!("Encoded parameters (single bytes): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params.clone());
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes with long data");
    
    assert_eq!(result.output.len(), 32);
    let expected_hash = keccak256(&encoded_params);
    assert_eq!(result.output.as_slice(), expected_hash.as_slice());
    info!("depositBytes with long data returned: {:?}", Bytes::from(result.output));
}