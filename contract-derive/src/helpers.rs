use alloy_core::primitives::keccak256;
use alloy_dyn_abi::DynSolType;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    FnArg, Ident, ImplItemMethod, LitStr, PathArguments, ReturnType, TraitItemMethod, Type,
};

// Unified method info from `ImplItemMethod` and `TraitItemMethod`
#[derive(Clone)]
pub struct MethodInfo<'a> {
    name: &'a Ident,
    args: Vec<syn::FnArg>,
    return_type: &'a ReturnType,
}

impl<'a> From<&'a ImplItemMethod> for MethodInfo<'a> {
    fn from(method: &'a ImplItemMethod) -> Self {
        Self {
            name: &method.sig.ident,
            args: method.sig.inputs.iter().cloned().collect(),
            return_type: &method.sig.output,
        }
    }
}

impl<'a> From<&'a TraitItemMethod> for MethodInfo<'a> {
    fn from(method: &'a TraitItemMethod) -> Self {
        Self {
            name: &method.sig.ident,
            args: method.sig.inputs.iter().cloned().collect(),
            return_type: &method.sig.output,
        }
    }
}

impl<'a> MethodInfo<'a> {
    pub fn is_mutable(&self) -> bool {
        match self.args.first() {
            Some(FnArg::Receiver(receiver)) => receiver.mutability.is_some(),
            Some(FnArg::Typed(_)) => panic!("First argument must be self"),
            None => panic!("Expected `self` as the first arg"),
        }
    }
}

// Helper function to get the parameter names + types of a method
fn get_arg_props<'a>(
    skip_first_arg: bool,
    method: &'a MethodInfo<'a>,
) -> (Vec<Ident>, Vec<&'a syn::Type>) {
    method
        .args
        .iter()
        .skip(if skip_first_arg { 1 } else { 0 })
        .enumerate()
        .map(|(i, arg)| {
            if let FnArg::Typed(pat_type) = arg {
                (format_ident!("arg{}", i), &*pat_type.ty)
            } else {
                panic!("Expected typed arguments");
            }
        })
        .unzip()
}

pub fn get_arg_props_skip_first<'a>(
    method: &'a MethodInfo<'a>,
) -> (Vec<Ident>, Vec<&'a syn::Type>) {
    get_arg_props(true, method)
}

pub fn get_arg_props_all<'a>(method: &'a MethodInfo<'a>) -> (Vec<Ident>, Vec<&'a syn::Type>) {
    get_arg_props(false, method)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterfaceNamingStyle {
    CamelCase,
}

pub struct InterfaceArgs {
    pub rename: Option<InterfaceNamingStyle>,
}

impl Parse for InterfaceArgs {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let rename_style = if !input.is_empty() {
            let value = if input.peek(LitStr) {
                input.parse::<LitStr>()?.value()
            } else {
                input.parse::<Ident>()?.to_string()
            };

            match value.as_str() {
                "camelCase" => Some(InterfaceNamingStyle::CamelCase),
                invalid => {
                    return Err(syn::Error::new(
                        input.span(),
                        format!(
                            "unsupported style: {}. Only 'camelCase' is supported",
                            invalid
                        ),
                    ))
                }
            }
        } else {
            None
        };

        Ok(InterfaceArgs {
            rename: rename_style,
        })
    }
}

// Helper function to generate interface impl from user-defined methods
pub fn generate_interface<T>(
    methods: &[&T],
    interface_name: &Ident,
    interface_style: Option<InterfaceNamingStyle>,
) -> quote::__private::TokenStream
where
    for<'a> MethodInfo<'a>: From<&'a T>,
{
    let methods: Vec<MethodInfo> = methods.iter().map(|&m| MethodInfo::from(m)).collect();
    let (mut_methods, immut_methods): (Vec<MethodInfo>, Vec<MethodInfo>) =
        methods.into_iter().partition(|m| m.is_mutable());

    // Generate implementations
    let mut_method_impls = mut_methods
        .iter()
        .map(|method| generate_method_impl(method, interface_style, true));
    let immut_method_impls = immut_methods
        .iter()
        .map(|method| generate_method_impl(method, interface_style, false));

    quote! {
        pub struct #interface_name<C: eth_riscv_runtime::CallCtx> {
            address: alloy_core::primitives::Address,
            _ctx: core::marker::PhantomData<C>
        }

        impl eth_riscv_runtime::InitInterface for #interface_name<eth_riscv_runtime::ReadOnly> {
            fn new(address: alloy_core::primitives::Address) -> eth_riscv_runtime::InterfaceBuilder<Self> {
                eth_riscv_runtime::InterfaceBuilder {
                    address,
                    _phantom: core::marker::PhantomData
                }
            }
        }

        // Implement conversion between interface types
        impl<C: eth_riscv_runtime::CallCtx> eth_riscv_runtime::IntoInterface<#interface_name<C>> for #interface_name<eth_riscv_runtime::ReadOnly> {
            fn into_interface(self) -> #interface_name<C> {
                #interface_name {
                    address: self.address,
                    _ctx: core::marker::PhantomData
                }
            }
        }

        impl<C: eth_riscv_runtime::CallCtx> eth_riscv_runtime::FromBuilder for #interface_name<C> {
            type Context = C;

            fn from_builder(builder: eth_riscv_runtime::InterfaceBuilder<Self>) -> Self {
                Self {
                    address: builder.address,
                    _ctx: core::marker::PhantomData
                }
            }
        }

        impl <C: eth_riscv_runtime::CallCtx> #interface_name<C> {
            pub fn address(&self) -> alloy_core::primitives::Address {
                self.address
            }
        }

        impl<C: eth_riscv_runtime::StaticCtx> #interface_name<C> {
            #(#immut_method_impls)*
        }

        impl<C: eth_riscv_runtime::MutableCtx> #interface_name<C> {
            #(#mut_method_impls)*
        }
    }
}

