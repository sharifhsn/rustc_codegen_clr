//! Proc macros that put a small, explicit native ABI behind safe Rust functions.
//!
//! `#[native_export]` keeps a safe Rust implementation and emits a private-by-convention C ABI
//! shim. `native_import!` emits the matching raw import and a safe Rust wrapper. The supported
//! contract is deliberately narrow: primitive scalar arguments, `&str`, `&[T]`, `&mut [T]`, and
//! `Result<primitive | String | Vec<T> | (), i32>`. Generated glue copies owned results before
//! freeing them in the native library. Unsupported ownership or layout is a compile-time error
//! rather than a guessed ABI.

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    Error, FnArg, ItemFn, LitStr, Pat, PatIdent, Result, ReturnType, Signature, Token, Type,
    TypePath, Visibility, parse::Parse, parse::ParseStream, parse_macro_input,
    punctuated::Punctuated,
};

const INVALID_ARGUMENT_STATUS: i32 = -2_147_483_647;
const PANIC_STATUS: i32 = -2_147_483_646;
const ZERO_ERROR_STATUS: i32 = -2_147_483_645;
const INVALID_UTF8_STATUS: i32 = -2_147_483_644;
const INVALID_OWNED_RESULT_STATUS: i32 = -2_147_483_643;

/// Emit a C ABI export for a safe Rust function.
///
/// The function may use scalar values, `&str`, and scalar slices as inputs. Results may contain a
/// scalar, `String`, `Vec<scalar>`, or `()`. The generated symbol is
/// `rust_dotnet_native__<function-name>` unless overridden with
/// `#[native_export(symbol = "...")]`; owned results also emit a matching private deallocator.
#[proc_macro_attribute]
pub fn native_export(attribute: TokenStream, item: TokenStream) -> TokenStream {
    match native_export_impl(attribute.into(), parse_macro_input!(item as ItemFn)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Generate a safe Rust import and its private raw C ABI declaration.
///
/// ```ignore
/// native_import! {
///     library = "demo_native";
///     pub fn sum(values: &[i32]) -> Result<i64, i32>;
/// }
/// ```
#[proc_macro]
pub fn native_import(input: TokenStream) -> TokenStream {
    match native_import_impl(parse_macro_input!(input as ImportInput)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn native_export_impl(attribute: TokenStream2, function: ItemFn) -> Result<TokenStream2> {
    let symbol = parse_export_symbol(attribute, &function.sig.ident)?;
    let contract = Contract::from_signature(&function.sig)?;
    let raw_name = format_ident!("__rust_dotnet_native_export_{}", function.sig.ident);
    let raw_free_name = format_ident!("__rust_dotnet_native_free_{}", function.sig.ident);
    let free_symbol = owned_free_symbol(&symbol);
    let raw_parameters = contract.raw_parameters();
    let conversions = contract.native_conversions();
    let calls = contract.call_arguments();
    let function_name = &function.sig.ident;
    let output_parameters = contract.output_parameters();
    let output_validation = contract.output_validation();
    let success = contract.export_success();
    let free_export = contract.free_export(&raw_free_name, &free_symbol);

    Ok(quote! {
        #function

        #[doc(hidden)]
        #[unsafe(export_name = #symbol)]
        unsafe extern "C" fn #raw_name(#(#raw_parameters,)* #(#output_parameters),*) -> i32 {
            #output_validation
            #(#conversions)*
            match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| #function_name(#(#calls),*))) {
                Ok(Ok(value)) => #success,
                Ok(Err(status)) if status == 0 => #ZERO_ERROR_STATUS,
                Ok(Err(status)) => status,
                Err(_) => #PANIC_STATUS,
            }
        }

        #free_export
    })
}

fn native_import_impl(input: ImportInput) -> Result<TokenStream2> {
    let contract = Contract::from_signature(&input.signature)?;
    let function_name = &input.signature.ident;
    let raw_module = format_ident!("__rust_dotnet_native_import_{}", function_name);
    let raw_function = format_ident!("call");
    let raw_free_function = format_ident!("free_owned");
    let symbol = input
        .symbol
        .unwrap_or_else(|| default_symbol(function_name));
    let free_symbol = owned_free_symbol(&symbol);
    let raw_parameters = contract.raw_parameters();
    let raw_arguments = contract.import_raw_arguments();
    let visibility = input.visibility;
    let library = input.library;
    let arguments = input.arguments;
    let raw_output_parameters = contract.output_parameters();
    let raw_free_declaration = contract.free_declaration(&raw_free_function, &free_symbol);
    let import_body = contract.import_body(
        &raw_module,
        &raw_function,
        &raw_free_function,
        &raw_arguments,
    );
    let output_type = contract.output_type();

    Ok(quote! {
        #[doc(hidden)]
        mod #raw_module {
            #[link(name = #library)]
            unsafe extern "C" {
                #[link_name = #symbol]
                pub(super) fn #raw_function(
                    #(#raw_parameters,)*
                    #(#raw_output_parameters),*
                ) -> i32;
                #raw_free_declaration
            }
        }

        #visibility fn #function_name(#arguments) -> ::core::result::Result<#output_type, i32> {
            #import_body
        }
    })
}

