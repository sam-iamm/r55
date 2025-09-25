#![no_std]
#![no_main]
//! ETHBridge (R55/RISC-V)
//!
//! Cross-chain ETH bridge mirroring the Solidity reference implementation.
//! - Deposits: records an `ETHDeposit`, signals the deposit id via `SignalService`,
//!   and emits `DepositMade`.
//! - Claims/Cancels: requires a verified signal from the counterpart chain via
//!   `SignalService.verifySignal` and enforces replay protection via `processed`.
//! - Value transfer: uses runtime `call_contract` and intentionally ignores
//!   returndata (no success flag), matching the reference’s semantics.

use alloy_core::primitives::{keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, U256};
use contract_derive::{contract, interface, payable, storage, Event};
use eth_riscv_runtime::{msg_sender, msg_value, call_contract};
use eth_riscv_runtime::types::*;

type B32 = FixedBytes<32>;

extern crate alloc;

/// ETHDeposit struct matching Solidity interface
#[derive(Clone, Debug, Default)]
pub struct ETHDeposit {
    pub nonce: U256,
    pub from: Address,
    pub to: Address,
    pub amount: U256,
    pub data: Bytes,
    pub context: Bytes,
    pub canceler: Address,
}

impl ETHDeposit {
    pub fn new(
        nonce: U256,
        from: Address,
        to: Address,
        amount: U256,
        data: Bytes,
        context: Bytes,
        canceler: Address,
    ) -> Self {
        Self {
            nonce,
            from,
            to,
            amount,
            data,
            context,
            canceler,
        }
    }
}

// Event: DepositMade(bytes32 indexed id, ETHDeposit deposit)
#[derive(Event)]
struct DepositMade {
    #[indexed]
    id: B32,
    // Represent Solidity struct as a tuple in the same field order
    deposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
}

// Event: DepositClaimed(bytes32 indexed id, ETHDeposit deposit)
#[derive(Event)]
struct DepositClaimed {
    #[indexed]
    id: B32,
    deposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
}

// Event: DepositCancelled(bytes32 indexed id, address claimee)
#[derive(Event)]
struct DepositCancelled {
    #[indexed]
    id: B32,
    claimee: Address,
}

#[interface("camelCase")]
trait ISignalService {
    fn sendSignal(&mut self, value: B32) -> B32;
    fn verifySignal(&self, sender: Address, value: B32, proof: Bytes);
}

/// Storage
#[storage]
pub struct ETHBridge {
    processed: Mapping<B32, Slot<U256>>, // 1 == processed
    global_deposit_nonce: Slot<U256>,
    signal_service: Slot<Address>,
    counterpart: Slot<Address>,
}

#[contract]
impl ETHBridge {
    pub fn new(signal_service: Address, counterpart: Address) -> Self {
        if signal_service == Address::ZERO || counterpart == Address::ZERO {
            revert();
        }

        let mut s = ETHBridge::default();
        s.signal_service.write(signal_service);
        s.counterpart.write(counterpart);
        s
    }

    // processed(bytes32) -> bool — replay protection state
    pub fn processed(&self, id: B32) -> bool {
        // Solidity mapping(bytes32 => bool) parity: store 1 for true, 0 for false
        self.processed[id].read() == U256::from(1u8)
    }

    // getDepositId(ETHDeposit) -> bytes32 (Solidity abi.encode struct hashing)
    pub fn getDepositId(
        &self,
        eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
    ) -> B32 {
        // keccak256(abi.encode(ETHDeposit)) using alloy ABI encoder for exact Solidity parity
        // Normally for multiple params we use abi_encode_params, but this is a struct so we use abi_encode
        B32::from(alloy_keccak256(&alloy_sol_types::SolValue::abi_encode(
            &eth_deposit,
        )))
    }