fn generate_method_impl(
    method: &MethodInfo,
    interface_style: Option<InterfaceNamingStyle>,
    is_mutable: bool,
) -> TokenStream {
    let name = method.name;
    let return_type = method.return_type;
    let method_selector = u32::from_be_bytes(
        generate_fn_selector(method, interface_style).expect("Unable to generate fn selector"),
    );

    let (arg_names, arg_types) = get_arg_props_skip_first(method);

    // ABI encoding policy for calldata construction:
    // - 0 args: write the 4-byte selector only.
    // - 1 arg: use `abi_encode()` which encodes a single value, matching Solidity `abi.encode(arg)`.
    // - >=2 args: use `abi_encode_params()` which encodes multiple top-level params, matching
    //   Solidity `abi.encode(a,b,...)`. This matters when any nested value is dynamic (e.g. string,
    //   bytes) because encoding a single tuple vs. multiple params differs in head/tail layout.
    //
    // Note: Whether a Rust type can be encoded is governed by `alloy_sol_types::SolValue` impls.
    // Selector mapping in `rust_type_to_sol_type` does not guarantee encoder support at call sites.
    let calldata = if arg_names.is_empty() {
        quote! {
            let mut complete_calldata = Vec::with_capacity(4);
            complete_calldata.extend_from_slice(&[
                #method_selector.to_be_bytes()[0],
                #method_selector.to_be_bytes()[1],
                #method_selector.to_be_bytes()[2],
                #method_selector.to_be_bytes()[3],
            ]);
        }
    } else if arg_names.len() == 1 {
        // Single-argument: use abi_encode() with nested u8→U256 upcasting
        {
            let name0 = &arg_names[0];
            let ty0 = arg_types[0];
            let enc_expr = gen_upcast_expr(quote! { #name0 }, ty0);
            quote! {
                let mut args_calldata = (#enc_expr,).abi_encode();
                let mut complete_calldata = Vec::with_capacity(4 + args_calldata.len());
                complete_calldata.extend_from_slice(&[
                    #method_selector.to_be_bytes()[0],
                    #method_selector.to_be_bytes()[1],
                    #method_selector.to_be_bytes()[2],
                    #method_selector.to_be_bytes()[3],
                ]);
                complete_calldata.append(&mut args_calldata);
            }
        }
    } else {
        // Multi-argument: encode as standard Solidity params (abi.encode(a,b,...)) with nested u8→U256 upcasting
        {
            let enc_exprs: Vec<_> = arg_names.iter().zip(arg_types.iter())
                .map(|(name, ty)| gen_upcast_expr(quote! { #name }, ty))
                .collect();
            quote! {
                let mut args_calldata = (#( #enc_exprs ),*).abi_encode_params();
                let mut complete_calldata = Vec::with_capacity(4 + args_calldata.len());
                complete_calldata.extend_from_slice(&[
                    #method_selector.to_be_bytes()[0],
                    #method_selector.to_be_bytes()[1],
                    #method_selector.to_be_bytes()[2],
                    #method_selector.to_be_bytes()[3],
                ]);
                complete_calldata.append(&mut args_calldata);
            }
        }
    };

    let (call_fn, self_param) = if is_mutable {
        (
            quote! { eth_riscv_runtime::call_contract },
            quote! { &mut self },
        )
    } else {
        (
            quote! { eth_riscv_runtime::staticcall_contract },
            quote! { &self},
        )
    };

    // Generate implementations handling Result<Bytes, Bytes> from call_contract.
    // The `result` variable is the direct output of `call_contract`, which returns `Ok` or `Err`
    // based on the success flag read from the a0 register.
    //
    // The generated error handling depends on the function's return type. This provides a
    // choice between manual error handling and automatic revert propagation (like Solidity).
    // - `Result<T, E>`: Decodes and returns the `Err`, letting the developer handle the failure.
    // - `Option<T>` or default: Automatically reverts the transaction on failure.
    // Reference: https://github.com/sam-iamm/r55/pull/9
    match extract_wrapper_types(&method.return_type) {
        // If `Result<T, E>` handle each individual type
        WrapperType::Result(ok_type, err_type) => {
            let ok_decode_block = {
                let ok_syn = extract_result_ok_type_syn(&method.return_type).expect("ok type");
                if type_needs_u8_upcast(ok_syn) {
                    let up_ty = upcast_type_tokens(ok_syn);
                    let down_expr = gen_downcast_expr(quote! { __dec_ok }, ok_syn);
                    quote! {
                        match <#up_ty>::abi_decode(&bytes) {
                            Ok(__dec_ok) => Ok(#down_expr),
                            Err(_) => Err(<#err_type>::abi_decode(&bytes, true))
                        }
                    }
                } else {
                    quote! {
                        match <#ok_type>::abi_decode(&bytes) {
                            Ok(decoded) => Ok(decoded),
                            Err(_) => Err(<#err_type>::abi_decode(&bytes, true))
                        }
                    }
                }
            };
            quote! {
            pub fn #name(#self_param, #(#arg_names: #arg_types),*) -> Result<#ok_type, #err_type>  {
                use alloy_sol_types::SolValue;
                use alloc::vec::Vec;

                #calldata

                let result = #call_fn(
                    self.address,
                    0_u64,
                    &complete_calldata,
                    None
                );

                match result {
                    // Call succeeded - decode return data
                    Ok(bytes) => { #ok_decode_block },
                    // Call reverted - return error (no auto-revert, user handles Result)
                    Err(revert_data) => Err(<#err_type>::abi_decode(&revert_data, true))
                }
            }
        }
        },
        // If `Option<T>` unwrap the type to decode, and wrap it back
        WrapperType::Option(return_ty) => {
            let opt_decode_block = {
                let inner_syn = extract_option_inner_type_syn(&method.return_type).expect("inner type");
                if type_needs_u8_upcast(inner_syn) {
                    let up_ty = upcast_type_tokens(inner_syn);
                    let down_expr = gen_downcast_expr(quote! { __dec_ok }, inner_syn);
                    quote! {
                        match <#up_ty>::abi_decode(&bytes) {
                            Ok(__dec_ok) => Some(#down_expr),
                            Err(_) => None
                        }
                    }
                } else {
                    quote! {
                        match <#return_ty>::abi_decode(&bytes) {
                            Ok(decoded) => Some(decoded),
                            Err(_) => None
                        }
                    }
                }
            };
            quote! {
                pub fn #name(#self_param, #(#arg_names: #arg_types),*) -> Option<#return_ty> {
                    use alloy_sol_types::SolValue;
                    use alloc::vec::Vec;

                    #calldata

                    let result = #call_fn(
                        self.address,
                        0_u64,
                        &complete_calldata,
                        None
                    );

                    match result {
                        // Call succeeded - decode and return
                        Ok(bytes) => { #opt_decode_block },
                        // Call reverted - auto-propagate (if A→B reverts, A reverts)
                        Err(revert_data) => {
                            eth_riscv_runtime::revert_with_error(&revert_data);
                        }
                    }
                }
            }
        }
        // Otherwise, simply decode the value + wrap it in an `Option` to force error-handling
        WrapperType::None => {
            let return_ty = match return_type {
                ReturnType::Default => quote! { () },
                ReturnType::Type(_, ty) => quote! { #ty },
            };
            let none_decode_block = {
                let inner_syn_opt: Option<&Type> = match &method.return_type {
                    ReturnType::Type(_, ty) => Some(ty.as_ref()),
                    ReturnType::Default => None,
                };
                if let Some(inner_syn) = inner_syn_opt {
                    if type_needs_u8_upcast(inner_syn) {
                        let up_ty = upcast_type_tokens(inner_syn);
                        let down_expr = gen_downcast_expr(quote! { __dec_ok }, inner_syn);
                        quote! {
                            match <#up_ty>::abi_decode(&bytes) {
                                Ok(__dec_ok) => Some(#down_expr),
                                Err(_) => None
                            }
                        }
                    } else {
                        quote! {
                            match <#return_ty>::abi_decode(&bytes) {
                                Ok(decoded) => Some(decoded),
                                Err(_) => None
                            }
                        }
                    }
                } else {
                    quote! { Some(()) }
                }
            };
            quote! {
                pub fn #name(#self_param, #(#arg_names: #arg_types),*) -> Option<#return_ty> {
                    use alloy_sol_types::SolValue;
                    use alloc::vec::Vec;

                    #calldata

                    let result = #call_fn(
                        self.address,
                        0_u64,
                        &complete_calldata,
                        None
                    );

                    match result {
                        // Call succeeded - decode and return
                        Ok(bytes) => { #none_decode_block },
                        // Call reverted - auto-propagate
                        Err(revert_data) => {
                            eth_riscv_runtime::revert_with_error(&revert_data);
                        }
                    }
                }
            }
        }
    }
}

pub enum WrapperType {
    Result(TokenStream, TokenStream),
    Option(TokenStream),
    None,
}

// Helper function to extract Result or Option types if present
pub fn extract_wrapper_types(return_type: &ReturnType) -> WrapperType {
    let type_path = match return_type {
        ReturnType::Default => return WrapperType::None,
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Path(type_path) => type_path,
            _ => return WrapperType::None,
        },
    };

    let last_segment = match type_path.path.segments.last() {
        Some(segment) => segment,
        None => return WrapperType::None,
    };

    match last_segment.ident.to_string().as_str() {
        "Result" => {
            let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
                return WrapperType::None;
            };

            let type_args: Vec<_> = args.args.iter().collect();
            if type_args.len() != 2 {
                return WrapperType::None;
            }

            // Convert the generic arguments to TokenStreams directly
            let ok_type = match &type_args[0] {
                syn::GenericArgument::Type(t) => quote!(#t),
                _ => return WrapperType::None,
            };

            let err_type = match &type_args[1] {
                syn::GenericArgument::Type(t) => quote!(#t),
                _ => return WrapperType::None,
            };

            WrapperType::Result(ok_type, err_type)
        }
        "Option" => {
            let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
                return WrapperType::None;
            };

            let type_args: Vec<_> = args.args.iter().collect();
            if type_args.len() != 1 {
                return WrapperType::None;
            }

            // Convert the generic argument to TokenStream
            let inner_type = match &type_args[0] {
                syn::GenericArgument::Type(t) => quote!(#t),
                _ => return WrapperType::None,
            };

            WrapperType::Option(inner_type)
        }
        _ => WrapperType::None,
    }
}

// Extract syn type of Result<Ok, Err>'s Ok type
pub fn extract_result_ok_type_syn(return_type: &ReturnType) -> Option<&Type> {
    match return_type {
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Path(type_path) => type_path.path.segments.last().and_then(|seg| {
                if seg.ident == "Result" {
                    if let PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(ok_ty)) = args.args.first() {
                            return Some(ok_ty);
                        }
                    }
                }
                None
            }),
            _ => None,
        },
        _ => None,
    }
}

// Extract syn type of Option<T>'s T
pub fn extract_option_inner_type_syn(return_type: &ReturnType) -> Option<&Type> {
    match return_type {
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Path(type_path) => type_path.path.segments.last().and_then(|seg| {
                if seg.ident == "Option" {
                    if let PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                            return Some(inner_ty);
                        }
                    }
                }
                None
            }),
            _ => None,
        },
        _ => None,
    }
}

