//! ERC20Bridge (R55/RISC-V)
//!
//! Cross-chain ERC20 bridge, mirroring the Solidity reference interface.
//!
//! Lifecycle overview:
//! - Record token description on source chain and emit a signal.
//! - On destination, verify the signal and deploy `BridgedERC20` counterpart.
//! - Deposits burn bridged tokens or escrow source tokens, then signal.
//! - Claims verify deposit signal, then mint bridged tokens or release escrow.
//!
//! ABI parity notes:
//! - Use params-encoding (`abi_encode_params`) when combining a prefix with a tuple
//!   containing dynamic fields. Raw tuple `abi_encode` differs from Solidity.
//! - `sol!`-generated types implement `SolValue` and avoid `u8` tuple encoding gaps.
//!
//! Security notes:
//! - Reentrancy: `deposit` and `claimDeposit` use a simple guard and clear it at end.
//! - Replay: `processed[id]` enforces single-claim semantics; set before effects in claim.
//! - Address validation: constructor rejects zero addresses for critical dependencies.
//!
//! Solidity refs:
//! /Users/michael/Documents/stack/stack/contracts/src/shared/interfaces/IERC20Bridge.sol
//! /Users/michael/Documents/stack/stack/contracts/src/shared/SignalService.sol
//!
//! R55 refs:
//! /Users/michael/Documents/stack/r55/examples/hydra-erc20-bridge/src/lib.rs

#![no_std]
#![no_main]

use alloy_core::primitives::{keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, U256};
use alloy_sol_types::sol;
use contract_derive::{contract, storage, Event, interface};
use eth_riscv_runtime::revert;
use eth_riscv_runtime::types::*;

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

type B32 = FixedBytes<32>;

mod deployable;
use deployable::BridgedERC20;

// Using proper Deployable interface for BridgedERC20; runtime bytecode is wired in deployable.rs

sol! {
    struct TokenDescription {
        // The source token address on the source chain
        address sourceToken;
        // The token name
        string name;
        // The token symbol
        string symbol;
        // The token decimals
        uint8 decimals;
    }

    struct ERC20Deposit {
        // The nonce of the deposit
        uint256 nonce;
        // The sender of the deposit
        address from;
        // The receiver of the deposit
        address to;
        // The source ERC20 token address (always refers to the source token, not bridged)
        address sourceToken;
        // The amount of the deposit
        uint256 amount;
    }
}

// Events (parity with Solidity)
#[derive(Event)]
struct TokenDescriptionRecorded {
    #[indexed]
    id: B32,
    description: (Address, String, String, u8),
}

#[derive(Event)]
struct CounterpartTokenDeployed {
    #[indexed]
    id: B32,
    description: (Address, String, String, u8),
    #[indexed]
    deployed_token: Address,
}

#[derive(Event)]
struct DepositMade {
    #[indexed]
    id: B32,
    deposit: (U256, Address, Address, Address, U256),
    local_token: Address,
}

#[derive(Event)]
struct DepositClaimed {
    #[indexed]
    id: B32,
    deposit: (U256, Address, Address, Address, U256),
}

// Note: Older r55 versions duplicated imports for multiple #[interface("camelCase")] usages.
// Ensure you build with a version that contains the helper fix to avoid duplicate trait/crate imports.

/// SignalService interface (ABI encoding via r55 interface macro)
#[interface("camelCase")]
trait ISignalService {
    fn verifySignal(&mut self, sender: Address, value: B32, proof: Bytes);
    fn sendSignal(&mut self, value: B32) -> B32;
}

/// Minimal ERC20 interface used by the bridge
#[interface("camelCase")]
trait IERC20 {
    fn transfer(&mut self, to: Address, amount: U256) -> bool;
    fn transferFrom(&mut self, from: Address, to: Address, amount: U256) -> bool;
}

/// Bridged ERC20 interface (destination chain representation)
#[interface("camelCase")]
trait IBridgedERC20 {
    fn mint(&mut self, to: Address, amount: U256);
    fn burn(&mut self, amount: U256);
    fn sourceTokenAddress(&self) -> Address;
}

