//! Bridged ERC721 Token Implementation (R55/RISC-V)
//!
//! A bridged ERC721 token compiled to RISC-V bytecode for execution in the Hydra EVM.
//! This contract mirrors BridgedERC721 semantics: bridge-controlled mint/burn, metadata,
//! and optional per-token URIs sourced from the origin token.
//!
//! ## Features
//! - Standard ERC721 operations: `transferFrom`, `approve`, `setApprovalForAll`, `mint`, `burn`
//! - Bridge-controlled mint/burn (only bridge can mint/burn tokens)
//! - Metadata: `name`, `symbol`, per-token `tokenURI` mapping
//! - Event emission for transfers and approvals
//!
//! ## Storage
//! - Uses `DynamicSlot<String>` for `name`, `symbol`, and each token’s `tokenURI`
//! - Slot-based storage for fixed-size values, including `total_supply`
//! - Nested mappings for operator approvals and ownership
//!
//! ## Selector parity and deviations
//! - All public/external methods use camelCase names to match Solidity selectors
//! - Deviations from common ERC721 practice:
//!   - No ownership role (bridge is immutable authority)
//!   - Mint/burn restricted to bridge
//!   - No safeTransfer hooks or onERC721Received checks (Hydra-level enforcement)
//! 
//! 
//! 

#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::{contract, payable, storage, Error, Event};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{call_contract, staticcall_contract};

use alloy_core::primitives::{Address, U256, Bytes, FixedBytes};
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

/// BridgedERC721 token contract
#[storage]
pub struct BridgedERC721 {
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
    /// Token name
    name: DynamicSlot<String>,
    /// Token symbol
    symbol: DynamicSlot<String>,
    /// Mapping from token ID to custom token URI
    token_uri: Mapping<U256, DynamicSlot<String>>, // new storage

    /// Address of the source token (L1/L2 counterpart)
    source_token_address: Slot<Address>,
    /// Bridge contract address (only this address can mint/burn)
    bridge_address: Slot<Address>,
}

// =============================================================================
// IMPLEMENTATION
// =============================================================================

#[contract]
impl BridgedERC721 {
    // -------------------------------------------------------------------------
    // CONSTRUCTOR
    // -------------------------------------------------------------------------

    /// Initializes a new BridgedERC721 token with metadata.
    ///
    /// # Arguments
    /// * `name` - Human-readable token name (e.g., "MyNFT")
    /// * `symbol` - Trading symbol (e.g., "MNFT")
    /// * `source_token` - Address of the source token on the origin chain
    /// * `erc721_bridge` - Address of the bridge contract
    ///
    /// # Returns
    /// Initialized BridgedERC721 contract instance
    pub fn new(name: String, symbol: String, source_token: Address, erc721_bridge: Address) -> Self {
        let mut erc721 = BridgedERC721::default();
        erc721.source_token_address.write(source_token);
        erc721.bridge_address.write(erc721_bridge);
        erc721.name.write(name);
        erc721.symbol.write(symbol);
        erc721
    }

    // -------------------------------------------------------------------------
    // STATE-MODIFYING FUNCTIONS
    // -------------------------------------------------------------------------

    /// Mints a new NFT to a specified address (bridge only) with per-token URI.
    ///
    /// Increases recipient balance and total supply.
    /// Emits Transfer event with `from` = Address::ZERO.
    ///
    /// # Arguments
    /// * `to` - Recipient address
    /// * `id` - Token ID to mint
    /// * `token_uri_` - Token URI string to associate with `id`
    ///
    /// Solidity parity: no return value; reverts on error.
    pub fn mint(&mut self, to: Address, id: U256, token_uri_: String) {
        // Access control: only bridge can mint
        if msg_sender() != self.bridge_address.read() {
            eth_riscv_runtime::revert_with_error(&ERC721Error::OnlyOwner.abi_encode());
        }
        if to == Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::ZeroAddress.abi_encode());
        }
        if self.owner_of[id].read() != Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::AlreadyMinted.abi_encode());
        }

        // Update state
        self.owner_of[id].write(to);
        self.token_uri[id].write(token_uri_);

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

    /// Burns an existing ERC721 token (bridge only).
    ///
    /// Reverts if token does not exist or caller is not the bridge.
    /// Emits Transfer(from = owner, to = ZERO).
    ///
    /// # Arguments
    /// * `token_id` - ID of the token to burn
    /// Solidity parity: no return value; reverts on error.
    pub fn burn(&mut self, token_id: U256) {
        if msg_sender() != self.bridge_address.read() {
            eth_riscv_runtime::revert_with_error(&ERC721Error::OnlyOwner.abi_encode());
        }

        let owner = self.owner_of[token_id].read();
        if owner == Address::ZERO {
            eth_riscv_runtime::revert_with_error(&ERC721Error::NotMinted.abi_encode());
        }

        // Clear approval
        self.approval_of[token_id].write(Address::ZERO);

        // Decrement balance
        let bal = self.balance_of[owner].read();
        self.balance_of[owner].write(bal - U256::from(1));

        // Clear owner and token URI
        self.owner_of[token_id].write(Address::ZERO);
        self.token_uri[token_id].write(String::new());

        // Decrement total supply
        let ts = self.total_supply.read();
        self.total_supply.write(ts - U256::from(1));

        // Emit Transfer to zero
        log::emit(Transfer::new(owner, Address::ZERO, token_id));
    }

    /// Approves a spender for a specific token ID.
    ///
    /// # Arguments
    /// * `spender` - Address authorized to transfer the token
    /// * `id` - Token ID
    ///
    /// # Returns
    /// * `Ok(true)` on success
    /// * `Err(ERC721Error)` if unauthorized or non-existent
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
        data: Bytes,
    ) {
        // Perform transfer first (state change + event)
        self.transferFrom(from, to, id);

        // Post-transfer acceptance check
        // Skip check for EOAs (no code)
        if eth_riscv_runtime::has_code(to) {
            // Build calldata: selector(4) + abi.encode(operator, from, tokenId, data)
            const ON_RECEIVED: [u8; 4] = [0x15, 0x0b, 0x7a, 0x02];
            let operator = msg_sender();
            let mut args = (operator, from, id, data.clone()).abi_encode_params();
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

    // -------------------------------------------------------------------------
    // VIEW FUNCTIONS
    // -------------------------------------------------------------------------

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
    /// Reverts if token does not exist.
    pub fn getApproved(&self, id: U256) -> Result<Address, ERC721Error> {
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

    /// Returns the address of the source token.
    pub fn sourceTokenAddress(&self) -> Address {
        self.source_token_address.read()
    }

    /// Returns the address of the bridge contract.
    pub fn bridgeAddress(&self) -> Address {
        self.bridge_address.read()
    }

    /// Returns tokenURI for a given token id.
    /// Reverts (NotMinted) if token does not exist.
    pub fn tokenURI(&self, id: U256) -> Result<String, ERC721Error> {
        let owner = self.owner_of[id].read();
        if owner == Address::ZERO {
            return Err(ERC721Error::NotMinted);
        }
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
