extern crate alloc;
use alloc::string::String;
use alloy_core::primitives::{Address, Bytes, U32, U256, I256, FixedBytes};
use alloy_sol_types::{SolType, SolValue};
use ext_alloc::vec::Vec;
use core::{arch::asm, marker::PhantomData, u64};
use eth_riscv_syscalls::Syscall;

use crate::{FromBuilder, InitInterface, MethodCtx, ReadWrite};

// Minimal, non-invasive ABI upcast helper to mirror contract-derive's u8→U256 handling
// for constructor argument encoding.
//
// Encoding semantics:
// - bytes (dynamic): use `Bytes` (or `&Bytes`).
// - uint8[] (dynamic): `Vec<u8>` / `&[u8]` → `Vec<U256>` (element-wise upcast).
// - fixed arrays: `[T; N]` → `[T::Output; N]` via element upcast (e.g., `[u8; N]` → `[U256; N]`).
// - bytesN (fixed): pass `FixedBytes<N>` to encode as a single 32-byte word.
// - tuples: supported up to arity 10.
// - borrowed forms: `&T`, `&str`, `&[T]` supported without forcing ownership.
// - single-arg constructors: pass as a 1‑tuple `(arg,)` to use params encoding.
trait AbiUpcast {
    type Output;
    fn upcast(self) -> Self::Output;
}

macro_rules! impl_identity_upcast {
    ($($t:ty),+ $(,)?) => {$(
        impl AbiUpcast for $t { type Output = $t; #[inline] fn upcast(self) -> Self::Output { self } }
    )+}
}

impl AbiUpcast for u8 {
    type Output = U256;
    #[inline]
    fn upcast(self) -> Self::Output {
        U256::from(self)
    }
}

// Identity upcasts
impl_identity_upcast!(U256, I256, Address, Bytes, String, bool, u16, u32, u64, u128, i8, i16, i32, i64, i128);
impl<const N: usize> AbiUpcast for FixedBytes<N> { type Output = FixedBytes<N>; #[inline] fn upcast(self) -> Self::Output { self } }

// Containers
impl<T: AbiUpcast> AbiUpcast for alloc::vec::Vec<T> {
    type Output = alloc::vec::Vec<<T as AbiUpcast>::Output>;
    #[inline]
    fn upcast(self) -> Self::Output {
        self.into_iter().map(|v| v.upcast()).collect()
    }
}

// Borrowed forms to avoid forcing ownership at call sites
impl<'a, T> AbiUpcast for &'a T
where
    T: AbiUpcast + Clone,
{
    type Output = <T as AbiUpcast>::Output;
    #[inline]
    fn upcast(self) -> Self::Output { self.clone().upcast() }
}

impl<'a> AbiUpcast for &'a str {
    type Output = String;
    #[inline]
    fn upcast(self) -> Self::Output { String::from(self) }
}

impl<'a, T> AbiUpcast for &'a [T]
where
    T: AbiUpcast + Clone,
{
    type Output = Vec<<T as AbiUpcast>::Output>;
    #[inline]
    fn upcast(self) -> Self::Output { self.iter().cloned().map(|v| v.upcast()).collect() }
}

// Fixed-size arrays: element-wise upcast
impl<T: AbiUpcast + Clone, const N: usize> AbiUpcast for [T; N] {
    type Output = [<T as AbiUpcast>::Output; N];
    #[inline]
    fn upcast(self) -> Self::Output {
        let arr = self;
        core::array::from_fn(|i| arr[i].clone().upcast())
    }
}

// Unit tuple for zero-arg constructors
impl AbiUpcast for () { type Output = (); #[inline] fn upcast(self) -> Self::Output { self } }

// Tuple upcasts (cover common arities used by constructors; limited to 10)
impl<A: AbiUpcast> AbiUpcast for (A,) {
    type Output = (A::Output,);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(),) }
}

impl<A: AbiUpcast, B: AbiUpcast> AbiUpcast for (A, B) {
    type Output = (A::Output, B::Output);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(), self.1.upcast()) }
}

impl<A: AbiUpcast, B: AbiUpcast, C: AbiUpcast> AbiUpcast for (A, B, C) {
    type Output = (A::Output, B::Output, C::Output);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(), self.1.upcast(), self.2.upcast()) }
}

impl<A: AbiUpcast, B: AbiUpcast, C: AbiUpcast, D: AbiUpcast> AbiUpcast for (A, B, C, D) {
    type Output = (A::Output, B::Output, C::Output, D::Output);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(), self.1.upcast(), self.2.upcast(), self.3.upcast()) }
}

impl<A: AbiUpcast, B: AbiUpcast, C: AbiUpcast, D: AbiUpcast, E: AbiUpcast> AbiUpcast for (A, B, C, D, E) {
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(), self.1.upcast(), self.2.upcast(), self.3.upcast(), self.4.upcast()) }
}

impl<A: AbiUpcast, B: AbiUpcast, C: AbiUpcast, D: AbiUpcast, E: AbiUpcast, F: AbiUpcast> AbiUpcast for (A, B, C, D, E, F) {
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output, F::Output);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(), self.1.upcast(), self.2.upcast(), self.3.upcast(), self.4.upcast(), self.5.upcast()) }
}

impl<A: AbiUpcast, B: AbiUpcast, C: AbiUpcast, D: AbiUpcast, E: AbiUpcast, F: AbiUpcast, G: AbiUpcast> AbiUpcast for (A, B, C, D, E, F, G) {
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output, F::Output, G::Output);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(), self.1.upcast(), self.2.upcast(), self.3.upcast(), self.4.upcast(), self.5.upcast(), self.6.upcast()) }
}

