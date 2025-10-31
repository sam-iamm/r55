//! ERC20Bridge (R55/RISC-V)
//!
//! Cross-chain ERC20 bridge, mirroring the Solidity reference interface.
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
use contract_derive::{contract, storage, Event};
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
    description: TokenDescription,
}

#[derive(Event)]
struct CounterpartTokenDeployed {
    #[indexed]
    id: B32,
    description: TokenDescription,
    #[indexed]
    deployed_token: Address,
}

#[derive(Event)]
struct DepositMade {
    #[indexed]
    id: B32,
    deposit: ERC20Deposit,
    local_token: Address,
}

#[derive(Event)]
struct DepositClaimed {
    #[indexed]
    id: B32,
    deposit: ERC20Deposit,
}

// *** Interfaces currently commented out because r55 cannot support multiple interface macros
// *** This is because #[interface("camelCase")] re-imports multiple of the same traits/crates, and rust complains
// *** Applied a simple fix in /Users/michael/Documents/stack/r55/contract-derive/src/helpers.rs
// *** But r55-compile uses the develop branch so my local changes are not applied
// *** TODO: Fix in r55, or compile in r55 and copy the bytecode binary here

// /// SignalService interface (ABI encoding via r55 interface macro)
// #[interface("camelCase")]
// trait ISignalService {
//     fn verifySignal(&mut self, sender: Address, value: B32, proof: Bytes);
//     fn sendSignal(&mut self, value: B32) -> B32;
// }

// /// Minimal ERC20 interface used by the bridge
// #[interface("camelCase")]
// trait IERC20 {
//     fn transfer(&mut self, to: Address, amount: U256) -> bool;
//     fn transferFrom(&mut self, from: Address, to: Address, amount: U256) -> bool;
// }

// /// Bridged ERC20 interface (destination chain representation)
// #[interface("camelCase")]
// trait IBridgedERC20 {
//     fn mint(&mut self, to: Address, amount: U256);
//     fn burn(&mut self, amount: U256);
//     fn sourceTokenAddress(&self) -> Address;
// }

// /// ERC20 metadata interface used to read token name/symbol/decimals
// #[interface("camelCase")]
// trait IERC20Metadata {
//     fn name(&self) -> String;
//     fn symbol(&self) -> String;
//     fn decimals(&self) -> U256;
// }

#[storage]
pub struct ERC20Bridge {
    // Processed ids (1 = processed)
    processed: Mapping<B32, Slot<U256>>,
    // Source token -> deployed counterpart (on local chain)
    counterpart_token_of: Mapping<Address, Slot<Address>>,
    // Token address -> is bridged (1) or not (0)
    is_bridged_token: Mapping<Address, Slot<U256>>,
    // Global nonce for ERC20 deposits
    global_deposit_nonce: Slot<U256>,
    // Addresses
    signal_service: Slot<Address>,
    counterpart: Slot<Address>,
    this_address: Slot<Address>,
    // Reentrancy guard
    reentrancy_entered: Slot<U256>,
}

#[contract]
impl ERC20Bridge {
    // ---------------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------------
    pub fn new(signal_service: Address, counterpart: Address, _this_address: Address) -> Self {
        if signal_service == Address::ZERO || counterpart == Address::ZERO {
            revert();
        }
        let mut s = ERC20Bridge::default();
        s.signal_service.write(signal_service);
        s.counterpart.write(counterpart);
        s.this_address.write(_this_address);
        s
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
    /// NOTE (alloy uint8 limitation):
    /// - alloy 1.3.x does not implement `SolValue` for Rust primitive `u8`.
    ///   Encoding must use types with `SolValue` impls (e.g. `I256`/`U256`) or a signed `i8`
    ///   casted to `I256` for parity with Solidity’s int widening rules.
    /// - If/when alloy adds `SolValue` for `u8`/`uint8`, this function can accept `u8` directly
    ///   and drop the explicit `I256` cast. References:
    ///   - SolValue trait: https://docs.rs/alloy-sol-types/1.3.1/alloy_sol_types/trait.SolValue.html
    ///   - I256: https://docs.rs/alloy-core/1.3.1/alloy_core/primitives/struct.I256.html
    ///   - U256: https://docs.rs/alloy-core/1.3.1/alloy_core/primitives/struct.U256.html
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
    pub fn recordTokenDescription(&mut self, token: Address) -> B32 {
        // Parity checks
        if token == Address::ZERO {
            revert();
        }
        if self.is_bridged_token[token].read() == U256::from(1u8) {
            revert();
        }

        // --- Read token metadata; if all three fail, mirror base revert behavior ---
        // Try/catch parity: each metadata call may fail independently; apply per-field fallbacks
        let name = erc20_metadata_name(token).unwrap_or_else(|| String::from("Unknown Token Name"));
        let symbol = erc20_metadata_symbol(token).unwrap_or_else(|| String::from("UNKNOWN"));
        let decimals_u8: u8 = erc20_metadata_decimals(token).unwrap_or(18);

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

        // Rebuild TokenDescription to move into event payload
        let token_desc = TokenDescription {
            sourceToken: token,
            name,
            symbol,
            decimals: decimals_u8,
        };

        // Signal via SignalService: sendSignal(bytes32) -> bytes32 (slot)
        // Propagate revert on failure to match Solidity behavior
        let sig_addr = self.signal_service.read();
        let mut calldata = Vec::with_capacity(4 + 32);
        calldata.extend_from_slice(&function_selector(b"sendSignal(bytes32)"));
        calldata.extend_from_slice(id.as_slice());
        match eth_riscv_runtime::call::call_contract(sig_addr, 0, &calldata, Some(32)) {
            Ok(_) => { /* ignore returned slot */ }
            Err(_) => {
                revert();
            }
        }

        // Emit TokenDescriptionRecorded(id, tokenDesc)
        eth_riscv_runtime::log::emit(TokenDescriptionRecorded {
            id,
            description: token_desc,
        });

        id
    }

    /// deployCounterpartToken(TokenDescription tokenDesc, bytes proof) -> address
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
        // ABI: verifySignal(address,bytes32,bytes)
        let sig_addr = self.signal_service.read();
        let counterparty = self.counterpart.read();
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&function_selector(b"verifySignal(address,bytes32,bytes)"));
        let args = (counterparty, id, proof.clone());
        calldata.extend_from_slice(&alloy_sol_types::SolValue::abi_encode_params(&args));
        match eth_riscv_runtime::call::call_contract(sig_addr, 0, &calldata, None) {
            Ok(_) => {}
            Err(_) => {
                revert();
            }
        }