    // deposit(address to, bytes data, bytes context, address canceler) -> bytes32
    // Increments global nonce, posts a signal to SignalService, and emits `DepositMade`.
    #[payable]
    pub fn deposit(&mut self, to: Address, data: Bytes, context: Bytes, canceler: Address) -> B32 {
        let from = msg_sender();
        let amount = msg_value();
        let nonce = self.global_deposit_nonce.read();

        // Solidity struct as tuple
        let deposit_t = (nonce, from, to, amount, data, context, canceler);
        let id = self.getDepositId(deposit_t.clone());

        // ++_globalDepositNonce (wrap-free increment)
        self.global_deposit_nonce.write(nonce + U256::from(1));

        // signal the deposit id on SignalService: sendSignal(bytes32)
        let sig_addr = self.signal_service.read();
        let _ = ISignalService::new(sig_addr).with_ctx(self).sendSignal(id);

        // emit DepositMade(id, ethDeposit)
        log::emit(DepositMade::new(id, deposit_t));

        id
    }

    // claimDeposit(ETHDeposit, bytes proof)
    // Verifies counterpart signal via `SignalService.verifySignal`, marks processed,
    // and transfers ETH to recipient with user-supplied calldata.
    pub fn claimDeposit(&mut self, eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address), proof: Bytes) {
        // get the deposit id
        let id = self.getDepositId(eth_deposit.clone());

        // check if the id is already processed
        if self.processed[id].read() == U256::from(1u8) {
            revert_selector(selector_already_claimed());
        }

        // verify the signal on SignalService
        let signal_service = self.signal_service.read();
        let counterpart = self.counterpart.read();
        let _ = ISignalService::new(signal_service).with_ctx(&*self).verifySignal(counterpart, id, proof);

        // mark the id as processed
        self.processed[id].write(U256::from(1u8));

        // transfer ETH to the specified `to` using R55 runtime call
        let to = eth_deposit.2;
        let amount_be = eth_deposit.3.to_be_bytes::<32>();
        // reject amounts that don't fit into u64 (runtime call ABI constraint)
        if amount_be[..24].iter().any(|&b| b != 0) {
            revert_selector(selector_failed_claim());
        }
        let amount = u64::from_be_bytes(amount_be[24..32].try_into().unwrap());
        let data = eth_deposit.4.clone();
        // perform CALL with value; returndata ignored (no success flag available)
        let _ = call_contract(to, amount, &data, None);

        // emit DepositClaimed(id, ethDeposit)
        eth_riscv_runtime::log::emit(DepositClaimed::new(id, eth_deposit));
    }

    // cancelDeposit(ETHDeposit, address claimee, bytes proof)
    // Only the designated canceler can cancel; verifies signal and pays `claimee`.
    pub fn cancelDeposit(
        &mut self,
        eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
        claimee: Address,
        proof: Bytes,
    ) {
        // only canceler can cancel
        if msg_sender() != eth_deposit.6 {
            revert_selector(selector_only_canceler());
        }

        // derive id and reject already-processed
        let id = self.getDepositId(eth_deposit.clone());
        if self.processed[id].read() == U256::from(1u8) {
            revert_selector(selector_already_claimed());
        }

        // verify the signal on SignalService
        let signal_service = self.signal_service.read();
        let counterpart = self.counterpart.read();
        let _ = ISignalService::new(signal_service).with_ctx(&*self).verifySignal(counterpart, id, proof);

        // mark processed
        self.processed[id].write(U256::from(1u8));

        // send ETH to claimee with empty data
        let amount_be = eth_deposit.3.to_be_bytes::<32>();
        if amount_be[..24].iter().any(|&b| b != 0) {
            revert_selector(selector_failed_claim());
        }
        let amount = u64::from_be_bytes(amount_be[24..32].try_into().unwrap());
        let empty = Bytes::new();

        let _ = call_contract(claimee, amount, &empty, None);

        // emit DepositCancelled(id, claimee)
        log::emit(DepositCancelled::new(id, claimee));
    }
}

// --- Internals ---

fn revert_selector(sel: [u8; 4]) -> ! {
    revert_with_error(&sel)
}

fn selector_already_claimed() -> [u8; 4] {
    // keccak256("AlreadyClaimed()")[:4]
    let sig = b"AlreadyClaimed()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

fn selector_failed_claim() -> [u8; 4] {
    // keccak256("FailedClaim()")[:4]
    let sig = b"FailedClaim()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

fn selector_only_canceler() -> [u8; 4] {
    // keccak256("OnlyCanceler()")[:4]
    let sig = b"OnlyCanceler()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}