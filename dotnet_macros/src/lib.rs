//! Procedural macro for `rustc_codegen_clr` interop.
//!
//! `#[dotnet_class]` turns an ordinary Rust `struct` into a managed .NET class. It expands to the
//! *same* `rustc_codegen_clr_comptime_entrypoint` shape the declarative `dotnet_typedef!` already
//! produces — a fn whose MIR calls the "magic" intrinsics in `::mycorrhiza::comptime` (`new_typedef`
//! / `add_field_def` / `add_primary_ctor` / `finish_type`), which the backend's comptime interpreter
//! (`src/comptime.rs`) reads to register a real `ClassDef`. The proc-macro just parses a real
//! `syn::ItemStruct` instead of a bespoke `tt`-muncher DSL — real field syntax, real diagnostics —
//! and emits a *field-initializing primary constructor* so C# can `new <Name>(field0, field1, …)`
//! (the capability the hand-written `dotnet_typedef!` lacked).
//!
//! The consumer crate must enable `#![feature(adt_const_params, unsized_const_params)]` (the metadata
//! is carried as `&'static str` const generics) and depend on `mycorrhiza`.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, punctuated::Punctuated, ItemStruct, LitBool, LitStr, MetaNameValue, Token,
};

/// Split a `"[Assembly]Namespace.Type"` spec into `(assembly, type_name)`. An empty/`[]`-less spec
/// yields `("", spec)`.
fn split_dotnet_ref(spec: &str) -> (String, String) {
    if let Some(rest) = spec.strip_prefix('[') {
        if let Some((asm, name)) = rest.split_once(']') {
            return (asm.to_string(), name.to_string());
        }
    }
    (String::new(), spec.to_string())
}

/// `#[dotnet_class(extends = "[System.Runtime]System.Object", value_type = false)]` on a struct.
///
/// Emits: the original struct (unchanged); a `<Name>Handle` managed-handle alias (a method receiver /
/// the type C# sees); and a comptime entrypoint that registers the class, one field per struct field,
/// and a *primary constructor* (`new(field0, field1, …)` storing each arg into the matching field).
#[proc_macro_attribute]
pub fn dotnet_class(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);

    // ---- attribute args: extends = "...", value_type = bool ----
    let mut extends = "[System.Runtime]System.Object".to_string();
    let mut value_type = false;
    if !attr.is_empty() {
        let parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
        let metas = match syn::parse::Parser::parse(parser, attr) {
            Ok(m) => m,
            Err(e) => return e.to_compile_error().into(),
        };
        for m in metas {
            if m.path.is_ident("extends") {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &m.value
                {
                    extends = s.value();
                }
            } else if m.path.is_ident("value_type") {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Bool(LitBool { value, .. }),
                    ..
                }) = &m.value
                {
                    value_type = *value;
                }
            }
        }
    }
    let (super_asm, super_name) = split_dotnet_ref(&extends);

    let name = input.ident.clone();
    let span = name.span();
    let handle_ident = format_ident!("{}Handle", name);
    let entry_mod = format_ident!("__dotnet_class_{}", name);
    let name_lit = LitStr::new(&name.to_string(), span);
    let super_asm_lit = LitStr::new(&super_asm, span);
    let super_name_lit = LitStr::new(&super_name, span);

    // One `add_field_def::<FieldTy, "name">` per struct field, in declaration order — the primary
    // ctor's parameters follow the same order.
    let field_calls = input.fields.iter().map(|f| {
        let fname = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
        let fname_lit = LitStr::new(&fname, span);
        let fty = &f.ty;
        quote! {
            let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_field_def::<#fty, #fname_lit>(class);
        }
    });

    let expanded = quote! {
        #input

        /// Managed handle to this Rust-defined .NET class. (C# refers to the class by its plain name;
        /// this alias is for Rust-side references — e.g. a future method receiver.)
        #[allow(non_camel_case_types, dead_code)]
        pub type #handle_ident =
            ::mycorrhiza::intrinsics::RustcCLRInteropManagedClass<"", #name_lit>;

        #[allow(non_snake_case, dead_code, unused_variables, internal_features)]
        mod #entry_mod {
            use super::*;
            // The comptime interpreter only *reads* this fn's MIR; nothing calls it, so a `#[used]`
            // root is required or the dead-code pass would drop it (and with it the whole class).
            #[used]
            static PREVENT_DCE: fn() = rustc_codegen_clr_comptime_entrypoint;
            #[inline(never)]
            pub fn rustc_codegen_clr_comptime_entrypoint() {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_new_typedef::<
                    #name_lit, #value_type, #super_asm_lit, #super_name_lit,
                >();
                #(#field_calls)*
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_primary_ctor(class);
                ::mycorrhiza::comptime::rustc_codegen_clr_finish_type(class);
            }
        }
    };
    expanded.into()
}
