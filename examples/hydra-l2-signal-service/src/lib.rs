#![no_std]
#![no_main]

use alloy_core::primitives::{Address, Bytes, FixedBytes, U256};
use contract_derive::{contract, storage, Event};
use eth_riscv_runtime::types::*;
use eth_riscv_runtime::{keccak256, msg_sender, revert, sload, sstore};

extern crate alloc;
use alloc::vec::Vec;
use alloc::vec;

// bytes32 alias that the macro recognizes AND abi-decodes without conversions
type B32 = FixedBytes<32>;

// ---- minimal ABI decoder for (bytes[] accountProof, bytes[] storageProof, bytes32 stateRoot)

fn read_u256_value(data: &[u8], off: usize) -> Option<usize> {
    if off + 32 > data.len() { return None; }
    let word = &data[off..off+32];
    if word[..24].iter().any(|&b| b != 0) { return None; }
    let mut u = [0u8; 8];
    u.copy_from_slice(&word[24..]);
    Some(u64::from_be_bytes(u) as usize)
}

fn read_len_prefixed_bytes(data: &[u8], off: usize) -> Option<&[u8]> {
    let len = read_u256_value(data, off)?;
    let start = off + 32;
    let end = start + len;
    if end > data.len() { return None; }
    Some(&data[start..end])
}

// bytes[] decoding: at base offset (from start), layout = [len][len * 32 offsets][tails...]
fn decode_bytes_array(data: &[u8], base: usize) -> Option<alloc::vec::Vec<Bytes>> {
    if base + 32 > data.len() { return None; }
    let count = read_u256_value(data, base)?;
    let heads = base + 32;
    if heads + count * 32 > data.len() { return None; }
    let mut out = alloc::vec::Vec::with_capacity(count);
    for i in 0..count {
        let rel = read_u256_value(data, heads + i * 32)?;
        let elem_head = base + rel;
        let elem = read_len_prefixed_bytes(data, elem_head)?;
        out.push(Bytes::from(elem.to_vec()));
    }
    Some(out)
}

struct DecodedProof {
    account_proof: alloc::vec::Vec<Bytes>,
    storage_proof: alloc::vec::Vec<Bytes>,
    state_root: B32,
}

fn decode_signal_proof(data: &[u8]) -> Option<DecodedProof> {
    // head: 3*32 -> [off_account][off_storage][stateRoot]
    if data.len() < 96 { return None; }
    let off_account = read_u256_value(data, 0)?;
    let off_storage = read_u256_value(data, 32)?;
    let mut sr = [0u8; 32];
    sr.copy_from_slice(&data[64..96]);
    let account_proof = decode_bytes_array(data, off_account)?;
    let storage_proof = decode_bytes_array(data, off_storage)?;
    Some(DecodedProof {
        account_proof,
        storage_proof,
        state_root: FixedBytes::<32>::from(sr),
    })
}

#[derive(Event)]
pub struct SignalSent {
    #[indexed]
    pub sender: Address,
    pub value: B32,
}

#[derive(Event)]
pub struct SignalVerified {
    #[indexed]
    pub sender: Address,
    pub value: B32,
}

#[storage]
pub struct SignalService {
    state_root_publisher: Slot<Address>,
    this_address: Slot<Address>,
}

#[contract]
impl SignalService {
    pub fn new(state_root_publisher: Address, this_address: Address) -> Self {
        let mut s = SignalService::default();
        s.state_root_publisher.write(state_root_publisher);
        s.this_address.write(this_address);
        s
    }

    // sendSignal(bytes32) -> bytes32
    pub fn sendSignal(&mut self, value: B32) -> B32 {
        let sender = msg_sender();
        let slot = derive_slot(value, sender);
        sstore(slot, U256::from(1u8));
        let out = slot.to_be_bytes::<32>();
        let ret = FixedBytes::<32>::from(out);
        log::emit(SignalSent::new(sender, value));
        ret
    }

    // isSignalStored(bytes32,address) -> bool
    pub fn isSignalStored(&self, value: B32, sender: Address) -> bool {
        let slot = derive_slot(value, sender);
        sload(slot) == U256::from(1u8)
    }

    // verifySignal(address,bytes32,bytes)
    pub fn verifySignal(&self, _sender: Address, _value: B32, proof: Bytes) {
        // Decode (bytes[] accountProof, bytes[] storageProof, bytes32 stateRoot)
        let DecodedProof { account_proof, storage_proof, state_root } =
            decode_signal_proof(proof.as_ref()).unwrap_or_else(|| revert());
    
        // Account proof must be non-empty (AccountProofEmpty)
        if account_proof.is_empty() { revert(); }
    
        // Require publisher signaled the state root (StateRootNotFound)
        let publisher = self.state_root_publisher.read();
        if !self.isSignalStored(state_root, publisher) { revert(); }
    
        // Perform MPT verification when the `mpt` feature is enabled.
        #[cfg(feature = "mpt")]
        {
            let slot_u256 = derive_slot(_value, _sender);
            let slot_b32 = FixedBytes::<32>::from(slot_u256.to_be_bytes::<32>());
            if !verify_mpt_signal(self.this_address.read(), slot_b32, state_root, &account_proof, &storage_proof) {
                revert();
            }
        }
    
        log::emit(SignalVerified::new(_sender, _value));
    }
}