/// ERC20 metadata interface used to read token name/symbol/decimals
#[interface("camelCase")]
trait IERC20Metadata {
    fn name(&self) -> String;
    fn symbol(&self) -> String;
    fn decimals(&self) -> U256;
}

#[storage]
pub struct ERC20Bridge {
    /// Processed ids (1 = processed)
    processed: Mapping<B32, Slot<U256>>,
    /// Source token -> deployed counterpart (on local chain)
    counterpart_token_of: Mapping<Address, Slot<Address>>,
    /// Token address -> is bridged (1) or not (0)
    is_bridged_token: Mapping<Address, Slot<U256>>,
    /// Global nonce for ERC20 deposits
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
impl ERC20Bridge {
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
        let mut storage = ERC20Bridge::default();
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

    /// getTokenDescriptionId(TokenDescription) -> bytes32
    /// Computes keccak256(abi.encode(TOKEN_DESCRIPTION_SIGNAL_PREFIX, TokenDescription)).
    ///
    /// IMPORTANT (ABI parity note):
    /// - Solidity encodes two parameters here: `(prefix, tokenDesc)`.
    /// - `alloy_sol_types::SolValue::abi_encode` encodes ONE value. Passing `(prefix, tokenDesc)`
    ///   to `abi_encode` encodes a single tuple value, which diverges in layout for dynamic members
    ///   (e.g. `string name`, `string symbol`).
    /// - Use `abi_encode_params(&(prefix, tokenDesc))` to encode “two params” exactly like
    ///   Solidity `abi.encode(prefix, tokenDesc)`.
    ///
    /// Note on `u8` encoding:
    /// - Raw Rust tuples including `u8` do not implement `SolValue` in some alloy versions.
    /// - Using `sol!` structs (e.g., `TokenDescription`) avoids this issue because the generated
    ///   types implement `SolValue` and encode correctly.
    pub fn getTokenDescriptionId(&self, token_desc: (Address, String, String, u8)) -> B32 {
        let prefix = token_description_prefix();
        let (source, name, symbol, decimals) = token_desc;
        // Match Solidity: keccak256(abi.encode(prefix, TokenDescription)) where TokenDescription is a tuple param
        let td = TokenDescription {
            sourceToken: source,
            name,
            symbol,
            decimals,
        };
        let params = (prefix, td);
        let bytes = alloy_sol_types::SolValue::abi_encode_params(&params);
        B32::from(alloy_keccak256(&bytes))
    }

    /// getDepositId(ERC20Deposit) -> bytes32
    /// Computes keccak256(abi.encode(ERC20Deposit)).
    ///
    /// NOTE: This currently works with `abi_encode` because `ERC20Deposit` is all static types.
    /// For static-only inner tuples, encoding a single tuple vs. multiple params produces identical
    /// bytes. If any dynamic field is ever added here, switch to `abi_encode_params(&(prefix, erc20Deposit))`
    /// to preserve Solidity parity.
    pub fn getDepositId(&self, erc20_deposit: (U256, Address, Address, Address, U256)) -> B32 {
        let prefix = deposit_prefix();
        let tuple = (prefix, erc20_deposit);
        B32::from(alloy_keccak256(&alloy_sol_types::SolValue::abi_encode(
            &tuple,
        )))
    }

    // ---------------------------------------------------------------------
    // Mutating functions (scaffolded)
    // ---------------------------------------------------------------------

