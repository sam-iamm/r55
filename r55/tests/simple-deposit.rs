use alloy_primitives::{Address, Bytes, keccak256, U256};
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
use alloy_dyn_abi::{DynSolType, DynSolValue};

// Helper function to convert bytes to hex string
fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut hex, "{:02x}", byte).unwrap();
    }
    hex
}

fn assert_calldata_encoding<F>(
    sig: &str,
    our_encoded_params: Vec<u8>,
    foreign_hex: Option<&[u8]>,
    invariant: F,
) where
    F: Fn(&[u8], &[u8]),
{
    let selector = get_selector_from_sig(sig);
    let our_calldata = get_calldata(selector, our_encoded_params);

    if let Some(hex_bytes) = foreign_hex {
        let foreign_str = std::str::from_utf8(hex_bytes).unwrap();
        let foreign = hex::decode(&foreign_str[2..]).unwrap();
        // 1) selector matches
        assert_eq!(&foreign[0..4], &selector);
        // 2) args differ
        assert_ne!(&our_calldata[4..], &foreign[4..]);
        // apply invariant checks on (our_args, foreign_args)
        invariant(&our_calldata[4..], &foreign[4..]);
    } else {
        // apply invariant checks on (our_args, empty)
        invariant(&our_calldata[4..], &[]);
    }
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

#[test]
fn test_deposit_address_bytes() {
    info!("Testing depositAddressBytes function with address and bytes parameters");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositAddressBytes(address,bytes)");
    info!("Function signature: depositAddressBytes(address,bytes)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector));
    
    // Test with empty bytes and zero address
    let empty_data = Bytes::new();
    let zero_address = Address::ZERO;
    info!("Parameters:");
    info!("  data: 0x{} (length: {})", bytes_to_hex(&empty_data), empty_data.len());
    info!("  address: {}", zero_address);
    
    let encoded_params = (zero_address, empty_data).abi_encode();
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositAddressBytes");
    
    assert_eq!(result.output.len(), 32);
    // Should return [0x42; 32] as per the contract implementation
    let expected = [0x42u8; 32];
    assert_eq!(result.output, expected);
    info!("depositAddressBytes returned expected result: {:?}", Bytes::from(result.output));
    
    // Test with some data and a real address
    let test_data = Bytes::from("Test");
    info!("Parameters:");
    info!("  data: 0x{} (length: {})", bytes_to_hex(&test_data), test_data.len());
    info!("  address: {}", BOB);
    
    let encoded_params = (BOB, test_data).abi_encode();
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositAddressBytes with test data");
    
    assert_eq!(result.output.len(), 32);
    assert_eq!(result.output, expected);
    info!("depositAddressBytes with test data returned expected result");
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
    let test_data = Bytes::from("Test");
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
    // Contract returns keccak256(abi.encode(U256(2)))
    let mut encoded = [0u8; 32];
    encoded.copy_from_slice(&U256::from(2u8).to_be_bytes::<32>());
    let expected_hash = keccak256(&encoded);
    assert_eq!(result.output.as_slice(), expected_hash.as_slice());
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
    assert_eq!(result.output.as_slice(), expected_hash.as_slice());
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
    
    let encoded_params = (ALICE, test_data1, test_data2, CAROL).abi_encode();
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
fn test_deposit_bytes32() {
    info!("Testing depositBytes32 function with bytes32 parameter");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytes32(bytes32)");
    info!("Function signature: depositBytes32(bytes32)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector));
    
    let data: [u8; 32] = [0x42u8; 32];
    info!("Parameters:");
    info!("  data: 0x{}", bytes_to_hex(&data));
    
    let encoded_params = alloy_primitives::FixedBytes::<32>::from(data).abi_encode();
    info!("Encoded parameters: 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositBytes32");
    
    assert_eq!(result.output.len(), 32);
    let expected_hash = keccak256(&data);
    assert_eq!(result.output.as_slice(), expected_hash.as_slice());
    info!("depositBytes32 returned expected result: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_bytes_bytes_addres() {
    info!("Testing depositBytesBytesAddres function (typo in function name)");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("depositBytesBytesAddres(bytes,bytes,address)");
    info!("Function signature: depositBytesBytesAddres(bytes,bytes,address)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector));
    
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
        .expect("Error executing depositBytesBytesAddres");
    
    assert_eq!(result.output.len(), 32);
    // Contract returns keccak256(abi.encode(U256(1)))
    let mut encoded = [0u8; 32];
    encoded.copy_from_slice(&U256::from(1u8).to_be_bytes::<32>());
    let expected_hash = keccak256(&encoded);
    assert_eq!(result.output.as_slice(), expected_hash.as_slice());
    info!("depositBytesBytesAddres returned expected result: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit() {
    info!("Testing main deposit function with all parameters");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let selector = get_selector_from_sig("deposit(address,bytes,bytes,address)");
    info!("Function signature: deposit(address,bytes,bytes,address)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector));
    
    // Test with some data and addresses
    let test_data1 = Bytes::from("First data");
    let test_data2 = Bytes::from("Second data");
    info!("Parameters:");
    info!("  to: {}", BOB);
    info!("  data: 0x{} (length: {})", bytes_to_hex(&test_data1), test_data1.len());
    info!("  data2: 0x{} (length: {})", bytes_to_hex(&test_data2), test_data2.len());
    info!("  to2: {}", CAROL);
    
    let encoded_params = (BOB, test_data1, test_data2, CAROL).abi_encode();
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let calldata = get_calldata(selector, encoded_params);
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata));
    info!("Complete calldata length: {} bytes", calldata.len());
    
    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing deposit");
    
    assert_eq!(result.output.len(), 32);
    // Contract returns [0x42; 32] as per the implementation
    let expected = [0x42u8; 32];
    assert_eq!(result.output, expected);
    info!("deposit returned expected result: {:?}", Bytes::from(result.output));
}

#[test]
fn test_deposit_arbitrary_calldata_alloy_1_3_1() {
    info!("Testing arbitrary calldata from tuple encoding using alloy 1.3.1");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    let calldata_hex = b"0x486d0d84000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000";
    
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

#[test]
fn test_calldata_encoding_comparison() {
    info!("Generic calldata encoding comparison across selected signatures");
    let SimpleDepositSetup { db: _, contract: _ } = simple_deposit_setup();

    // Case 1: depositAddressBytesBytesAddress(address,bytes,bytes,address)
    let d1 = Bytes::from("First data");
    let d2 = Bytes::from("Second data");
    let params1 = (ALICE, d1, d2, CAROL).abi_encode();
    let alloy_hex1 = b"0x783c6ddc000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000";
    
    // Log comparable full calldata (selector + encoded params)
    info!("Alloy 1.3.1 calldata (full failing calldata): {}", std::str::from_utf8(alloy_hex1).unwrap());
    let selector1 = get_selector_from_sig("depositAddressBytesBytesAddress(address,bytes,bytes,address)");
    let our_full_calldata1 = get_calldata(selector1, params1.clone());
    info!("Our 0.8.20 tuple encoding (full working calldata): 0x{}", bytes_to_hex(&our_full_calldata1));
    
    assert_calldata_encoding(
        "depositAddressBytesBytesAddress(address,bytes,bytes,address)",
        params1,
        Some(alloy_hex1),
        |our_args, foreign_args| {
            use alloy_primitives::U256 as U256P;
            // head: [address, off(data1), off(data2), address]
            let off1 = U256P::from_be_slice(&our_args[32..64]).to::<u64>();
            let off2 = U256P::from_be_slice(&our_args[64..96]).to::<u64>();
            let addr2 = &our_args[96..128];
            assert_eq!(off1, 0x80);
            assert_eq!(off2, 0xC0);
            let mut bob_word = [0u8; 32];
            bob_word[12..32].copy_from_slice(BOB.as_slice());
            assert_eq!(addr2, &bob_word);
            // foreign arg's 4th head word should be 0x0c per Alloy 1.3.1 sample
            let foreign_head4 = U256P::from_be_slice(&foreign_args[96..128]).to::<u64>();
            assert_eq!(foreign_head4, 12u64);
        },
    );

    // Case 2: depositBytesAddress(bytes,address)
    let params2 = (Bytes::from("data"), BOB).abi_encode();
    assert_calldata_encoding(
        "depositBytesAddress(bytes,address)",
        params2,
        None,
        |our_args, _| {
            use alloy_primitives::U256 as U256P;
            // head: [off(bytes), address]
            let off = U256P::from_be_slice(&our_args[0..32]).to::<u64>();
            let addr = &our_args[32..64];
            assert_eq!(off, 0x40);
            let mut bob_word = [0u8; 32];
            bob_word[12..32].copy_from_slice(BOB.as_slice());
            assert_eq!(addr, &bob_word);
        },
    );
}

#[test]
fn test_decode_standard_bytes_bytes_address_without_sol_macro() {
    info!("Decoding standard (bytes,bytes,address) using alloy-dyn-abi without sol! macro");
    // Build calldata using the same local encoding approach as other r55 tests
    let selector = get_selector_from_sig("depositBytesBytesAddress(bytes,bytes,address)");
    let data1 = Bytes::from("First data");
    let data2 = Bytes::from("Second data");
    let to = BOB;
    let params = (data1.clone(), data2.clone(), to).abi_encode();
    let calldata = get_calldata(selector, params);

    // Decode with alloy-dyn-abi (skip selector). Try tuple-wrapped first; fallback to standard.
    let args_ty = DynSolType::Tuple(vec![
        DynSolType::Bytes,
        DynSolType::Bytes,
        DynSolType::Address,
    ]);
    let wrapped = DynSolType::Tuple(vec![args_ty.clone()]);
    let values_b = match wrapped.abi_decode_params(&calldata[4..]) {
        Ok(DynSolValue::Tuple(mut outer)) => {
            let inner = outer.remove(0);
            match inner { DynSolValue::Tuple(v) => v, _ => unreachable!("inner tuple") }
        }
        _ => match args_ty.abi_decode_params(&calldata[4..]) {
            Ok(DynSolValue::Tuple(v)) => v,
            _ => panic!("decode failed both wrapped and standard"),
        },
    };
    assert_eq!(values_b.len(), 3);
    let b0 = match &values_b[0] { DynSolValue::Bytes(b) => b.clone(), _ => unreachable!() };
    let b1_dec = match &values_b[1] { DynSolValue::Bytes(b) => b.clone(), _ => unreachable!() };
    let b2_dec = match &values_b[2] { DynSolValue::Address(a) => *a, _ => unreachable!() };
    assert_eq!(b0.as_slice(), data1.as_ref());
    assert_eq!(b1_dec.as_slice(), data2.as_ref());
    assert_eq!(b2_dec.as_slice(), to.as_slice());

    // Selector sanity check matches function signature keccak4
    let sig = b"depositBytesBytesAddress(bytes,bytes,address)";
    let mut expect_sel = [0u8; 4];
    expect_sel.copy_from_slice(&keccak256(sig)[..4]);
    assert_eq!(&calldata[..4], &expect_sel);
}

#[test]
fn test_decode_address_bytes_bytes_address_without_sol_macro() {
    info!("Decoding standard (address,bytes,bytes,address) using alloy-dyn-abi without sol! macro");
    // Build calldata using the same local encoding approach as other r55 tests
    let selector = get_selector_from_sig("depositAddressBytesBytesAddress(address,bytes,bytes,address)");
    let params = (ALICE, Bytes::from("First data"), Bytes::from("Second data"), CAROL).abi_encode();
    let calldata = get_calldata(selector, params);

    // Verify selector
    assert_eq!(&calldata[..4], &[0x78, 0x3c, 0x6d, 0xdc]);

    // Decode args back (try standard first, then a single-arg tuple-wrapped fallback)
    let args_ty = DynSolType::Tuple(vec![
        DynSolType::Address,
        DynSolType::Bytes,
        DynSolType::Bytes,
        DynSolType::Address,
    ]);
    let values = match args_ty.abi_decode_params(&calldata[4..]) {
        Ok(DynSolValue::Tuple(v)) => v,
        _ => {
            let wrapped = DynSolType::Tuple(vec![args_ty.clone()]);
            let DynSolValue::Tuple(mut outer) = wrapped.abi_decode_params(&calldata[4..]).expect("decode wrapped") else { unreachable!("outer tuple") };
            let inner = outer.remove(0);
            let DynSolValue::Tuple(inner) = inner else { unreachable!("inner tuple") };
            inner
        }
    };
    assert_eq!(values.len(), 4);

    let to = match &values[0] { DynSolValue::Address(a) => *a, _ => unreachable!() };
    let data1 = match &values[1] { DynSolValue::Bytes(b) => b.clone(), _ => unreachable!() };
    let data2 = match &values[2] { DynSolValue::Bytes(b) => b.clone(), _ => unreachable!() };
    let to2 = match &values[3] { DynSolValue::Address(a) => *a, _ => unreachable!() };

    assert_eq!(to.as_slice(), ALICE.as_slice());
    assert_eq!(to2.as_slice(), CAROL.as_slice());
    assert_eq!(data1.as_slice(), b"First data");
    assert_eq!(data2.as_slice(), b"Second data");

    // Only verify decoding and selector here (execution is covered elsewhere)
}
