//! ERC20 Token Implementation (R55/RISC-V)
//!
//! A complete ERC20 token implementation compiled to RISC-V bytecode for execution
//! in the Hydra EVM. This contract provides standard token functionality with ownership
//! controls and metadata (name, symbol, decimals).
//!
//! ## Features
//! - Standard ERC20 operations: `transfer`, `approve`, `transferFrom`
//! - Ownership-based minting with `mint` function
//! - Token metadata: `name`, `symbol`, `decimals`
//! - Ownership transfer capability
//! - Custom error types for clear revert reasons
//! - Event emission for all state changes
//!
//! ## Storage Optimization
//! - Uses `FixedBytes<32>` for name/symbol (efficient on-chain storage)
//! - Nested mappings for allowances
//! - Slot-based storage for all state variables

#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, storage, Event, Error};
use eth_riscv_runtime::types::*;

use alloy_core::primitives::{Address, U256};

extern crate alloc;
use alloc::string::String;

// =============================================================================
// EVENTS
// =============================================================================

/// Emitted when tokens are transferred between accounts
/// Minting emits Transfer with `from` = Address::ZERO
#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub amount: U256,
}

/// Emitted when an allowance is set via approve()
#[derive(Event)]
pub struct Approval {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub spender: Address,
    pub amount: U256,
}

/// Emitted when contract ownership is transferred
#[derive(Event)]
pub struct OwnershipTransferred {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
}

// =============================================================================
// ERRORS
// =============================================================================

/// Custom error types for ERC20 operations
/// These provide clear revert reasons and maintain Solidity ERC20 parity
#[derive(Error)]
pub enum ERC20Error {
    /// Only the contract owner can perform this operation
    OnlyOwner,
    /// Account has insufficient balance for the transfer
    InsufficientBalance(U256),
    /// Spender has insufficient allowance for transferFrom
    InsufficientAllowance(U256),
    /// Cannot approve yourself as spender
    SelfApproval,
    /// Cannot transfer to yourself
    SelfTransfer,
    /// Transfer/mint amount must be non-zero
    ZeroAmount,
    /// Address cannot be zero address
    ZeroAddress,
}

// =============================================================================
// CONTRACT STATE
// =============================================================================

/// ERC20 token contract with ownership controls
/// 
/// Storage layout uses Slot-based persistence for all state variables.
/// Dynamic strings for name/symbol are stored via StringSlot with hashed multi-slot backing.
#[storage]
pub struct ERC20 {
    /// Total token supply across all holders
    total_supply: Slot<U256>,
    /// Mapping from address to token balance
    balance_of: Mapping<Address, Slot<U256>>,
    /// Nested mapping: owner -> spender -> allowance amount
    allowance_of: Mapping<Address, Mapping<Address, Slot<U256>>>,
    /// Contract owner (authorized to mint)
    owner: Slot<Address>,
    /// Token decimals (typically 18 for standard ERC20)
    decimals: Slot<U256>,
    /// Token name
    name: DynamicSlot<String>,
    /// Token symbol
    symbol: DynamicSlot<String>,
}

// =============================================================================
// IMPLEMENTATION
// =============================================================================

#[contract]
impl ERC20 {
    // -------------------------------------------------------------------------
    // CONSTRUCTOR
    // -------------------------------------------------------------------------
    
    /// Initializes a new ERC20 token with metadata
    /// 
    /// # Arguments
    /// * `owner` - Address that will own the contract and have minting rights
    /// * `name` - Human-readable token name (e.g., "Ethereum")
    /// * `symbol` - Trading symbol (e.g., "ETH")
    /// * `decimals` - Number of decimal places (typically 18)
    /// 
    /// # Returns
    /// Initialized ERC20 contract instance
    pub fn new(owner: Address, name: String, symbol: String, decimals: U256) -> Self {
        let mut erc20 = ERC20::default();

        // Update state
        erc20.owner.write(owner);
        erc20.decimals.write(decimals);

        // Store dynamic metadata
        erc20.name.write(name);
        erc20.symbol.write(symbol);

        erc20
    }

    // -------------------------------------------------------------------------
    // STATE-MODIFYING FUNCTIONS
    // -------------------------------------------------------------------------
    
    /// Mints new tokens to a specified address (owner only)
    /// 
    /// Increases recipient balance and total supply.
    /// Emits Transfer event with `from` = Address::ZERO.
    /// 
    /// # Arguments
    /// * `to` - Recipient address
    /// * `amount` - Tokens to mint
    /// 
    /// # Returns
    /// * `Ok(true)` on success
    /// * `Err(ERC20Error)` on validation failure
    #[payable]
    pub fn mint(&mut self, to: Address, amount: U256) -> Result<bool, ERC20Error> {
        // Access control: only owner can mint
        if msg_sender() != self.owner.read() { return Err(ERC20Error::OnlyOwner) }; 
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };
        if to == Address::ZERO { return Err(ERC20Error::ZeroAddress) };

