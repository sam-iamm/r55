//! ERC721Bridge (R55/RISC-V)
//!
//! Cross-chain ERC721 bridge, mirroring the Solidity reference interface.
//!
//! Lifecycle overview:
//! - Record token description on source chain and emit a signal.
//! - On destination, verify the signal and deploy `BridgedERC721` counterpart.
//!
//!
//!
//! Solidity refs:
//! /Users/michael/Documents/stack/stack/contracts/src/shared/interfaces/IERC721Bridge.sol
//! /Users/michael/Documents/stack/stack/contracts/src/shared/ERC721Bridge.sol
//! /Users/michael/Documents/stack/stack/contracts/src/shared/tokens/BridgedERC721.sol
//! /Users/michael/Documents/stack/stack/contracts/src/shared/SignalService.sol
//!
//! R55 refs:
//! /Users/michael/Documents/stack/r55/examples/hydra-erc721-bridge/src/lib.rs
//! 

#![no_std]
#![no_main]

use alloy_core::primitives::{keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, U256};
use alloy_sol_types::sol;
use contract_derive::{contract, storage, Event, interface};
use eth_riscv_runtime::revert;
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{InitInterface};
use eth_riscv_runtime::create::Deployable;

extern crate alloc;
use alloc::string::String;

type B32 = FixedBytes<32>;
type B4 = FixedBytes<4>;

mod deployable;
use deployable::BridgedERC721;

// Using proper Deployable interface for BridgedERC721; runtime bytecode is wired in deployable.rs

sol! {
    struct TokenDescription {
        // The source token address on the source chain
        address sourceToken;
        // The token name
        string name;
        // The token symbol
        string symbol;
    }

    struct ERC721Deposit {
        // The nonce of the deposit
        uint256 nonce;
        // The sender of the deposit
        address from;
        // The receiver of the deposit
        address to;
        // The source ERC721 token address (always refers to the source token, not bridged)
        address sourceToken;
        // The token ID
        uint256 tokenId;
        // The token URI (metadata) for this specific token
        string tokenURI;
        // Address that is allowed to cancel the deposit on the destination chain (zero address means deposit is
        // uncancellable)
        address canceler;
    }
}

// Events (parity with Solidity)
#[derive(Event)]
struct TokenDescriptionRecorded {
    #[indexed]
    id: B32,
    description: (Address, String, String),
}

#[derive(Event)]
struct CounterpartTokenDeployed {
    #[indexed]
    id: B32,
    description: (Address, String, String),
    #[indexed]
    deployed_token: Address,
}

#[derive(Event)]
struct DepositMade {
    #[indexed]
    id: B32,
    deposit: (U256, Address, Address, Address, U256, String, Address),
    local_token: Address,
}

#[derive(Event)]
struct DepositClaimed {
    #[indexed]
    id: B32,
    deposit: (U256, Address, Address, Address, U256, String, Address),
}

#[derive(Event)]
struct DepositCancelled {
    #[indexed]
    id: B32,
    claimee: Address,
}

/// SignalService interface (ABI encoding via r55 interface macro)
#[interface("camelCase")]
trait ISignalService {
    fn verifySignal(&mut self, sender: Address, value: B32, proof: Bytes);
    fn sendSignal(&mut self, value: B32) -> B32;
}

/// Minimal ERC721 interface used by the bridge
#[interface("camelCase")]
trait IERC721 {
    fn safeTransferFrom(&mut self, from: Address, to: Address, tokenId: U256);
}

/// Bridged ERC721 interface (destination chain representation)
#[interface("camelCase")]
trait IBridgedERC721 {
    fn mint(&mut self, to: Address, tokenId: U256, tokenURI: String);
    fn burn(&mut self, tokenId: U256);
    fn sourceTokenAddress(&self) -> Address;
}

/// ERC721 metadata interface used to read token name/symbol
#[interface("camelCase")]
trait IERC721Metadata {
    fn name(&self) -> String;
    fn symbol(&self) -> String;
    fn tokenURI(&self, tokenId: U256) -> String;
}