impl<A: AbiUpcast, B: AbiUpcast, C: AbiUpcast, D: AbiUpcast, E: AbiUpcast, F: AbiUpcast, G: AbiUpcast, H: AbiUpcast> AbiUpcast for (A, B, C, D, E, F, G, H) {
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output, F::Output, G::Output, H::Output);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(), self.1.upcast(), self.2.upcast(), self.3.upcast(), self.4.upcast(), self.5.upcast(), self.6.upcast(), self.7.upcast()) }
}

impl<A: AbiUpcast, B: AbiUpcast, C: AbiUpcast, D: AbiUpcast, E: AbiUpcast, F: AbiUpcast, G: AbiUpcast, H: AbiUpcast, I: AbiUpcast> AbiUpcast for (A, B, C, D, E, F, G, H, I) {
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output, F::Output, G::Output, H::Output, I::Output);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(), self.1.upcast(), self.2.upcast(), self.3.upcast(), self.4.upcast(), self.5.upcast(), self.6.upcast(), self.7.upcast(), self.8.upcast()) }
}

impl<A: AbiUpcast, B: AbiUpcast, C: AbiUpcast, D: AbiUpcast, E: AbiUpcast, F: AbiUpcast, G: AbiUpcast, H: AbiUpcast, I: AbiUpcast, J: AbiUpcast> AbiUpcast for (A, B, C, D, E, F, G, H, I, J) {
    type Output = (A::Output, B::Output, C::Output, D::Output, E::Output, F::Output, G::Output, H::Output, I::Output, J::Output);
    #[inline]
    fn upcast(self) -> Self::Output { (self.0.upcast(), self.1.upcast(), self.2.upcast(), self.3.upcast(), self.4.upcast(), self.5.upcast(), self.6.upcast(), self.7.upcast(), self.8.upcast(), self.9.upcast()) }
}


pub trait Deployable {
    type Interface: InitInterface;

    /// Returns the contract's runtime bytecode
    fn __runtime() -> &'static [u8];

    /// Returns the contract's runtime bytecode
    fn bytecode() -> Bytes {
        Bytes::from(Self::__runtime())
    }

    // Creates a deployment builder that captures the constructor args
    fn deploy<Args>(args: Args) -> DeploymentBuilder<Self, Args>
    where
        Self: Sized,
        Args: AbiUpcast,
    {
        DeploymentBuilder {
            args,
            _phantom: PhantomData,
        }
    } 
}

pub struct DeploymentBuilder<D: Deployable + ?Sized, Args>
where
    Args: AbiUpcast,
{
    args: Args,
    _phantom: PhantomData<D>,
}

impl<D: Deployable, Args> DeploymentBuilder<D, Args>
where
    Args: AbiUpcast,
{

    /// Return the interface with the appropriate context.
    ///
    /// Constructor ABI policy:
    /// - Encodes constructor args using Solidity params-encoding (`abi.encode(a,b,...)`).
    /// - For single-arg constructors, pass a 1-tuple `(arg,)` so the encoder treats it
    ///   as a parameter list; this is critical for dynamic types (string/bytes) parity.
    pub fn with_ctx<M, T>(self, ctx: M) -> T
    where
        M: MethodCtx<Allowed = ReadWrite>, // Constrain to mutable contexts only
        D::Interface: InitInterface,
        T: FromBuilder<Context = M::Allowed>,
        D::Interface: crate::IntoInterface<T>,
        for<'a> << <Args as AbiUpcast>::Output as SolValue>::SolType as SolType>::Token<'a>: alloy_sol_types::abi::TokenSeq<'a>,
        <Args as AbiUpcast>::Output: SolValue,
    {
        let deploy_bin = D::__runtime();
        // Upcast nested u8 to U256 and preserve shapes (Vec/tuples/[u8;N]) before ABI params encoding
        let upcast_args = self.args.upcast();
        let encoded_args = upcast_args.abi_encode_params();

        // Craft R55 initcode expected by r55-evm:
        // [4-byte codesize][deploy_bin (starts with 0xFF)][constructor_args]
        let codesize = U32::from(deploy_bin.len());

        let mut init_code = Vec::new();
        init_code.extend_from_slice(&Bytes::from(codesize.to_be_bytes_vec()));
        init_code.extend_from_slice(deploy_bin);
        init_code.extend_from_slice(&encoded_args);

        let offset = init_code.as_ptr() as u64;
        let size = init_code.len() as u64;

        // TODO: think of an ergonomic API to handle deployments with values
        create(0, offset, size);

        // Get deployment address
        let mut ret_data = Vec::with_capacity(20);
        ret_data.resize(20 as usize, 0);
        return_create_address(ret_data.as_ptr() as u64);

        let address = Address::from_slice(&ret_data);
        
        // Create the interface builder
        let builder = D::Interface::new(address);
        
        // Convert to the actual interface with context
        builder.with_ctx(ctx)
    }
}

fn create(value: u64, data_offset: u64, data_size: u64) {
    unsafe {
        asm!(
            "ecall",
            in("a0") value, in("a1") data_offset, in("a2") data_size,
            in("t0") u8::from(Syscall::Create)
        );
    }
}

fn return_create_address(data_offset: u64) {
    unsafe {
        asm!(
            "ecall", in("a0") data_offset, in("t0") u8::from(Syscall::ReturnCreateAddress));
    }
}
