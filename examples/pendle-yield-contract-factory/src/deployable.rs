//! Auto-generated based on Cargo.toml dependencies
//! This file provides Deployable implementations for contract dependencies
//! TODO (phase-2): rather than using `fn deploy(args: Args)`, figure out the constructor selector from the contract dependency

use alloy_core::primitives::{Address, Bytes};
use eth_riscv_runtime::{create::Deployable, InitInterface, ReadOnly};
use core::include_bytes;

use pendle_principal_token::IPendlePrincipalToken;
use pendle_yield_token::IPendleYieldToken;

const PENDLEPRINCIPALTOKEN_BYTECODE: &'static [u8] = include_bytes!("../../../r55-output-bytecode/pendle-principal-token.bin");
const PENDLEYIELDTOKEN_BYTECODE: &'static [u8] = include_bytes!("../../../r55-output-bytecode/pendle-yield-token.bin");

pub struct PendlePrincipalToken;

impl Deployable for PendlePrincipalToken {
    type Interface = IPendlePrincipalToken<ReadOnly>;

    fn __runtime() -> &'static [u8] {
        PENDLEPRINCIPALTOKEN_BYTECODE
    }
}

pub struct PendleYieldToken;

impl Deployable for PendleYieldToken {
    type Interface = IPendleYieldToken<ReadOnly>;

    fn __runtime() -> &'static [u8] {
        PENDLEYIELDTOKEN_BYTECODE
    }
}