fn parse_export_symbol(attribute: TokenStream2, function: &Ident) -> Result<LitStr> {
    if attribute.is_empty() {
        return Ok(default_symbol(function));
    }
    let attribute: ExportAttribute = syn::parse2(attribute)?;
    Ok(attribute.symbol)
}

struct ExportAttribute {
    symbol: LitStr,
}

impl Parse for ExportAttribute {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let name: Ident = input.parse()?;
        if name != "symbol" {
            return Err(Error::new(name.span(), "expected `symbol = \"...\"`"));
        }
        input.parse::<Token![=]>()?;
        let symbol = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("expected only `symbol = \"...\"`"));
        }
        Ok(Self { symbol })
    }
}

struct ImportInput {
    library: LitStr,
    symbol: Option<LitStr>,
    visibility: Visibility,
    signature: Signature,
    arguments: Punctuated<FnArg, Token![,]>,
}

impl Parse for ImportInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let keyword: Ident = input.parse()?;
        if keyword != "library" {
            return Err(Error::new(keyword.span(), "expected `library = \"...\";`"));
        }
        input.parse::<Token![=]>()?;
        let library = input.parse()?;
        input.parse::<Token![;]>()?;

        let mut symbol = None;
        if input.peek(syn::Ident) {
            let fork = input.fork();
            let candidate: Ident = fork.parse()?;
            if candidate == "symbol" {
                let _: Ident = input.parse()?;
                input.parse::<Token![=]>()?;
                symbol = Some(input.parse()?);
                input.parse::<Token![;]>()?;
            }
        }

        let visibility = input.parse()?;
        input.parse::<Token![fn]>()?;
        let name: Ident = input.parse()?;
        let content;
        let parenthesized = syn::parenthesized!(content in input);
        let arguments = content.parse_terminated(FnArg::parse, Token![,])?;
        let output = if input.peek(Token![->]) {
            input.parse()?
        } else {
            ReturnType::Default
        };
        input.parse::<Token![;]>()?;
        if !input.is_empty() {
            return Err(input.error("expected one native function declaration"));
        }
        Ok(Self {
            library,
            symbol,
            visibility,
            signature: Signature {
                constness: None,
                asyncness: None,
                unsafety: None,
                abi: None,
                fn_token: Token![fn](name.span()),
                ident: name,
                generics: Default::default(),
                paren_token: parenthesized,
                inputs: arguments.clone(),
                variadic: None,
                output,
            },
            arguments,
        })
    }
}

#[derive(Clone)]
struct Contract {
    arguments: Vec<Argument>,
    output: Output,
}

#[derive(Clone)]
enum Output {
    Scalar(Type),
    String,
    Vec(Type),
    Unit,
}

#[derive(Clone)]
enum Argument {
    Scalar {
        name: Ident,
        ty: Type,
    },
    Slice {
        name: Ident,
        element: Type,
        mutable: bool,
    },
    Str {
        name: Ident,
    },
}

impl Contract {
    fn from_signature(signature: &Signature) -> Result<Self> {
        if !signature.generics.params.is_empty()
            || signature.asyncness.is_some()
            || signature.unsafety.is_some()
        {
            return Err(Error::new_spanned(
                signature,
                "native contracts cannot be generic, async, or unsafe; keep the public function safe and concrete",
            ));
        }
        let mut arguments = Vec::new();
        for argument in &signature.inputs {
            let FnArg::Typed(argument) = argument else {
                return Err(Error::new_spanned(
                    argument,
                    "methods with `self` are not supported",
                ));
            };
            let Pat::Ident(PatIdent { ident, .. }) = &*argument.pat else {
                return Err(Error::new_spanned(
                    &argument.pat,
                    "native contract arguments must be simple identifiers",
                ));
            };
            arguments.push(parse_argument(ident.clone(), &argument.ty)?);
        }
        let ReturnType::Type(_, result) = &signature.output else {
            return Err(Error::new_spanned(
                signature,
                "expected `Result<supported_value, i32>` return type",
            ));
        };
        let output = parse_result_output(result)?;
        Ok(Self { arguments, output })
    }