        // Update recipient balance
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);

        // Update total supply
        self.total_supply += amount;
        
        // Emit Transfer event (from = 0x0 for mints)
        log::emit(Transfer::new(Address::ZERO, to, amount));
        Ok(true)
    }

    /// Sets spending allowance for a spender
    /// 
    /// Allows `spender` to withdraw up to `amount` tokens via transferFrom().
    /// 
    /// # Arguments
    /// * `spender` - Address authorized to spend
    /// * `amount` - Maximum spendable amount
    /// 
    /// # Returns
    /// * `Ok(true)` on success
    /// * `Err(ERC20Error)` on validation failure
    pub fn approve(&mut self, spender: Address, amount: U256) -> Result<bool, ERC20Error> {
        let owner = msg_sender();

        // Validation checks
        if spender == Address::ZERO { return Err(ERC20Error::ZeroAddress) };
        if spender == owner { return Err(ERC20Error::SelfApproval) };

        // Update allowance mapping
        self.allowance_of[owner][spender].write(amount);

        log::emit(Approval::new(owner, spender, amount));
        Ok(true)
    }

    /// Transfers tokens from caller to another address
    /// 
    /// # Arguments
    /// * `to` - Recipient address
    /// * `amount` - Tokens to transfer
    /// 
    /// # Returns
    /// * `Ok(true)` on success
    /// * `Err(ERC20Error)` on validation failure or insufficient balance
    pub fn transfer(&mut self, to: Address, amount: U256) -> Result<bool, ERC20Error> {
        let from = msg_sender();

        // Validation checks
        if to == Address::ZERO { return Err(ERC20Error::ZeroAddress) };
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };
        if from == to { return Err(ERC20Error::SelfTransfer) };

        // Load current balances
        let from_balance = self.balance_of[from].read();
        let to_balance = self.balance_of[to].read();

        // Check sufficient balance
        if from_balance < amount { return Err(ERC20Error::InsufficientBalance(from_balance)) }

        // Update balances atomically
        self.balance_of[from].write(from_balance - amount);
        self.balance_of[to].write(to_balance + amount);

        log::emit(Transfer::new(from, to, amount));
        Ok(true)
    }

    /// Transfers tokens on behalf of another address using allowance
    /// 
    /// Caller must have sufficient allowance from `from` address.
    /// 
    /// # Arguments
    /// * `from` - Token owner (must have approved caller)
    /// * `to` - Recipient address
    /// * `amount` - Tokens to transfer
    /// 
    /// # Returns
    /// * `Ok(true)` on success
    /// * `Err(ERC20Error)` on insufficient allowance or balance
    pub fn transfer_from(&mut self, from: Address, to: Address, amount: U256) -> Result<bool, ERC20Error> {
        let msg_sender = msg_sender();

        // Validation checks
        if to == Address::ZERO { return Err(ERC20Error::ZeroAddress) };
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };
        if from == to { return Err(ERC20Error::SelfTransfer) };

        // Check allowance (caller must be approved by `from`)
        let allowance = self.allowance_of[from][msg_sender].read();
        if allowance < amount { return Err(ERC20Error::InsufficientAllowance(allowance)) };

        // Check balance
        let from_balance = self.balance_of[from].read();
        if from_balance < amount { return Err(ERC20Error::InsufficientBalance(from_balance)) };

        // Update allowance (decrease by amount spent)
        self.allowance_of[from][msg_sender].write(allowance - amount);
        
        // Update balances atomically
        self.balance_of[from].write(from_balance - amount);
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);

        log::emit(Transfer::new(from, to, amount));
        Ok(true)
    }

    /// Transfers contract ownership to a new address (owner only)
    /// 
    /// New owner will have minting rights.
    /// 
    /// # Arguments
    /// * `new_owner` - New contract owner
    /// 
    /// # Returns
    /// * `Ok(true)` on success
    /// * `Err(ERC20Error::OnlyOwner)` if caller is not owner
    pub fn transfer_ownership(&mut self, new_owner: Address) -> Result<bool, ERC20Error> {
        let from = msg_sender();

        // Access control + validation
        if from != self.owner.read() { return Err(ERC20Error::OnlyOwner) }; 
        if from == new_owner { return Err(ERC20Error::SelfTransfer) }; 

        // Update owner
        self.owner.write(new_owner);

        log::emit(OwnershipTransferred::new(from, new_owner));
        Ok(true)
    }

    // -------------------------------------------------------------------------
    // VIEW FUNCTIONS
    // -------------------------------------------------------------------------
    
    /// Returns the current contract owner address
    pub fn owner(&self) -> Address {
        self.owner.read()
    }

    /// Returns the total token supply
    pub fn total_supply(&self) -> U256 {
        self.total_supply.read()
    }

    /// Returns the token balance of an address
    pub fn balance_of(&self, owner: Address) -> U256 {
        self.balance_of[owner].read()
    }

    /// Returns the allowance granted by owner to spender
    pub fn allowance(&self, owner: Address, spender: Address) -> U256 {
        self.allowance_of[owner][spender].read()
    }

    /// Returns token decimals (standard is 18)
    pub fn decimals(&self) -> U256 {
        self.decimals.read()
    }

    // -------------------------------------------------------------------------
    // METADATA (IERC20Metadata)
    // -------------------------------------------------------------------------
    
    /// Returns token name
    pub fn name(&self) -> String { self.name.read() }

    /// Returns token symbol
    pub fn symbol(&self) -> String { self.symbol.read() }
}
