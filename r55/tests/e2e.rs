use alloy_primitives::{address, Address, Bytes, FixedBytes, U256};
use alloy_sol_types::SolValue;
use std::fmt::Write;
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{add_balance_to_db, get_selector_from_sig, initialize_logger},
};
use revm::{InMemoryDB, Evm};
use tracing::{debug, error, info};

// Helper function to convert bytes to hex string
fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut hex, "{:02x}", byte).unwrap();
    }
    hex
}

// Helper function to run transactions with ETH value
fn run_tx_with_value(
    db: &mut InMemoryDB,
    addr: &Address,
    calldata: Vec<u8>,
    caller: &Address,
    value: u64,
) -> revm::primitives::ExecutionResult {
    use r55::exec::handle_register;
    
    let mut evm = Evm::builder()
        .with_db(db)
        .modify_tx_env(|tx| {
            tx.caller = *caller;
            tx.transact_to = revm::primitives::TransactTo::Call(*addr);
            tx.data = calldata.into();
            tx.value = U256::from(value);
            tx.gas_price = U256::from(42);
            tx.gas_limit = 100_000_000;
        })
        .modify_cfg_env(|cfg| cfg.limit_contract_code_size = Some(usize::MAX))
        .append_handler_register(handle_register)
        .build();

    evm.transact_commit().unwrap()
}

#[test]
fn erc20() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let constructor = alice.abi_encode();
    // let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH).unwrap();
    let bytecode = get_bytecode("erc20");
    let erc20 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

    let total_supply = get_selector_from_sig("total_supply()");
    let selector_balance = get_selector_from_sig("balance_of(address)");
    let selector_mint = get_selector_from_sig("mint(address,uint256)");

    info!("----------------------------------------------------------");
    info!("-- MINT TX -----------------------------------------------");
    info!("----------------------------------------------------------");
    let value_mint = U256::from(42e18);
    let mut calldata_mint = (alice, value_mint).abi_encode();
    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_mint.clone())
    );
    match run_tx(&mut db, &erc20, complete_calldata_mint.clone(), &alice) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- TOTAL SUPPLY ------------------------------------------");
    info!("----------------------------------------------------------");
    debug!("Tx Calldata:\n> {:#?}", Bytes::from(total_supply.to_vec()));
    match run_tx(&mut db, &erc20, total_supply.to_vec(), &alice) {
        Ok(res) => info!("Success! {}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- BALANCE OF TX -----------------------------------------");
    info!("----------------------------------------------------------");
    let mut calldata_balance = alice.abi_encode();
    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_balance.clone())
    );
    match run_tx(&mut db, &erc20, complete_calldata_balance.clone(), &alice) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };
}

#[test]
fn erc20x() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let erc20x = deploy_contract(&mut db, get_bytecode("erc20x"), None).unwrap();

    let selector_x_deploy = get_selector_from_sig("x_deploy(address)");
    let total_supply = get_selector_from_sig("total_supply()");
    let selector_x_balance = get_selector_from_sig("x_balance_of(address,address)");
    let selector_x_mint = get_selector_from_sig("x_mint(address,uint256,address)");

    info!("----------------------------------------------------------");
    info!("-- X-DEPLOY ERC20 ----------------------------------------");
    info!("----------------------------------------------------------");
    let mut complete_calldata_x_deploy = selector_x_deploy.to_vec();
    complete_calldata_x_deploy.append(&mut erc20x.abi_encode());
    let (erc20, owner) = match run_tx(
        &mut db,
        &erc20x,
        complete_calldata_x_deploy.to_vec(),
        &alice,
    ) {
        Ok(res) => (
            Address::from_slice(&res.output.as_slice()[12..32]),
            Address::from_slice(&res.output.as_slice()[44..]),
        ),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };
    assert_eq!(owner, erc20x);
    info!("ERC20 x-deployed at: {}\n", erc20);

    info!("----------------------------------------------------------");
    info!("-- X-MINT TX -----------------------------------------------");
    info!("----------------------------------------------------------");
    let value_x_mint = U256::from(42e18);
    let mut complete_calldata_x_mint = selector_x_mint.to_vec();
    complete_calldata_x_mint.append(&mut (alice, value_x_mint, erc20).abi_encode());

    debug!(
        "Tx Calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_mint.clone())
    );
    match run_tx(&mut db, &erc20x, complete_calldata_x_mint.clone(), &alice) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- TOTAL SUPPLY ------------------------------------------");
    info!("----------------------------------------------------------");
    debug!("Tx Calldata:\n> {:#?}", Bytes::from(total_supply.to_vec()));
    match run_tx(&mut db, &erc20, total_supply.to_vec(), &alice) {
        Ok(res) => info!("Success! {}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!()
        }
    };

    info!("----------------------------------------------------------");
    info!("-- X-CONTRACT BALANCE OF TX ------------------------------");
    info!("----------------------------------------------------------");
    let mut calldata_x_balance = (alice, erc20).abi_encode();
    let mut complete_calldata_x_balance = selector_x_balance.to_vec();
    complete_calldata_x_balance.append(&mut calldata_x_balance);

    debug!(
        "Tx calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_balance.clone())
    );
    match run_tx(
        &mut db,
        &erc20x,
        complete_calldata_x_balance.clone(),
        &alice,
    ) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!();
        }
    }
}