#[storage]
pub struct ERC721Bridge {
    /// Processed ids (1 = processed)
    processed: Mapping<B32, Slot<U256>>,
    /// Source token -> deployed counterpart (on local chain)
    counterpart_token_of: Mapping<Address, Slot<Address>>,
    /// Token address -> is bridged (1) or not (0)
    is_bridged_token: Mapping<Address, Slot<U256>>,
    /// Global nonce for ERC721 deposits
    global_deposit_nonce: Slot<U256>,
    /// Address of SignalService contract
    signal_service: Slot<Address>,
    /// Counterpart bridge address on the other chain
    counterpart: Slot<Address>,
    /// This bridge's address
    this_address: Slot<Address>,
    /// Reentrancy guard flag (1 = entered)
    reentrancy_entered: Slot<U256>,
}

#[contract]
impl ERC721Bridge {
    // ---------------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------------
    /// Initializes the bridge with its SignalService, counterpart bridge, and
    /// this contract's address.
    ///
    /// Reverts if either `signal_service` or `counterpart` is the zero address.
    pub fn new(signal_service: Address, counterpart: Address, _this_address: Address) -> Self {
        if signal_service == Address::ZERO || counterpart == Address::ZERO {
            revert();
        }
        let mut storage = ERC721Bridge::default();
        storage.signal_service.write(signal_service);
        storage.counterpart.write(counterpart);
        storage.this_address.write(_this_address);
        storage
    }

    // ---------------------------------------------------------------------
    // Views
    // ---------------------------------------------------------------------

    /// processed(bytes32 id) -> bool
    pub fn processed(&self, id: B32) -> bool {
        self.processed[id].read() == U256::from(1u8)
    }

    /// signalService() -> address
    pub fn signalService(&self) -> Address {
        self.signal_service.read()
    }

    /// counterpart() -> address
    pub fn counterpart(&self) -> Address {
        self.counterpart.read()
    }

    /// getCounterpartToken(address sourceToken) -> address
    pub fn getCounterpartToken(&self, source_token: Address) -> Address {
        self.counterpart_token_of[source_token].read()
    }

    /// supportsInterface(bytes4) -> bool
    /// Returns true for ERC165 (0x01ffc9a7) and IERC721Receiver (0x150b7a02).
    pub fn supportsInterface(&self, interface_id: B4) -> bool {
        let id = interface_id.as_slice();
        const ERC165: [u8; 4] = [0x01, 0xff, 0xc9, 0xa7];
        const IERC721_RECEIVER: [u8; 4] = [0x15, 0x0b, 0x7a, 0x02];
        const IERC721_BRIDGE: [u8; 4] = [0xa9, 0x24, 0x1f, 0xb3];
        id == &ERC165 || id == &IERC721_RECEIVER || id == &IERC721_BRIDGE
    }

    /// onERC721Received(address,address,uint256,bytes) -> bytes4
    /// Accept all ERC721 safe transfers into the bridge by returning magic value.
    pub fn onERC721Received(
        &mut self,
        _operator: Address,
        _from: Address,
        _token_id: U256,
        _data: Bytes,
    ) -> B4 {
        B4::from([0x15, 0x0b, 0x7a, 0x02])
    }

    /// getTokenDescriptionId(TokenDescription) -> bytes32
    /// Computes keccak256(abi.encode(ERC721_TOKEN_DESCRIPTION_PREFIX, TokenDescription)).
    ///
    /// IMPORTANT (ABI parity): Use params-encoding for (prefix, tokenDesc) to match Solidity's
    /// abi.encode(prefix, tokenDesc) when tokenDesc contains dynamic fields (strings).
    pub fn getTokenDescriptionId(&self, token_desc: (Address, String, String)) -> B32 {
        let prefix = token_description_prefix();
        let (source_token, name, symbol) = token_desc;
        let td = TokenDescription { sourceToken: source_token, name, symbol };
        let params = (prefix, td);
        let bytes = alloy_sol_types::SolValue::abi_encode_params(&params);
        B32::from(alloy_keccak256(&bytes))
    }

