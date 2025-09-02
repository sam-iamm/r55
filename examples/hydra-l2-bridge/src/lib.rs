#![no_std]
#![no_main]

use alloy_core::primitives::{Address, Bytes, FixedBytes, U256};
use contract_derive::{contract, storage, interface, payable, Event};
use eth_riscv_runtime::{call::*, msg_sender, msg_value, revert};
use eth_riscv_runtime::types::*;

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

    /// depositTestBytes32(address to, bytes32 data, bytes32 context, address canceler) payable returns (bytes32)
    #[payable]
    pub fn depositTestBytes32(
        &mut self,
        to: Address,
        data: B32,
        context: B32,
        canceler: Address,
    ) -> B32 {
        let nonce = self.global_deposit_nonce.read();
        let from = msg_sender();
        let amount = U256::from(msg_value());
        let empty_data = Bytes::new();
        let empty_context = Bytes::new();
        let id = keccak_deposit(nonce, from, to, amount, &empty_data, &empty_context, canceler);

        // increment nonce
        self.global_deposit_nonce.write(nonce + U256::from(1u8));

        // signal the deposit id on SignalService: sendSignal(bytes32)
        let sig_addr = self.signal_service.read();
        let _ = ISignalService::new(sig_addr).with_ctx(self).sendSignal(id);

        // emit DepositMade with full flattened fields
        log::emit(DepositMade::new(id, nonce, from, to, amount, empty_data, empty_context, canceler));

        id
    }

    /// deposit(address to, bytes data, bytes context, address canceler) payable returns (bytes32)
    #[payable]
    pub fn deposit(
        &mut self,
        to: Address,
        data: Bytes,
        context: Bytes,
        canceler: Address,
    ) -> B32 {
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
        log::emit(DepositMade::new(id, nonce, from, to, amount, data, context, canceler));

        id
    }

    #[payable]
    pub fn depositTest(&mut self, to: Address, canceler: Address) -> B32 {
        let nonce = self.global_deposit_nonce.read();
        let from = msg_sender();
        let amount = U256::from(msg_value());
        let id = keccak_deposit(nonce, from, to, amount, &Bytes::new(), &Bytes::new(), canceler);
        
        // increment nonce
        self.global_deposit_nonce.write(nonce + U256::from(1u8));

        // signal the deposit id on SignalService: sendSignal(bytes32)
        let sig_addr = self.signal_service.read();
        let _ = ISignalService::new(sig_addr).with_ctx(self).sendSignal(id);

        log::emit(DepositMade::new(id, nonce, from, to, amount, Bytes::new(), Bytes::new(), canceler));

        id
    }

    /// claimDeposit((nonce,from,to,amount,data,context,canceler), bytes proof)
    pub fn claimDeposit(
        &mut self,
        nonce: U256,
        from: Address,
        to: Address,
        amount: U256,
        data: Bytes,
        context: Bytes,
        canceler: Address,
        proof: Bytes,
    ) {
        let id = self._claimDeposit(
            nonce.clone(),
            from,
            to,
            amount.clone(),
            data.clone(),
            context.clone(),
            canceler,
            to,
            data.clone(),
            proof,
        );

        // emit DepositClaimed with full flattened fields
        log::emit(DepositClaimed::new(id, nonce, from, to, amount, data, context, canceler));
    }

    /// cancelDeposit((...), address claimee, bytes proof)
    pub fn cancelDeposit(
        &mut self,
        nonce: U256,
        from: Address,
        to: Address,
        amount: U256,
        data: Bytes,
        context: Bytes,
        canceler: Address,
        claimee: Address,
        proof: Bytes,
    ) {
        if msg_sender() != canceler {
            revert();
        }
        let id = self._claimDeposit(
            nonce,
            from,
            to,
            amount,
            data,
            context,
            canceler,
            claimee,
            Bytes::new(),
            proof,
        );
        log::emit(DepositCancelled::new(id, claimee));
    }

    pub fn _claimDeposit(
        &mut self,
        nonce: U256,
        from: Address,
        to: Address,
        amount: U256,
        data: Bytes,
        context: Bytes,
        canceler: Address,
        payout_to: Address,
        payout_data: Bytes,
        proof: Bytes,
    ) -> B32 {
        // Generate the deposit ID
        let id = keccak_deposit(nonce, from, to, amount, &data, &context, canceler);

        // Revert if already processed
        if self.processed(id) {
            revert();
        }

        // --- call signalService.verifySignal(counterpart, id, proof) ---
        let sig_addr = self.signal_service.read();
        let counterpart = self.counterpart.read();
        let selector = 0x93e90f7eu32.to_be_bytes(); // keccak("verifySignal(address,bytes32,bytes)")[0..4]
        // Correct ABI encoding for dynamic bytes requires encoding the full tuple
        // to compute offsets, then prefixing the 4-byte selector.
        let mut args = (counterpart, id, proof).abi_encode();
        let mut calldata = Vec::with_capacity(4 + args.len());
        calldata.extend_from_slice(&selector);
        calldata.append(&mut args);
        let _ = call_contract(sig_addr, 0, &calldata, None);

        // Mark deposit as processed
        self.processed[id].write(U256::from(1u8));

        // --- ETH Transfer ---
        let Ok(value) = u64::try_from(amount) else {
            // If amount doesn't fit into u64, revert
            revert();
        };

        // ETH transfer – if this fails, runtime reverts automatically
        let _ = call_contract(payout_to, value, &payout_data, None);

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
    let encoded = (nonce, from, to, amount, data.clone(), context.clone(), canceler).abi_encode();
    FixedBytes::<32>::from(
        eth_riscv_runtime::keccak256(encoded.as_ptr() as u64, encoded.len() as u64)
            .to_be_bytes::<32>(),
    )
}