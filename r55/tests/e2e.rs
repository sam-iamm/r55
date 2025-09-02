use alloy_primitives::{address, Address, Bytes, U256, B256};
use alloy_sol_types::SolValue;
use r55::{
    exec::{deploy_contract, run_tx},
    get_bytecode,
    test_utils::{add_balance_to_db, get_selector_from_sig, initialize_logger},
};
use revm::InMemoryDB;
use tracing::{debug, error, info};

#[test]
fn erc20() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    // ERC20 constructor: (owner, name: bytes, symbol: bytes, decimals: uint256)
    let constructor = (
        alice,
        Bytes::from(b"Token".to_vec()),
        Bytes::from(b"TKN".to_vec()),
        U256::from(18u8),
    )
    .abi_encode();
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
fn eth_bridge_deposit_id_parity_and_processed_flag() {
    initialize_logger();

    let mut db = InMemoryDB::default();
    let alice: Address = address!("000000000000000000000000000000000000000A");
    let publisher: Address = address!("00000000000000000000000000000000000000B0");
    add_balance_to_db(&mut db, alice, 1e18 as u64);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    // Deploy service and bridge
    let sig = deploy_contract(&mut db, get_bytecode("hydra_l2_signal_service"), Some((publisher, Address::ZERO).abi_encode())).unwrap();
    let counterpart: Address = address!("9999999999999999999999999999999999999999");
    let bridge = deploy_contract(&mut db, get_bytecode("hydra_l2_bridge"), Some((sig, counterpart).abi_encode())).unwrap();

    info!("----------------------------------------------------------");
    info!("-- ETH BRIDGE: ID PARITY + PROCESSED FLAG ----------------");
    info!("----------------------------------------------------------");
    info!("1) deposit(to,data,context,canceler)");
    let sel_deposit = get_selector_from_sig("deposit(address,bytes,bytes,address)");
    let to = alice;
    let data = Bytes::new();
    let context = Bytes::new();
    let canceler = alice;
    let calldata = [sel_deposit.as_slice(), (to, data.clone(), context.clone(), canceler).abi_encode().as_slice()].concat();
    debug!("Calldata deposit:\n> {:#?}", Bytes::from(calldata.clone()));
    let res = run_tx(&mut db, &bridge, calldata, &alice).expect("deposit failed");
    let id_from_deposit = B256::from_slice(&res.output);
    info!("deposit returned id: 0x{:x}", id_from_deposit);

    info!("2) getDepositId(nonce,from,to,amount,data,context,canceler)");
    let sel_get_id = get_selector_from_sig("getDepositId(uint256,address,address,uint256,bytes,bytes,address)");
    let nonce = U256::from(0);
    let amount = U256::from(0);
    let calldata_get = [
        sel_get_id.as_slice(),
        (nonce, alice, to, amount, data.clone(), context.clone(), canceler).abi_encode().as_slice(),
    ]
    .concat();
    debug!("Calldata getDepositId:\n> {:#?}", Bytes::from(calldata_get.clone()));
    let res_id = run_tx(&mut db, &bridge, calldata_get, &alice).expect("getDepositId failed");
    let id_from_get = B256::from_slice(&res_id.output);
    info!("getDepositId computed id: 0x{:x}", id_from_get);
    assert_eq!(id_from_deposit, id_from_get, "id parity mismatch");

    info!("3) processed(bytes32) should be false before claim");
    let sel_processed = get_selector_from_sig("processed(bytes32)");
    let cal_proc = [sel_processed.as_slice(), id_from_deposit.abi_encode().as_slice()].concat();
    debug!("Calldata processed:\n> {:#?}", Bytes::from(cal_proc.clone()));
    let res_proc = run_tx(&mut db, &bridge, cal_proc, &alice)
        .expect("processed() call failed");
    let is_processed = bool::abi_decode(&res_proc.output, true).unwrap();
    assert!(!is_processed, "processed should be false before claim");
    info!("processed=false before claim OK");
}

#[test]
fn dyn_bytes() {
    initialize_logger();

    let mut db = InMemoryDB::default();

    let alice: Address = address!("000000000000000000000000000000000000000A");
    add_balance_to_db(&mut db, alice, 1e18 as u64);

    let dyn_bytes = deploy_contract(&mut db, get_bytecode("dyn_bytes"), None).unwrap();

    let selector_x_dyn_bytes = get_selector_from_sig("x_dyn_bytes(bytes)");

    info!("----------------------------------------------------------");
    info!("-- X-DYN BYTES TX -----------------------------------------");
    info!("----------------------------------------------------------");
    let mut complete_calldata_x_dyn_bytes = selector_x_dyn_bytes.to_vec();
    complete_calldata_x_dyn_bytes.append(&mut (Bytes::from(b"hello")).abi_encode());

    debug!(
        "Tx calldata:\n> {:#?}",
        Bytes::from(complete_calldata_x_dyn_bytes.clone())
    );
    match run_tx(&mut db, &dyn_bytes, complete_calldata_x_dyn_bytes.clone(), &alice) {
        Ok(res) => info!("{}", res),
        Err(e) => {
            error!("Error when executing tx! {}", e);
            panic!();
        }
    }
}

#[test]
fn eth_bridge_deposit_non_empty_bytes_succeeds() {
    initialize_logger();

    let mut db = InMemoryDB::default();
    let alice: Address = address!("000000000000000000000000000000000000000A");
    let publisher: Address = address!("00000000000000000000000000000000000000B0");
    add_balance_to_db(&mut db, alice, 1e18 as u64);
    add_balance_to_db(&mut db, publisher, 1e18 as u64);

    // Deploy service and bridge
    let sig = deploy_contract(&mut db, get_bytecode("signal_service"), Some((publisher, Address::ZERO).abi_encode())).unwrap();
    let counterpart: Address = address!("9999999999999999999999999999999999999999");
    let bridge = deploy_contract(&mut db, get_bytecode("bridge_eth"), Some((sig, counterpart).abi_encode())).unwrap();

    // Call deposit with non-empty dynamic bytes for data and context
    let sel_deposit = get_selector_from_sig("deposit(address,bytes,bytes,address)");
    let to = alice;
    let data = Bytes::from(b"hello".to_vec());
    let context = Bytes::from(b"world".to_vec());
    let canceler = alice;
    let mut args = (to, data.clone(), context.clone(), canceler).abi_encode();
    let mut calldata = sel_deposit.to_vec();
    calldata.append(&mut args);

    debug!("Calldata deposit (non-empty bytes):\n> {:#?}", Bytes::from(calldata.clone()));
    let res = run_tx(&mut db, &bridge, calldata, &alice).expect("deposit with non-empty bytes failed");
    assert!(res.status, "deposit reverted unexpectedly");
}