//! MessageRelayer (R55/RISC-V)
//! 
//! Solidity refs:
//! @link stack/contracts/src/l2/MessageRelayer.sol
//! @link stack/contracts/src/l2/interfaces/IMessageRelayer.sol
//! @link stack/contracts/src/shared/interfaces/IETHBridge.sol
//!
//! Relays ETH bridge deposits to a target address with tip routing.
//!
//! Responsibilities:
//! - `relayMessage`: stores tip recipient, calls `ethBridge.claimDeposit`.
//! - `receiveMessage`: splits msg.value into forward value and tip, enforces
//!   reentrancy guard, validates gas limits, forwards to `to`, pays tip.
//!
//! R55 Parity Notes:
//! - Transient storage: uses regular slots (tip_recipient, reentrancy_entered).
//!   Reverts roll back state, so observable behavior matches Solidity TSTORE/TLOAD.
//! - Gas-limit: `gas_left()` (0x5A) with BUFFER=20,000 and 63/64 rule. `call_contract`
//!   has no gas param, so actual subcall uses all remaining gas. Revert path matches.
//! - Value overflow: `parse_u64_amount` reverts if value > u64::MAX.
//!

#![no_std]
#![no_main]

extern crate alloc;

use alloy_core::primitives::{keccak256 as alloy_keccak256, Address, Bytes, U256};
use contract_derive::{contract, storage, interface, Event, payable};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{call_contract, gas_left, msg_value, revert, revert_with_error};

// =============================================================================
// External interfaces
// =============================================================================

#[interface("camelCase")]
trait IETHBridge {
    fn claimDeposit(
        &mut self,
        ethDeposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
        proof: Bytes,
    );
}

// =============================================================================
// Storage
// =============================================================================

#[storage]
pub struct MessageRelayer {
    /// Address of the ETH bridge on this L2.
    eth_bridge: Slot<Address>,
    /// Pending tip recipient, matching `TIP_RECIPIENT_SLOT` semantics.
    tip_recipient: Slot<Address>,
    /// Reentrancy guard flag for `receiveMessage` (0 = not entered, 1 = entered).
    reentrancy_entered: Slot<U256>,
}

// =============================================================================
// Events
// =============================================================================

/// Event: MessageForwarded(address to, uint256 value, bytes data, address tipRecipient, uint256 tip)
#[derive(Event)]
pub struct MessageForwarded {
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
    pub tip_recipient: Address,
    pub tip: U256,
}

// =============================================================================
// Implementation
// =============================================================================

#[contract]
impl MessageRelayer {
    /// Constructor: store ETH bridge address.
    pub fn new(eth_bridge: Address) -> Self {
        if eth_bridge == Address::ZERO {
            revert();
        }
        let mut storage = MessageRelayer::default();
        storage.eth_bridge.write(eth_bridge);
        storage
    }

    /// ethBridge() → address
    pub fn ethBridge(&self) -> Address {
        self.eth_bridge.read()
    }

    /// relayMessage(ETHDeposit, bytes proof, address tipRecipient)
    pub fn relayMessage(
        &mut self,
        ethDeposit: (U256, Address, Address, U256, Bytes, Bytes, Address),
        proof: Bytes,
        tipRecipient: Address,
    ) {
        self.tip_recipient.write(tipRecipient);

        let bridge_addr = self.eth_bridge.read();
        let mut bridge = IETHBridge::new(bridge_addr).with_ctx(&mut *self);
        let result = bridge.claimDeposit(ethDeposit, proof);

        // Clear tip recipient to emulate transient storage (TSTORE) behavior,
        // preventing persistence if receiveMessage was skipped.
        self.tip_recipient.write(Address::ZERO);

        if result.is_none() {
            revert();
        }
    }

    /// receiveMessage(address to, uint256 tip, address userSelectedTipRecipient, uint256 gasLimit, bytes data)
    #[payable]
    pub fn receiveMessage(
        &mut self,
        to: Address,
        tip: U256,
        userSelectedTipRecipient: Address,
        gasLimit: U256,
        data: Bytes,
    ) {
        // Reentrancy guard
        if self.reentrancy_entered.read() == U256::from(1u8) {
            revert();
        }
        self.reentrancy_entered.write(U256::from(1u8));

        // Resolve tip recipient
        let mut tip_recipient = userSelectedTipRecipient;
        if tip_recipient == Address::ZERO {
            tip_recipient = self.tip_recipient.read();
            if tip_recipient == Address::ZERO {
                revert_selector(selector_no_tip_recipient());
            }
        }

        // Split msg.value
        let total = msg_value();
        if tip > total {
            revert();
        }
        let value_to_send = total - tip;
        let value_to_send_u64 = parse_u64_amount(value_to_send);
        let tip_u64 = parse_u64_amount(tip);

        // Forward value to `to`. See module docstring for gas-limit parity notes.
        const BUFFER: u64 = 20_000;
        let forward_result = if gasLimit.is_zero() {
            call_contract(to, value_to_send_u64, &data, None)
        } else {
            // EIP-150: 63/64 rule with buffer
            let current_gas = gas_left();
            let max_forwardable_gas = current_gas.saturating_sub(BUFFER) * 63 / 64;
            let gas_limit_u64 = parse_u64_amount(gasLimit);
            if gas_limit_u64 > max_forwardable_gas {
                revert_selector(selector_insufficient_gas());
            }
            call_contract(to, value_to_send_u64, &data, None)
        };

        if forward_result.is_err() {
            revert_selector(selector_message_forwarding_failed());
        }

        // Clear pending tip recipient, pay tip
        self.tip_recipient.write(Address::ZERO);
        let tip_result = call_contract(tip_recipient, tip_u64, &Bytes::new(), None);
        if tip_result.is_err() {
            revert_selector(selector_tip_transfer_failed());
        }

        // Emit event
        eth_riscv_runtime::log::emit(MessageForwarded {
            to,
            value: value_to_send,
            data: data.clone(),
            tip_recipient,
            tip,
        });

        // Clear reentrancy flag (only needed on success path)
        self.reentrancy_entered.write(U256::from(0u8));
    }
}

// =============================================================================
// Internals
// =============================================================================

/// Parse U256 → u64; reverts on overflow.
fn parse_u64_amount(amount: U256) -> u64 {
    let bytes = amount.to_be_bytes::<32>();
    if bytes[..24].iter().any(|&b| b != 0) {
        revert();
    }
    u64::from_be_bytes(bytes[24..32].try_into().unwrap())
}

/// Revert with provided selector.
fn revert_selector(sel: [u8; 4]) -> ! {
    revert_with_error(&sel)
}

/// keccak256("NoTipRecipient()")[:4]
fn selector_no_tip_recipient() -> [u8; 4] {
    let h = alloy_keccak256(b"NoTipRecipient()");
    [h[0], h[1], h[2], h[3]]
}

/// keccak256("MessageForwardingFailed()")[:4]
fn selector_message_forwarding_failed() -> [u8; 4] {
    let h = alloy_keccak256(b"MessageForwardingFailed()");
    [h[0], h[1], h[2], h[3]]
}

/// keccak256("TipTransferFailed()")[:4]
fn selector_tip_transfer_failed() -> [u8; 4] {
    let h = alloy_keccak256(b"TipTransferFailed()");
    [h[0], h[1], h[2], h[3]]
}

/// keccak256("InsufficientGas()")[:4]
fn selector_insufficient_gas() -> [u8; 4] {
    let h = alloy_keccak256(b"InsufficientGas()");
    [h[0], h[1], h[2], h[3]]
}