// Recursively determine if a type contains any u8 that needs upcasting
pub fn type_needs_u8_upcast(ty: &Type) -> bool {
    match ty {
        Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|s| {
                if s.ident == "u8" {
                    true
                } else if s.ident == "Vec" {
                    if let PathArguments::AngleBracketed(args) = &s.arguments {
                        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                            type_needs_u8_upcast(inner)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .unwrap_or(false),
        Type::Tuple(t) => t.elems.iter().any(type_needs_u8_upcast),
        Type::Array(a) => type_needs_u8_upcast(&a.elem),
        _ => false,
    }
}

// Build a type TokenStream where all u8 occurrences are replaced by U256 recursively
pub fn upcast_type_tokens(ty: &Type) -> TokenStream {
    match ty {
        Type::Path(tp) => {
            let seg = tp.path.segments.last().unwrap();
            if seg.ident == "u8" {
                quote! { alloy_core::primitives::U256 }
            } else if seg.ident == "Vec" {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        let inner_up = upcast_type_tokens(inner);
                        quote! { alloc::vec::Vec<#inner_up> }
                    } else {
                        quote! { #ty }
                    }
                } else {
                    quote! { #ty }
                }
            } else {
                quote! { #ty }
            }
        }
        Type::Tuple(t) => {
            let elems = t.elems.iter().map(upcast_type_tokens);
            quote! { ( #( #elems ),* ) }
        }
        Type::Array(a) => {
            let inner = upcast_type_tokens(&a.elem);
            let len = &a.len;
            quote! { [#inner; #len] }
        }
        _ => quote! { #ty },
    }
}

// Generate an expression converting a value to its upcasted form recursively
pub fn gen_upcast_expr(var: TokenStream, ty: &Type) -> TokenStream {
    match ty {
        Type::Path(tp) => {
            let seg = tp.path.segments.last().unwrap();
            if seg.ident == "u8" {
                quote! { alloy_core::primitives::U256::from(#var as u64) }
            } else if seg.ident == "Vec" {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        let inner_conv = gen_upcast_expr(quote! { __el }, inner);
                        quote! { #var.into_iter().map(|__el| { #inner_conv }).collect::<alloc::vec::Vec<_>>() }
                    } else {
                        quote! { #var }
                    }
                } else {
                    quote! { #var }
                }
            } else {
                quote! { #var }
            }
        }
        Type::Tuple(t) => {
            let elems = t
                .elems
                .iter()
                .enumerate()
                .map(|(i, ty_i)| gen_upcast_expr(quote! { #var.#i }, ty_i));
            quote! { ( #( #elems ),* ) }
        }
        Type::Array(a) => {
            let inner_conv = gen_upcast_expr(quote! { __el }, &a.elem);
            quote! { core::array::from_fn(|__i| { let __el = #var[__i].clone(); #inner_conv }) }
        }
        _ => quote! { #var },
    }
}

// Generate an expression converting an upcasted value back to its original type recursively
pub fn gen_downcast_expr(var: TokenStream, ty: &Type) -> TokenStream {
    match ty {
        Type::Path(tp) => {
            let seg = tp.path.segments.last().unwrap();
            if seg.ident == "u8" {
                quote! { (#var.as_limbs()[0] as u8) }
            } else if seg.ident == "Vec" {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        let inner_conv = gen_downcast_expr(quote! { __el }, inner);
                        quote! { #var.into_iter().map(|__el| { #inner_conv }).collect::<alloc::vec::Vec<_>>() }
                    } else {
                        quote! { #var }
                    }
                } else {
                    quote! { #var }
                }
            } else {
                quote! { #var }
            }
        }
        Type::Tuple(t) => {
            let elems = t
                .elems
                .iter()
                .enumerate()
                .map(|(i, ty_i)| gen_downcast_expr(quote! { #var.#i }, ty_i));
            quote! { ( #( #elems ),* ) }
        }
        Type::Array(a) => {
            let inner_conv = gen_downcast_expr(quote! { __el }, &a.elem);
            quote! { core::array::from_fn(|__i| { let __el = #var[__i].clone(); #inner_conv }) }
        }
        _ => quote! { #var },
    }
}

// Helper function to generate fn selector
pub fn generate_fn_selector(
    method: &MethodInfo,
    style: Option<InterfaceNamingStyle>,
) -> Option<[u8; 4]> {
    // Normalize Rust ident by stripping a trailing "__overload<digits>" overload suffix, if present.
    let mut base = method.name.to_string();
    if let Some(idx) = base.rfind("__overload") {
        let suffix = &base[(idx + "__overload".len())..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            base.truncate(idx);
        }
    }
    let name = match style {
        None => base,
        Some(style) => match style {
            InterfaceNamingStyle::CamelCase => to_camel_case(base),
        },
    };

    let (_, arg_types) = get_arg_props_skip_first(method);
    let args = arg_types
        .iter()
        .map(|ty| rust_type_to_sol_type(ty))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let args_str = args
        .iter()
        .map(|ty| ty.sol_type_name().into_owned())
        .collect::<Vec<_>>()
        .join(",");

    let selector = format!("{}({})", name, args_str);
    let selector_bytes = keccak256(selector.as_bytes())[..4].try_into().ok()?;
    Some(selector_bytes)
}

// Helper function to convert rust types to their solidity equivalent
// NOTE: This mapping is used for selector generation only (string signatures -> keccak4).
// Encoder/decoder support is governed by `alloy_sol_types::SolValue` impls at call sites; adding
// a type here does not imply that `abi_encode`/`abi_encode_params` is available for that Rust type.
// TODO: make sure that the impl is robust, so far only tested with "simple types"
pub fn rust_type_to_sol_type(ty: &Type) -> Result<DynSolType, &'static str> {
    match ty {
        Type::Path(type_path) => {
            let path = &type_path.path;
            let segment = path.segments.last().ok_or("Empty type path")?;
            let ident = &segment.ident;
            let type_name = ident.to_string();

            match type_name.as_str() {
                // Fixed-size types
                "Address" => Ok(DynSolType::Address),
                "Function" => Ok(DynSolType::Function),
                "bool" | "Bool" => Ok(DynSolType::Bool),
                "String" | "str" => Ok(DynSolType::String),
                "Bytes" => Ok(DynSolType::Bytes),
                // Primitive unsigned integers
                "u8" => Ok(DynSolType::Uint(8)),
                "u16" => Ok(DynSolType::Uint(16)),
                "u32" => Ok(DynSolType::Uint(32)),
                "u64" => Ok(DynSolType::Uint(64)),
                "u128" => Ok(DynSolType::Uint(128)),
                // Primitive signed integers
                "i8" => Ok(DynSolType::Int(8)),
                "i16" => Ok(DynSolType::Int(16)),
                "i32" => Ok(DynSolType::Int(32)),
                "i64" => Ok(DynSolType::Int(64)),
                "i128" => Ok(DynSolType::Int(128)),
                // Fixed-size bytes
                b if b.starts_with('B') => {
                    let size: usize = b
                        .trim_start_matches('B')
                        .parse()
                        .map_err(|_| "Invalid fixed bytes size")?;
                    if size > 0 && size <= 32 {
                        Ok(DynSolType::FixedBytes(size))
                    } else {
                        Err("Invalid fixed bytes size (between 1-32)")
                    }
                }
                // Fixed-size unsigned integers
                u if u.starts_with('U') => {
                    let size: usize = u
                        .trim_start_matches('U')
                        .parse()
                        .map_err(|_| "Invalid uint size")?;
                    if size > 0 && size <= 256 && size % 8 == 0 {
                        Ok(DynSolType::Uint(size))
                    } else {
                        Err("Invalid uint size (multiple of 8 + leq 256)")
                    }
                }
                // Fixed-size signed integers
                i if i.starts_with('I') => {
                    let size: usize = i
                        .trim_start_matches('I')
                        .parse()
                        .map_err(|_| "Invalid int size")?;
                    if size > 0 && size <= 256 && size % 8 == 0 {
                        Ok(DynSolType::Int(size))
                    } else {
                        Err("Invalid int size (must be multiple of 8, max 256)")
                    }
                }
                // Handle vecs
                _ => {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        match type_name.as_str() {
                            "Vec" => {
                                let inner = args.args.first().ok_or("Empty Vec type argument")?;
                                if let syn::GenericArgument::Type(inner_ty) = inner {
                                    let inner_sol_type = rust_type_to_sol_type(inner_ty)?;
                                    Ok(DynSolType::Array(Box::new(inner_sol_type)))
                                } else {
                                    Err("Invalid Vec type argument")
                                }
                            }
                            _ => Err("Unsupported generic type"),
                        }
                    } else {
                        Err("Unsupported type")
                    }
                }
            }
        }
        Type::Array(array) => {
            let inner_sol_type = rust_type_to_sol_type(&array.elem)?;
            if let syn::Expr::Lit(lit) = &array.len {
                if let syn::Lit::Int(size) = &lit.lit {
                    let size: usize = size
                        .base10_digits()
                        .parse()
                        .map_err(|_| "Invalid array size")?;
                    Ok(DynSolType::FixedArray(Box::new(inner_sol_type), size))
                } else {
                    Err("Invalid array size literal")
                }
            } else {
                Err("Invalid array size expression")
            }
        }
        Type::Tuple(tuple) => {
            let inner_types = tuple
                .elems
                .iter()
                .map(rust_type_to_sol_type)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DynSolType::Tuple(inner_types))
        }
        _ => Err("Unsupported type"),
    }
}

fn to_camel_case(s: String) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;

    // Iterate through characters, skipping non-alphabetic separators
    for (i, c) in s.chars().enumerate() {
        if c.is_alphanumeric() {
            if i == 0 {
                result.push(c.to_ascii_lowercase());
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        } else {
            // Set flag to capitalize next char  with non-alphanumeric ones
            capitalize_next = true;
        }
    }

    result
}

// Helper function to generate the deployment code
//
// ABI decoding policy for constructor args (Solidity parity):
// - 0 args: call `new()`
// - 1 arg: decode single value via `<T>::abi_decode(msg_data)`
// - >=2 args: decode parameter list via
//   `<(T1,...,Tn)>::abi_decode_params_validate(msg_data)`
//
// Notes:
// - Encode path (runtime `with_ctx`) uses params-encoding for constructors
//   (`abi.encode(a,b,...)`).
// - Single-arg constructors: params-encoding a 1‑tuple is equivalent to
//   decoding a single value; pass `(arg,)` at encode time to preserve parity
//   with dynamic types (string/bytes).
pub fn generate_deployment_code(
    struct_name: &Ident,
    constructor: Option<&ImplItemMethod>,
) -> quote::__private::TokenStream {
    // Decode constructor args + trigger constructor logic
    let constructor_code = match constructor {
        Some(method) => {
            let method_info = MethodInfo::from(method);
            let (arg_names, arg_types) = get_arg_props_all(&method_info);

            let decode_and_init = match arg_types.len() {
                0 => {
                    quote! {
                        #struct_name::new();
                    }
                }
                1 => {
                    let ty0 = arg_types[0];
                    let name0 = &arg_names[0];
                    let needs = type_needs_u8_upcast(ty0);
                    let up_ty = upcast_type_tokens(ty0);
                    let down_expr = gen_downcast_expr(quote! { __dec0 }, ty0);
                    if needs {
                        quote! {
                            // Get encoded constructor args
                            let calldata = eth_riscv_runtime::msg_data();
                            let __dec0 = <#up_ty>::abi_decode(&calldata)
                                .expect("Failed to decode constructor args");
                            let #name0 = #down_expr;
                            #struct_name::new(#name0);
                        }
                    } else {
                        quote! {
                            // Get encoded constructor args
                            let calldata = eth_riscv_runtime::msg_data();
                            let #name0 = <#ty0>::abi_decode(&calldata)
                                .expect("Failed to decode constructor args");
                            #struct_name::new(#name0);
                        }
                    }
                }
                _ => {
                    // Build decode as tuple of possibly-upcast types
                    let dec_types: Vec<proc_macro2::TokenStream> = arg_types.iter()
                        .map(|ty| upcast_type_tokens(ty))
                        .collect();
                    let dec_names: Vec<proc_macro2::Ident> = (0..arg_names.len()).map(|i| format_ident!("__ctor_arg{}_dec", i)).collect();
                    let cast_binds: Vec<proc_macro2::TokenStream> = arg_names.iter().zip(dec_names.iter()).zip(arg_types.iter())
                        .map(|((name, dec), ty)| {
                            let down_expr = gen_downcast_expr(quote! { #dec }, ty);
                            quote! { let #name = #down_expr; }
                        }).collect();

                    quote! {
                        // Get encoded constructor args
                        let calldata = eth_riscv_runtime::msg_data();
                        let (#( #dec_names ),*) = <(#( #dec_types ),*)>::abi_decode_params_validate(&calldata)
                            .expect("Failed to decode constructor args");
                        #(#cast_binds)*
                        #struct_name::new(#(#arg_names),*);
                    }
                }
            };

            quote! {
                impl #struct_name { #method }
                #decode_and_init
            }
        }
        None => quote! {
            #struct_name::default();
        },
    };

    quote! {
        use alloc::vec::Vec;
        use alloy_core::primitives::U32;

        #[no_mangle]
        pub extern "C" fn main() -> ! {
            #constructor_code

            // Return runtime code
            let runtime: &[u8] = include_bytes!("../target/riscv64imac-unknown-none-elf/release/runtime");
            let mut prepended_runtime = Vec::with_capacity(1 + runtime.len());
            prepended_runtime.push(0xff);
            prepended_runtime.extend_from_slice(runtime);

            let prepended_runtime_slice: &[u8] = &prepended_runtime;
            let result_ptr = prepended_runtime_slice.as_ptr() as u64;
            let result_len = prepended_runtime_slice.len() as u64;
            eth_riscv_runtime::return_riscv(result_ptr, result_len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    struct MockMethod {
        method: ImplItemMethod,
    }

    impl MockMethod {
        fn new(name: &str, args: Vec<&str>) -> Self {
            let name_ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            let args_tokens = if args.is_empty() {
                quote!()
            } else {
                let args = args.iter().map(|arg| {
                    let parts: Vec<&str> = arg.split(": ").collect();
                    let arg_name = syn::Ident::new(parts[0], proc_macro2::Span::call_site());
                    let type_str = parts[1];
                    let type_tokens: proc_macro2::TokenStream = type_str.parse().unwrap();
                    quote!(#arg_name: #type_tokens)
                });
                quote!(, #(#args),*)
            };

            let method: ImplItemMethod = parse_quote! {
                fn #name_ident(&self #args_tokens) {}
            };
            Self { method }
        }

        fn info(&self) -> MethodInfo {
            MethodInfo::from(self)
        }
    }

    impl<'a> From<&'a MockMethod> for MethodInfo<'a> {
        fn from(test_method: &'a MockMethod) -> Self {
            MethodInfo::from(&test_method.method)
        }
    }

    pub fn get_selector_from_sig(sig: &str) -> [u8; 4] {
        keccak256(sig.as_bytes())[0..4]
            .try_into()
            .expect("Selector should have exactly 4 bytes")
    }

    #[test]
    fn test_rust_to_sol_basic_types() {
        let test_cases = vec![
            (parse_quote!(Address), DynSolType::Address),
            (parse_quote!(Function), DynSolType::Function),
            (parse_quote!(bool), DynSolType::Bool),
            (parse_quote!(Bool), DynSolType::Bool),
            (parse_quote!(String), DynSolType::String),
            (parse_quote!(str), DynSolType::String),
            (parse_quote!(Bytes), DynSolType::Bytes),
        ];

        for (rust_type, expected_sol_type) in test_cases {
            assert_eq!(
                rust_type_to_sol_type(&rust_type).unwrap(),
                expected_sol_type
            );
        }
    }

    #[test]
    fn test_rust_to_sol_fixed_bytes() {
        let test_cases = vec![
            (parse_quote!(B1), DynSolType::FixedBytes(1)),
            (parse_quote!(B16), DynSolType::FixedBytes(16)),
            (parse_quote!(B32), DynSolType::FixedBytes(32)),
        ];

        for (rust_type, expected_sol_type) in test_cases {
            assert_eq!(
                rust_type_to_sol_type(&rust_type).unwrap(),
                expected_sol_type
            );
        }

        // Invalid cases
        assert!(rust_type_to_sol_type(&parse_quote!(B0)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(B33)).is_err());
    }

    #[test]
    fn test_rust_to_sol_lower_primitives() {
        assert_eq!(rust_type_to_sol_type(&parse_quote!(u8)).unwrap(), DynSolType::Uint(8));
        assert_eq!(rust_type_to_sol_type(&parse_quote!(u16)).unwrap(), DynSolType::Uint(16));
        assert_eq!(rust_type_to_sol_type(&parse_quote!(u32)).unwrap(), DynSolType::Uint(32));
        assert_eq!(rust_type_to_sol_type(&parse_quote!(u64)).unwrap(), DynSolType::Uint(64));
        assert_eq!(rust_type_to_sol_type(&parse_quote!(u128)).unwrap(), DynSolType::Uint(128));
        assert_eq!(rust_type_to_sol_type(&parse_quote!(i8)).unwrap(), DynSolType::Int(8));
        assert_eq!(rust_type_to_sol_type(&parse_quote!(i16)).unwrap(), DynSolType::Int(16));
        assert_eq!(rust_type_to_sol_type(&parse_quote!(i32)).unwrap(), DynSolType::Int(32));
        assert_eq!(rust_type_to_sol_type(&parse_quote!(i64)).unwrap(), DynSolType::Int(64));
        assert_eq!(rust_type_to_sol_type(&parse_quote!(i128)).unwrap(), DynSolType::Int(128));
    }

    #[test]
    fn test_rust_to_sol_integers() {
        let test_cases = vec![
            (parse_quote!(U8), DynSolType::Uint(8)),
            (parse_quote!(U256), DynSolType::Uint(256)),
            (parse_quote!(I8), DynSolType::Int(8)),
            (parse_quote!(I256), DynSolType::Int(256)),
        ];

        for (rust_type, expected_sol_type) in test_cases {
            assert_eq!(
                rust_type_to_sol_type(&rust_type).unwrap(),
                expected_sol_type
            );
        }

        // Invalid cases
        assert!(rust_type_to_sol_type(&parse_quote!(U0)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(U257)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(U7)).is_err()); // Not multiple of 8
        assert!(rust_type_to_sol_type(&parse_quote!(I0)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(I257)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(I7)).is_err()); // Not multiple of 8
    }

    #[test]
    fn test_rust_to_sol_arrays() {
        // Dynamic arrays (Vec)
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!(Vec<U256>)).unwrap(),
            DynSolType::Array(Box::new(DynSolType::Uint(256)))
        );

        assert_eq!(
            rust_type_to_sol_type(&parse_quote!(Vec<Bool>)).unwrap(),
            DynSolType::Array(Box::new(DynSolType::Bool))
        );

        // Fixed-size arrays
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!([U256; 5])).unwrap(),
            DynSolType::FixedArray(Box::new(DynSolType::Uint(256)), 5)
        );

        assert_eq!(
            rust_type_to_sol_type(&parse_quote!([Bool; 3])).unwrap(),
            DynSolType::FixedArray(Box::new(DynSolType::Bool), 3)
        );
    }

    #[test]
    fn test_rust_to_sol_tuples() {
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!((U256, Bool))).unwrap(),
            DynSolType::Tuple(vec![DynSolType::Uint(256), DynSolType::Bool])
        );

        assert_eq!(
            rust_type_to_sol_type(&parse_quote!((Address, B32, I128))).unwrap(),
            DynSolType::Tuple(vec![
                DynSolType::Address,
                DynSolType::FixedBytes(32),
                DynSolType::Int(128)
            ])
        );
    }

    #[test]
    fn test_rust_to_sol_nested_types() {
        // Nested Vec
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!(Vec<Vec<U256>>)).unwrap(),
            DynSolType::Array(Box::new(DynSolType::Array(Box::new(DynSolType::Uint(256)))))
        );

        // Nested fixed array
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!([[U256; 2]; 3])).unwrap(),
            DynSolType::FixedArray(
                Box::new(DynSolType::FixedArray(Box::new(DynSolType::Uint(256)), 2)),
                3
            )
        );

        // Nested tuple
        assert_eq!(
            rust_type_to_sol_type(&parse_quote!((U256, (Bool, Address)))).unwrap(),
            DynSolType::Tuple(vec![
                DynSolType::Uint(256),
                DynSolType::Tuple(vec![DynSolType::Bool, DynSolType::Address])
            ])
        );
    }

    #[test]
    fn test_rust_to_sol_invalid_types() {
        // Invalid type names
        assert!(rust_type_to_sol_type(&parse_quote!(InvalidType)).is_err());

        // Invalid generic types
        assert!(rust_type_to_sol_type(&parse_quote!(Option<U256>)).is_err());
        assert!(rust_type_to_sol_type(&parse_quote!(Result<U256>)).is_err());
    }

    #[test]
    fn test_fn_selector() {
        // No arguments
        let method = MockMethod::new("balance", vec![]);
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("balance()"),
        );

        // Single argument
        let method = MockMethod::new("transfer", vec!["to: Address"]);
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("transfer(address)"),
        );

        // Multiple arguments
        let method = MockMethod::new(
            "transfer_from",
            vec!["from: Address", "to: Address", "amount: U256"],
        );
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("transfer_from(address,address,uint256)")
        );

        // Dynamic arrays
        let method = MockMethod::new("batch_transfer", vec!["recipients: Vec<Address>"]);
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("batch_transfer(address[])")
        );

        // Tuples
        let method = MockMethod::new(
            "complex_transfer",
            vec!["data: (Address, U256)", "check: (Vec<Address>, Vec<Bool>)"],
        );
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("complex_transfer((address,uint256),(address[],bool[]))")
        );

        // Fixed arrays
        let method = MockMethod::new("multi_transfer", vec!["amounts: [U256; 3]"]);
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("multi_transfer(uint256[3])")
        );
    }

    #[test]
    fn test_fn_selector_rename_camel_case() {
        let method = MockMethod::new("get_balance", vec![]);
        assert_eq!(
            generate_fn_selector(&method.info(), Some(InterfaceNamingStyle::CamelCase)).unwrap(),
            get_selector_from_sig("getBalance()")
        );

        let method = MockMethod::new("transfer_from_account", vec!["to: Address"]);
        assert_eq!(
            generate_fn_selector(&method.info(), Some(InterfaceNamingStyle::CamelCase)).unwrap(),
            get_selector_from_sig("transferFromAccount(address)")
        );
    }

    #[test]
    fn test_fn_selector_erc20() {
        let cases = vec![
            ("totalSupply", vec![], "totalSupply()"),
            ("balanceOf", vec!["account: Address"], "balanceOf(address)"),
            (
                "transfer",
                vec!["recipient: Address", "amount: U256"],
                "transfer(address,uint256)",
            ),
            (
                "allowance",
                vec!["owner: Address", "spender: Address"],
                "allowance(address,address)",
            ),
            (
                "approve",
                vec!["spender: Address", "amount: U256"],
                "approve(address,uint256)",
            ),
            (
                "transferFrom",
                vec!["sender: Address", "recipient: Address", "amount: U256"],
                "transferFrom(address,address,uint256)",
            ),
        ];

        for (name, args, signature) in cases {
            let method = MockMethod::new(name, args);
            assert_eq!(
                generate_fn_selector(&method.info(), None).unwrap(),
                get_selector_from_sig(signature),
                "Selector mismatch for {}",
                signature
            );
        }
    }

    #[test]
    fn test_fn_selector_erc721() {
        let cases = vec![
            (
                "safeTransferFrom",
                vec![
                    "from: Address",
                    "to: Address",
                    "tokenId: U256",
                    "data: Bytes",
                ],
                "safeTransferFrom(address,address,uint256,bytes)",
            ),
            ("name", vec![], "name()"),
            ("symbol", vec![], "symbol()"),
            ("tokenURI", vec!["tokenId: U256"], "tokenURI(uint256)"),
            (
                "approve",
                vec!["to: Address", "tokenId: U256"],
                "approve(address,uint256)",
            ),
            (
                "setApprovalForAll",
                vec!["operator: Address", "approved: bool"],
                "setApprovalForAll(address,bool)",
            ),
        ];

        for (name, args, signature) in cases {
            let method = MockMethod::new(name, args);
            assert_eq!(
                generate_fn_selector(&method.info(), None).unwrap(),
                get_selector_from_sig(signature),
                "Selector mismatch for {}",
                signature
            );
        }
    }

    #[test]
    fn test_fn_selector_overload_suffix_stripped() {
        // When the Rust ident carries an overload suffix, it should be stripped for selector naming
        let method = MockMethod::new(
            "safeTransferFrom__overload1",
            vec!["from: Address", "to: Address", "tokenId: U256"],
        );
        assert_eq!(
            generate_fn_selector(&method.info(), None).unwrap(),
            get_selector_from_sig("safeTransferFrom(address,address,uint256)")
        );
    }

    #[test]
    fn test_constructor_params_decode_roundtrip_dynamic() {
        use alloy_sol_types::SolValue;
        use std::string::String;

        let name = String::from("Test Token");
        let symbol = String::from("TEST");
        let decimals: u16 = 18;

        // Encode as Solidity-style params (not single tuple)
        let encoded = <(String, String, u16)>::abi_encode_params(
            &(name.clone(), symbol.clone(), decimals),
        );

        // Decode using params-validate
        let (d_name, d_symbol, d_decimals) = <(String, String, u16)>::abi_decode_params_validate(&encoded)
            .expect("decode failed");

        assert_eq!(d_name, name);
        assert_eq!(d_symbol, symbol);
        assert_eq!(d_decimals, decimals);
    }

    #[test]
    fn test_constructor_single_arg_params_roundtrip_dynamic() {
        use alloy_sol_types::SolValue;
        use std::string::String;

        let name = String::from("Only Name");

        // Encode as Solidity-style params for a single argument by passing a 1-tuple
        let encoded = <(String,)>::abi_encode_params(&(name.clone(),));

        // Decode as a single value
        let decoded = <String>::abi_decode(&encoded).expect("decode failed");
        assert_eq!(decoded, name);
    }
}
