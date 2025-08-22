mod error;
pub mod exec;
mod gas;

mod generated;
pub use generated::get_bytecode;

pub mod test_utils;

#[cfg(test)]
mod tests {
    use crate::{
        exec::{deploy_contract, run_tx},
        get_bytecode,
        test_utils::*,
    };

    use alloy_core::hex::{self, ToHexExt};
    use alloy_primitives::B256;
    use alloy_sol_types::SolValue;

    fn setup_erc20(owner: Address) -> (InMemoryDB, Address) {
        initialize_logger();
        let mut db = InMemoryDB::default();

        // Fund user accounts with some ETH
        for user in [ALICE, BOB, CAROL] {
            add_balance_to_db(&mut db, user, 1e18 as u64);
        }

        // Deploy contract
        let constructor = owner.abi_encode();
        let bytecode = get_bytecode("erc20");
        let erc20 = deploy_contract(&mut db, bytecode, Some(constructor)).unwrap();

        (db, erc20)
    }

    fn setup_erc20x(db: &mut InMemoryDB) -> Address {
        // Deploy contract
        let bytecode = get_bytecode("erc20x");
        deploy_contract(db, bytecode, None).unwrap()
    }

    #[test]
    fn test_runtime() {
        let (mut db, erc20) = setup_erc20(ALICE);

        // Define fn selectors
        let selector_owner = get_selector_from_sig("owner()");
        let selector_total_supply = get_selector_from_sig("total_supply()");
        let selector_balance = get_selector_from_sig("balance_of(address)");
        let selector_mint = get_selector_from_sig("mint(address,uint256)");
        let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
        let selector_approve = get_selector_from_sig("approve(address,uint256)");
        let selector_allowance = get_selector_from_sig("allowance(address,address)");

        // Check that Alice is the contract owner
        let owner_result = run_tx(
            &mut db,
            &erc20,
            get_calldata(selector_owner, vec![]),
            &ALICE,
        )
        .expect("Error executing tx")
        .output;

        assert_eq!(
            B256::from_slice(owner_result.as_slice()),
            ALICE.into_word(),
            "Incorrect owner"
        );

        // Mint 42 tokens to Alice
        let value_mint = U256::from(42e18);
        let calldata_mint = get_calldata(selector_mint, (ALICE, value_mint).abi_encode());
        let mint_result = run_tx(&mut db, &erc20, calldata_mint, &ALICE).unwrap();

        assert!(mint_result.status, "Mint transaction failed");

        // Check total supply
        let total_supply_result = run_tx(
            &mut db,
            &erc20,
            get_calldata(selector_total_supply, vec![]),
            &ALICE,
        )
        .expect("Error executing tx")
        .output;

        assert_eq!(
            U256::from_be_bytes::<32>(total_supply_result.as_slice().try_into().unwrap()),
            value_mint,
            "Incorrect total supply"
        );

        // Check Alice's balance
        let calldata_alice_balance = get_calldata(selector_balance, ALICE.abi_encode());
        let alice_balance_result = run_tx(&mut db, &erc20, calldata_alice_balance.clone(), &ALICE)
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(alice_balance_result.as_slice().try_into().unwrap()),
            value_mint,
            "Incorrect balance"
        );

        // Transfer 21 tokens from Alice to Bob
        let value_transfer = U256::from(21e18);
        let calldata_transfer = get_calldata(selector_transfer, (BOB, value_transfer).abi_encode());
        let transfer_result = run_tx(&mut db, &erc20, calldata_transfer.clone(), &ALICE).unwrap();
        assert!(transfer_result.status, "Transfer transaction failed");

