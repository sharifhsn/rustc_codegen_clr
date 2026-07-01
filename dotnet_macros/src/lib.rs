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
    parse_macro_input, punctuated::Punctuated, spanned::Spanned, FnArg, ItemFn, ItemStruct,
    LitBool, LitStr, MetaNameValue, ReturnType, Token, Type,
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

// ============================================================================
// #[dotnet_export] — auto-marshal a Rust fn so C# can call it as a plain typed method.
// ============================================================================

/// How one Rust param/return type crosses the managed seam: the CIL-visible type the shim uses, plus
/// the code that converts between it and the idiomatic Rust type.
struct Marshal {
    /// The type the `#[no_mangle] extern "C"` shim uses at the seam (what C# sees).
    seam_ty: proc_macro2::TokenStream,
    /// Given a binding `#id` of `seam_ty`, produce an expression of the idiomatic Rust type to pass
    /// to the inner fn. `None` means "pass `#id` through unchanged" (identity marshalling).
    to_rust: Option<fn(&syn::Ident) -> proc_macro2::TokenStream>,
    /// Given a binding `#id` of the idiomatic Rust return type, produce an expression of `seam_ty`.
    /// `None` means "return `#id` unchanged".
    from_rust: Option<fn(&syn::Ident) -> proc_macro2::TokenStream>,
}

/// The set of primitive types passed across the seam unchanged (they are already `ManagedSafe` and
/// map 1:1 to a CIL primitive C# understands).
fn is_passthrough_primitive(path: &str) -> bool {
    matches!(
        path,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "isize"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
    )
}

/// The single-segment path name of a plain type path (e.g. `String`, `i32`), or `None` for anything
/// more complex (generics, qualified paths, …).
fn simple_path_ident(ty: &Type) -> Option<String> {
    if let Type::Path(tp) = ty {
        if tp.qself.is_none() && tp.path.segments.len() == 1 {
            let seg = &tp.path.segments[0];
            if seg.arguments.is_empty() {
                return Some(seg.ident.to_string());
            }
        }
    }
    None
}

