//! Auto-generated based on Cargo.toml dependencies
//! This file provides Deployable implementations for contract dependencies
//! TODO (phase-2): rather than using `fn deploy(args: Args)`, figure out the constructor selector from the contract dependency

use alloy_core::primitives::{Address, Bytes};
use eth_riscv_runtime::{create::Deployable, InitInterface, ReadOnly};
use core::include_bytes;

use hydra_bridged_erc721::IBridgedERC721;

const BRIDGEDERC721_BYTECODE: &'static [u8] = include_bytes!("../../../r55-output-bytecode/hydra-bridged-erc721.bin");

pub struct BridgedERC721;

impl Deployable for BridgedERC721 {
    type Interface = IBridgedERC721<ReadOnly>;

    fn __runtime() -> &'static [u8] {
        BRIDGEDERC721_BYTECODE
    }
}

