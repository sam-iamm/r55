//! L1StateRoot (R55/RISC-V)
//!
//! Proves L1 state roots on L2 using EIP-4788 beacon roots and RLP header verification.
//!
//! Responsibilities:
//! - `proveL1StateRoot`: fetches beacon root, verifies header hash, extracts state root,
//!   stores it, signals via SignalService, emits `L1StateRootProven`.
//! - `getL1StateRoot`: returns stored state root (reverts if missing).
//! - `isProven`: returns true if state root proven for given L1 block hash.
//!
//! R55 Parity Notes:
//! - Beacon roots: Solidity hardcodes EIP-4788 address; R55 takes it as constructor arg.
//! - RLP decoding: uses `alloy_rlp` to extract state root (index 3) and block number (index 8).
//! - Reverts: Solidity uses `require(..., "msg")`; R55 uses bare `revert()`. Hydra relaxed
//!   matching allows different payloads if result kind matches.
//!
//! Solidity refs:
//! stack/contracts/src/l2/L1StateRoot.sol
//! stack/contracts/src/l2/interfaces/IL1StateRoot.sol

#![no_std]
#![no_main]

use alloy_core::primitives::{
    keccak256 as alloy_keccak256, Address, Bytes, FixedBytes, B256, U256,
};
use alloy_rlp::{self, Decodable};
use contract_derive::{contract, interface, storage, Event};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{revert, staticcall_contract};

extern crate alloc;

type B32 = FixedBytes<32>;

// =============================================================================
// Events
// =============================================================================

/// Event: L1StateRootProven(bytes32 indexed l1BlockHash, bytes32 indexed stateRoot, bytes l1BlockNumber)
#[derive(Event)]
pub struct L1StateRootProven {
    #[indexed]
    pub l1_block_hash: B32,
    #[indexed]
    pub state_root: B32,
    pub l1_block_number: Bytes,
}

// =============================================================================
// External interfaces
// =============================================================================

#[interface("camelCase")]
trait ISignalService {
    fn sendSignal(&mut self, value: B32) -> B32;
}

// =============================================================================
// Storage
// =============================================================================

#[storage]
pub struct L1StateRoot {
    /// Address of the L2 SignalService contract.
    signal_service: Slot<Address>,
    /// Address of the EIP-4788 beacon roots predeploy on this L2.
    beacon_roots: Slot<Address>,
    /// Mapping: L1 block hash -> L1 state root.
    state_roots: Mapping<B32, Slot<B32>>,
}

// Indices within the RLP-encoded L1 block header list.
const STATE_ROOT_HEADER_INDEX: usize = 3;
const BLOCK_NUMBER_HEADER_INDEX: usize = 8;

// =============================================================================
// Implementation
// =============================================================================

#[contract]
impl L1StateRoot {
    /// Constructor: store SignalService and beacon roots addresses.
    pub fn new(signal_service: Address, beacon_roots: Address) -> Self {
        let mut s = L1StateRoot::default();
        s.signal_service.write(signal_service);
        s.beacon_roots.write(beacon_roots);
        s
    }

    /// getL1StateRoot(bytes32 l1BlockHash) → bytes32
    pub fn getL1StateRoot(&self, l1_block_hash: B32) -> B32 {
        let state_root = self.state_roots[l1_block_hash].read();
        if state_root == B32::from([0u8; 32]) {
            revert();
        }
        state_root
    }

    /// isProven(bytes32 l1BlockHash) → bool
    pub fn isProven(&self, l1_block_hash: B32) -> bool {
        self.state_roots[l1_block_hash].read() != B32::from([0u8; 32])
    }

    /// proveL1StateRoot(uint256 l2BlockTimestamp, bytes l1HeaderRLP)
    pub fn proveL1StateRoot(&mut self, l2_block_timestamp: U256, l1_header_rlp: Bytes) {
        // Fetch L1 block hash from EIP-4788 beacon roots
        let calldata = alloy_sol_types::SolValue::abi_encode_params(&(l2_block_timestamp,));
        let beacon_roots_addr = self.beacon_roots.read();
        let precompile_ret = staticcall_contract(beacon_roots_addr, 0, &calldata, None)
            .unwrap_or_else(|_| revert());

        if precompile_ret.len() != 32 {
            revert();
        }
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&precompile_ret[..32]);
        let l1_block_hash = B32::from(hash_bytes);

        // Require non-zero hash
        if l1_block_hash == B32::from([0u8; 32]) {
            revert();
        }

        // Verify header hash
        let header_hash: B256 = alloy_keccak256(l1_header_rlp.as_ref());
        if header_hash.as_slice() != l1_block_hash.as_slice() {
            revert();
        }

        // RLP-decode header, extract stateRoot and blockNumber
        let mut buf: &[u8] = l1_header_rlp.as_ref();
        let items = match alloy_rlp::Header::decode_raw(&mut buf) {
            Ok(alloy_rlp::PayloadView::List(list)) => list,
            _ => revert(),
        };

        if items.len() <= STATE_ROOT_HEADER_INDEX || items.len() <= BLOCK_NUMBER_HEADER_INDEX {
            revert();
        }

        // State root at index 3
        let mut state_item = items[STATE_ROOT_HEADER_INDEX].clone();
        let state_bytes = match alloy_rlp::Bytes::decode(&mut state_item) {
            Ok(b) => b,
            Err(_) => revert(),
        };
        if state_bytes.len() != 32 {
            revert();
        }
        let mut sr_arr = [0u8; 32];
        sr_arr.copy_from_slice(&state_bytes[..32]);
        let state_root = B32::from(sr_arr);

        // Block number at index 8
        let mut number_item = items[BLOCK_NUMBER_HEADER_INDEX].clone();
        let block_number_bytes = match alloy_rlp::Bytes::decode(&mut number_item) {
            Ok(b) => b,
            Err(_) => revert(),
        };
        let l1_block_number = Bytes::from(block_number_bytes.to_vec());

        // Store state root
        self.state_roots[l1_block_hash].write(state_root);

        // Signal via SignalService
        let sig_addr = self.signal_service.read();
        let mut sig = ISignalService::new(sig_addr).with_ctx(&mut *self);
        if sig.sendSignal(state_root).is_none() {
            revert();
        }

        // Emit event
        eth_riscv_runtime::log::emit(L1StateRootProven {
            l1_block_hash,
            state_root,
            l1_block_number,
        });
    }
}
