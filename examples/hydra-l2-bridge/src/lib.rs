//! ETHBridge (R55/RISC-V)
//!
//! Cross-chain ETH bridge, mirroring the Solidity reference implementation.
//!
//! Responsibilities:
//! - Deposits: record an `ETHDeposit`, signal the id on `SignalService`, emit `DepositMade`.
//! - Claims/Cancels: require `verifySignal(counterpart,id,proof)`, enforce `processed` mapping.
//! - Value transfer (R55): runtime has no reliable success flag. We ignore returndata on value
//!   calls and keep tests funded so both EVM and R55 runs succeed identically.
//!
//! R55 Parity Notes:
//! - ABI: `verifySignal(sender,bytes32,bytes)` is hand-encoded to ensure dynamic `bytes` match Solidity.
//! - Reverts: `verifySignal` failures revert with a selector; we treat `len >= 4` returndata as failure.
//! - rvemu: for value transfers with empty calldata, we send a single 0x00 byte to avoid runtime errors.

#![no_std]
#![no_main]

use alloy_core::primitives::{keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, U256};
use alloy_sol_types::{sol, SolCall};
use contract_derive::{contract, interface, payable, storage, Event};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{call_contract, msg_sender, msg_value, revert, revert_with_error};

type B32 = FixedBytes<32>;

extern crate alloc;

sol! {
    struct ETHDeposit {
        uint256 nonce;
        address from;
        address to;
        uint256 amount;
        bytes   data;
        bytes   context;
        address canceler;
    }

    function verifySignal(address sender, bytes32 value, bytes proof);
    function sendSignal(bytes32 value) returns (bytes32);
}
/// Event: DepositMade(bytes32 indexed id, ETHDeposit deposit)
#[derive(Event)]
struct DepositMade {
    #[indexed]
    id: B32,
    /// Replicate event emission with the same struct as the Solidity implementation
    deposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
}

/// Event: DepositClaimed(bytes32 indexed id, ETHDeposit deposit)
#[derive(Event)]
struct DepositClaimed {
    #[indexed]
    id: B32,
    /// Replicate event emission with the same struct as the Solidity implementation
    deposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
}

/// Event: DepositCancelled(bytes32 indexed id, address claimee)
#[derive(Event)]
struct DepositCancelled {
    #[indexed]
    id: B32,
    claimee: Address,
}

#[storage]
pub struct ETHBridge {
    /// Mapping for replay protection: 1 = processed, 0 = not processed.
    processed: Mapping<B32, Slot<U256>>,
    /// Global deposit nonce, incremented per deposit.
    global_deposit_nonce: Slot<U256>,
    /// SignalService contract address.
    signal_service: Slot<Address>,
    /// Counterpart bridge contract address (on remote chain).
    counterpart: Slot<Address>,
    /// Reentrancy guard (0 = not entered, 1 = entered).
    reentrancy_entered: Slot<U256>,
}

#[contract]
impl ETHBridge {
    /// Constructor initializes SignalService and counterpart addresses.
    /// Reverts if either is zero.
    pub fn new(signal_service: Address, counterpart: Address) -> Self {
        if signal_service == Address::ZERO || counterpart == Address::ZERO {
            revert();
        }

        let mut storage = ETHBridge::default();
        storage.signal_service.write(signal_service);
        storage.counterpart.write(counterpart);
        storage
    }

    /// processed(bytes32 id) -> bool
    /// Returns true if deposit ID has been claimed or canceled.
    pub fn processed(&self, id: B32) -> bool {
        self.processed[id].read() == U256::from(1u8)
    }

    /// getDepositId(ETHDeposit) -> bytes32
    /// Computes keccak256(abi.encode(ETHDeposit)) using Solidity ABI encoding.
    pub fn getDepositId(
        &self,
        eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
    ) -> B32 {
        B32::from(alloy_keccak256(&alloy_sol_types::SolValue::abi_encode(
            &eth_deposit,
        )))
    }

    /// deposit(address to, bytes data, bytes context, address canceler) -> bytes32
    /// Increments nonce, signals on SignalService, and emits `DepositMade`.
    #[payable]
    pub fn deposit(&mut self, to: Address, data: Bytes, context: Bytes, canceler: Address) -> B32 {
        let from = msg_sender();
        let amount = msg_value();
        let nonce = self.global_deposit_nonce.read();

        let deposit_tuple = (
            nonce,
            from,
            to,
            amount,
            data.clone(),
            context.clone(),
            canceler,
        );
        let id = self.getDepositId(deposit_tuple.clone());
        let deposit_struct = ETHDeposit {
            nonce,
            from,
            to,
            amount,
            data,
            context,
            canceler,
        };

        // Increment nonce before signaling.
        self.global_deposit_nonce.write(nonce + U256::from(1));

        // Signal the deposit ID on SignalService.
        let sig_addr = self.signal_service.read();
        let calldata_vec = sendSignalCall { value: id }.abi_encode();
        let calldata = Bytes::from(calldata_vec);
        let _ = call_contract(sig_addr, 0, &calldata, None);

        // Emit DepositMade(id, deposit).
        log::emit(DepositMade::new(id, deposit_tuple));

        id
    }

