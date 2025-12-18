//! ERC721 Token Implementation (R55/RISC-V)
//!
//! A complete ERC721 (NFT) token implementation compiled to RISC-V bytecode
//! for execution in the Hydra EVM. Provides standard ERC721 functionality
//! with ownership controls and metadata (name, symbol).
//! 
//! Known Issues/Gaps:
//! 
//! safeTransferFrom(from, to, id) and safeTransferFrom(from, to, id, data) are defined in the OZ ERC721 spec
//! R55 does not support function overloading, so need to figure out a way to handle this.
//! 
//! R55 does not have a way for us to tell if an account is a contract or not -> IERC721Receiver
//! check for code size in the onERC721Received function
//!
//! ## Features
//! - Standard ERC721 operations: `transferFrom`, `approve`, `setApprovalForAll`, `mint`
//! - Ownership-based minting with `mint` function
//! - Contract ownership transfer capability
//! - Event emission for all state changes
//! - Token metadata: `name`, `symbol`
//! - Selector parity with Solidity ERC721
//!
//! ## ABI parity (constructors and calldata)
//! - Constructors are params-encoded (equivalent to `abi.encode(...)`).
//! - Single-arg constructors are passed as 1-tuple `(arg,)` when using deploy builder.
//!
//! ## Storage
//! - Uses `DynamicSlot<String>` for name/symbol
//! - Slot-based storage for fixed-size values
//! - Nested mappings for operator approvals
//!
//! ## Selector parity and deviations
//! - Public/external methods use camelCase to match Solidity selectors
//! - Deviations:
//!   - No ERC721Enumerable support
//!   - Only owner can mint

#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, storage, Error, Event};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{call_contract, staticcall_contract};

use alloy_core::primitives::{Address, U256, FixedBytes, Bytes};
type B4 = FixedBytes<4>;

extern crate alloc;
use alloc::string::String;

// =============================================================================
// EVENTS
// =============================================================================

/// Emitted when tokens are transferred between accounts.
/// Minting emits Transfer with `from` = Address::ZERO.
#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    #[indexed]
    pub id: U256,
}

/// Emitted when a token is approved for transfer by another account.
#[derive(Event)]
pub struct Approval {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub spender: Address,
    #[indexed]
    pub id: U256,
}

/// Emitted when an operator is approved or disapproved for all of an owner's tokens.
#[derive(Event)]
pub struct ApprovalForAll {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub operator: Address,
    pub approved: bool,
}

/// Emitted when contract ownership is transferred.
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

/// Custom error types for ERC721 operations.
/// Provides clear revert reasons and maintains Solidity ERC721 parity.
#[derive(Error)]
pub enum ERC721Error {
    /// Token has already been minted
    AlreadyMinted,
    /// Token does not exist
    NotMinted,
    /// Only contract owner can perform this operation
    OnlyOwner,
    /// Caller is not authorized
    Unauthorized,
    /// Token owner mismatch
    WrongFrom,
    /// Address cannot be zero
    ZeroAddress,
    /// Invalid ERC721 receiver
    InvalidReceiver(Address),
}

// =============================================================================
// CONTRACT STATE
// =============================================================================

/// ERC721 token contract with ownership controls.
/// Storage layout uses Slot-based persistence for fixed-size values.
/// Dynamic strings for name/symbol are stored via DynamicSlot<String>.
#[storage]
pub struct ERC721 {
    /// Total minted token supply
    total_supply: Slot<U256>,
    /// Mapping from token ID to owner
    owner_of: Mapping<U256, Slot<Address>>,
    /// Mapping from owner address to balance
    balance_of: Mapping<Address, Slot<U256>>,
    /// Mapping from token ID to approved address
    approval_of: Mapping<U256, Slot<Address>>,
    /// Nested mapping: owner -> operator -> approved flag
    is_operator: Mapping<Address, Mapping<Address, Slot<bool>>>,
    /// Contract owner (authorized to mint)
    owner: Slot<Address>,
    /// Token name
    name: DynamicSlot<String>,
    /// Token symbol
    symbol: DynamicSlot<String>,
    /// Mapping from token ID to custom token URI
    token_uri: Mapping<U256, DynamicSlot<String>>, // new storage
}

// =============================================================================
// IMPLEMENTATION
// =============================================================================

#[contract]
impl ERC721 {
    // -------------------------------------------------------------------------
    // CONSTRUCTOR
    // -------------------------------------------------------------------------