    /// recordTokenDescription(address token) -> bytes32
    ///
    /// Reads metadata from the source token, computes and sends a description signal,
    /// emits `TokenDescriptionRecorded`, and returns the description id.
    ///
    /// Reverts if `token` is zero or already marked as a bridged token.
    pub fn recordTokenDescription(&mut self, token: Address) -> B32 {
        // Parity checks
        if token == Address::ZERO {
            revert();
        }
        if self.is_bridged_token[token].read() == U256::from(1u8) {
            revert();
        }

        // --- Read token metadata via IERC20Metadata interface ---
        let meta = IERC20Metadata::new(token).with_ctx(&*self);
        let name = meta.name().unwrap_or_else(|| String::from("Unknown Token Name"));
        let symbol = meta.symbol().unwrap_or_else(|| String::from("UNKNOWN"));
        let decimals_u8: u8 = meta
            .decimals()
            .map(|v| v.to_be_bytes::<32>()[31])
            .unwrap_or(18);

        // Compute ID using params-encode
        // IMPORTANT: Solidity encodes two params `(prefix, tokenDesc)`; use abi_encode_params
        let id = {
            let prefix = token_description_prefix();
            let td = TokenDescription {
                sourceToken: token,
                name: name.clone(),
                symbol: symbol.clone(),
                decimals: decimals_u8,
            };
            let params = (prefix, td);
            let bytes = alloy_sol_types::SolValue::abi_encode_params(&params);
            B32::from(alloy_keccak256(&bytes))
        };

        // Keep ownership of name/symbol for event payload

        // Signal via SignalService interface
        let sig_addr = self.signal_service.read();
        let mut sig = ISignalService::new(sig_addr).with_ctx(&mut *self);
        if sig.sendSignal(id).is_none() { revert(); }

        // Emit TokenDescriptionRecorded(id, tokenDesc)
        eth_riscv_runtime::log::emit(TokenDescriptionRecorded {
            id,
            description: (token, name, symbol, decimals_u8),
        });

        id
    }

    /// deployCounterpartToken(TokenDescription tokenDesc, bytes proof) -> address
    ///
    /// Verifies the description signal from the counterpart chain, deploys the
    /// `BridgedERC20` representation, records mappings/processed state, emits
    /// `CounterpartTokenDeployed`, and returns the deployed address.
    ///
    /// Reverts if already processed, counterpart exists, verification fails, or deployment fails.
    pub fn deployCounterpartToken(
        &mut self,
        token_desc: (Address, String, String, u8),
        proof: Bytes,
    ) -> Address {
        // tokenDesc tuple unpack
        let (source_token, name, symbol, decimals_u8) = token_desc;

        // Compute id = keccak256(abi.encode(TOKEN_DESCRIPTION_SIGNAL_PREFIX, tokenDesc))
        let id =
            self.getTokenDescriptionId((source_token, name.clone(), symbol.clone(), decimals_u8));

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

        // Read this_address
        let this_address = self.this_address.read();

        // Deploy bridged token using R55 Deployable builder (intentionally using R55 bytecode)
        let child = BridgedERC20::deploy((
            name.clone(),
            symbol.clone(),
            decimals_u8,
            source_token,
            this_address,
        ))
        .with_ctx(&mut *self);
        let deployed = child.address();

        if deployed == Address::ZERO {
            revert();
        }

        // _counterpartTokens[source] = deployed; _isBridgedTokens[deployed] = true; _processed[id] = true
        self.counterpart_token_of[source_token].write(deployed);
        self.is_bridged_token[deployed].write(U256::from(1u8));
        self.processed[id].write(U256::from(1u8));

        // Emit CounterpartTokenDeployed(id, tokenDesc, deployed)
        eth_riscv_runtime::log::emit(CounterpartTokenDeployed {
            id,
            description: (source_token, name, symbol, decimals_u8),
            deployed_token: deployed,
        });

        deployed
    }