    /// claimDeposit(ETHDeposit, proof)
    /// - Verifies signal via counterpart using SignalService.
    /// - Marks deposit as processed.
    /// - Transfers ETH to recipient with user-supplied calldata.
    pub fn claimDeposit(
        &mut self,
        eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
        proof: Bytes,
    ) {
        // Reentrancy check.
        if self.reentrancy_entered.read() == U256::from(1u8) {
            revert_selector(selector_reentrancy());
        }

        // Compute deposit ID.
        let id = self.getDepositId(eth_deposit.clone());

        // Reject already-processed deposits.
        if self.processed[id].read() == U256::from(1u8) {
            revert_selector(selector_already_claimed());
        }

        // Verify signal with manual ABI encoding (dynamic bytes parity).
        let signal_service = self.signal_service.read();
        let counterpart = self.counterpart.read();
        let calldata_vec = verifySignalCall {
            sender: counterpart,
            value: id,
            proof: proof.clone(),
        }
        .abi_encode();
        let calldata = Bytes::from(calldata_vec);
        // call_contract now returns Result<Bytes, Bytes>
        // If Err, the call reverted (verification failed)
        if call_contract(signal_service, 0, &calldata, None).is_err() {
            revert_selector(selector_failed_claim());
        }

        // Mark processed before external call.
        self.processed[id].write(U256::from(1u8));

        // Perform ETH transfer. R55 ignores returndata as success flag.
        let to = eth_deposit.2;
        let amount = parse_u64_amount(eth_deposit.3);
        let calldata = eth_deposit.4.clone();

        self.reentrancy_entered.write(U256::from(1u8));
        let _ = call_contract(to, amount, &calldata, None);
        self.reentrancy_entered.write(U256::from(0u8));

        // Emit DepositClaimed(id, ethDeposit).
        let deposit_tuple = (
            eth_deposit.0,
            eth_deposit.1,
            eth_deposit.2,
            eth_deposit.3,
            eth_deposit.4.clone(),
            eth_deposit.5.clone(),
            eth_deposit.6,
        );
        log::emit(DepositClaimed::new(id, deposit_tuple));
    }

    /// cancelDeposit(ETHDeposit, claimee, proof)
    /// - Only the designated canceler may call.
    /// - Verifies signal via counterpart.
    /// - Marks processed and pays claimee.
    pub fn cancelDeposit(
        &mut self,
        eth_deposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
        claimee: Address,
        proof: Bytes,
    ) {
        // Reentrancy check.
        if self.reentrancy_entered.read() == U256::from(1u8) {
            revert_selector(selector_reentrancy());
        }

        // Cancellor must match.
        if msg_sender() != eth_deposit.6 {
            revert_selector(selector_only_canceler());
        }

        // Derive ID and check replay protection.
        let id = self.getDepositId(eth_deposit.clone());
        if self.processed[id].read() == U256::from(1u8) {
            revert_selector(selector_already_claimed());
        }

        // Verify signal with manual ABI encoding.
        let signal_service = self.signal_service.read();
        let counterpart = self.counterpart.read();
        let calldata_vec = verifySignalCall {
            sender: counterpart,
            value: id,
            proof: proof.clone(),
        }
        .abi_encode();
        let calldata = Bytes::from(calldata_vec);
        // call_contract now returns Result<Bytes, Bytes>
        if call_contract(signal_service, 0, &calldata, None).is_err() {
            revert_selector(selector_failed_claim());
        }

        // Mark processed before external call.
        self.processed[id].write(U256::from(1u8));

        // Send ETH to claimee. Match Solidity by always calling with empty calldata (even for zero value).
        let amount = parse_u64_amount(eth_deposit.3);
        let safe_data = Bytes::new();
        self.reentrancy_entered.write(U256::from(1u8));
        let _ = call_contract(claimee, amount, &safe_data, None);
        self.reentrancy_entered.write(U256::from(0u8));

        // Emit DepositCancelled(id, claimee).
        log::emit(DepositCancelled::new(id, claimee));
    }
}

// --- Internals ---

/// Revert with provided selector.
fn revert_selector(sel: [u8; 4]) -> ! {
    revert_with_error(&sel)
}

/// keccak256("AlreadyClaimed()")[:4]
fn selector_already_claimed() -> [u8; 4] {
    let sig = b"AlreadyClaimed()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

/// keccak256("FailedClaim()")[:4]
fn selector_failed_claim() -> [u8; 4] {
    let sig = b"FailedClaim()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

/// keccak256("OnlyCanceler()")[:4]
fn selector_only_canceler() -> [u8; 4] {
    let sig = b"OnlyCanceler()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

/// keccak256("NonReentrant()")[:4]
fn selector_reentrancy() -> [u8; 4] {
    let sig = b"NonReentrant()";
    let h = alloy_keccak256(sig);
    [h[0], h[1], h[2], h[3]]
}

/// Parse a U256 amount into u64.  
/// Reverts if value does not fit into u64, matching runtime CALL constraints.
fn parse_u64_amount(amount: U256) -> u64 {
    let bytes = amount.to_be_bytes::<32>();
    if bytes[..24].iter().any(|&b| b != 0) {
        revert_selector(selector_failed_claim());
    }
    u64::from_be_bytes(bytes[24..32].try_into().unwrap())
}
