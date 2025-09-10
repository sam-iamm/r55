use alloy_core::hex;
use alloy_primitives::{Address, Bytes};
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{add_balance_to_db, initialize_logger, ALICE, BOB, CAROL},
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

// --- Scaffolding helpers ---
fn decode_calldata_from_hex(calldata_hex: &[u8]) -> Vec<u8> {
    let calldata_str = core::str::from_utf8(calldata_hex).unwrap();
    hex::decode(&calldata_str[2..]).unwrap() // Remove the 0x prefix
}

#[test]
fn test_deposit_arbitrary_calldata_alloy_1_3_1() {
    info!("Testing arbitrary calldata from tuple encoding using alloy 1.3.1");
    let SimpleDepositSetup {
        mut db,
        contract,
    } = simple_deposit_setup();

    // this is the correctly generated calldata using standard, ethereum abi encoding
    let proper_calldata_hex = b"0x486d0d84000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000";

    // this is the incorrectly generated calldata using r55's methodology
    let _r55_calldata_hex = b"0x486d0d840000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000";

    let calldata_hex = proper_calldata_hex;
    
    let calldata = decode_calldata_from_hex(calldata_hex);

    let result = run_tx(&mut db, &contract, calldata, &ALICE)
        .expect("Error executing depositAddressBytesBytesAddress with test data");
    
    info!(" depositAddressBytesBytesAddress with test data returned expected result: {:?}", Bytes::from(result.output));
}

// --- Scaffolding tests for arbitrary calldata ---

#[test]
fn test_calldata_deposit_bytes() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let calldata_hex = b"0x1ac32b94000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045465737400000000000000000000000000000000000000000000000000000000"; // fill with selector + encoded args for depositBytes(bytes)
    let calldata = decode_calldata_from_hex(calldata_hex);
    let result = run_tx(&mut db, &contract, calldata, &ALICE).expect("depositBytes(bytes) call failed");
    info!("depositBytes returned: {:?}", Bytes::from(result.output));
}

#[test]
fn test_calldata_deposit_bytes_address() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let calldata_hex = b"0x8072bf6c0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000b00000000000000000000000000000000000000000000000000000000000000045465737400000000000000000000000000000000000000000000000000000000"; // fill with selector + encoded args for depositBytesAddress(bytes,address)
    let calldata = decode_calldata_from_hex(calldata_hex);
    let result = run_tx(&mut db, &contract, calldata, &ALICE).expect("depositBytesAddress(bytes,address) call failed");
    info!("depositBytesAddress returned: {:?}", Bytes::from(result.output));
}

#[test]
fn test_calldata_deposit_bytes_bytes_address() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let calldata_hex = b"0x486d0d84000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000"; // fill with selector + encoded args for depositBytesBytesAddress(bytes,bytes,address)
    let calldata = decode_calldata_from_hex(calldata_hex);
    let result = run_tx(&mut db, &contract, calldata, &ALICE).expect("depositBytesBytesAddress(bytes,bytes,address) call failed");
    info!("depositBytesBytesAddress returned: {:?}", Bytes::from(result.output));
}

#[test]
fn test_calldata_deposit_address_bytes() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let calldata_hex = b"0xb4c9f882000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000045465737400000000000000000000000000000000000000000000000000000000"; // fill with selector + encoded args for depositAddressBytes(address,bytes)
    let calldata = decode_calldata_from_hex(calldata_hex);
    let result = run_tx(&mut db, &contract, calldata, &ALICE).expect("depositAddressBytes(address,bytes) call failed");
    info!("depositAddressBytes returned: {:?}", Bytes::from(result.output));
}

#[test]
fn test_calldata_deposit_address_bytes_bytes_address() {
    let SimpleDepositSetup { mut db, contract } = simple_deposit_setup();
    let calldata_hex = b"0x783c6ddc000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000a4669727374206461746100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b5365636f6e642064617461000000000000000000000000000000000000000000"; // fill with selector + encoded args for depositAddressBytesBytesAddress(address,bytes,bytes,address)
    let calldata = decode_calldata_from_hex(calldata_hex);
    let result = run_tx(&mut db, &contract, calldata, &ALICE).expect("depositAddressBytesBytesAddress(address,bytes,bytes,address) call failed");
    info!("depositAddressBytesBytesAddress returned: {:?}", Bytes::from(result.output));
}