#[test]
fn hydra_l2_bridge() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    let bob: Address = address!("000000000000000000000000000000000000000B");
    let carol: Address = address!("000000000000000000000000000000000000000C");
    
    // Fund user accounts with some ETH
    for user in [alice, bob, carol] {
        add_balance_to_db(&mut db, user, 1e18 as u64);
    }

    info!("----------------------------------------------------------");
    info!("-- DEPLOY SIGNAL SERVICE --------------------------------");
    info!("----------------------------------------------------------");
    
    // Deploy SignalService first
    let signal_service_constructor = (alice, Address::ZERO).abi_encode(); // state_root_publisher, this_address
    let signal_service_bytecode = get_bytecode("hydra_l2_signal_service");
    let signal_service = deploy_contract(&mut db, signal_service_bytecode, Some(signal_service_constructor)).unwrap();
    info!("SignalService deployed at: {}", signal_service);

    info!("----------------------------------------------------------");
    info!("-- DEPLOY BRIDGE -----------------------------------------");
    info!("----------------------------------------------------------");
    
    // Deploy BridgeEth with signal service and counterpart addresses
    let bridge_constructor = (signal_service, bob).abi_encode(); // signal_service, counterpart
    let bridge_bytecode = get_bytecode("hydra_l2_bridge");
    let bridge = deploy_contract(&mut db, bridge_bytecode, Some(bridge_constructor)).unwrap();
    info!("BridgeEth deployed at: {}", bridge);

    // Get function selectors
    let selector_deposit = get_selector_from_sig("deposit(address,bytes,bytes,address)");
    let selector_get_deposit_id = get_selector_from_sig("getDepositId(uint256,address,address,uint256,bytes,bytes,address)");
    let selector_processed = get_selector_from_sig("processed(bytes32)");
    let selector_is_signal_stored = get_selector_from_sig("isSignalStored(bytes32,address)");

    info!("----------------------------------------------------------");
    info!("-- TEST GET DEPOSIT ID -----------------------------------");
    info!("----------------------------------------------------------");
    
    // Test getDepositId function
    let test_nonce = U256::from(0u64);
    let test_from = alice;
    let test_to = carol;
    let test_amount = U256::from(1e17 as u64); // 0.1 ETH
    let test_data = Bytes::from("test deposit data");
    let test_context = Bytes::from("test context");
    let test_canceler = bob;
    
    info!("Function signature: getDepositId(uint256,address,address,uint256,bytes,bytes,address)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector_get_deposit_id));
    
    info!("Parameters:");
    info!("  nonce: {}", test_nonce);
    info!("  from: {}", test_from);
    info!("  to: {}", test_to);
    info!("  amount: {} wei", test_amount);
    info!("  data: 0x{} (length: {})", bytes_to_hex(&test_data), test_data.len());
    info!("  context: 0x{} (length: {})", bytes_to_hex(&test_context), test_context.len());
    info!("  canceler: {}", test_canceler);
    
    // Encode parameters as a tuple (same as deposit function)
    let mut encoded_params = (
        test_nonce,
        test_from,
        test_to,
        test_amount,
        test_data.clone(),
        test_context.clone(),
        test_canceler,
    ).abi_encode();
    
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    let mut calldata_get_id = selector_get_deposit_id.to_vec();
    calldata_get_id.append(&mut encoded_params);
    
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata_get_id));
    info!("Complete calldata length: {} bytes", calldata_get_id.len());
    let deposit_id_result = match run_tx(&mut db, &bridge, calldata_get_id.clone(), &alice) {
        Ok(res) => {
            info!("GetDepositId result: {}", res);
            FixedBytes::<32>::from_slice(&res.output)
        },
        Err(e) => {
            error!("Error when executing getDepositId! {}", e);
            panic!()
        }
    };
    info!("Calculated deposit ID: {:?}", deposit_id_result);

    info!("----------------------------------------------------------");
    info!("-- TEST PROCESSED FUNCTION (BEFORE DEPOSIT) -------------");
    info!("----------------------------------------------------------");
    
    // Test processed function before any deposit
    let mut calldata_processed = selector_processed.to_vec();
    calldata_processed.append(&mut deposit_id_result.abi_encode());
    
    debug!("Processed calldata: {:#?}", Bytes::from(calldata_processed.clone()));
    let processed_result = match run_tx(&mut db, &bridge, calldata_processed.clone(), &alice) {
        Ok(res) => {
            info!("Processed result: {}", res);
            // Should return false (0) for unprocessed deposit
            assert_eq!(res.output, vec![0u8; 32]);
            false
        },
        Err(e) => {
            error!("Error when executing processed! {}", e);
            panic!()
        }
    };
    assert!(!processed_result, "Deposit should not be processed initially");

    info!("----------------------------------------------------------");
    info!("-- TEST DEPOSIT FUNCTION ---------------------------------");
    info!("----------------------------------------------------------");
    
    // Test deposit function with ETH value
    let deposit_amount = 1e17 as u64; // 0.1 ETH
    info!("Deposit amount: {} wei ({} ETH)", deposit_amount, deposit_amount as f64 / 1e18);
    
    // Log the function signature and selector
    info!("Function signature: deposit(address,bytes,bytes,address)");
    info!("Function selector: 0x{}", bytes_to_hex(&selector_deposit));
    
    // Log individual parameters before encoding
    info!("Parameters:");
    info!("  to: {}", test_to);
    info!("  data: 0x{} (length: {})", bytes_to_hex(&test_data), test_data.len());
    info!("  context: 0x{} (length: {})", bytes_to_hex(&test_context), test_context.len());
    info!("  canceler: {}", test_canceler);
    
    // Encode parameters as a tuple (this is standard ABI encoding for function calls)
    let mut encoded_params = (
        test_to,
        test_data.clone(),
        test_context.clone(),
        test_canceler,
    ).abi_encode();
    
    info!("Encoded parameters (tuple): 0x{}", bytes_to_hex(&encoded_params));
    info!("Encoded parameters length: {} bytes", encoded_params.len());
    
    // Build complete calldata: selector + encoded parameters
    let mut calldata_deposit = selector_deposit.to_vec();
    calldata_deposit.append(&mut encoded_params);
    
    info!("Complete calldata: 0x{}", bytes_to_hex(&calldata_deposit));
    info!("Complete calldata length: {} bytes", calldata_deposit.len());
    let deposit_result = match run_tx_with_value(&mut db, &bridge, calldata_deposit.clone(), &alice, deposit_amount) {
        revm::primitives::ExecutionResult::Success { output: revm::primitives::Output::Call(output), .. } => {
            info!("Deposit result: {:?}", output);
            FixedBytes::<32>::from_slice(&output)
        },
        result => {
            error!("Unexpected execution result: {:?}", result);
            panic!()
        }
    };
    info!("Deposit ID from transaction: {:?}", deposit_result);
    
    // Verify the deposit ID matches what we calculated
    assert_eq!(deposit_result, deposit_id_result, "Deposit ID should match calculated ID");

    info!("----------------------------------------------------------");
    info!("-- TEST PROCESSED FUNCTION (AFTER DEPOSIT) --------------");
    info!("----------------------------------------------------------");
    
    // Test processed function after deposit (should still be false as we haven't processed it)
    let processed_result_after = match run_tx(&mut db, &bridge, calldata_processed.clone(), &alice) {
        Ok(res) => {
            info!("Processed result after deposit: {}", res);
            // Should still return false (0) as deposit is not processed yet
            assert_eq!(res.output, vec![0u8; 32]);
            false
        },
        Err(e) => {
            error!("Error when executing processed after deposit! {}", e);
            panic!()
        }
    };
    assert!(!processed_result_after, "Deposit should still not be processed");

    info!("----------------------------------------------------------");
    info!("-- TEST SIGNAL SERVICE INTEGRATION ----------------------");
    info!("----------------------------------------------------------");
    
    // Test that signal was sent to SignalService
    let mut calldata_is_signal = selector_is_signal_stored.to_vec();
    calldata_is_signal.append(&mut (deposit_result, bridge).abi_encode());
    
    debug!("IsSignalStored calldata: {:#?}", Bytes::from(calldata_is_signal.clone()));
    let signal_stored = match run_tx(&mut db, &signal_service, calldata_is_signal.clone(), &alice) {
        Ok(res) => {
            info!("IsSignalStored result: {}", res);
            // Should return true (1) as signal was sent
            assert_eq!(res.output, vec![0u8; 31].into_iter().chain(vec![1u8]).collect::<Vec<u8>>());
            true
        },
        Err(e) => {
            error!("Error when executing isSignalStored! {}", e);
            panic!()
        }
    };
    assert!(signal_stored, "Signal should be stored in SignalService");

    info!("----------------------------------------------------------");
    info!("-- TEST SECOND DEPOSIT -----------------------------------");
    info!("----------------------------------------------------------");
    
    // Test second deposit to verify nonce increment
    let second_deposit_amount = 5e16 as u64; // 0.05 ETH
    let second_test_data = Bytes::from("second deposit data");
    let second_test_context = Bytes::from("second context");
    
    // Calculate expected deposit ID for second deposit (nonce = 1)
    let second_nonce = U256::from(1u64);
    let mut calldata_second_id = selector_get_deposit_id.to_vec();
    calldata_second_id.append(&mut (
        second_nonce,
        alice,
        carol,
        U256::from(second_deposit_amount),
        second_test_data.clone(),
        second_test_context.clone(),
        bob,
    ).abi_encode());
    
    let second_deposit_id = match run_tx(&mut db, &bridge, calldata_second_id.clone(), &alice) {
        Ok(res) => FixedBytes::<32>::from_slice(&res.output),
        Err(e) => {
            error!("Error when calculating second deposit ID! {}", e);
            panic!()
        }
    };
    
    // Execute second deposit
    let mut calldata_second_deposit = selector_deposit.to_vec();
    calldata_second_deposit.append(&mut (
        carol,
        second_test_data.clone(),
        second_test_context.clone(),
        bob,
    ).abi_encode());
    
    let second_deposit_result = match run_tx_with_value(&mut db, &bridge, calldata_second_deposit.clone(), &alice, second_deposit_amount) {
        revm::primitives::ExecutionResult::Success { output: revm::primitives::Output::Call(output), .. } => {
            info!("Second deposit result: {:?}", output);
            FixedBytes::<32>::from_slice(&output)
        },
        result => {
            error!("Unexpected execution result: {:?}", result);
            panic!()
        }
    };
    
    assert_eq!(second_deposit_result, second_deposit_id, "Second deposit ID should match calculated ID");
    assert_ne!(second_deposit_result, deposit_result, "Second deposit should have different ID");

    info!("----------------------------------------------------------");
    info!("-- BRIDGE TESTS COMPLETED SUCCESSFULLY ------------------");
    info!("----------------------------------------------------------");
}