/// Resolve how a **parameter** type is marshalled, or `Err(message)` if unsupported.
fn marshal_param(ty: &Type) -> Result<Marshal, String> {
    // `&str` (shared ref to a `str`) → managed `System.String` inbound.
    if let Type::Reference(r) = ty {
        if r.mutability.is_none() {
            if let Type::Path(tp) = &*r.elem {
                if tp.qself.is_none() && tp.path.is_ident("str") {
                    return Ok(Marshal {
                        seam_ty: quote! { ::mycorrhiza::system::MString },
                        to_rust: Some(|id| {
                            quote! {
                                let #id: ::std::string::String =
                                    ::mycorrhiza::system::DotNetString::from_handle(#id).to_rust_string();
                            }
                        }),
                        from_rust: None,
                    });
                }
            }
        }
        return Err(format!(
            "#[dotnet_export]: unsupported reference parameter type `{}`; only `&str` is marshalled \
             among references. Pass an owned/primitive type, or marshal manually with `MString`.",
            quote! { #ty }
        ));
    }

    if let Some(name) = simple_path_ident(ty) {
        if name == "String" {
            return Ok(Marshal {
                seam_ty: quote! { ::mycorrhiza::system::MString },
                to_rust: Some(|id| {
                    quote! {
                        let #id: ::std::string::String =
                            ::mycorrhiza::system::DotNetString::from_handle(#id).to_rust_string();
                    }
                }),
                from_rust: None,
            });
        }
        if is_passthrough_primitive(&name) {
            return Ok(Marshal {
                seam_ty: quote! { #ty },
                to_rust: None,
                from_rust: None,
            });
        }
        return Err(format!(
            "#[dotnet_export]: unsupported parameter type `{name}`. Supported: the integer/float \
             primitives, `bool`, `&str`, and `String`."
        ));
    }

    Err(format!(
        "#[dotnet_export]: unsupported parameter type `{}`. Supported: the integer/float primitives, \
         `bool`, `&str`, and `String`.",
        quote! { #ty }
    ))
}

/// Resolve how the **return** type is marshalled, or `Err(message)` if unsupported.
fn marshal_return(ty: &Type) -> Result<Marshal, String> {
    // `&str` return (typically a `&'static str`) → managed string outbound.
    if let Type::Reference(r) = ty {
        if r.mutability.is_none() {
            if let Type::Path(tp) = &*r.elem {
                if tp.qself.is_none() && tp.path.is_ident("str") {
                    return Ok(Marshal {
                        seam_ty: quote! { ::mycorrhiza::system::MString },
                        to_rust: None,
                        from_rust: Some(|id| {
                            quote! { ::mycorrhiza::system::DotNetString::from(#id).handle() }
                        }),
                    });
                }
            }
        }
        return Err(format!(
            "#[dotnet_export]: unsupported reference return type `{}`; only `&str` is marshalled \
             among references.",
            quote! { #ty }
        ));
    }

    if let Some(name) = simple_path_ident(ty) {
        if name == "String" {
            return Ok(Marshal {
                seam_ty: quote! { ::mycorrhiza::system::MString },
                to_rust: None,
                from_rust: Some(|id| {
                    quote! { ::mycorrhiza::system::DotNetString::from(#id.as_str()).handle() }
                }),
            });
        }
        if is_passthrough_primitive(&name) {
            return Ok(Marshal {
                seam_ty: quote! { #ty },
                to_rust: None,
                from_rust: None,
            });
        }
        return Err(format!(
            "#[dotnet_export]: unsupported return type `{name}`. Supported: the integer/float \
             primitives, `bool`, `&str`, `String`, and `()`."
        ));
    }

    Err(format!(
        "#[dotnet_export]: unsupported return type `{}`. Supported: the integer/float primitives, \
         `bool`, `&str`, `String`, and `()`.",
        quote! { #ty }
    ))
}

/// `#[dotnet_export]` on a free function — makes it callable from C# as a plain, typed method on
/// `MainModule`, with no hand-written `(ptr, len)` buffer dance.
///
/// The user writes an ordinary Rust fn:
///
/// ```ignore
/// #[dotnet_export]
/// pub fn greet(name: &str) -> String { format!("Hello, {name}!") }
/// ```
///
/// and C# calls `MainModule.greet("x")`, getting back a `string`. The macro leaves the original fn
/// untouched (still callable from Rust) and emits a hidden `#[no_mangle] extern "C"` **shim** that
/// crosses the managed seam: each supported argument/return type is marshalled to/from its
/// CIL-visible form. `&str`/`String` cross as a real managed `System.String` (so C# sees `string`,
/// not a pointer pair); the numeric/`bool` primitives pass through unchanged.
///
/// The consuming `cdylib` must depend on `mycorrhiza`. Types outside the supported set produce a
/// clear compile error (marshalling is never faked).
#[proc_macro_attribute]
pub fn dotnet_export(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let sig = &func.sig;

    // Refuse constructs the seam can't express, with a precise message.
    if let Some(c) = &sig.constness {
        return syn::Error::new(c.span(), "#[dotnet_export]: `const fn` cannot be exported")
            .to_compile_error()
            .into();
    }
    if let Some(a) = &sig.asyncness {
        return syn::Error::new(
            a.span(),
            "#[dotnet_export]: `async fn` is not yet supported (Task/async bridge is separate)",
        )
        .to_compile_error()
        .into();
    }
    if !sig.generics.params.is_empty() {
        return syn::Error::new(
            sig.generics.span(),
            "#[dotnet_export]: generic functions cannot be exported (each C# call needs one concrete \
             .NET signature)",
        )
        .to_compile_error()
        .into();
    }
    if let Some(v) = &sig.variadic {
        return syn::Error::new(v.span(), "#[dotnet_export]: variadic functions cannot be exported")
            .to_compile_error()
            .into();
    }

    let fn_name = sig.ident.clone();
    let shim_mod = format_ident!("__dotnet_export_{}", fn_name);

    // Marshal each parameter. `receiver` (self) is rejected — only free functions are exportable.
    let mut seam_params = Vec::new(); // `#pname: #seam_ty` tokens for the shim signature.
    let mut pre_call = Vec::new(); // in-conversion statements (seam value → idiomatic Rust value).
    let mut call_args = Vec::new(); // expressions passed to the inner fn.
    for (idx, arg) in sig.inputs.iter().enumerate() {
        let pat_ty = match arg {
            FnArg::Receiver(r) => {
                return syn::Error::new(
                    r.span(),
                    "#[dotnet_export]: methods with `self` cannot be exported; use a free function",
                )
                .to_compile_error()
                .into();
            }
            FnArg::Typed(pt) => pt,
        };
        // Use a fresh, positional binding name so we don't depend on the user's pattern being a
        // plain identifier (it may be `mut x`, `_`, a tuple pattern, …).
        let pname = format_ident!("arg{}", idx);
        let marshal = match marshal_param(&pat_ty.ty) {
            Ok(m) => m,
            Err(msg) => {
                return syn::Error::new(pat_ty.ty.span(), msg)
                    .to_compile_error()
                    .into();
            }
        };
        let seam_ty = &marshal.seam_ty;
        seam_params.push(quote! { #pname: #seam_ty });
        match marshal.to_rust {
            Some(conv) => {
                // `#pname` is rebound (shadowed) to the idiomatic Rust value; `&str` params also
                // need a borrow at the call site.
                pre_call.push(conv(&pname));
                if matches!(&*pat_ty.ty, Type::Reference(_)) {
                    call_args.push(quote! { &#pname });
                } else {
                    call_args.push(quote! { #pname });
                }
            }
            None => call_args.push(quote! { #pname }),
        }
    }

    // Marshal the return type.
    let (seam_ret, ret_expr) = match &sig.output {
        ReturnType::Default => (quote! {}, quote! { __ret }), // `-> ()`; identity.
        ReturnType::Type(_, ty) => {
            let marshal = match marshal_return(ty) {
                Ok(m) => m,
                Err(msg) => {
                    return syn::Error::new(ty.span(), msg).to_compile_error().into();
                }
            };
            let seam_ty = marshal.seam_ty;
            let ret_ident = format_ident!("__ret");
            let expr = match marshal.from_rust {
                Some(conv) => conv(&ret_ident),
                None => quote! { #ret_ident },
            };
            (quote! { -> #seam_ty }, expr)
        }
    };

    let call = quote! { super::#fn_name(#(#call_args),*) };

    let expanded = quote! {
        // The user's function, verbatim — still callable from Rust with its idiomatic signature.
        #func

        // The generated seam shim. `#[no_mangle]` gives it the flat symbol `#fn_name`, which the
        // backend marks `Access::Extern` (a DCE root) and the exporter emits as a public static
        // method on the assembly's `MainModule` — so C# calls it as `MainModule::#fn_name`.
        #[doc(hidden)]
        #[allow(non_snake_case, unused_imports, clippy::useless_conversion)]
        mod #shim_mod {
            use super::*;
            #[no_mangle]
            pub extern "C" fn #fn_name(#(#seam_params),*) #seam_ret {
                #(#pre_call)*
                let __ret = #call;
                #ret_expr
            }
        }
    };
    expanded.into()
}