    /// getDepositId(ERC721Deposit) -> bytes32
    /// Computes keccak256(abi.encode(ERC721_DEPOSIT_PREFIX, ERC721Deposit)).
    ///
    /// IMPORTANT: ERC721Deposit includes a dynamic string (tokenURI). Always use params-encoding
    /// for (prefix, erc721Deposit) to preserve Solidity parity.
    pub fn getDepositId(
        &self,
        erc721_deposit: (U256, Address, Address, Address, U256, String, Address),
    ) -> B32 {
        let prefix = deposit_prefix();
        let (nonce, from, to, source_token, token_id, token_uri, canceler) = erc721_deposit;
        let dep = ERC721Deposit {
            nonce,
            from,
            to,
            sourceToken: source_token,
            tokenId: token_id,
            tokenURI: token_uri,
            canceler,
        };
        let params = (prefix, dep);
        let bytes = alloy_sol_types::SolValue::abi_encode_params(&params);
        B32::from(alloy_keccak256(&bytes))
    }

    // ---------------------------------------------------------------------
    // Mutating functions (incremental)
    // ---------------------------------------------------------------------

    /// recordTokenDescription(address token) -> bytes32
    ///
    /// Reads metadata from the source ERC721 token, computes and sends a description signal,
    /// emits `TokenDescriptionRecorded`, and returns the description id.
    ///
    /// Reverts if `token` is zero or already marked as a bridged token.
    pub fn recordTokenDescription(&mut self, token: Address) -> B32 {
        if token == Address::ZERO {
            revert();
        }
        if self.is_bridged_token[token].read() == U256::from(1u8) {
            revert();
        }

        // Read token metadata via IERC721Metadata (fallbacks on failure)
        let meta = IERC721Metadata::new(token).with_ctx(&*self);
        let name = meta.name().unwrap_or_else(|| String::from("Unknown NFT Name"));
        let symbol = meta.symbol().unwrap_or_else(|| String::from("UNKNOWN"));

        // Compute id = keccak256(abi.encode(PREFIX, TokenDescription))
        let id = {
            let prefix = token_description_prefix();
            let td = TokenDescription {
                sourceToken: token,
                name: name.clone(),
                symbol: symbol.clone(),
            };
            let params = (prefix, td);
            let bytes = alloy_sol_types::SolValue::abi_encode_params(&params);
            B32::from(alloy_keccak256(&bytes))
        };

        // Signal via SignalService
        let sig_addr = self.signal_service.read();
        let mut sig = ISignalService::new(sig_addr).with_ctx(&mut *self);
        if sig.sendSignal(id).is_none() {
            revert();
        }

        // Emit TokenDescriptionRecorded(id, tokenDesc)
        eth_riscv_runtime::log::emit(TokenDescriptionRecorded {
            id,
            description: (token, name, symbol),
        });

        id
    }

    /// deposit(address to, address localToken, uint256 tokenId, address canceler) -> bytes32
    ///
    /// Locks or burns NFT on this chain and signals the deposit to the counterpart chain.
    /// If `localToken` is a bridged token, it is burned; otherwise, the source token is
    /// transferred into the bridge. Emits `DepositMade` and returns the deposit id.
    pub fn deposit(&mut self, to: Address, local_token: Address, token_id: U256, canceler: Address) -> B32 {
        // Reentrancy guard
        if self.reentrancy_entered.read() == U256::from(1u8) {
            revert();
        }
        self.reentrancy_entered.write(U256::from(1u8));

        // Resolve bridged vs local token
        let is_bridged = self.is_bridged_token[local_token].read() == U256::from(1u8);
        let source_token = if is_bridged {
            let ro = IBridgedERC721::new(local_token).with_ctx(&*self);
            match ro.sourceTokenAddress() { Some(addr) => addr, None => revert() }
        } else {
            local_token
        };

        // Best-effort tokenURI; fallback to empty string
        let token_uri = {
            let meta = IERC721Metadata::new(local_token).with_ctx(&*self);
            meta.tokenURI(token_id).unwrap_or_else(|| String::from(""))
        };

        // Build deposit tuple
        let nonce = self.global_deposit_nonce.read();
        let from = eth_riscv_runtime::msg_sender();
        let deposit_tuple = (nonce, from, to, source_token, token_id, token_uri.clone(), canceler);

        // Compute id = keccak256(abi.encode(PREFIX, erc721Deposit))
        let id = self.getDepositId(deposit_tuple.clone());

        // Increment nonce (unchecked semantics)
        self.global_deposit_nonce
            .write(nonce.saturating_add(U256::from(1u8)));

        // Transfer NFT into bridge
        let this_address = self.this_address.read();
        let mut erc721 = IERC721::new(local_token).with_ctx(&mut *self);
        if erc721.safeTransferFrom(from, this_address, token_id).is_none() {
            revert();
        }

        // If bridged, burn on local token
        if is_bridged {
            let mut bridged = IBridgedERC721::new(local_token).with_ctx(&mut *self);
            if bridged.burn(token_id).is_none() { revert(); }
        }

        // Signal via SignalService
        let sig_addr = self.signal_service.read();
        let mut sig = ISignalService::new(sig_addr).with_ctx(&mut *self);
        if sig.sendSignal(id).is_none() { revert(); }

        // Emit DepositMade(id, erc721Deposit, localToken)
        eth_riscv_runtime::log::emit(DepositMade {
            id,
            deposit: deposit_tuple,
            local_token: local_token,
        });

        // Clear reentrancy guard
        self.reentrancy_entered.write(U256::from(0u8));

        id
    }
    