// ---- helpers: ERC-7201 slot derivation ----

// abi.encodePacked(value (32), account (20))
fn packed_value_sender(value: B32, account: Address) -> Vec<u8> {
    let mut buf = Vec::with_capacity(52);
    buf.extend_from_slice(value.as_slice()); // value is FixedBytes<32>
    buf.extend_from_slice(account.as_slice());
    buf
}

// slot = keccak256( keccak256(ns_bytes) - 1 as 32 bytes ) & ~0xff
fn erc7201_slot(ns: &[u8]) -> U256 {
    let h1 = keccak256(ns.as_ptr() as u64, ns.len() as u64);
    let h1m1 = h1 - U256::from(1u8);
    let h1m1_be = h1m1.to_be_bytes::<32>(); // specify const generic size
    let h2 = keccak256(h1m1_be.as_ptr() as u64, h1m1_be.len() as u64);
    h2 & !U256::from(0xffu8)
}

// deriveSlot(value, account)
fn derive_slot(value: B32, account: Address) -> U256 {
    let ns = packed_value_sender(value, account);
    erc7201_slot(&ns)
}

// ---- MPT verification (feature-gated) ----

#[cfg(feature = "mpt")]
fn verify_mpt_signal(
    contract_addr: Address,
    slot: B32,
    state_root: B32,
    account_proof: &alloc::vec::Vec<Bytes>,
    storage_proof: &alloc::vec::Vec<Bytes>,
) -> bool {
    use ethereum_triedb::{EIP1186Layout, StorageProof};
    // Minimal no_std Keccak hasher plumbing for trie_db/memory_db
    use core::hash::Hasher;
    use primitive_types::H256;
    use rlp::Rlp;
    use trie_db::{Trie, TrieDBBuilder};

    #[derive(Default)]
    struct KeccakStreamingHasher {
        buf: alloc::vec::Vec<u8>,
    }

    impl Hasher for KeccakStreamingHasher {
        fn finish(&self) -> u64 {
            let h = eth_riscv_runtime::keccak256(self.buf.as_ptr() as u64, self.buf.len() as u64);
            let b = h.to_be_bytes::<32>();
            u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        }
        fn write(&mut self, bytes: &[u8]) { self.buf.extend_from_slice(bytes); }
    }

    struct KeccakHasher;
    impl hash_db::Hasher for KeccakHasher {
        type Out = H256;
        type StdHasher = KeccakStreamingHasher;
        const LENGTH: usize = 32;
        fn hash(x: &[u8]) -> Self::Out {
            let h = eth_riscv_runtime::keccak256(x.as_ptr() as u64, x.len() as u64);
            H256::from_slice(&h.to_be_bytes::<32>())
        }
    }

    // RLP(uint256(1)) is the single byte 0x01
    let expected_value_rlp: Vec<u8> = vec![0x01];

    // 1) account proof → storageRoot
    let state_root_h256 = H256::from_slice(state_root.as_slice());

    let account_db = StorageProof::new(
        account_proof
            .iter()
            .map(|b| b.as_ref().to_vec())
            .collect::<Vec<Vec<u8>>>(),
    ).into_memory_db::<KeccakHasher>();

    let account_trie = TrieDBBuilder::<EIP1186Layout<KeccakHasher>>::new(&account_db, &state_root_h256).build();

    // keccak256(address) as the secure key
    let addr_bytes = contract_addr.as_slice();
    let addr_hash_u256 = crate::keccak256(addr_bytes.as_ptr() as u64, addr_bytes.len() as u64);
    let account_key = <[u8; 32]>::from(addr_hash_u256.to_be_bytes::<32>());

    let raw_account = match account_trie.get(&account_key) {
        Ok(Some(val)) => val.to_vec(),
        _ => return false,
    };

    // RLP account fields: [nonce, balance, storageRoot, codeHash]
    let account_rlp = Rlp::new(&raw_account);
    let storage_root_bytes = match account_rlp.at(2).and_then(|it| it.data()) {
        Ok(bytes) if bytes.len() == 32 => bytes,
        _ => return false,
    };
    let storage_root_h256 = H256::from_slice(storage_root_bytes);

    // 2) storage proof → value at keccak256(slot)
    let storage_db = StorageProof::new(
        storage_proof
            .iter()
            .map(|b| b.as_ref().to_vec())
            .collect::<Vec<Vec<u8>>>(),
    ).into_memory_db::<KeccakHasher>();

    let storage_trie = TrieDBBuilder::<EIP1186Layout<KeccakHasher>>::new(&storage_db, &storage_root_h256).build();

    let slot_arr: [u8; 32] = <[u8; 32]>::from(slot);
    let slot_hash_u256 = crate::keccak256(slot_arr.as_ptr() as u64, slot_arr.len() as u64);
    let storage_key = <[u8; 32]>::from(slot_hash_u256.to_be_bytes::<32>());

    let raw_value = match storage_trie.get(&storage_key) {
        Ok(Some(val)) => val.to_vec(),
        _ => return false,
    };

    raw_value.as_slice() == expected_value_rlp.as_slice()
}