#![no_std]
#![no_main]

use contract_derive::{contract, payable, storage, Event};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{msg_sender, revert_with_error};

use alloy_core::primitives::{Address, U256, Bytes};

extern crate alloc;

// -- EVENTS -------------------------------------------------------------------
#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub amount: U256,
}

#[derive(Event)]
pub struct Approval {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub spender: Address,
    pub amount: U256,
}

#[derive(Event)]
pub struct OwnershipTransferred {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
}

// -- CONTRACT -----------------------------------------------------------------
#[storage]
pub struct ERC20 {
    total_supply: Slot<U256>,
    balance_of: Mapping<Address, Slot<U256>>,
    allowance_of: Mapping<Address, Mapping<Address, Slot<U256>>>,
    owner: Slot<Address>,
    name: Slot<U256>,
    symbol: Slot<U256>,
    decimals: Slot<U256>,
}

#[contract]
impl ERC20 {
    // -- CONSTRUCTOR ----------------------------------------------------------
    pub fn new(owner: Address, name: Bytes, symbol: Bytes, decimals: U256) -> Self {
        // Init the contract
        let mut erc20 = ERC20::default();

        // Update state
        erc20.owner.write(owner);

        // Convert Bytes to a 32-byte array (U256) and write to storage
        let mut name_bytes = [0u8; 32];
        name_bytes[..name.len()].copy_from_slice(&name);
        erc20.name.write(U256::from_be_bytes(name_bytes));

        let mut symbol_bytes = [0u8; 32];
        symbol_bytes[..symbol.len()].copy_from_slice(&symbol);
        erc20.symbol.write(U256::from_be_bytes(symbol_bytes));

        erc20.decimals.write(decimals);

        // Return the initialized contract
        erc20
    }

    // -- STATE MODIFYING FUNCTIONS --------------------------------------------
    #[payable]
    pub fn mint(&mut self, to: Address, amount: U256) -> bool {
        // Perform sanity checks
        if msg_sender() != self.owner.read() { 
            revert_with_error(b"OnlyOwner");
            return false;
        }
        if amount == U256::ZERO { 
            revert_with_error(b"ZeroAmount");
            return false;
        }
        if to == Address::ZERO { 
            revert_with_error(b"ZeroAddress");
            return false;
        }

        // Increase user balance
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);

        // Increase total supply
        self.total_supply += amount;
        
        // Emit event + return `true` to stick to (EVM) ERC20 convention
        log::emit(Transfer::new(Address::ZERO, to, amount));
        true
    }

    pub fn approve(&mut self, spender: Address, amount: U256) -> bool {
        let owner = msg_sender();

        // Perform sanity checks
        if spender == Address::ZERO { 
            revert_with_error(b"ZeroAddress");
            return false;
        }
        if spender == owner { 
            revert_with_error(b"SelfApproval");
            return false;
        }

        // Update state
        self.allowance_of[owner][spender].write(amount);

        // Emit event + return 
        log::emit(Approval::new(owner, spender, amount));
        true
    }

    pub fn transfer(&mut self, to: Address, amount: U256) -> bool {
        let from = msg_sender();

        // Perform sanity checks
        if to == Address::ZERO { 
            revert_with_error(b"ZeroAddress");
            return false;
        }
        if amount == U256::ZERO { 
            revert_with_error(b"ZeroAmount");
            return false;
        }
        if from == to { 
            revert_with_error(b"SelfTransfer");
            return false;
        }

        // Read user balances
        let from_balance = self.balance_of[from].read();
        let to_balance = self.balance_of[to].read();

        // Ensure enough balance
        if from_balance < amount { 
            revert_with_error(b"InsufficientBalance");
            return false;
        }

        // Update state
        self.balance_of[from].write(from_balance - amount);
        self.balance_of[to].write(to_balance + amount);

        // Emit event + return 
        log::emit(Transfer::new(from, to, amount));
        true
    }

    pub fn transfer_from(&mut self, from: Address, to: Address, amount: U256) -> bool {
        let msg_sender = msg_sender();

        // Perform sanity checks
        if to == Address::ZERO { 
            revert_with_error(b"ZeroAddress");
            return false;
        }
        if amount == U256::ZERO { 
            revert_with_error(b"ZeroAmount");
            return false;
        }
        if from == to { 
            revert_with_error(b"SelfTransfer");
            return false;
        }

        // Ensure enough allowance
        let allowance = self.allowance_of[from][msg_sender].read();
        if allowance < amount { 
            revert_with_error(b"InsufficientAllowance");
            return false;
        }

        // Ensure enough balance
        let from_balance = self.balance_of[from].read();
        if from_balance < amount { 
            revert_with_error(b"InsufficientBalance");
            return false;
        }

        // Update state
        self.allowance_of[from][msg_sender].write(allowance - amount);
        self.balance_of[from].write(from_balance - amount);
        
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);

        // Emit event + return 
        log::emit(Transfer::new(from, to, amount));
        true
    }

    pub fn transfer_ownership(&mut self, new_owner: Address) -> bool {
        let from = msg_sender();

        // Perform safety check 
        if from != self.owner.read() { 
            revert_with_error(b"OnlyOwner");
            return false;
        } 
        if from == new_owner { 
            revert_with_error(b"SelfTransfer");
            return false;
        } 

        // Update state
        self.owner.write(new_owner);

        // Emit event + return 
        log::emit(OwnershipTransferred::new(from, new_owner));
        true
    }

    // -- READ-ONLY FUNCTIONS --------------------------------------------------
    pub fn owner(&self) -> Address {
        self.owner.read()
    }

    pub fn total_supply(&self) -> U256 {
        self.total_supply.read()
    }

    pub fn balance_of(&self, owner: Address) -> U256 {
        self.balance_of[owner].read()
    }

    pub fn allowance(&self, owner: Address, spender: Address) -> U256 {
        self.allowance_of[owner][spender].read()
    }

    // -- READ-ONLY FUNCTIONS (METADATA) ---------------------------------------
    pub fn name(&self) -> U256 {
        self.name.read()
    }

    pub fn symbol(&self) -> U256 {
        self.symbol.read()
    }

    pub fn decimals(&self) -> U256 {
        self.decimals.read()
    }
}