    /// deployCounterpartToken(TokenDescription tokenDesc, bytes proof) -> address
    ///
    /// Verifies the description signal from the counterpart chain, deploys the
    /// `BridgedERC721` representation, records mappings/processed state, emits
    /// `CounterpartTokenDeployed`, and returns the deployed address.
    ///
    /// Reverts if already processed, counterpart exists, or verification/deployment fails.
    pub fn deployCounterpartToken(
        &mut self,
        token_desc: (Address, String, String),
        proof: Bytes,
    ) -> Address {
        let (source_token, name, symbol) = token_desc;

        // Compute id = keccak256(abi.encode(TOKEN_DESCRIPTION_SIGNAL_PREFIX, tokenDesc))
        let id = self.getTokenDescriptionId((source_token, name.clone(), symbol.clone()));

        // require(!_processed[id])
        if self.processed[id].read() == U256::from(1u8) {
            revert();
        }

        // require(_counterpartTokens[sourceToken] == address(0))
        if self.counterpart_token_of[source_token].read() != Address::ZERO {
            revert();
        }

        // signalService.verifySignal(counterpart, id, proof)
        let sig_addr = self.signal_service.read();
        let counterparty = self.counterpart.read();
        let mut sig = ISignalService::new(sig_addr).with_ctx(&mut *self);
        if sig.verifySignal(counterparty, id, proof.clone()).is_none() {
            revert();
        }

        // Read this bridge address
        let this_address = self.this_address.read();

        // Deploy BridgedERC721(name, symbol, sourceToken, this_address)
        let child = BridgedERC721::deploy((
            name.clone(),
            symbol.clone(),
            source_token,
            this_address,
        ))
        .with_ctx(&mut *self);
        let deployed = child.address();
        if deployed == Address::ZERO {
            revert();
        }

        // Update mappings and processed flag
        self.counterpart_token_of[source_token].write(deployed);
        self.is_bridged_token[deployed].write(U256::from(1u8));
        self.processed[id].write(U256::from(1u8));

        // Emit event
        eth_riscv_runtime::log::emit(CounterpartTokenDeployed {
            id,
            description: (source_token, name, symbol),
            deployed_token: deployed,
        });

        deployed
    }