        // Check Alice's balance
        let alice_balance_result = run_tx(&mut db, &erc20, calldata_alice_balance.clone(), &ALICE)
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(alice_balance_result.as_slice().try_into().unwrap()),
            value_mint - value_transfer,
            "Incorrect balance"
        );

        // Check Bob's balance
        let calldata_bob_balance = get_calldata(selector_balance, BOB.abi_encode());
        let bob_balance_result = run_tx(&mut db, &erc20, calldata_bob_balance.clone(), &ALICE)
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(bob_balance_result.as_slice().try_into().unwrap()),
            value_transfer,
            "Incorrect balance"
        );

        // Approve Carol to spend 10 tokens from Alice
        let value_approve = U256::from(10e18);
        let calldata_approve = get_calldata(selector_approve, (CAROL, value_approve).abi_encode());
        let approve_result = run_tx(&mut db, &erc20, calldata_approve.clone(), &ALICE).unwrap();
        assert!(approve_result.status, "Approve transaction failed");

        // Check Carol's allowance
        let calldata_allowance = get_calldata(selector_allowance, (ALICE, CAROL).abi_encode());
        let carol_allowance_result = run_tx(&mut db, &erc20, calldata_allowance.clone(), &ALICE)
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(carol_allowance_result.as_slice().try_into().unwrap()),
            value_approve,
            "Incorrect balance"
        );
    }

    #[test]
    fn test_transfer_logs() {
        let (mut db, erc20) = setup_erc20(ALICE);

        // Mint tokens to Alice
        let selector_mint = get_selector_from_sig("mint(address,uint256)");
        let calldata_mint = get_calldata(selector_mint, (ALICE, 100u64).abi_encode());

        let mint_result = run_tx(&mut db, &erc20, calldata_mint, &ALICE).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Transfer tokens from Alice to Bob
        let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
        let calldata_transfer = get_calldata(selector_transfer, (BOB, 50u64).abi_encode());

        let transfer_result = run_tx(&mut db, &erc20, calldata_transfer, &ALICE).unwrap();

        // Assert the transfer log
        assert!(
            !transfer_result.logs.is_empty(),
            "No logs found in transfer transaction"
        );
        let log = &transfer_result.logs[0];
        let topics = log.data.topics();

        // Expected event hash for Transfer event
        let expected_event_hash = keccak256("Transfer(address,address,uint256)");
        assert_eq!(
            hex::encode(topics[0]),
            hex::encode(expected_event_hash),
            "Incorrect event hash"
        );

        // Assert "from" address in log
        assert_eq!(
            hex::encode(&topics[1][12..]),
            ALICE.encode_hex(),
            "Incorrect 'from' address in transfer log"
        );

        // Assert "to" address in log
        assert_eq!(
            hex::encode(&topics[2][12..]),
            BOB.encode_hex(),
            "Incorrect 'to' address in transfer log"
        );

        // Assert transfer amount
        let amount = U256::from_be_slice(log.data.data[..32].try_into().unwrap());
        assert_eq!(
            amount,
            U256::from(50),
            "Incorrect transfer amount in transfer log"
        );
    }

    #[test]
    fn test_storage_layout() {
        let (mut db, erc20) = setup_erc20(ALICE);

        // Mint tokens to Alice
        let mint_alice = U256::from(10e18);
        let selector_mint = get_selector_from_sig("mint(address,uint256)");
        let calldata_mint = get_calldata(selector_mint, (ALICE, mint_alice).abi_encode());

        let mint_result = run_tx(&mut db, &erc20, calldata_mint, &ALICE).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Mint tokens to Bob
        let mint_bob = U256::from(20e18);
        let calldata_mint = get_calldata(selector_mint, (BOB, mint_bob).abi_encode());

        let mint_result = run_tx(&mut db, &erc20, calldata_mint, &ALICE).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Approve Carol to spend 10 tokens from Alice
        let allowance_carol = U256::from(5e18);
        let selector_approve = get_selector_from_sig("approve(address,uint256)");
        let calldata_approve =
            get_calldata(selector_approve, (CAROL, allowance_carol).abi_encode());
        let approve_result = run_tx(&mut db, &erc20, calldata_approve, &ALICE).unwrap();
        assert!(approve_result.status, "Approve transaction failed");

        // EXPECTED STORAGE LAYOUT:
        //
        // pub struct ERC20 {
        //     total_supply: Slot<U256>,                                Slot: 0
        //     balances: Mapping<Address, U256>,                        Slot: keccak256(address, 1)
        //     allowances: Mapping<Address, Mapping<Address, U256>>,    Slot: keccak256(address, keccak256(address, 2))
        //     owner: Slot<Address>,                                    Slot: 3
        // }

        // Assert `total_supply` is set to track the correct slot
        let expected_slot = U256::from(0);
        assert_eq!(
            mint_alice + mint_bob,
            read_db_slot(&mut db, erc20, expected_slot)
        );

        let balances_id = U256::from(1);
        // Assert `balances[ALICE]` is set to track the correct slot
        let expected_slot = get_mapping_slot(ALICE.abi_encode(), balances_id);
        assert_eq!(mint_alice, read_db_slot(&mut db, erc20, expected_slot));

        // Assert `balances[BOB]` is set to track the correct slot
        let expected_slot = get_mapping_slot(BOB.abi_encode(), balances_id);
        assert_eq!(mint_bob, read_db_slot(&mut db, erc20, expected_slot));

        let allowances_id = U256::from(2);
        // Assert `allowance[ALICE][CAROL]` is set to track the correct slot
        let id = get_mapping_slot(ALICE.abi_encode(), allowances_id);
        let expected_slot = get_mapping_slot(CAROL.abi_encode(), id);
        assert_eq!(allowance_carol, read_db_slot(&mut db, erc20, expected_slot));

        // Assert `owner` is set to track the correct slot
        let expected_slot = U256::from(3);
        assert_eq!(
            read_db_slot(&mut db, erc20, expected_slot),
            U256::from_be_bytes(ALICE.into_word().0),
        );
    }

    #[test]
    fn test_custom_error() {
        let (mut db, erc20) = setup_erc20(ALICE);

        // Define fn selectors
        let selector_mint = get_selector_from_sig("mint(address,uint256)");
        let selector_approve = get_selector_from_sig("approve(address,uint256)");
        let selector_transfer = get_selector_from_sig("transfer(address,uint256)");
        let selector_transfer_from =
            get_selector_from_sig("transfer_from(address,address,uint256)");

        // Mint 42 tokens to Alice
        let value_mint = U256::from(42e18);
        let calldata_mint = get_calldata(selector_mint, (ALICE, value_mint).abi_encode());

        let mint_result = run_tx(&mut db, &erc20, calldata_mint.clone(), &ALICE).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Attempt mint with Bob (not contract owner)
        let only_owner_result =
            run_tx(&mut db, &erc20, calldata_mint, &BOB).expect_err("Mint transaction succeeded");
        assert!(
            only_owner_result.matches_custom_error("ERC20Error::OnlyOwner"),
            "Incorrect error"
        );

        // Attempt transfer 43 tokens (more than her balance) from Alice to Bob
        let value_transfer = U256::from(43e18);
        let calldata_transfer = get_calldata(selector_transfer, (BOB, value_transfer).abi_encode());

        assert!(value_transfer > value_mint);
        let insufficient_balance_result =
            run_tx(&mut db, &erc20, calldata_transfer.clone(), &ALICE)
                .expect_err("Transfer transaction succeeded");
        assert!(
            insufficient_balance_result.matches_custom_error_with_args(
                "ERC20Error::InsufficientBalance(uint256)",
                value_mint.abi_encode()
            ),
            "Incorrect error signature"
        );

        // Approve Carol to spend 10 tokens from Alice
        let value_approve = U256::from(10e18);
        let calldata_approve = get_calldata(selector_approve, (CAROL, value_approve).abi_encode());

        let approve_result = run_tx(&mut db, &erc20, calldata_approve.clone(), &ALICE).unwrap();
        assert!(approve_result.status, "Approve transaction failed");

        // Attempt transfer_from of all tokens (more than allowance) from Alice to Carol
        let calldata_transfer_from = get_calldata(
            selector_transfer_from,
            (ALICE, CAROL, value_mint).abi_encode(),
        );

        assert!(value_mint > value_approve);
        let insufficient_allowance_result =
            run_tx(&mut db, &erc20, calldata_transfer_from.clone(), &CAROL)
                .expect_err("Transfer From tx succeeded");
        assert!(
            insufficient_allowance_result.matches_custom_error_with_args(
                "ERC20Error::InsufficientAllowance(uint256)",
                value_approve.abi_encode()
            ),
            "Incorrect error signature"
        );
    }

    #[test]
    fn test_custom_error_with_cross_contract_call() {
        let (mut db, erc20) = setup_erc20(ALICE);
        let erc20x = setup_erc20x(&mut db);

        // Define fn selectors
        let selector_mint = get_selector_from_sig("mint(address,uint256)");
        let selector_x_mint = get_selector_from_sig("x_mint(address,uint256,address)");
        let selector_approve = get_selector_from_sig("approve(address,uint256)");
        let selector_balance_of = get_selector_from_sig("balance_of(address)");
        let selector_x_transfer_from =
            get_selector_from_sig("x_transfer_from(address,uint256,address)");

        // Mint 42 tokens to Alice
        let value_mint = U256::from(42e18);
        let calldata_mint = get_calldata(selector_mint, (ALICE, value_mint).abi_encode());

        let mint_result = run_tx(&mut db, &erc20, calldata_mint.clone(), &ALICE).unwrap();
        assert!(mint_result.status, "Mint transaction failed");

        // Attempt to cross-mint 100 tokens to Bob (erc20x is not the contract owner)
        let value_x_steal = U256::from(100e18);
        let calldata_x_mint =
            get_calldata(selector_x_mint, (BOB, value_x_steal, erc20).abi_encode());

        let only_owner_result = run_tx(&mut db, &erc20x, calldata_x_mint, &BOB)
            .expect_err("Mint transaction succeeded");
        assert!(
            only_owner_result.matches_custom_error("ERC20Error::OnlyOwner"),
            "Incorrect error"
        );

        // Attempt cross-transfer 100 tokens (without allowance) from Alice to Bob
        let calldata_x_transfer_from = get_calldata(
            selector_x_transfer_from,
            (ALICE, value_x_steal, erc20).abi_encode(),
        );

        let zero_amount_result = run_tx(&mut db, &erc20x, calldata_x_transfer_from.clone(), &BOB)
            .expect_err("Transfer transaction succeeded");
        assert!(
            zero_amount_result.matches_custom_error("ERC20Error::ZeroAmount"),
            "Incorrect error signature"
        );

        // Approve ERC20x to spend 10 tokens from Alice
        let value_approve = U256::from(10e18);
        let calldata_approve = get_calldata(selector_approve, (erc20x, value_approve).abi_encode());

        let approve_result = run_tx(&mut db, &erc20, calldata_approve.clone(), &ALICE).unwrap();
        assert!(approve_result.status, "Approve transaction failed");

        // Attempt cross-transfer 100 tokens (with a 10 token allowance) from Alice to Bob
        let fallback_x_transfer_result =
            run_tx(&mut db, &erc20x, calldata_x_transfer_from, &BOB).expect("Error executing tx");
        assert!(
            fallback_x_transfer_result.status,
            "Cross-transfer from transaction failed"
        );

        // Check Bob's balance
        let calldata_balance_of = get_calldata(selector_balance_of, BOB.abi_encode());

        let bob_balance_result = run_tx(&mut db, &erc20, calldata_balance_of.clone(), &BOB)
            .expect("Error executing tx")
            .output;

        assert_eq!(
            U256::from_be_bytes::<32>(bob_balance_result.as_slice().try_into().unwrap()),
            value_approve,
            "Incorrect balance"
        );
    }

    #[test]
    fn test_string_error() {
        let (mut db, erc20) = setup_erc20(ALICE);
        let erc20x = setup_erc20x(&mut db);

        // Define fn selectors
        let selector_panic = get_selector_from_sig("panics()");
        let selector_x_mint_panic = get_selector_from_sig("x_mint_panics(address,uint256,address)");

        // Attempt a call that panics with a string msg
        let panic_result = run_tx(
            &mut db,
            &erc20x,
            get_calldata(selector_panic, vec![]),
            &ALICE,
        )
        .expect_err("Tx succeeded");
        assert!(
            panic_result.matches_string_error("This function always panics"),
            "Incorrect error"
        );

        // Attempt a call that panics with a string msg
        let calldata_x_mint = get_calldata(
            selector_x_mint_panic,
            (ALICE, U256::from(1e18), erc20).abi_encode(),
        );

        let x_mint_panic_result =
            run_tx(&mut db, &erc20x, calldata_x_mint, &ALICE).expect_err("Tx succeeded");
        assert!(
            x_mint_panic_result.matches_string_error("ERC20::mint() failed!: OnlyOwner"),
            "Incorrect error"
        );
    }
}