    /// deposit(address to, address localToken, uint256 amount) -> bytes32
    ///
    /// Locks or burns tokens on this chain and signals the deposit to the
    /// counterpart chain. If `localToken` is a bridged token, it is burned;
    /// otherwise, the source tokens are transferred into the bridge.
    ///
    /// Reentrancy-protected. Emits `DepositMade` and returns the deposit id.
    pub fn deposit(&mut self, _to: Address, _local_token: Address, _amount: U256) -> B32 {
        // Reentrancy guard
        if self.reentrancy_entered.read() == U256::from(1u8) {
            revert();
        }
        self.reentrancy_entered.write(U256::from(1u8));

        // Resolve bridged vs local token
        let is_bridged = self.is_bridged_token[_local_token].read() == U256::from(1u8);

        // sourceToken = isBridged ? BridgedERC20(localToken).sourceTokenAddress() : localToken
        let source_token = if is_bridged {
            let ro = IBridgedERC20::new(_local_token).with_ctx(&*self);
            match ro.sourceTokenAddress() { Some(addr) => addr, None => revert() }
        } else {
            _local_token
        };

        // Build deposit tuple
        let nonce = self.global_deposit_nonce.read();
        let from = eth_riscv_runtime::msg_sender();
        let to = _to;
        let amount = _amount;
        let deposit_tuple = (nonce, from, to, source_token, amount);

        // Compute id = keccak256(abi.encode(DEPOSIT_SIGNAL_PREFIX, erc20Deposit))
        let id = self.getDepositId(deposit_tuple);

        // Increment nonce (unchecked in Solidity semantics)
        self.global_deposit_nonce
            .write(nonce.saturating_add(U256::from(1u8)));

        // IERC20(localToken).safeTransferFrom(msg.sender, address(this), amount)
        let this_address = self.this_address.read();
        let mut erc20 = IERC20::new(_local_token).with_ctx(&mut *self);
        if !erc20.transferFrom(from, this_address, amount).unwrap_or(false) { revert(); }

        // If bridged, burn on local token
        if is_bridged {
            let mut bridged = IBridgedERC20::new(_local_token).with_ctx(&mut *self);
            if bridged.burn(amount).is_none() { revert(); }
        }

        // signalService.sendSignal(id)
        let sig_addr = self.signal_service.read();
        let mut sig = ISignalService::new(sig_addr).with_ctx(&mut *self);
        if sig.sendSignal(id).is_none() { revert(); }

        // Emit DepositMade(id, erc20Deposit, localToken)
        eth_riscv_runtime::log::emit(DepositMade {
            id,
            deposit: deposit_tuple,
            local_token: _local_token,
        });

        // Clear reentrancy guard
        self.reentrancy_entered.write(U256::from(0u8));

        id
    }

    /// claimDeposit(ERC20Deposit, bytes proof)
    ///
    /// Verifies the deposit signal and completes the transfer on this chain by
    /// minting bridged tokens or transferring held source tokens to the recipient.
    ///
    /// Reentrancy-protected. Emits `DepositClaimed`.
    pub fn claimDeposit(
        &mut self,
        _erc20_deposit: (U256, Address, Address, Address, U256),
        _proof: Bytes,
    ) {
        // Reentrancy guard
        if self.reentrancy_entered.read() == U256::from(1u8) {
            revert();
        }
        self.reentrancy_entered.write(U256::from(1u8));

        let (nonce, from, to, source_token, amount) = _erc20_deposit;

        // Compute id = keccak256(abi.encode(DEPOSIT_SIGNAL_PREFIX, erc20Deposit))
        let id = self.getDepositId((nonce, from, to, source_token, amount));

        // require(!processed(id))
        if self.processed[id].read() == U256::from(1u8) {
            revert();
        }

        // signalService.verifySignal(counterpart, id, proof)
        let counterparty = self.counterpart.read();
        let sig_addr = self.signal_service.read();
        let mut sig = ISignalService::new(sig_addr).with_ctx(&mut *self);
        if sig.verifySignal(counterparty, id, _proof.clone()).is_none() { revert(); }

        // Mark processed before effects to match Solidity nonReentrant semantics ordering
        self.processed[id].write(U256::from(1u8));

        // _sendERC20(erc20Deposit)
        let counterpart = self.counterpart_token_of[source_token].read();
        if counterpart != Address::ZERO {
            // Mint on bridged token to recipient (msg.sender is bridge)
            let mut bridged = IBridgedERC20::new(counterpart).with_ctx(&mut *self);
            if bridged.mint(to, amount).is_none() { revert(); }
        } else {
            // Transfer held source tokens from bridge to recipient
            let mut erc20 = IERC20::new(source_token).with_ctx(&mut *self);
            if !erc20.transfer(to, amount).unwrap_or(false) { revert(); }
        }

        // Emit DepositClaimed(id, erc20Deposit)
        let erc20_tuple = (nonce, from, to, source_token, amount);
        eth_riscv_runtime::log::emit(DepositClaimed {
            id,
            deposit: erc20_tuple,
        });

        // Clear reentrancy guard
        self.reentrancy_entered.write(U256::from(0u8));
    }
}

// ---------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------

fn token_description_prefix() -> B32 {
    B32::from(alloy_keccak256(b"ERC20_TOKEN_DESCRIPTION"))
}

fn deposit_prefix() -> B32 {
    B32::from(alloy_keccak256(b"ERC20_DEPOSIT"))
}