    fn raw_parameters(&self) -> Vec<TokenStream2> {
        self.arguments
            .iter()
            .flat_map(|argument| match argument {
                Argument::Scalar { name, ty } => vec![quote!(#name: #ty)],
                Argument::Slice {
                    name,
                    element,
                    mutable,
                } => {
                    let pointer = format_ident!("{}_ptr", name);
                    let length = format_ident!("{}_len", name);
                    let pointer_type = if *mutable {
                        quote!(*mut #element)
                    } else {
                        quote!(*const #element)
                    };
                    vec![quote!(#pointer: #pointer_type), quote!(#length: usize)]
                }
                Argument::Str { name } => {
                    let pointer = format_ident!("{}_ptr", name);
                    let length = format_ident!("{}_len", name);
                    vec![quote!(#pointer: *const u8), quote!(#length: usize)]
                }
            })
            .collect()
    }

    fn native_conversions(&self) -> Vec<TokenStream2> {
        self.arguments.iter().filter_map(|argument| match argument {
            Argument::Scalar { .. } => None,
            Argument::Slice { name, element, mutable } => {
                let pointer = format_ident!("{}_ptr", name);
                let length = format_ident!("{}_len", name);
                let converted_pointer = format_ident!("__{}_ptr", name);
                let borrow = if *mutable {
                    quote!(unsafe { ::core::slice::from_raw_parts_mut(#converted_pointer, #length) })
                } else {
                    quote!(unsafe { ::core::slice::from_raw_parts(#converted_pointer, #length) })
                };
                let pointer_ty = if *mutable { quote!(*mut #element) } else { quote!(*const #element) };
                Some(quote! {
                    let #converted_pointer: #pointer_ty = if #length == 0 {
                        ::core::ptr::NonNull::<#element>::dangling().as_ptr()
                    } else {
                        if #length > (isize::MAX as usize) / ::core::mem::size_of::<#element>() {
                            return #INVALID_ARGUMENT_STATUS;
                        }
                        if #pointer.is_null()
                            || (#pointer as usize) % ::core::mem::align_of::<#element>() != 0
                        {
                            return #INVALID_ARGUMENT_STATUS;
                        }
                        #pointer
                    };
                    let #name = #borrow;
                })
            }
            Argument::Str { name } => {
                let pointer = format_ident!("{}_ptr", name);
                let length = format_ident!("{}_len", name);
                let converted_pointer = format_ident!("__{}_ptr", name);
                Some(quote! {
                    let #converted_pointer = if #length == 0 {
                        ::core::ptr::NonNull::<u8>::dangling().as_ptr()
                    } else {
                        if #length > isize::MAX as usize || #pointer.is_null() {
                            return #INVALID_ARGUMENT_STATUS;
                        }
                        #pointer
                    };
                    let __bytes = unsafe {
                        ::core::slice::from_raw_parts(#converted_pointer, #length)
                    };
                    let #name = match ::core::str::from_utf8(__bytes) {
                        ::core::result::Result::Ok(value) => value,
                        ::core::result::Result::Err(_) => return #INVALID_UTF8_STATUS,
                    };
                })
            }
        }).collect()
    }

    fn call_arguments(&self) -> Vec<TokenStream2> {
        self.arguments
            .iter()
            .map(|argument| match argument {
                Argument::Scalar { name, .. }
                | Argument::Slice { name, .. }
                | Argument::Str { name } => quote!(#name),
            })
            .collect()
    }

    fn import_raw_arguments(&self) -> Vec<TokenStream2> {
        self.arguments
            .iter()
            .flat_map(|argument| match argument {
                Argument::Scalar { name, .. } => vec![quote!(#name)],
                Argument::Slice { name, mutable, .. } => {
                    let pointer = if *mutable {
                        quote!(#name.as_mut_ptr())
                    } else {
                        quote!(#name.as_ptr())
                    };
                    vec![pointer, quote!(#name.len())]
                }
                Argument::Str { name } => {
                    vec![quote!(#name.as_ptr()), quote!(#name.len())]
                }
            })
            .collect()
    }

    fn output_parameters(&self) -> Vec<TokenStream2> {
        match &self.output {
            Output::Scalar(ty) => vec![quote!(out: *mut #ty)],
            Output::String => owned_output_parameters(&u8_type()),
            Output::Vec(element) => owned_output_parameters(element),
            Output::Unit => Vec::new(),
        }
    }

    fn output_validation(&self) -> TokenStream2 {
        match &self.output {
            Output::Scalar(ty) => quote! {
                if out.is_null() || (out as usize) % ::core::mem::align_of::<#ty>() != 0 {
                    return #INVALID_ARGUMENT_STATUS;
                }
            },
            Output::String | Output::Vec(_) => quote! {
                if out_ptr.is_null()
                    || (out_ptr as usize) % ::core::mem::align_of::<*mut u8>() != 0
                    || out_len.is_null()
                    || (out_len as usize) % ::core::mem::align_of::<usize>() != 0
                    || out_capacity.is_null()
                    || (out_capacity as usize) % ::core::mem::align_of::<usize>() != 0
                {
                    return #INVALID_ARGUMENT_STATUS;
                }
            },
            Output::Unit => TokenStream2::new(),
        }
    }

    fn output_type(&self) -> TokenStream2 {
        match &self.output {
            Output::Scalar(ty) => quote!(#ty),
            Output::String => quote!(::std::string::String),
            Output::Vec(element) => quote!(::std::vec::Vec<#element>),
            Output::Unit => quote!(()),
        }
    }

    fn export_success(&self) -> TokenStream2 {
        match &self.output {
            Output::Scalar(_) => quote!({
                unsafe { out.write(value) };
                0
            }),
            Output::String => owned_export_success(quote!(value.into_bytes())),
            Output::Vec(_) => owned_export_success(quote!(value)),
            Output::Unit => quote!(0),
        }
    }

    fn free_export(&self, name: &Ident, symbol: &LitStr) -> TokenStream2 {
        let Some(element) = self.owned_element() else {
            return TokenStream2::new();
        };
        quote! {
            #[doc(hidden)]
            #[unsafe(export_name = #symbol)]
            unsafe extern "C" fn #name(
                pointer: *mut #element,
                length: usize,
                capacity: usize,
            ) {
                if capacity == 0 {
                    return;
                }
                if pointer.is_null()
                    || length > capacity
                    || capacity
                        > (isize::MAX as usize) / ::core::mem::size_of::<#element>()
                    || (pointer as usize) % ::core::mem::align_of::<#element>() != 0
                {
                    return;
                }
                unsafe {
                    drop(::std::vec::Vec::from_raw_parts(pointer, length, capacity));
                }
            }
        }
    }

    fn free_declaration(&self, name: &Ident, symbol: &LitStr) -> TokenStream2 {
        let Some(element) = self.owned_element() else {
            return TokenStream2::new();
        };
        quote! {
            #[link_name = #symbol]
            pub(super) fn #name(
                pointer: *mut #element,
                length: usize,
                capacity: usize,
            );
        }
    }

    fn owned_element(&self) -> Option<Type> {
        match &self.output {
            Output::String => Some(u8_type()),
            Output::Vec(element) => Some(element.clone()),
            Output::Scalar(_) | Output::Unit => None,
        }
    }

    fn import_body(
        &self,
        raw_module: &Ident,
        raw_function: &Ident,
        raw_free_function: &Ident,
        raw_arguments: &[TokenStream2],
    ) -> TokenStream2 {
        match &self.output {
            Output::Scalar(ty) => quote! {
                let mut out = ::core::mem::MaybeUninit::<#ty>::uninit();
                let status = unsafe { #raw_module::#raw_function(#(#raw_arguments,)* out.as_mut_ptr()) };
                if status == 0 { Ok(unsafe { out.assume_init() }) } else { Err(status) }
            },
            Output::String => self.import_owned_body(
                &u8_type(),
                raw_module,
                raw_function,
                raw_free_function,
                raw_arguments,
                quote! {
                    ::std::string::String::from_utf8(copied)
                        .map_err(|_| #INVALID_UTF8_STATUS)
                },
            ),
            Output::Vec(element) => self.import_owned_body(
                element,
                raw_module,
                raw_function,
                raw_free_function,
                raw_arguments,
                quote!(::core::result::Result::Ok(copied)),
            ),
            Output::Unit => quote! {
                let status = unsafe { #raw_module::#raw_function(#(#raw_arguments),*) };
                if status == 0 { Ok(()) } else { Err(status) }
            },
        }
    }

    fn import_owned_body(
        &self,
        element: &Type,
        raw_module: &Ident,
        raw_function: &Ident,
        raw_free_function: &Ident,
        raw_arguments: &[TokenStream2],
        project: TokenStream2,
    ) -> TokenStream2 {
        quote! {
            let mut out_ptr = ::core::mem::MaybeUninit::<*mut #element>::uninit();
            let mut out_len = ::core::mem::MaybeUninit::<usize>::uninit();
            let mut out_capacity = ::core::mem::MaybeUninit::<usize>::uninit();
            let status = unsafe {
                #raw_module::#raw_function(
                    #(#raw_arguments,)*
                    out_ptr.as_mut_ptr(),
                    out_len.as_mut_ptr(),
                    out_capacity.as_mut_ptr(),
                )
            };
            if status != 0 {
                return ::core::result::Result::Err(status);
            }
            let pointer = unsafe { out_ptr.assume_init() };
            let length = unsafe { out_len.assume_init() };
            let capacity = unsafe { out_capacity.assume_init() };
            if length > capacity
                || capacity > (isize::MAX as usize) / ::core::mem::size_of::<#element>()
                || (capacity != 0
                    && (pointer.is_null()
                        || (pointer as usize) % ::core::mem::align_of::<#element>() != 0))
            {
                return ::core::result::Result::Err(#INVALID_OWNED_RESULT_STATUS);
            }

            struct NativeOwnedResult {
                pointer: *mut #element,
                length: usize,
                capacity: usize,
            }
            impl ::core::ops::Drop for NativeOwnedResult {
                fn drop(&mut self) {
                    unsafe {
                        #raw_module::#raw_free_function(
                            self.pointer,
                            self.length,
                            self.capacity,
                        );
                    }
                }
            }

            let native = NativeOwnedResult {
                pointer,
                length,
                capacity,
            };
            let copied = if length == 0 {
                ::std::vec::Vec::new()
            } else {
                unsafe { ::core::slice::from_raw_parts(pointer, length) }.to_vec()
            };
            drop(native);
            #project
        }
    }
}

fn parse_argument(name: Ident, ty: &Type) -> Result<Argument> {
    if is_scalar(ty) {
        return Ok(Argument::Scalar {
            name,
            ty: ty.clone(),
        });
    }
    let Type::Reference(reference) = ty else {
        return Err(Error::new_spanned(
            ty,
            "supported inputs are primitive scalars, `&str`, `&[T]`, and `&mut [T]`",
        ));
    };
    if matches!(&*reference.elem, Type::Path(path) if path.qself.is_none() && path.path.is_ident("str"))
    {
        if reference.mutability.is_some() {
            return Err(Error::new_spanned(ty, "`&mut str` is not supported"));
        }
        return Ok(Argument::Str { name });
    }
    let Type::Slice(slice) = &*reference.elem else {
        return Err(Error::new_spanned(
            ty,
            "supported borrowed inputs are `&str`, `&[T]`, and `&mut [T]`",
        ));
    };
    if !is_scalar(&slice.elem) {
        return Err(Error::new_spanned(
            &slice.elem,
            "slice elements must be primitive ABI scalars",
        ));
    }
    Ok(Argument::Slice {
        name,
        element: (*slice.elem).clone(),
        mutable: reference.mutability.is_some(),
    })
}

fn parse_result_output(ty: &Type) -> Result<Output> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return Err(Error::new_spanned(
            ty,
            "expected `Result<supported_value, i32>`",
        ));
    };
    let Some(segment) = path.segments.last() else {
        unreachable!()
    };
    if segment.ident != "Result" {
        return Err(Error::new_spanned(
            ty,
            "expected `Result<supported_value, i32>`",
        ));
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(Error::new_spanned(
            ty,
            "expected `Result<supported_value, i32>`",
        ));
    };
    let types: Vec<_> = arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            syn::GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect();
    if types.len() != 2 || !is_exact_i32(types[1]) {
        return Err(Error::new_spanned(
            ty,
            "native contracts require `Result<primitive_scalar_or_string_or_vec_or_unit, i32>`",
        ));
    }
    if is_scalar(types[0]) {
        Ok(Output::Scalar(types[0].clone()))
    } else if is_string(types[0]) {
        Ok(Output::String)
    } else if let Some(element) = vec_element(types[0]) {
        if !is_scalar(element) {
            return Err(Error::new_spanned(
                element,
                "owned vector elements must be primitive ABI scalars",
            ));
        }
        Ok(Output::Vec(element.clone()))
    } else if matches!(types[0], Type::Tuple(tuple) if tuple.elems.is_empty()) {
        Ok(Output::Unit)
    } else {
        Err(Error::new_spanned(
            types[0],
            "native contract results must be a primitive scalar, `String`, `Vec<T>`, or `()`",
        ))
    }
}

fn owned_output_parameters(element: &Type) -> Vec<TokenStream2> {
    vec![
        quote!(out_ptr: *mut *mut #element),
        quote!(out_len: *mut usize),
        quote!(out_capacity: *mut usize),
    ]
}

fn owned_export_success(value: TokenStream2) -> TokenStream2 {
    quote!({
        let mut owned = #value;
        let pointer = owned.as_mut_ptr();
        let length = owned.len();
        let capacity = owned.capacity();
        ::core::mem::forget(owned);
        unsafe {
            out_ptr.write(pointer);
            out_len.write(length);
            out_capacity.write(capacity);
        }
        0
    })
}

fn is_string(ty: &Type) -> bool {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return false;
    };
    is_alloc_path(path, "string", "String")
        && matches!(
            path.segments.last().unwrap().arguments,
            syn::PathArguments::None
        )
}

fn vec_element(ty: &Type) -> Option<&Type> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return None;
    };
    let segment = path.segments.last()?;
    if !is_alloc_path(path, "vec", "Vec") {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let mut types = arguments.args.iter().filter_map(|argument| match argument {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let element = types.next()?;
    types.next().is_none().then_some(element)
}

fn is_alloc_path(path: &syn::Path, module: &str, name: &str) -> bool {
    match path.segments.len() {
        1 => path.segments[0].ident == name,
        3 => {
            matches!(path.segments[0].ident.to_string().as_str(), "std" | "alloc")
                && path.segments[1].ident == module
                && path.segments[2].ident == name
        }
        _ => false,
    }
}

fn u8_type() -> Type {
    syn::parse_quote!(u8)
}

fn is_exact_i32(ty: &Type) -> bool {
    matches!(ty, Type::Path(TypePath { qself: None, path }) if path.is_ident("i32"))
}

fn is_scalar(ty: &Type) -> bool {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return false;
    };
    if path.segments.len() != 1 {
        return false;
    }
    matches!(
        path.segments[0].ident.to_string().as_str(),
        "i8" | "u8"
            | "i16"
            | "u16"
            | "i32"
            | "u32"
            | "i64"
            | "u64"
            | "isize"
            | "usize"
            | "f32"
            | "f64"
    )
}

fn default_symbol(function: &Ident) -> LitStr {
    LitStr::new(
        &format!("rust_dotnet_native__{function}"),
        Span::call_site(),
    )
}

fn owned_free_symbol(symbol: &LitStr) -> LitStr {
    LitStr::new(&format!("{}__free", symbol.value()), symbol.span())
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn rejects_owned_input_buffers_before_generating_an_abi() {
        let function: ItemFn = parse_quote! {
            pub fn owned(values: Vec<i32>) -> Result<i32, i32> {
                Ok(values.len() as i32)
            }
        };
        let error = native_export_impl(TokenStream2::new(), function).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("primitive scalars, `&str`, `&[T]`, and `&mut [T]`")
        );
    }

    #[test]
    fn rejects_non_status_error_types() {
        let function: ItemFn = parse_quote! {
            pub fn strings(value: i32) -> Result<i32, String> {
                Ok(value)
            }
        };
        let error = native_export_impl(TokenStream2::new(), function).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Result<primitive_scalar_or_string_or_vec_or_unit, i32>")
        );
    }

    #[test]
    fn accepts_strings_and_owned_scalar_vectors() {
        let string_function: ItemFn = parse_quote! {
            pub fn greet(name: &str) -> Result<String, i32> {
                Ok(format!("hello {name}"))
            }
        };
        native_export_impl(TokenStream2::new(), string_function).unwrap();

        let vector_function: ItemFn = parse_quote! {
            pub fn doubled(values: &[i32]) -> Result<Vec<i32>, i32> {
                Ok(values.iter().map(|value| value * 2).collect())
            }
        };
        native_export_impl(TokenStream2::new(), vector_function).unwrap();
    }

    #[test]
    fn rejects_async_contracts() {
        let function: ItemFn = parse_quote! {
            pub async fn later(value: i32) -> Result<i32, i32> {
                Ok(value)
            }
        };
        let error = native_export_impl(TokenStream2::new(), function).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cannot be generic, async, or unsafe")
        );
    }
}
