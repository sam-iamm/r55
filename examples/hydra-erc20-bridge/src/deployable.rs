//! Auto-generated based on Cargo.toml dependencies
//! This file provides Deployable implementations for contract dependencies
//! TODO (phase-2): rather than using `fn deploy(args: Args)`, figure out the constructor selector from the contract dependency

use alloy_core::primitives::{Address, Bytes};
use eth_riscv_runtime::{create::Deployable, InitInterface, ReadOnly};
use core::include_bytes;

use hydra_bridged_erc20::IBridgedERC20;

const BRIDGEDERC20_BYTECODE: &'static [u8] = include_bytes!("../../../r55-output-bytecode/hydra-bridged-erc20.bin");

pub struct BridgedERC20;

impl Deployable for BridgedERC20 {
    type Interface = IBridgedERC20<ReadOnly>;

    fn __runtime() -> &'static [u8] {
        BRIDGEDERC20_BYTECODE
    }
}