        // Read this_address
        let this_address = self.this_address.read();

        // Deploy bridged token using R55 Deployable builder (intentionally using R55 bytecode)
        let child = BridgedERC20::deploy((
            name.clone(),
            symbol.clone(),
            U256::from(decimals_u8),
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
        let td = TokenDescription {
            sourceToken: source_token,
            name,
            symbol,
            decimals: decimals_u8,
        };
        eth_riscv_runtime::log::emit(CounterpartTokenDeployed {
            id,
            description: td,
            deployed_token: deployed,
        });

        deployed
    }

    /// deposit(address to, address localToken, uint256 amount) -> bytes32
    pub fn deposit(&mut self, _to: Address, _local_token: Address, _amount: U256) -> B32 {
        // Resolve bridged vs local token
        let is_bridged = self.is_bridged_token[_local_token].read() == U256::from(1u8);

        // sourceToken = isBridged ? BridgedERC20(localToken).sourceTokenAddress() : localToken
        let source_token = if is_bridged {
            let mut calldata = Vec::with_capacity(4);
            calldata.extend_from_slice(&function_selector(b"sourceTokenAddress()"));
            match eth_riscv_runtime::call::staticcall_contract(_local_token, 0, &calldata, None) {
                Ok(bytes) => {
                    if let Ok(addr) = <Address as alloy_sol_types::SolValue>::abi_decode(&bytes) {
                        addr
                    } else {
                        revert()
                    }
                }
                Err(_) => revert(),
            }
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
        let mut tf_calldata = Vec::new();
        tf_calldata.extend_from_slice(&function_selector(b"transferFrom(address,address,uint256)"));
        let tf_args = (from, this_address, amount);
        tf_calldata.extend_from_slice(&alloy_sol_types::SolValue::abi_encode_params(&tf_args));
        match eth_riscv_runtime::call::call_contract(_local_token, 0, &tf_calldata, None) {
            Ok(bytes) => {
                // SafeERC20 semantics: if return data exists, it must decode to true
                if !bytes.is_empty() {
                    let ok =
                        <bool as alloy_sol_types::SolValue>::abi_decode(&bytes).unwrap_or(false);
                    if !ok {
                        revert();
                    }
                }
            }
            Err(_) => {
                revert();
            }
        }

        // If bridged, burn on local token
        if is_bridged {
            let mut burn_calldata = Vec::new();
            burn_calldata.extend_from_slice(&function_selector(b"burn(uint256)"));
            // Single static param; abi_encode is fine
            burn_calldata.extend_from_slice(&alloy_sol_types::SolValue::abi_encode(&amount));
            match eth_riscv_runtime::call::call_contract(_local_token, 0, &burn_calldata, None) {
                Ok(_) => {}
                Err(_) => {
                    revert();
                }
            }
        }

        // signalService.sendSignal(id)
        let sig_addr = self.signal_service.read();
        let mut ss_calldata = Vec::with_capacity(4 + 32);
        ss_calldata.extend_from_slice(&function_selector(b"sendSignal(bytes32)"));
        ss_calldata.extend_from_slice(id.as_slice());
        match eth_riscv_runtime::call::call_contract(sig_addr, 0, &ss_calldata, Some(32)) {
            Ok(_) => {}
            Err(_) => {
                revert();
            }
        }

        // Emit DepositMade(id, erc20Deposit, localToken)
        let erc20_deposit = ERC20Deposit {
            nonce,
            from,
            to,
            sourceToken: source_token,
            amount,
        };
        eth_riscv_runtime::log::emit(DepositMade {
            id,
            deposit: erc20_deposit,
            local_token: _local_token,
        });

        id
    }

    /// claimDeposit(ERC20Deposit, bytes proof)
    pub fn claimDeposit(
        &mut self,
        _erc20_deposit: (U256, Address, Address, Address, U256),
        _proof: Bytes,
    ) {
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
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&function_selector(b"verifySignal(address,bytes32,bytes)"));
        let args = (counterparty, id, _proof.clone());
        calldata.extend_from_slice(&alloy_sol_types::SolValue::abi_encode_params(&args));
        match eth_riscv_runtime::call::call_contract(sig_addr, 0, &calldata, None) {
            Ok(_) => {}
            Err(_) => {
                revert();
            }
        }

        // Mark processed before effects to match Solidity nonReentrant semantics ordering
        self.processed[id].write(U256::from(1u8));

        // _sendERC20(erc20Deposit)
        let counterpart = self.counterpart_token_of[source_token].read();
        if counterpart != Address::ZERO {
            // Mint on bridged token to recipient (msg.sender is bridge)
            let mut mint_calldata = Vec::new();
            mint_calldata.extend_from_slice(&function_selector(b"mint(address,uint256)"));
            let mint_args = (to, amount);
            mint_calldata
                .extend_from_slice(&alloy_sol_types::SolValue::abi_encode_params(&mint_args));
            match eth_riscv_runtime::call::call_contract(counterpart, 0, &mint_calldata, None) {
                Ok(_) => {}
                Err(_) => {
                    revert();
                }
            }
        } else {
            // Transfer held source tokens from bridge to recipient
            let mut t_calldata = Vec::new();
            t_calldata.extend_from_slice(&function_selector(b"transfer(address,uint256)"));
            let t_args = (to, amount);
            t_calldata.extend_from_slice(&alloy_sol_types::SolValue::abi_encode_params(&t_args));
            match eth_riscv_runtime::call::call_contract(source_token, 0, &t_calldata, None) {
                Ok(bytes) => {
                    if !bytes.is_empty() {
                        let ok = <bool as alloy_sol_types::SolValue>::abi_decode(&bytes)
                            .unwrap_or(false);
                        if !ok {
                            revert();
                        }
                    }
                }
                Err(_) => {
                    revert();
                }
            }
        }

        // Emit DepositClaimed(id, erc20Deposit)
        let erc20_deposit = ERC20Deposit {
            nonce,
            from,
            to,
            sourceToken: source_token,
            amount,
        };
        eth_riscv_runtime::log::emit(DepositClaimed {
            id,
            deposit: erc20_deposit,
        });
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

/// Compute a 4-byte function selector from a signature string, e.g. b"name()".
fn function_selector(signature: &[u8]) -> [u8; 4] {
    let hash = alloy_keccak256(signature);
    [hash[0], hash[1], hash[2], hash[3]]
}

/// Staticcall ERC20 name() -> Option<String>
fn erc20_metadata_name(token: Address) -> Option<String> {
    let mut calldata = Vec::with_capacity(4);
    calldata.extend_from_slice(&function_selector(b"name()"));
    match eth_riscv_runtime::call::staticcall_contract(token, 0, &calldata, None) {
        Ok(bytes) => <String as alloy_sol_types::SolValue>::abi_decode(&bytes).ok(),
        Err(_) => None,
    }
}

/// Staticcall ERC20 symbol() -> Option<String>
fn erc20_metadata_symbol(token: Address) -> Option<String> {
    let mut calldata = Vec::with_capacity(4);
    calldata.extend_from_slice(&function_selector(b"symbol()"));
    match eth_riscv_runtime::call::staticcall_contract(token, 0, &calldata, None) {
        Ok(bytes) => <String as alloy_sol_types::SolValue>::abi_decode(&bytes).ok(),
        Err(_) => None,
    }
}

/// Staticcall ERC20 decimals() -> Option<u8>
fn erc20_metadata_decimals(token: Address) -> Option<u8> {
    let mut calldata = Vec::with_capacity(4);
    calldata.extend_from_slice(&function_selector(b"decimals()"));
    match eth_riscv_runtime::call::staticcall_contract(token, 0, &calldata, None) {
        Ok(bytes) => {
            // decode as U256; take the least significant byte
            if let Ok(v) = <U256 as alloy_sol_types::SolValue>::abi_decode(&bytes) {
                let be = v.to_be_bytes::<32>();
                Some(be[31])
            } else {
                None
            }
        }
        Err(_) => None,
    }
}
