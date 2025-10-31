//! Bridged ERC20 Token Implementation (R55/RISC-V)
//!
//! A bridged ERC20 token compiled to RISC-V bytecode for execution in the Hydra EVM.
//! This contract mirrors BridgedERC20 semantics: bridge-controlled mint/burn, metadata,
//! and decimals sourced from the origin token.
//!
//! ## Features
//! - Standard ERC20 operations: `transfer`, `approve`, `transferFrom`
//! - Bridge-controlled mint/burn
//! - Token metadata: `name`, `symbol`, `decimals` (from source token)
//! - Custom error types for clear revert reasons
//! - Event emission for all state changes
//!
//! ## Storage
//! - Uses `DynamicSlot<String>` for name/symbol
//! - Slot-based storage for all fixed-size values
//! - Nested mappings for allowances

#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, storage, Event, Error};
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

/// Bridged ERC20 token with bridge-controlled mint/burn
/// 
/// Storage layout uses Slot-based persistence for fixed-size values.
/// Dynamic strings for name/symbol are stored via DynamicSlot<String>.
#[storage]
pub struct BridgedERC20 {
    /// Total token supply across all holders
    total_supply: Slot<U256>,
    /// Mapping from address to token balance
    balance_of: Mapping<Address, Slot<U256>>,
    /// Nested mapping: owner -> spender -> allowance amount
    allowance_of: Mapping<Address, Mapping<Address, Slot<U256>>>,
    /// Decimals for the source token (returned by decimals())
    source_token_decimals: Slot<U256>,
    /// Address of the source token (L1/L2 counterpart)
    source_token_address: Slot<Address>,
    /// Bridge contract address (only this address can mint/burn)
    bridge_address: Slot<Address>,
    /// Token name
    name: DynamicSlot<String>,
    /// Token symbol
    symbol: DynamicSlot<String>,
}

// =============================================================================
// IMPLEMENTATION
// =============================================================================

#[contract]
impl BridgedERC20 {
    // -------------------------------------------------------------------------
    // CONSTRUCTOR
    // -------------------------------------------------------------------------
    
    /// Initializes a new bridged ERC20 token
    /// 
    /// # Arguments
    /// * `name` - Human-readable token name
    /// * `symbol` - Trading symbol
    /// * `decimals` - Decimals for the source token
    /// * `source_token` - Address of the source token on the origin chain
    /// 
    /// # Returns
    /// Initialized ERC20 contract instance
    pub fn new(name: String, symbol: String, decimals: U256, source_token: Address, erc20_bridge: Address) -> Self {
        let mut erc20 = BridgedERC20::default();

        // Bridge authority and source info
        erc20.bridge_address.write(erc20_bridge);
        erc20.source_token_decimals.write(decimals);
        erc20.source_token_address.write(source_token);

        // Store dynamic metadata
        erc20.name.write(name);
        erc20.symbol.write(symbol);

        erc20
    }

    // -------------------------------------------------------------------------
    // STATE-MODIFYING FUNCTIONS
    // -------------------------------------------------------------------------
    
    /// Mints new tokens to a specified address (bridge only)
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
    pub fn mint(&mut self, to: Address, amount: U256) {
        // Access control: only bridge can mint
        if msg_sender() != self.bridge_address.read() { eth_riscv_runtime::revert(); };
        if amount == U256::ZERO { eth_riscv_runtime::revert(); };
        if to == Address::ZERO { eth_riscv_runtime::revert(); };

        // Update recipient balance
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);

        // Update total supply
        self.total_supply += amount;
        
        // Emit Transfer event (from = 0x0 for mints)
        log::emit(Transfer::new(Address::ZERO, to, amount));
    }

    /// Burns tokens from the bridge address (bridge only)
    /// Decreases sender balance and total supply.
    pub fn burn(&mut self, amount: U256) {
        // Access control: only bridge can burn
        let bridge = msg_sender();
        if bridge != self.bridge_address.read() { eth_riscv_runtime::revert(); };
        if amount == U256::ZERO { eth_riscv_runtime::revert(); };

        let bal = self.balance_of[bridge].read();
        if bal < amount { eth_riscv_runtime::revert(); };

        self.balance_of[bridge].write(bal - amount);
        self.total_supply.write(self.total_supply.read() - amount);

        log::emit(Transfer::new(bridge, Address::ZERO, amount));
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
    pub fn transferFrom(&mut self, from: Address, to: Address, amount: U256) -> Result<bool, ERC20Error> {
        let caller = msg_sender();

        if to == Address::ZERO { return Err(ERC20Error::ZeroAddress) };
        if amount == U256::ZERO { return Err(ERC20Error::ZeroAmount) };
        if from == to { return Err(ERC20Error::SelfTransfer) };

        let allowance = self.allowance_of[from][caller].read();
        if allowance < amount { return Err(ERC20Error::InsufficientAllowance(allowance)) };

        let from_balance = self.balance_of[from].read();
        if from_balance < amount { return Err(ERC20Error::InsufficientBalance(from_balance)) };

        self.allowance_of[from][caller].write(allowance - amount);
        self.balance_of[from].write(from_balance - amount);
        let to_balance = self.balance_of[to].read();
        self.balance_of[to].write(to_balance + amount);

        log::emit(Transfer::new(from, to, amount));
        Ok(true)
    }

    // No ownership transfer in bridged token; bridge is immutable authority.

    // -------------------------------------------------------------------------
    // VIEW FUNCTIONS
    // -------------------------------------------------------------------------
    
    /// ERC20 camelCase view: totalSupply()
    pub fn totalSupply(&self) -> U256 { self.total_supply.read() }

    /// ERC20 camelCase view: balanceOf(address)
    pub fn balanceOf(&self, owner: Address) -> U256 {
        self.balance_of[owner].read()
    }

    /// Returns the allowance granted by owner to spender
    pub fn allowance(&self, owner: Address, spender: Address) -> U256 {
        self.allowance_of[owner][spender].read()
    }

    /// Returns token decimals (from source token)
    pub fn decimals(&self) -> U256 { self.source_token_decimals.read() }

    /// Getter parity with Solidity public immutable: sourceTokenDecimals()
    pub fn sourceTokenDecimals(&self) -> U256 { self.source_token_decimals.read() }

    /// Getter parity with Solidity public immutable: sourceTokenAddress()
    pub fn sourceTokenAddress(&self) -> Address { self.source_token_address.read() }

    /// Getter parity with Solidity public immutable: bridgeAddress()
    pub fn bridgeAddress(&self) -> Address { self.bridge_address.read() }

    // -------------------------------------------------------------------------
    // METADATA (IERC20Metadata)
    // -------------------------------------------------------------------------
    
    /// Returns token name
    pub fn name(&self) -> String { self.name.read() }

    /// Returns token symbol
    pub fn symbol(&self) -> String { self.symbol.read() }
}