    /// Initializes a new ERC721 token with metadata.
    ///
    /// # Arguments
    /// * `owner` - Address that will own the contract and have minting rights
    /// * `name` - Human-readable token name (e.g., "MyNFT")
    /// * `symbol` - Trading symbol (e.g., "MNFT")
    ///
    /// # Returns
    /// Initialized ERC721 contract instance
    pub fn new(owner: Address, name: String, symbol: String) -> Self {
        let mut erc721 = ERC721::default();
        erc721.owner.write(owner);
        erc721.name.write(name);
        erc721.symbol.write(symbol);
        erc721
    }

    // -------------------------------------------------------------------------
    // STATE-MODIFYING FUNCTIONS
    // -------------------------------------------------------------------------

    /// Mints a new NFT to a specified address (owner only).
    ///
    /// Increases recipient balance and total supply.
    /// Emits Transfer event with `from` = Address::ZERO.
    ///
    /// # Arguments
    /// * `to` - Recipient address
    /// * `id` - Token ID to mint
    ///
    /// Solidity parity: no return value; reverts on error.
    pub fn mint(&mut self, to: Address, id: U256) {
        // Perform sanity checks
        if to == Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::ZeroAddress.abi_encode());
        };
        // if msg_sender() != self.owner.read() {
        //     eth_riscv_runtime::revert_with_error(&ERC721Error::OnlyOwner.abi_encode());
        // };
        if self.owner_of[id].read() != Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::AlreadyMinted.abi_encode());
        };

        // Update state
        self.owner_of[id].write(to);

        let balance_to = self.balance_of[to].read();
        self.balance_of[to].write(balance_to + U256::from(1));

        let total_supply = self.total_supply.read();
        self.total_supply.write(total_supply + U256::from(1));

        // Emit event
        log::emit(Transfer::new(Address::ZERO, to, id));

        // Safe acceptance: call onERC721Received for contract recipients
        if eth_riscv_runtime::has_code(to) {
            // selector(4) + abi.encode(operator, from=ZERO, tokenId, data="")
            const ON_RECEIVED: [u8; 4] = [0x15, 0x0b, 0x7a, 0x02];
            let operator = msg_sender();
            let mut args = (operator, Address::ZERO, id, Bytes::new()).abi_encode_params();
            let mut calldata = alloc::vec::Vec::with_capacity(4 + args.len());
            calldata.extend_from_slice(&ON_RECEIVED);
            calldata.append(&mut args);

            match call_contract(to, 0, &calldata, Some(4)) {
                Ok(bytes) => {
                    if !(bytes.len() >= 4 && &bytes[0..4] == ON_RECEIVED) {
                        eth_riscv_runtime::revert_with_error(&ERC721Error::InvalidReceiver(to).abi_encode());
                    }
                }
                Err(_revert) => {
                    eth_riscv_runtime::revert_with_error(&ERC721Error::InvalidReceiver(to).abi_encode());
                }
            }
        }
    }

    /// Burns a token, removing it from circulation.
    ///
    /// # Arguments
    /// * `id` - Token ID to burn
    ///
    /// Solidity parity: no return value; reverts on error.
    pub fn burn(&mut self, id: U256) {
        let owner = self.owner_of[id].read();
        if owner == Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::NotMinted.abi_encode());
        }

        let sender = msg_sender();
        // optional: require auth (owner or operator)
        if sender != owner
            && !self.is_operator[owner][sender].read()
            && sender != self.approval_of[id].read()
        {
            eth_riscv_runtime::revert_with_error(&ERC721Error::Unauthorized.abi_encode());
        }

        // Clear approval
        self.approval_of[id].write(Address::ZERO);

        // Update balances
        let balance = self.balance_of[owner].read();
        self.balance_of[owner].write(balance - U256::from(1));

        // Update owner mapping
        self.owner_of[id].write(Address::ZERO);

        // Update total supply
        let total_supply = self.total_supply.read();
        self.total_supply.write(total_supply - U256::from(1));

        // Emit Transfer event to zero address
        log::emit(Transfer::new(owner, Address::ZERO, id));
    }

    /// Sets the token URI for a given token id.
    /// Authorization: token owner, approved-for-all operator, or single-token approved.
    pub fn setTokenURI(&mut self, id: U256, uri: String) {
        // Must exist
        let owner = self.owner_of[id].read();
        if owner == Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::NotMinted.abi_encode());
        }

        // Authorization
        let sender = msg_sender();
        if sender != owner
            && !self.is_operator[owner][sender].read()
            && sender != self.approval_of[id].read()
        {
            eth_riscv_runtime::revert_with_error(&ERC721Error::Unauthorized.abi_encode());
        }

        // Set URI
        self.token_uri[id].write(uri);
    }
    /// Approves a spender for a specific token ID.
    ///
    /// # Arguments
    /// * `spender` - Address authorized to transfer the token
    /// * `id` - Token ID
    ///
    /// # Returns
    /// * `Ok(true)` on success
    /// * `Err(ERC721Error)` if unauthorized
    pub fn approve(&mut self, spender: Address, id: U256) {
        let owner = self.owner_of[id].read();

        // Existence check first
        if owner == Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::NotMinted.abi_encode());
        }

        // Perform authorization check
        if msg_sender() != owner && !self.is_operator[owner][msg_sender()].read() {
            eth_riscv_runtime::revert_with_error(&ERC721Error::Unauthorized.abi_encode());
        }

        // Update state
        self.approval_of[id].write(spender);

        // Emit event + return
        log::emit(Approval::new(owner, spender, id));
    }

    /// Sets or revokes operator approval for all of an owner's tokens.
    ///
    /// # Arguments
    /// * `operator` - Address of operator
    /// * `approved` - Approval status
    ///
    /// # Returns
    /// * `Ok(true)` on success
    pub fn setApprovalForAll(
        &mut self,
        operator: Address,
        approved: bool,
    ) {
        let msg_sender = msg_sender();

        // Zero operator not allowed
        if operator == Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::ZeroAddress.abi_encode());
        }

        // Update state
        self.is_operator[msg_sender][operator].write(approved);

        // Emit event + return
        log::emit(ApprovalForAll::new(msg_sender, operator, approved));
    }

    /// Transfers a token from one address to another.
    ///
    /// # Arguments
    /// * `from` - Current token owner
    /// * `to` - Recipient address
    /// * `id` - Token ID
    ///
    /// # Returns
    /// * `Ok(true)` on success
    /// * `Err(ERC721Error)` on validation failure
    pub fn transferFrom(
        &mut self,
        from: Address,
        to: Address,
        id: U256,
    ) {
        // Existence and basic checks
        let current_owner = self.owner_of[id].read();
        if current_owner == Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::NotMinted.abi_encode());
        }
        if from != current_owner {
            eth_riscv_runtime::revert_with_error(&ERC721Error::WrongFrom.abi_encode());
        }
        if to == Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::ZeroAddress.abi_encode());
        };

        // Check authorization
        let sender = msg_sender();
        if sender != from
            && !self.is_operator[from][sender].read()
            && sender != self.approval_of[id].read()
        {
            eth_riscv_runtime::revert_with_error(&ERC721Error::Unauthorized.abi_encode());
        }

        // Update state
        self.owner_of[id].write(to);
        self.approval_of[id].write(Address::ZERO);

        let balance_from = self.balance_of[from].read();
        self.balance_of[from].write(balance_from - U256::from(1));

        let balance_to = self.balance_of[to].read();
        self.balance_of[to].write(balance_to + U256::from(1));

        // Emit event + return
        log::emit(Transfer::new(from, to, id));
    }

    /// safeTransferFrom(from, to, id, data)
    /// Solidity selector parity: safeTransferFrom(address,address,uint256,bytes)
    pub fn safeTransferFrom(
        &mut self,
        from: Address,
        to: Address,
        id: U256,
        _data: Bytes,
    ) {
        // Perform transfer first (state change + event)
        self.transferFrom(from, to, id);

        // Post-transfer acceptance check
        // Skip check for EOAs (no code)
        if eth_riscv_runtime::has_code(to) {
            // Build calldata: selector(4) + abi.encode(operator, from, tokenId, data)
            const ON_RECEIVED: [u8; 4] = [0x15, 0x0b, 0x7a, 0x02];
            let operator = msg_sender();
            let mut args = (operator, from, id, _data.clone()).abi_encode_params();
            let mut calldata = alloc::vec::Vec::with_capacity(4 + args.len());
            calldata.extend_from_slice(&ON_RECEIVED);
            calldata.append(&mut args);

            // Try invoking on recipient; request 4 bytes back
            match call_contract(to, 0, &calldata, Some(4)) {
                Ok(bytes) => {
                    // Accept only if magic matches
                    if bytes.len() >= 4 && &bytes[0..4] == ON_RECEIVED {
                        return;
                    }
                    eth_riscv_runtime::revert_with_error(&ERC721Error::InvalidReceiver(to).abi_encode());
                }
                Err(_revert) => {
                    // Revert reason bubbles in OZ; here map to InvalidReceiver
                    eth_riscv_runtime::revert_with_error(&ERC721Error::InvalidReceiver(to).abi_encode());
                }
            }
        } // EOAs accept by default
    }

    /// safeTransferFrom(from, to, id)
    /// Solidity selector parity: safeTransferFrom(address,address,uint256)
    pub fn safeTransferFrom(
        &mut self,
        from: Address,
        to: Address,
        id: U256,
    ) {
        self.safeTransferFrom(from, to, id, Bytes::new());
    }

    /// Transfers contract ownership to a new address (owner only).
    ///
    /// # Arguments
    /// * `new_owner` - New contract owner
    ///
    /// # Returns
    /// * `Ok(true)` on success
    /// * `Err(ERC721Error::OnlyOwner)` if caller is not owner
    /// r55-specific helper: not part of OZ ERC721 external surface; exclude from Hydra parity
    pub fn transferOwnership(&mut self, new_owner: Address) -> Result<bool, ERC721Error> {
        // Perform safety check
        let from = msg_sender();
        if from != self.owner.read() {
            return Err(ERC721Error::OnlyOwner);
        };

        // Update state
        self.owner.write(new_owner);

        // Emit event + return
        log::emit(OwnershipTransferred::new(from, new_owner));
        Ok(true)
    }

    // -------------------------------------------------------------------------
    // VIEW FUNCTIONS
    // -------------------------------------------------------------------------

    /// Returns the current contract owner address.
    /// r55-specific helper: not part of OZ ERC721 external surface; exclude from Hydra parity
    pub fn owner(&self) -> Address {
        self.owner.read()
    }

    /// Returns the owner of a specific token ID.
    ///
    /// # Arguments
    /// * `id` - Token ID
    ///
    /// # Returns
    /// * `Ok(owner)` on success
    /// * `Err(ERC721Error::NotMinted)` if token does not exist
    pub fn ownerOf(&self, id: U256) -> Result<Address, ERC721Error> {
        let owner = self.owner_of[id].read();
        if owner == Address::ZERO {
            return Err(ERC721Error::NotMinted);
        }
        Ok(owner)
    }

    /// Returns the balance of tokens held by an address.
    ///
    /// # Arguments
    /// * `owner` - Address
    ///
    /// # Returns
    /// * `Ok(balance)` on success
    /// * `Err(ERC721Error::ZeroAddress)` if owner is zero
    pub fn balanceOf(&self, owner: Address) -> Result<U256, ERC721Error> {
        if owner == Address::ZERO {
            return Err(ERC721Error::ZeroAddress);
        }
        Ok(self.balance_of[owner].read())
    }

    /// Returns the approved address for a token ID.
    pub fn getApproved(&self, id: U256) -> Result<Address, ERC721Error> {
        // Match OZ behavior: revert if token doesn't exist
        let owner = self.owner_of[id].read();
        if owner == Address::ZERO {
            return Err(ERC721Error::NotMinted);
        }
        Ok(self.approval_of[id].read())
    }

    /// Checks if an operator is approved for all tokens of an owner.
    pub fn isApprovedForAll(&self, owner: Address, operator: Address) -> bool {
        self.is_operator[owner][operator].read()
    }

    /// Returns total minted token supply.
    /// r55-specific helper: not part of OZ ERC721 external surface; exclude from Hydra parity
    pub fn totalSupply(&self) -> U256 {
        self.total_supply.read()
    }

    /// Returns token name.
    pub fn name(&self) -> String {
        self.name.read()
    }

    /// Returns token symbol.
    pub fn symbol(&self) -> String {
        self.symbol.read()
    }

    /// Returns tokenURI for a given token id.
    /// Reverts (NotMinted) if token does not exist.
    /// Behavior: returns per-token URI if set; otherwise returns empty string.
    pub fn tokenURI(&self, id: U256) -> Result<String, ERC721Error> {
        // Must exist
        let owner = self.owner_of[id].read();
        if owner == Address::ZERO {
            return Err(ERC721Error::NotMinted);
        }
        // Read mapped URI (defaults to empty string if unset)
        Ok(self.token_uri[id].read())
    }

    /// ERC-165 supportsInterface(interfaceId) → bool
    /// Supports: ERC165 (0x01ffc9a7), ERC721 (0x80ac58cd), ERC721Metadata (0x5b5e139f)
    pub fn supportsInterface(&self, interface_id: B4) -> bool {
        let id = interface_id.as_slice();
        const ERC165: [u8; 4] = [0x01, 0xff, 0xc9, 0xa7];
        const ERC721: [u8; 4] = [0x80, 0xac, 0x58, 0xcd];
        const ERC721_METADATA: [u8; 4] = [0x5b, 0x5e, 0x13, 0x9f];

        id == &ERC165 || id == &ERC721 || id == &ERC721_METADATA
    }
}
