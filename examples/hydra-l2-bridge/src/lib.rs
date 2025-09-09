#![no_std]
#![no_main]

use alloy_core::primitives::{Address, Bytes, FixedBytes, U256};
use contract_derive::{contract, interface, payable, storage, Event};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{call::*, msg_sender, msg_value, revert};

extern crate alloc;
use alloc::vec::Vec;

type B32 = FixedBytes<32>;

#[interface("camelCase")]
trait ISignalService {
    fn sendSignal(&mut self, value: B32) -> B32;
}

/// Events flattened to align with Solidity’s IETHBridge ABI
#[derive(Event)]
pub struct DepositMade {
    #[indexed]
    pub id: B32,
    pub nonce: U256,
    pub from: Address,
    pub to: Address,
    pub amount: U256,
    pub data: Bytes,
    pub context: Bytes,
    pub canceler: Address,
}

#[derive(Event)]
pub struct DepositClaimed {
    #[indexed]
    pub id: B32,
    pub nonce: U256,
    pub from: Address,
    pub to: Address,
    pub amount: U256,
    pub data: Bytes,
    pub context: Bytes,
    pub canceler: Address,
}

#[derive(Event)]
pub struct DepositCancelled {
    #[indexed]
    pub id: B32,
    pub claimee: Address,
}

/// Storage
#[storage]
pub struct BridgeEth {
    processed: Mapping<B32, Slot<U256>>, // 1 == processed
    global_deposit_nonce: Slot<U256>,
    signal_service: Slot<Address>,
    counterpart: Slot<Address>,
}

#[contract]
impl BridgeEth {
    pub fn new(signal_service: Address, counterpart: Address) -> Self {
        if signal_service == Address::ZERO || counterpart == Address::ZERO {
            revert();
        }

        let mut s = BridgeEth::default();
        s.signal_service.write(signal_service);
        s.counterpart.write(counterpart);
        s
    }

    /// processed(bytes32) -> bool
    pub fn processed(&self, id: B32) -> bool {
        self.processed[id].read() == U256::from(1u8)
    }

    /// getDepositId((nonce, from, to, amount, data, context, canceler)) -> bytes32
    pub fn getDepositId(
        &self,
        nonce: U256,
        from: Address,
        to: Address,
        amount: U256,
        data: Bytes,
        context: Bytes,
        canceler: Address,
    ) -> B32 {
        keccak_deposit(nonce, from, to, amount, &data, &context, canceler)
    }

    /// deposit(address to, bytes data, bytes context, address canceler) payable returns (bytes32)
    #[payable]
    pub fn deposit(&mut self, to: Address, data: Bytes, context: Bytes, canceler: Address) -> B32 {
        let nonce = self.global_deposit_nonce.read();
        let from = msg_sender();
        let amount = U256::from(msg_value());
        let id = keccak_deposit(nonce, from, to, amount, &data, &context, canceler);

        // increment nonce
        self.global_deposit_nonce.write(nonce + U256::from(1u8));

        // signal the deposit id on SignalService: sendSignal(bytes32)
        let sig_addr = self.signal_service.read();
        let _ = ISignalService::new(sig_addr).with_ctx(self).sendSignal(id);

        // emit DepositMade with full flattened fields
        log::emit(DepositMade::new(
            id, nonce, from, to, amount, data, context, canceler,
        ));

        id
    }
}

/// Computes keccak256(abi.encode(ETHDeposit)) to derive unique deposit IDs
fn keccak_deposit(
    nonce: U256,
    from: Address,
    to: Address,
    amount: U256,
    data: &Bytes,
    context: &Bytes,
    canceler: Address,
) -> B32 {
    // abi.encode((nonce, from, to, amount, data, context, canceler)) then keccak
    let encoded = (
        nonce,
        from,
        to,
        amount,
        data.clone(),
        context.clone(),
        canceler,
    )
        .abi_encode();
    FixedBytes::<32>::from(
        eth_riscv_runtime::keccak256(encoded.as_ptr() as u64, encoded.len() as u64)
            .to_be_bytes::<32>(),
    )
}
