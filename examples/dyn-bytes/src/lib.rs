#![no_std]
#![no_main]

use core::default::Default;

use alloy_core::primitives::{Address, U256, Bytes};
use contract_derive::{contract, show_streams};

extern crate alloc;

#[derive(Default, )]
pub struct DynBytes;

#[contract]
impl DynBytes {
    pub fn x_dyn_bytes(&mut self, input: Bytes) -> Bytes {
        input
    }
}