    /// claimDeposit(ERC721Deposit, bytes proof)
    ///
    /// Verifies the deposit signal and completes the transfer on this chain by
    /// minting bridged NFT or transferring held source NFT to the recipient.
    /// Emits `DepositClaimed`. Reentrancy-protected.
    pub fn claimDeposit(
        &mut self,
        erc721_deposit: (U256, Address, Address, Address, U256, String, Address),
        proof: Bytes,
    ) {
        // Reentrancy guard
        if self.reentrancy_entered.read() == U256::from(1u8) {
            revert();
        }
        self.reentrancy_entered.write(U256::from(1u8));

        let (nonce, from, to, source_token, token_id, token_uri, _canceler) = erc721_deposit.clone();

        // Compute id
        let id = self.getDepositId(erc721_deposit.clone());

        // require(!processed(id))
        if self.processed[id].read() == U256::from(1u8) {
            revert();
        }

        // Verify signal
        let counterparty = self.counterpart.read();
        let sig_addr = self.signal_service.read();
        let mut sig = ISignalService::new(sig_addr).with_ctx(&mut *self);
        if sig.verifySignal(counterparty, id, proof.clone()).is_none() {
            revert();
        }

        // Mark processed before effects
        self.processed[id].write(U256::from(1u8));

        // Send/mint NFT to recipient
        send_erc721(self, (nonce, from, to, source_token, token_id, token_uri.clone(), _canceler), to);

        // Emit event
        eth_riscv_runtime::log::emit(DepositClaimed {
            id,
            deposit: (nonce, from, to, source_token, token_id, token_uri, _canceler),
        });

        // Clear guard
        self.reentrancy_entered.write(U256::from(0u8));
    }

    /// cancelDeposit(ERC721Deposit, claimee, proof)
    ///
    /// Only erc721Deposit.canceler may call. Marks processed and sends NFT to claimee.
    pub fn cancelDeposit(
        &mut self,
        erc721_deposit: (U256, Address, Address, Address, U256, String, Address),
        claimee: Address,
        proof: Bytes,
    ) {
        // Reentrancy guard
        if self.reentrancy_entered.read() == U256::from(1u8) {
            revert();
        }
        self.reentrancy_entered.write(U256::from(1u8));

        let (nonce, from, _to, source_token, token_id, token_uri, canceler) = erc721_deposit.clone();

        // Authorization: msg.sender == erc721Deposit.canceler
        if eth_riscv_runtime::msg_sender() != canceler {
            revert();
        }

        // Compute id
        let id = self.getDepositId(erc721_deposit.clone());

        // require(!processed(id))
        if self.processed[id].read() == U256::from(1u8) {
            revert();
        }

        // Verify signal
        let counterparty = self.counterpart.read();
        let sig_addr = self.signal_service.read();
        let mut sig = ISignalService::new(sig_addr).with_ctx(&mut *self);
        if sig.verifySignal(counterparty, id, proof.clone()).is_none() {
            revert();
        }

        // Mark processed before effects
        self.processed[id].write(U256::from(1u8));

        // Send/mint NFT to claimee
        send_erc721(self, (nonce, from, _to, source_token, token_id, token_uri.clone(), canceler), claimee);

        // Emit event
        eth_riscv_runtime::log::emit(DepositCancelled {
            id,
            claimee,
        });

        // Clear guard
        self.reentrancy_entered.write(U256::from(0u8));
    }

    // ---------------------------------------------------------------------
    // Internals (methods)
    // ---------------------------------------------------------------------
}

// ---------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------

fn token_description_prefix() -> B32 {
    B32::from(alloy_keccak256(b"ERC721_TOKEN_DESCRIPTION"))
}

fn deposit_prefix() -> B32 {
    B32::from(alloy_keccak256(b"ERC721_DEPOSIT"))
}

/// Internal helper to deliver NFT to recipient:
/// - If counterpart deployed: mint on bridged token with tokenURI
/// - Else: transfer held source token from bridge to recipient
fn send_erc721(
    this: &mut ERC721Bridge,
    erc721_deposit: (U256, Address, Address, Address, U256, String, Address),
    to: Address,
) {
    let (_nonce, _from, _to, source_token, token_id, token_uri, _canceler) = erc721_deposit;
    let deployed = this.counterpart_token_of[source_token].read();
    if deployed != Address::ZERO {
        // Mint on bridged token to recipient
        let mut bridged = IBridgedERC721::new(deployed).with_ctx(&mut *this);
        if bridged.mint(to, token_id, token_uri).is_none() { revert(); }
    } else {
        // Transfer held source token from bridge to recipient
        let this_address = this.this_address.read();
        let mut erc721 = IERC721::new(source_token).with_ctx(&mut *this);
        if erc721.safeTransferFrom(this_address, to, token_id).is_none() { revert(); }
    }
}
