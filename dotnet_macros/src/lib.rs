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
    parse_macro_input, punctuated::Punctuated, spanned::Spanned, FnArg, ImplItem, ItemFn, ItemImpl,
    ItemStruct, LitBool, LitStr, MetaNameValue, ReturnType, Token, Type,
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

/// Detects a single-generic-argument suffix on a `.NET` type reference — e.g.
/// `"Ns.IFace<[Asm]Ns.Ty>"` splits into `("Ns.IFace", Some("[Asm]Ns.Ty"))` — so `implements = "…"`
/// can name a generic interface (`IEnumerator<T>`, `IAsyncEnumerator<T>`, …) bound to a concrete
/// external type, mirroring `rustc_codegen_clr_add_generic_interface_impl`'s own doc: the generic
/// argument is a plain `[Asm]Ns.Type` reference like any other, never derived from a Rust type.
/// Returns `(spec, None)` unchanged if there's no such suffix. Only a single generic argument is
/// supported (multi-argument interfaces like `IDictionary<K,V>` aren't expressible this way today).
fn split_generic_suffix(spec: &str) -> (String, Option<String>) {
    if let Some(open) = spec.find('<') {
        if let Some(inner) = spec.strip_suffix('>') {
            return (spec[..open].to_string(), Some(inner[open + 1..].to_string()));
        }
    }
    (spec.to_string(), None)
}

/// Reject a `"[Assembly]Namespace.Type"` spec with an opened-but-unclosed `[` — a common typo
/// (`"[Foo"` instead of `"[Foo]Foo.Bar"`) that `split_dotnet_ref` would otherwise silently treat as
/// a bracket-less type name, registering a garbage assembly reference with no diagnostic at all.
fn validate_dotnet_ref(spec: &str, span: proc_macro2::Span) -> syn::Result<()> {
    if spec.starts_with('[') && !spec.contains(']') {
        return Err(syn::Error::new(
            span,
            format!(
                "malformed .NET type reference `{spec}`: an opening `[` must be matched by a closing \
                 `]`, e.g. `\"[System.Runtime]System.Object\"`"
            ),
        ));
    }
    Ok(())
}

/// Extract a string-literal value from an attribute's `= value` expression, or a precise error at
/// the value's own span if it isn't one (instead of silently keeping the field's prior/default
/// value, which is what a plain `if let` match-and-ignore would do).
fn str_lit_value(expr: &syn::Expr) -> syn::Result<String> {
    if let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(s),
        ..
    }) = expr
    {
        Ok(s.value())
    } else {
        Err(syn::Error::new(
            expr.span(),
            "expected a string literal, e.g. `\"...\"`",
        ))
    }
}

/// Extract a bool-literal value from an attribute's `= value` expression, or a precise error at the
/// value's own span if it isn't one.
fn bool_lit_value(expr: &syn::Expr) -> syn::Result<bool> {
    if let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Bool(LitBool { value, .. }),
        ..
    }) = expr
    {
        Ok(*value)
    } else {
        Err(syn::Error::new(
            expr.span(),
            "expected a bool literal, `true` or `false`",
        ))
    }
}

/// `#[dotnet_class(extends = "[System.Runtime]System.Object", value_type = false)]` on a struct.
///
/// Emits: the original struct (unchanged); a `<Name>Handle` managed-handle alias (a method receiver /
/// the type C# sees); and a comptime entrypoint that registers the class, one field per struct field,
/// and a *primary constructor* (`new(field0, field1, …)` storing each arg into the matching field).
#[proc_macro_attribute]
pub fn dotnet_class(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);

    // ---- attribute args: extends = "...", value_type = bool, default_ctor = bool,
    //      field_setters = bool ----
    let mut extends = "[System.Runtime]System.Object".to_string();
    let mut value_type = false;
    let mut default_ctor = false;
    let mut field_setters = false;
    // Managed interfaces this class implements, `;`-separated in one string (usually just one), e.g.
    // `implements = "[MyLib]MyLib.IService"` or `"[A]A.I1;[B]B.I2"`. See the interface `add_*` intrinsic.
    let mut implements: Vec<String> = Vec::new();
    if !attr.is_empty() {
        let parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
        let metas = match syn::parse::Parser::parse(parser, attr) {
            Ok(m) => m,
            Err(e) => return e.to_compile_error().into(),
        };
        for m in metas {
            if m.path.is_ident("extends") {
                match str_lit_value(&m.value) {
                    Ok(s) => {
                        if let Err(e) = validate_dotnet_ref(&s, m.value.span()) {
                            return e.to_compile_error().into();
                        }
                        extends = s;
                    }
                    Err(e) => return e.to_compile_error().into(),
                }
            } else if m.path.is_ident("value_type") {
                match bool_lit_value(&m.value) {
                    Ok(v) => value_type = v,
                    Err(e) => return e.to_compile_error().into(),
                }
            } else if m.path.is_ident("default_ctor") {
                match bool_lit_value(&m.value) {
                    Ok(v) => default_ctor = v,
                    Err(e) => return e.to_compile_error().into(),
                }
            } else if m.path.is_ident("field_setters") {
                match bool_lit_value(&m.value) {
                    Ok(v) => field_setters = v,
                    Err(e) => return e.to_compile_error().into(),
                }
            } else if m.path.is_ident("implements") {
                let s = match str_lit_value(&m.value) {
                    Ok(s) => s,
                    Err(e) => return e.to_compile_error().into(),
                };
                let mut specs = Vec::new();
                for spec in s.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                    if let Err(e) = validate_dotnet_ref(spec, m.value.span()) {
                        return e.to_compile_error().into();
                    }
                    let (_, generic) = split_generic_suffix(spec);
                    match generic {
                        Some(inner) => {
                            if let Err(e) = validate_dotnet_ref(&inner, m.value.span()) {
                                return e.to_compile_error().into();
                            }
                        }
                        None if spec.contains('<') => {
                            return syn::Error::new(
                                m.value.span(),
                                format!(
                                    "malformed generic interface reference `{spec}`: a `<...>` \
                                     generic argument must be closed with `>` at the end, e.g. \
                                     `\"Ns.IFace<[Asm]Ns.Ty>\"`"
                                ),
                            )
                            .to_compile_error()
                            .into();
                        }
                        None => {}
                    }
                    specs.push(spec.to_string());
                }
                implements = specs;
            } else {
                let path = &m.path;
                return syn::Error::new(
                    m.path.span(),
                    format!(
                        "#[dotnet_class]: unknown attribute key `{}`; expected one of `extends`, \
                         `value_type`, `default_ctor`, `field_setters`, `implements`",
                        quote! { #path }
                    ),
                )
                .to_compile_error()
                .into();
            }
        }
    }
    let (super_asm, super_name) = split_dotnet_ref(&extends);

    let name = input.ident.clone();
    let span = name.span();

    // One `add_interface_impl::<"asm", "name">` per implemented interface. The virtual methods a
    // `#[dotnet_methods]` block adds satisfy them by name+signature (implicit interface impl).
    let interface_calls: Vec<_> = implements
        .iter()
        .map(|spec| {
            let (asm, iface_raw) = split_dotnet_ref(spec);
            let asm_lit = LitStr::new(&asm, span);
            let (iface, generic) = split_generic_suffix(&iface_raw);
            let iface_lit = LitStr::new(&iface, span);
            match generic {
                None => quote! {
                    let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_interface_impl::<#asm_lit, #iface_lit>(class);
                },
                Some(generic_spec) => {
                    // A leading `valuetype ` marker (mirroring IL's own keyword) says the generic
                    // argument is a .NET value type (`System.Int32`, a user struct, …) rather than
                    // the default reference type (`System.String`, a user class, …) — see
                    // `rustc_codegen_clr_add_generic_interface_impl`'s doc for why this can't be
                    // inferred: the argument is a plain string with no backing Rust type.
                    let (generic_spec, is_valuetype) =
                        match generic_spec.strip_prefix("valuetype ") {
                            Some(rest) => (rest.to_string(), true),
                            None => (generic_spec, false),
                        };
                    let (gen_asm, gen_name) = split_dotnet_ref(&generic_spec);
                    let gen_asm_lit = LitStr::new(&gen_asm, span);
                    let gen_name_lit = LitStr::new(&gen_name, span);
                    let is_valuetype_lit = LitBool::new(is_valuetype, span);
                    quote! {
                        let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_generic_interface_impl::<#asm_lit, #iface_lit, #gen_asm_lit, #gen_name_lit, #is_valuetype_lit>(class);
                    }
                }
            }
        })
        .collect();
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

    // Optional extras, gated on the attribute flags: a parameterless default ctor (overloading the
    // primary ctor) and a `set_<field>` mutator per field (paired with the `read_<field>` accessor).
    let default_ctor_call = if default_ctor {
        quote! { let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_default_ctor(class); }
    } else {
        quote! {}
    };
    let field_setters_call = if field_setters {
        quote! { let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_field_setters(class); }
    } else {
        quote! {}
    };

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
                #(#interface_calls)*
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_primary_ctor(class);
                #default_ctor_call
                #field_setters_call
                ::mycorrhiza::comptime::rustc_codegen_clr_finish_type(class);
            }
        }
    };
    expanded.into()
}

// ============================================================================
// #[dotnet_methods] — attach static / instance methods to a `#[dotnet_class]` type.
// ============================================================================

/// `#[dotnet_methods]` on an inherent `impl <Name> { … }` block attaches the block's functions to the
/// managed class `<Name>` that a `#[dotnet_class] struct <Name>` declared. It emits a *second* comptime
/// entrypoint that re-opens `<Name>` (the backend's `finish_type` is idempotent — it reuses the
/// already-registered class and just appends these methods) and adds one method per `fn`:
///
///   * a `fn` whose FIRST parameter is `<Name>Handle` (the managed-handle alias `#[dotnet_class]` emits)
///     becomes a **virtual instance** method `<Name>.method(this, …)` — C# calls `obj.method(…)`;
///   * any other `fn` becomes a **static** method `<Name>.method(…)` — C# calls `<Name>.method(…)`.
///
/// The functions are ordinary Rust code (still callable from Rust), codegen'd separately; the managed
/// method *aliases* the Rust fn, so its signature must match (the receiver, for an instance method, is
/// the explicit first `<Name>Handle` arg). Methods must be free-standing (no `self`): the alias targets
/// a static symbol.
///
/// ```ignore
/// #[dotnet_class]
/// pub struct Counter { value: i32 }
///
/// #[dotnet_methods]
/// impl Counter {
///     pub fn make(value: i32) -> CounterHandle { /* … */ }          // static:   Counter.make(5)
///     pub fn get(this: CounterHandle) -> i32 { this.read_value() }   // instance: c.get()
/// }
/// ```
#[proc_macro_attribute]
pub fn dotnet_methods(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as ItemImpl);

    // The class name is the impl's self type (a plain path like `Counter`); its handle alias is
    // `<Name>Handle` (what an instance method takes as its receiver).
    let self_ty = &input.self_ty;
    let class_name = match &**self_ty {
        Type::Path(tp) if tp.qself.is_none() => match tp.path.segments.last() {
            Some(seg) => seg.ident.to_string(),
            None => {
                return syn::Error::new(self_ty.span(), "#[dotnet_methods]: empty self type path")
                    .to_compile_error()
                    .into();
            }
        },
        _ => {
            return syn::Error::new(
                self_ty.span(),
                "#[dotnet_methods]: expected an inherent `impl <Name> { … }` over a plain type name",
            )
            .to_compile_error()
            .into();
        }
    };
    let handle_name = format!("{class_name}Handle");
    let name_lit = LitStr::new(&class_name, self_ty.span());
    let entry_mod = format_ident!("__dotnet_methods_{}", class_name);

    // Emit one `add_*_method_def` call per method, in declaration order. `self`-taking methods and
    // non-fn impl items are rejected loudly.
    let mut method_calls = Vec::new();
    let mut keep_anchors = Vec::new();
    for it in &mut input.items {
        let ImplItem::Fn(f) = it else {
            return syn::Error::new(
                it.span(),
                "#[dotnet_methods]: only `fn` items are supported in the impl block",
            )
            .to_compile_error()
            .into();
        };
        // `#[dotnet_override("[Asm]Ns.BaseType")]` — an explicit ECMA-335 `.override` of that base
        // type's same-named virtual (see `rustc_codegen_clr_mark_last_method_override`'s doc for
        // the intentionally narrow scope). Stripped from the method before `#input` is re-emitted
        // below, since it isn't a real Rust attribute the compiler would otherwise accept.
        let mut override_base: Option<String> = None;
        for attr in &f.attrs {
            if attr.path().is_ident("dotnet_override") {
                let spec = match attr.parse_args::<syn::LitStr>() {
                    Ok(lit) => lit.value(),
                    Err(_) => {
                        return syn::Error::new(
                            attr.span(),
                            "#[dotnet_override(\"[Asm]Ns.BaseType\")]: expected a single string \
                             literal argument naming the base type",
                        )
                        .to_compile_error()
                        .into();
                    }
                };
                if let Err(e) = validate_dotnet_ref(&spec, attr.span()) {
                    return e.to_compile_error().into();
                }
                override_base = Some(spec);
            }
        }
        f.attrs.retain(|a| !a.path().is_ident("dotnet_override"));

        let fn_ident = &f.sig.ident;
        let fname_lit = LitStr::new(&fn_ident.to_string(), fn_ident.span());

        // A `#[used]` anchor holding the reified fn pointer forces rustc's own mono-collector to
        // codegen this method (a plain `pub fn` on a cdylib type is otherwise pruned as unreachable —
        // nothing *calls* it; the comptime entrypoint only names it, and that entrypoint is
        // interpreted, not codegen'd). Without this the managed method's `AliasFor` edge would dangle
        // (`method_def_from_ref` → None → "alias for an extern function" panic at typecheck time).
        // Build the fn-pointer type `fn(<in-types>) -> <out>` from the method's signature (dropping
        // the parameter *patterns*, keeping just the types), so the anchor is a `Sync`, const-eval-OK
        // `static` — `*const ()`/`usize` casts fail in const-eval, but a fn-pointer static is fine
        // (this is the same anchor shape the older `dotnet_typedef!` used).
        let in_types = f.sig.inputs.iter().map(|arg| match arg {
            FnArg::Typed(pt) => (*pt.ty).clone(),
            // `self` was already rejected above; unreachable, but keep the map total.
            FnArg::Receiver(_) => syn::parse_quote! { () },
        });
        let out_ty = match &f.sig.output {
            ReturnType::Default => quote! { () },
            ReturnType::Type(_, ty) => quote! { #ty },
        };
        let keep_ident = format_ident!("KEEP_{}", fn_ident);
        keep_anchors.push(quote! {
            #[used]
            static #keep_ident: fn(#(#in_types),*) -> #out_ty = #self_ty::#fn_ident;
        });

        // Decide static vs instance by the first parameter's type. A `self` receiver is rejected — the
        // alias must target a static symbol, so instance methods take the handle explicitly.
        if let Some(FnArg::Receiver(r)) = f.sig.inputs.first() {
            return syn::Error::new(
                r.span(),
                "#[dotnet_methods]: methods with a `self` receiver are not supported; take the \
                 receiver explicitly as a first `<Name>Handle` parameter for an instance method",
            )
            .to_compile_error()
            .into();
        }
        let first_is_handle = matches!(
            f.sig.inputs.first(),
            Some(FnArg::Typed(pt)) if type_last_ident_is(&pt.ty, &handle_name)
        );

        if first_is_handle {
            // Instance (virtual) method: signature includes the receiver as arg 0.
            method_calls.push(quote! {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_method_def::<
                    "pub", "virtual", #fname_lit, _,
                >(class, #self_ty::#fn_ident);
            });
            if let Some(spec) = override_base {
                let (base_asm, base_type) = split_dotnet_ref(&spec);
                let base_asm_lit = LitStr::new(&base_asm, fn_ident.span());
                let base_type_lit = LitStr::new(&base_type, fn_ident.span());
                method_calls.push(quote! {
                    let class = ::mycorrhiza::comptime::rustc_codegen_clr_mark_last_method_override::<
                        #base_asm_lit, #base_type_lit,
                    >(class);
                });
            }
        } else {
            if let Some(spec) = override_base {
                let _ = spec;
                return syn::Error::new(
                    fn_ident.span(),
                    "#[dotnet_override]: only supported on an instance (virtual) method — a \
                     static method has no vtable slot to override",
                )
                .to_compile_error()
                .into();
            }
            // Static method: signature verbatim, no receiver.
            method_calls.push(quote! {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_static_method_def::<
                    #fname_lit, _,
                >(class, #self_ty::#fn_ident);
            });
        }
    }

    let expanded = quote! {
        // The user's impl block, verbatim — the methods remain ordinary callable Rust functions.
        #input

        #[allow(non_snake_case, dead_code, unused_variables, internal_features, non_upper_case_globals)]
        mod #entry_mod {
            use super::*;
            // The comptime interpreter only *reads* this fn's MIR; nothing calls it, so a `#[used]`
            // root is required or the dead-code pass would drop it.
            #[used]
            static PREVENT_DCE: fn() = rustc_codegen_clr_comptime_entrypoint;
            // Keep each aliased method fn alive through rustc's mono-collector (see above).
            #(#keep_anchors)*
            #[inline(never)]
            pub fn rustc_codegen_clr_comptime_entrypoint() {
                // Re-open the class: same name/super/value-type as the `#[dotnet_class]` decl, so the
                // idempotent `finish_type` finds the already-registered class and appends these methods.
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_new_typedef::<
                    #name_lit, false, "System.Runtime", "System.Object",
                >();
                #(#method_calls)*
                ::mycorrhiza::comptime::rustc_codegen_clr_finish_type(class);
            }
        }
    };
    expanded.into()
}

/// True if `ty` is a plain path whose last segment ident equals `name` (e.g. matching `CounterHandle`
/// regardless of any leading path qualification).
fn type_last_ident_is(ty: &Type, name: &str) -> bool {
    matches!(
        ty,
        Type::Path(tp)
            if tp.qself.is_none()
                && tp.path.segments.last().is_some_and(|s| s.ident == name)
    )
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
    /// to the inner fn. `None` means "pass `#id` through unchanged" (identity marshalling). A boxed
    /// closure (not a bare `fn` pointer) so arms that need to close over data derived from the source
    /// type (e.g. the `Vec<T>` arm closing over `T`'s own tokens) can do so.
    to_rust: Option<Box<dyn Fn(&syn::Ident) -> proc_macro2::TokenStream>>,
    /// Given a binding `#id` of the idiomatic Rust return type, produce an expression of `seam_ty`.
    /// `None` means "return `#id` unchanged".
    from_rust: Option<Box<dyn Fn(&syn::Ident) -> proc_macro2::TokenStream>>,
    /// `true` if the **return** type itself is (or directly embeds) a managed object reference
    /// (`RustcCLRInteropManagedClass`/`RustcCLRInteropManagedGeneric`-shaped) — e.g.
    /// `mycorrhiza::task::Task`/`TaskT<T>`. Such a value must NOT be threaded through
    /// `catch_unwind`'s `Result<T, Box<dyn Any + Send>>` return: that `Result` is an enum with
    /// overlapping variant storage, and `cilly`'s `ClassDef::layout_check` correctly rejects ANY
    /// managed reference in an overlapping field (the same GC-soundness rule `mycorrhiza::task`'s own
    /// docs describe for coroutine state) — so `catch_unwind::<F, T>` would need a `ClassDef` for
    /// `Result<T, ..>` with a GC-ref `Ok` payload placed in overlapping storage, which is unsound and
    /// correctly refused, surfacing as a `ManagedRefInOverlapingField` compiler panic. The shim
    /// generator (`dotnet_export`) checks this flag to route such returns through a raw-pointer
    /// out-slot instead (see its body), keeping the `catch_unwind` payload a plain `()`.
    returns_managed_handle: bool,
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

/// If `ty` is a single-segment generic path `Name<Arg>` (exactly one angle-bracketed type argument,
/// e.g. `Vec<i32>` or `TaskT<i32>`) — regardless of any leading path qualification (`mycorrhiza::
/// task::TaskT<T>` matches on `TaskT`) — returns `(Name, &Arg)`. `None` for anything else (no
/// generics, more than one argument, a lifetime/const argument, a qualified `Self` path, …).
fn single_generic_arg(ty: &Type) -> Option<(String, &Type)> {
    let Type::Path(tp) = ty else { return None };
    if tp.qself.is_some() {
        return None;
    }
    let seg = tp.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    let syn::GenericArgument::Type(inner) = &args.args[0] else {
        return None;
    };
    Some((seg.ident.to_string(), inner))
}

/// True if `ty` is a plain (possibly path-qualified) reference to `mycorrhiza::task::Task` — the
/// non-generic managed `Task` handle. Matched by trailing ident only (like [`type_last_ident_is`]),
/// so both `mycorrhiza::task::Task` and a locally `use`d bare `Task` resolve.
fn is_task_type(ty: &Type) -> bool {
    type_last_ident_is(ty, "Task")
}

/// If `ty` is `mycorrhiza::task::TaskT<T>` (the generic, result-bearing Task handle — named `TaskT`
/// precisely to avoid colliding with the non-generic `Task`), returns `T`. `None` otherwise.
fn task_t_inner(ty: &Type) -> Option<&Type> {
    let (name, inner) = single_generic_arg(ty)?;
    (name == "TaskT").then_some(inner)
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
                        to_rust: Some(Box::new(|id| {
                            quote! {
                                let #id: ::std::string::String =
                                    ::mycorrhiza::system::DotNetString::from_handle(#id).to_rust_string();
                            }
                        })),
                        from_rust: None,
                        returns_managed_handle: false,
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
                to_rust: Some(Box::new(|id| {
                    quote! {
                        let #id: ::std::string::String =
                            ::mycorrhiza::system::DotNetString::from_handle(#id).to_rust_string();
                    }
                })),
                from_rust: None,
                returns_managed_handle: false,
            });
        }
        if is_passthrough_primitive(&name) {
            return Ok(Marshal {
                seam_ty: quote! { #ty },
                to_rust: None,
                from_rust: None,
                returns_managed_handle: false,
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
                        from_rust: Some(Box::new(|id| {
                            quote! { ::mycorrhiza::system::DotNetString::from(#id).handle() }
                        })),
                        returns_managed_handle: false,
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

    // `mycorrhiza::task::Task` — the idiomatic non-generic managed `Task` wrapper. NOTE: unlike
    // `TaskT<T>` below, `task::Task` is NOT itself a bare alias for `RustcCLRInteropManagedClass` —
    // it's a genuine newtype struct (`struct Task { h: RawTask }`, see `mycorrhiza/src/task.rs`)
    // kept distinct from `bindings::Task` so the module can carry its own inherent methods. The
    // backend's magic-type recognition matches `RustcCLRInteropManagedClass`/`RustcCLRInteropManaged
    // Generic` BY NAME, so a bare `Task` local would lower to an ordinary (wrong) managed class
    // `mycorrhiza.task.Task`, not the real `System.Threading.Tasks.Task` C# expects — unwrap it via
    // `.raw()` to the real handle alias `mycorrhiza::System::Threading::Tasks::Task` at the seam
    // (re-exported at that path by `mycorrhiza`'s crate-root `pub use bindings::*;` — the same path
    // `mycorrhiza::task` itself uses internally, e.g. `crate::System::Threading::Tasks::
    // TaskCompletionSource`), which round-trips through `Task::from_raw`/`Task::raw` (both
    // `#[inline]`, zero-cost). This intentionally does NOT
    // accept `async fn` sugar (rejected earlier in `dotnet_export`, independent of this arm): the fn
    // body must itself construct the `Task` (typically via `mycorrhiza::task::future_to_task_unit`)
    // and return it, exactly like any other ordinary, non-async `#[dotnet_export]` fn. Checked BEFORE
    // `simple_path_ident` (which only matches single-segment paths) because callers typically spell
    // this as the multi-segment `mycorrhiza::task::Task`. `returns_managed_handle: true` routes the
    // shim generator around `catch_unwind`'s `Result<T, _>` payload (see that field's doc comment) —
    // the *seam* type here is the raw handle, which is exactly as GC-ref-bearing as `TaskT<T>`.
    if is_task_type(ty) {
        return Ok(Marshal {
            seam_ty: quote! { ::mycorrhiza::System::Threading::Tasks::Task },
            to_rust: None,
            from_rust: Some(Box::new(|id| quote! { #id.raw() })),
            returns_managed_handle: true,
        });
    }

    if let Some(name) = simple_path_ident(ty) {
        if name == "String" {
            return Ok(Marshal {
                seam_ty: quote! { ::mycorrhiza::system::MString },
                to_rust: None,
                from_rust: Some(Box::new(|id| {
                    quote! { ::mycorrhiza::system::DotNetString::from(#id.as_str()).handle() }
                })),
                returns_managed_handle: false,
            });
        }
        if is_passthrough_primitive(&name) {
            return Ok(Marshal {
                seam_ty: quote! { #ty },
                to_rust: None,
                from_rust: None,
                returns_managed_handle: false,
            });
        }
        return Err(format!(
            "#[dotnet_export]: unsupported return type `{name}`. Supported: the integer/float \
             primitives, `bool`, `&str`, `String`, `()`, `mycorrhiza::task::Task`, \
             `mycorrhiza::task::TaskT<T>`, and `Vec<T>` of a passthrough primitive `T`."
        ));
    }

    // `mycorrhiza::task::TaskT<T>` — the generic, result-bearing managed `Task<T>` handle
    // (`RustcCLRInteropManagedGeneric`). Its layout is a single `object_ref: usize` plus a
    // zero-sized `PhantomData<T>` — identical regardless of `T` — so, like the non-generic `Task`
    // above, it is already FFI-safe across the seam and passes through unchanged for ANY `T`; the
    // element type never affects the handle's own layout. The fn must construct the `TaskT<T>`
    // itself (typically via `mycorrhiza::task::future_to_task`) — `async fn` sugar stays rejected.
    // Also `returns_managed_handle: true` — same `catch_unwind` hazard as non-generic `Task` above.
    if task_t_inner(ty).is_some() {
        return Ok(Marshal {
            seam_ty: quote! { #ty },
            to_rust: None,
            from_rust: None,
            returns_managed_handle: true,
        });
    }

    // `Vec<T>` of a passthrough primitive `T` -> `RustVec<T>` at the seam (a `usize` opaque handle
    // into a size-erased Rust-owned buffer). Requires the consuming crate to have called
    // `mycorrhiza::export_rust_containers!()` once at its crate root (same requirement as any other
    // `RustVec<T>` consumer) so the `rcl_vec_*` core functions exist in this crate to call into.
    // Only primitive `T` is supported for this first slice (skips `String`/managed-handle elements —
    // narrower `RustBoxVec`/GCHandle-boxed marshalling is a separate follow-up, not attempted here).
    if let Some((name, elem_ty)) = single_generic_arg(ty) {
        if name == "Vec" {
            if let Some(elem_name) = simple_path_ident(elem_ty) {
                if is_passthrough_primitive(&elem_name) {
                    let elem_ty = elem_ty.clone();
                    return Ok(Marshal {
                        seam_ty: quote! { ::core::primitive::usize },
                        to_rust: None,
                        from_rust: Some(Box::new(move |id| {
                            quote! {
                                {
                                    let __elems: ::std::vec::Vec<#elem_ty> = #id;
                                    let __handle: ::core::primitive::usize =
                                        crate::rcl_vec_new(::core::mem::size_of::<#elem_ty>());
                                    for __elem in __elems.iter() {
                                        unsafe {
                                            crate::rcl_vec_push(
                                                __handle,
                                                (__elem as *const #elem_ty) as *const u8,
                                            );
                                        }
                                    }
                                    __handle
                                }
                            }
                        })),
                        returns_managed_handle: false,
                    });
                }
                return Err(format!(
                    "#[dotnet_export]: unsupported `Vec<{elem_name}>` return element type. Only the \
                     integer/float primitives and `bool` are supported as `Vec<T>` elements today \
                     (this marshals to a `RustVec<T>`, whose C# side is `T : unmanaged`)."
                ));
            }
            return Err(format!(
                "#[dotnet_export]: unsupported `Vec<{}>` return element type. Only the integer/float \
                 primitives and `bool` are supported as `Vec<T>` elements today.",
                quote! { #elem_ty }
            ));
        }
    }

    Err(format!(
        "#[dotnet_export]: unsupported return type `{}`. Supported: the integer/float primitives, \
         `bool`, `&str`, `String`, `()`, `mycorrhiza::task::Task`, `mycorrhiza::task::TaskT<T>`, and \
         `Vec<T>` of a passthrough primitive `T`.",
        quote! { #ty }
    ))
}

/// Scrape `#[doc = "..."]` attrs off an `ItemFn` (i.e. `/// ...` doc comments, which desugar to
/// `#[doc = "..."]` before the proc-macro ever sees them) and join them into one doc-comment body,
/// trimming the single leading space rustdoc conventionally inserts after `///`.
fn scrape_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(MetaNameValue {
            value: syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }),
            ..
        }) = &attr.meta
        {
            let line = s.value();
            lines.push(line.strip_prefix(' ').unwrap_or(&line).to_string());
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Map a `#[dotnet_export]`-supported Rust parameter/return type to the exact CLR metadata type
/// name ECMA-334 member-ID strings use, or `None` if `ty` isn't one of the types that surface
/// participates in (return `None` rather than guess — an XML-doc entry with a wrong parameter type
/// silently fails to match at doc-lookup time, which is worse than skipping it).
///
/// `&str`/`String` are supported params of `dotnet_export` (marshalled to `MString`, which is a
/// managed handle to a real `System.String`); everything else in the supported surface is one of
/// the passthrough primitives, each of which XML-doc member-ID syntax spells with its full CLR
/// name, not its C#/Rust keyword (`System.Int32`, not `int`/`i32`).
fn clr_member_id_type_name(ty: &Type) -> Option<&'static str> {
    if let Type::Reference(r) = ty {
        if r.mutability.is_none() {
            if let Type::Path(tp) = &*r.elem {
                if tp.qself.is_none() && tp.path.is_ident("str") {
                    return Some("System.String");
                }
            }
        }
        return None;
    }
    let name = simple_path_ident(ty)?;
    Some(match name.as_str() {
        "String" => "System.String",
        "bool" => "System.Boolean",
        "i8" => "System.SByte",
        "i16" => "System.Int16",
        "i32" => "System.Int32",
        "i64" => "System.Int64",
        "i128" => "System.Int128",
        "u8" => "System.Byte",
        "u16" => "System.UInt16",
        "u32" => "System.UInt32",
        "u64" => "System.UInt64",
        "u128" => "System.UInt128",
        "isize" => "System.IntPtr",
        "usize" => "System.UIntPtr",
        "f32" => "System.Single",
        "f64" => "System.Double",
        _ => return None,
    })
}

/// Append one XML-doc entry for a `#[dotnet_export]`'d fn to a per-crate sidecar-doc scratch file,
/// so `cargo-dotnet`'s packaging step can later assemble the standard ECMA-334 `<AssemblyName>.xml`
/// doc file next to the built DLL. See `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md` ("Tier C research
/// findings", item 4) for the design this implements.
///
/// The entry is keyed by the exact ECMA-334 member-ID this fn will resolve to at the C# call site:
/// `M:MainModule.<fn_name>(<ParamType1>,<ParamType2>,...)` — `#[dotnet_export]`'d free functions are
/// always emitted as public static methods directly on the assembly's `MainModule` class (see the
/// `dotnet_export` doc comment above), with NO namespace/enclosing-type nesting, so this qualified
/// name is exact for the whole supported parameter/return surface today (the seam only marshals
/// short, non-generic, non-nested types) and needs no FNV-hash-shortening logic — that mechanism
/// (`cilly::dotnet_class_name`) only triggers for *type* names exceeding .NET's 1023-char metadata
/// limit, which a hand-written fn name can't realistically hit. If `MainModule` is ever partitioned
/// across per-module classes for size (see `cilly/src/ir/il_exporter/partition.rs`), or exported fns
/// gain a namespace/nesting option, this qualified-name derivation will need to track that — noted
/// here as the known limitation for this first slice.
///
/// One line of newline-delimited JSON per entry (robust against arbitrary doc-comment text, e.g.
/// embedded quotes/newlines) is appended to
/// `<CARGO_MANIFEST_DIR>/target/dotnet_xmldoc/<crate_name>.xmldoc.jsonl`. The proc-macro runs once
/// per fn at the consumer's compile time, so appending (not overwriting) is required; cargo-dotnet's
/// build stage clears stale entries by deleting the file up front (see `xmldoc::collect`).
fn emit_xmldoc_entry(fn_name: &syn::Ident, params: &[String], doc: &str) {
    let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") else { return };
    let Ok(crate_name) = std::env::var("CARGO_PKG_NAME") else { return };
    let member_id = format!("M:MainModule.{}({})", fn_name, params.join(","));

    let dir = std::path::Path::new(&manifest_dir).join("target").join("dotnet_xmldoc");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file_path = dir.join(format!("{crate_name}.xmldoc.jsonl"));
    let entry = serde_json_line(&member_id, doc);
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file_path) {
        let _ = writeln!(f, "{entry}");
    }
}

/// Hand-rolled minimal JSON-object-line encoder (`{"member":"...","summary":"..."}`) so this crate
/// doesn't need a `serde_json` dependency just for two escaped string fields.
fn serde_json_line(member: &str, summary: &str) -> String {
    fn escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }
    format!("{{\"member\":\"{}\",\"summary\":\"{}\"}}", escape(member), escape(summary))
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
    let mut doc_param_types = Vec::new(); // CLR type names, for the XML-doc member-ID (see below).
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
        if let Some(clr_name) = clr_member_id_type_name(&pat_ty.ty) {
            doc_param_types.push(clr_name.to_string());
        }
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

    // Scrape `#[doc]` attrs and, if present, record an XML-doc sidecar entry keyed by the exact
    // ECMA-334 member-ID this fn resolves to as a `MainModule` static method. Best-effort: any
    // failure to write is silently ignored (never fails the actual compile over doc generation).
    if let Some(doc) = scrape_doc_comment(&func.attrs) {
        emit_xmldoc_entry(&fn_name, &doc_param_types, &doc);
    }

    // Marshal the return type. `returns_managed_handle` (Task/TaskT<T>) picks a different
    // `catch_unwind` shape below — see that field's doc comment on `Marshal` for why.
    let (seam_ret, ret_expr, ret_ty_for_slot, returns_managed_handle) = match &sig.output {
        ReturnType::Default => (quote! {}, quote! { __ret }, None, false), // `-> ()`; identity.
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
            (
                quote! { -> #seam_ty },
                expr,
                Some((**ty).clone()),
                marshal.returns_managed_handle,
            )
        }
    };

    let call = quote! { super::#fn_name(#(#call_args),*) };

    // The shared panic-message-extraction arm both `catch_unwind` shapes below raise through.
    let throw_arm = quote! {
        let __msg: ::std::string::String =
            if let ::std::option::Option::Some(__s) =
                __panic_payload.downcast_ref::<&'static str>()
            {
                (*__s).to_string()
            } else if let ::std::option::Option::Some(__s) =
                __panic_payload.downcast_ref::<::std::string::String>()
            {
                __s.clone()
            } else {
                ::std::string::String::from("Rust panic (no message available)")
            };
        ::mycorrhiza::error::throw_message(&__msg)
    };

    // The inner call is wrapped in `catch_unwind`: a plain `extern "C"` is a `nounwind` ABI
    // boundary (the same rule as native Rust FFI — unwinding across it is UB), so an uncaught
    // panic would otherwise reach the true edge and the runtime hard-aborts the whole process
    // (`Environment.FailFast`) with no chance for the C# caller to recover. Catching it *inside*
    // the shim and re-raising as a genuine managed `System.Exception`
    // (`::mycorrhiza::error::throw_message`) turns an unrecoverable process abort into an ordinary
    // `catch`-able error.
    //
    // Two shapes, chosen by `returns_managed_handle`:
    //
    // * The common case: `catch_unwind`'s `Result<RetTy, _>` carries the value straight through the
    //   `Ok` arm. Fine for primitives/`MString`/`usize` — none of those are managed object
    //   references, so `Result<RetTy, Box<dyn Any + Send>>`'s (correctly) overlapping-variant
    //   layout never has to hold a GC reference.
    // * `returns_managed_handle` (Task/TaskT<T>): `Result<RetTy, _>` would instead place a managed
    //   reference directly in that overlapping storage, which `cilly`'s `ClassDef::layout_check`
    //   correctly refuses (a real GC-soundness rule, not a bug to route around) — surfacing as a
    //   `ManagedRefInOverlapingField` compiler panic. So this arm keeps `catch_unwind`'s payload a
    //   plain `()`: the closure writes the call's result through a raw pointer into an
    //   already-allocated `MaybeUninit<RetTy>` local (never itself living inside the `Result`), and
    //   the `Ok(())` arm reads it back out afterward. `RetTy` is `Copy` here (Task/TaskT<T> are
    //   `#[repr(C)]`/`Copy` handles), so writing through the raw pointer and reading the
    //   `MaybeUninit` back out is sound.
    let body = if returns_managed_handle {
        let ret_ty = ret_ty_for_slot.expect("returns_managed_handle implies a return type");
        quote! {
            let mut __slot: ::core::mem::MaybeUninit<#ret_ty> = ::core::mem::MaybeUninit::uninit();
            let __slot_ptr: *mut #ret_ty = __slot.as_mut_ptr();
            match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                let __v = #call;
                // SAFETY: `__slot_ptr` points at `__slot`'s own storage, valid for the duration of
                // this closure call; written at most once, before `catch_unwind` returns, and read
                // back only from the `Ok` arm below (i.e. only once initialized).
                unsafe { __slot_ptr.write(__v) };
            })) {
                ::std::result::Result::Ok(()) => {
                    // SAFETY: the closure above wrote a valid `#ret_ty` on its only non-unwinding
                    // path, which is exactly the path that reaches this arm.
                    let __ret: #ret_ty = unsafe { __slot.assume_init() };
                    #ret_expr
                }
                ::std::result::Result::Err(__panic_payload) => {
                    #throw_arm
                }
            }
        }
    } else {
        quote! {
            match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| #call)) {
                ::std::result::Result::Ok(__ret) => {
                    #ret_expr
                }
                ::std::result::Result::Err(__panic_payload) => {
                    #throw_arm
                }
            }
        }
    };

    let expanded = quote! {
        // The user's function, verbatim — still callable from Rust with its idiomatic signature.
        #func

        // The generated seam shim. `#[no_mangle]` gives it the flat symbol `#fn_name`, which the
        // backend marks `Access::Extern` (a DCE root) and the exporter emits as a public static
        // method on the assembly's `MainModule` — so C# calls it as `MainModule::#fn_name`. See
        // `body`'s construction above for the two `catch_unwind` shapes and why a second one is
        // needed.
        //
        // The shim itself is declared `extern "C-unwind"`, not plain `extern "C"`. This is NOT
        // optional: `throw_message`'s `ExceptionDispatchInfo::Throw()` call performs a genuine CIL
        // `throw`, which this backend must (correctly) model as an operation that can unwind — and
        // that call sits in the `Err` arm, OUTSIDE the `catch_unwind` closure (only the call/write is
        // protected). A call that can unwind, sitting directly in a `nounwind extern "C"` function
        // body with nothing downstream to catch it, is legitimately flagged by rustc's MIR builder as
        // crossing a nounwind ABI boundary — the exact same analysis that would fire for a second bare
        // `panic!()` placed right after `catch_unwind` in ordinary native Rust — and lowered to the
        // same hard-abort landing pad as any other escaping unwind, silently defeating the whole
        // point of this wrapper (verified empirically: with plain `extern "C"` the process still
        // `FailFast`s on the managed throw, exactly as it did with no `catch_unwind` at all).
        // `extern "C-unwind"` tells rustc this function's *outer* boundary itself may unwind, so the
        // managed-throw call in the `Err` arm is no longer treated as escaping a nounwind frame; the
        // backend's C-unwind lowering is what lets a genuine .NET exception (not a Rust unwind) leave
        // the shim and reach the C# caller's `try`/`catch`, which is exactly the shape already proven
        // out by `mycorrhiza::error`'s `extern "C-unwind"` try/catch trampolines. The non-panicking
        // path is untouched: `catch_unwind` has no overhead on the `Ok` arm beyond the landing-pad
        // setup the panic runtime already needs, and `extern "C-unwind"` vs `extern "C"` makes no
        // difference to a call that never unwinds.
        #[doc(hidden)]
        #[allow(non_snake_case, unused_imports, clippy::useless_conversion)]
        mod #shim_mod {
            use super::*;
            #[no_mangle]
            pub extern "C-unwind" fn #fn_name(#(#seam_params),*) #seam_ret {
                #(#pre_call)*
                #body
            }
        }
    };
    expanded.into()
}

// ============================================================================
// #[dotnet_entity] — retype an entity struct's fields to `mycorrhiza::linq::Field<Root, Val>` markers
// and generate an explicit `::new()` constructor (plus a `Default` impl delegating to it), so
// predicate-building reads as real Rust field access on a value the CALLER constructs themselves
// (`let person = Person::new(); person.age.ge(18)`) instead of a `::`-qualified associated-const path
// (`Person::AGE.ge(18)`) or a hidden, auto-generated singleton the caller never wrote a binding for.
// ============================================================================

/// snake_case -> PascalCase, e.g. `is_active` -> `IsActive`, `age` -> `Age`. This is the DEFAULT .NET
/// property-name convention a `#[dotnet_entity]` field maps to; `#[dotnet(rename = "...")]` on a field
/// overrides it for cases where the convention doesn't match.
fn to_pascal_case(snake: &str) -> String {
    snake
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// `#[dotnet_entity]` — by default resolves the .NET **class name** to the Rust struct's own name, and
/// the .NET **namespace**/**assembly** to the crate-level default declared once via
/// [`mycorrhiza::linq::dotnet_namespace!`](../mycorrhiza/linq/macro.dotnet_namespace.html) (namespace
/// and assembly default to the SAME value — the common small-project convention that a project's
/// namespace and its assembly name are identical). Each piece has an independent escape-hatch attribute:
///
/// - `#[dotnet(namespace = "...")]` — override just the namespace.
/// - `#[dotnet(assembly = "...")]` — override just the assembly.
/// - `#[dotnet(name = "...")]` — override just the class name (still uses the crate-level default for
///   namespace/assembly unless those are ALSO given).
///
/// **This is an attribute macro, not a `derive`** — deliberately, because it needs to do something a
/// `derive` structurally cannot: RETYPE the struct's own fields. Every named field `f: OrigTy` becomes
/// `f: ::mycorrhiza::linq::Field<Self, OrigTy>`, and the macro additionally emits an explicit `impl
/// Person { pub const fn new() -> Self { .. } }` constructor plus `impl Default for Person { fn
/// default() -> Self { Self::new() } }`, with each field initialized via `Field::new(..)` (which stays a
/// `const fn`, so `Person::new()` itself is usable in const contexts, buildable at zero runtime cost).
/// This is what lets a caller write genuine Rust field-access syntax to build a predicate —
/// `person.age.ge(min_age) & person.name.contains(name_contains)` — with real dot-chains, not a
/// `::`-qualified associated-const path, while keeping the binding fully explicit and visible in the
/// caller's own code:
///
/// ```ignore
/// let person = Person::new(); // or Person::default()
/// let pred = person.age.ge(18) & person.name.contains("a");
/// ```
///
/// (An earlier version of this API generated `Field` values as associated consts — `Person::AGE.ge(..)`
/// — forcing `::` path syntax for every predicate; user feedback replaced that with a hidden,
/// auto-generated singleton `const` instance (`person: Person`) that appeared in scope with no visible
/// declaration. Further feedback called THAT "too magical — `person` comes out of nowhere" and asked for
/// an explicit constructor instead, which is what this version generates: no more hidden singleton, no
/// more `#[allow(non_upper_case_globals)]` workaround — the caller writes `Person::new()` themselves.)
///
/// The .NET property name each retyped field's `Field` carries defaults to the PascalCase conversion of
/// the Rust field name (`is_active` -> `IsActive`); override per-field with `#[dotnet(rename = "...")]`
/// when the convention doesn't match.
///
/// ```ignore
/// // Once, near the crate root:
/// mycorrhiza::linq::dotnet_namespace!("LinqDemo");
///
/// #[dotnet_entity]
/// struct Person { id: i32, name: String, age: i32, is_active: bool }
///
/// // Generates (roughly):
/// struct Person {
///     pub id: ::mycorrhiza::linq::Field<Person, i32>,
///     pub name: ::mycorrhiza::linq::Field<Person, String>,
///     pub age: ::mycorrhiza::linq::Field<Person, i32>,
///     pub is_active: ::mycorrhiza::linq::Field<Person, bool>,
/// }
/// impl Person {
///     pub const fn new() -> Self {
///         Person {
///             id: ::mycorrhiza::linq::Field::new(crate::__MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT, "Person", crate::__MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT, "Id"),
///             // ...name, age, is_active likewise.
///         }
///     }
/// }
/// impl Default for Person {
///     fn default() -> Self { Self::new() }
/// }
///
/// // Usage — explicit construction, then real field access, no `::` path:
/// let person = Person::new();
/// let pred = person.age.ge(18) & person.name.contains("a");
/// ```
///
/// **Important**: this macro does NOT need to see the `dotnet_namespace!` invocation at expansion time
/// — it only emits a reference to the FIXED name `crate::__MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT`
/// (whenever no `namespace`/`assembly` override is given), and ordinary Rust name resolution finds it at
/// the consuming crate's normal compile time, regardless of expansion order. If a struct provides
/// neither an override nor a crate-level `dotnet_namespace!` declaration, the generated code fails with
/// a plain "cannot find const `__MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT` in this scope" — that is the
/// correct, honest failure mode (no default to fall back to).
///
/// This is pure compile-time constant generation — unlike `#[dotnet_class]`/`#[dotnet_methods]`/
/// `#[dotnet_export]`, it does NOT touch the backend's comptime interpreter (no `rustc_codegen_clr_*`
/// intrinsic calls, no `#[used]` DCE anchor): the macro only ever expands to the retyped struct plus the
/// `new`/`Default` impls, which is why it needs none of that machinery.
#[proc_macro_attribute]
pub fn dotnet_entity(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let struct_name = &input.ident;
    let span = struct_name.span();

    // ---- struct-level `#[dotnet(namespace = "...", assembly = "...", name = "...")]` ----
    // Each is independently optional; an absent one falls back to a `TokenStream` that references the
    // crate-level default const (namespace/assembly) or the struct's own Rust name (class name).
    let mut namespace_override: Option<String> = None;
    let mut assembly_override: Option<String> = None;
    let mut name_override: Option<String> = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("dotnet") {
            continue;
        }
        let syn::Meta::List(list) = &attr.meta else {
            return syn::Error::new(
                attr.span(),
                "#[dotnet_entity]: expected `#[dotnet(namespace = \"...\", assembly = \"...\", name = \"...\")]`",
            )
            .to_compile_error()
            .into();
        };
        let parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
        let metas = match syn::parse::Parser::parse(parser, list.tokens.clone().into()) {
            Ok(m) => m,
            Err(e) => return e.to_compile_error().into(),
        };
        for m in metas {
            if m.path.is_ident("type_name") {
                return syn::Error::new(
                    m.path.span(),
                    "#[dotnet_entity]: `type_name` was replaced by separate `namespace`/`assembly`/\
                     `name` attributes (each independently overridable; namespace/assembly default to \
                     the crate-level `mycorrhiza::linq::dotnet_namespace!` declaration)",
                )
                .to_compile_error()
                .into();
            }
            if !m.path.is_ident("namespace") && !m.path.is_ident("assembly") && !m.path.is_ident("name")
            {
                let path = &m.path;
                return syn::Error::new(
                    m.path.span(),
                    format!(
                        "#[dotnet_entity]: unknown attribute key `{}`; expected one of `namespace`, \
                         `assembly`, `name`",
                        quote! { #path }
                    ),
                )
                .to_compile_error()
                .into();
            }
            let s = match str_lit_value(&m.value) {
                Ok(s) => s,
                Err(e) => return e.to_compile_error().into(),
            };
            if m.path.is_ident("namespace") {
                namespace_override = Some(s);
            } else if m.path.is_ident("assembly") {
                assembly_override = Some(s);
            } else if m.path.is_ident("name") {
                name_override = Some(s);
            }
        }
    }

    // Namespace/assembly: an explicit override is a plain string literal; otherwise reference the
    // crate-level default const BY NAME (resolved at ordinary Rust compile time, not here — see the
    // macro's doc comment for why this needs no macro-expansion-order coordination).
    let namespace_expr = match &namespace_override {
        Some(s) => {
            let lit = LitStr::new(s, span);
            quote! { #lit }
        }
        None => quote! { crate::__MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT },
    };
    let assembly_expr = match &assembly_override {
        Some(s) => {
            let lit = LitStr::new(s, span);
            quote! { #lit }
        }
        None => quote! { crate::__MYCORRHIZA_DOTNET_NAMESPACE_DEFAULT },
    };
    // Class name: an explicit override, or the struct's own Rust identifier, verbatim.
    let class_name = name_override.unwrap_or_else(|| struct_name.to_string());
    let class_name_lit = LitStr::new(&class_name, span);

    // ---- named fields only ----
    let fields = match &input.fields {
        syn::Fields::Named(named) => &named.named,
        _ => {
            return syn::Error::new(
                span,
                "#[dotnet_entity]: tuple structs and unit structs are not supported; use named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    // Two parallel outputs per field: the RETYPED field declaration (`pub name: Field<Self, OrigTy>`,
    // replacing the struct's own field), and the constructor's initializer expression for that field
    // (`name: Field::new(...)`). Built together since both need the same per-field namespace/prop-name
    // resolution.
    let mut retyped_fields = Vec::new();
    let mut ctor_inits = Vec::new();
    for f in fields {
        let Some(fident) = &f.ident else {
            continue;
        };
        let fname = fident.to_string();
        let fspan = fident.span();

        // Per-field `#[dotnet(rename = "...")]` escape hatch, defaulting to PascalCase(fname).
        let mut prop_name = to_pascal_case(&fname);
        for attr in &f.attrs {
            if !attr.path().is_ident("dotnet") {
                continue;
            }
            let syn::Meta::List(list) = &attr.meta else {
                return syn::Error::new(
                    attr.span(),
                    "#[dotnet_entity]: expected `#[dotnet(rename = \"...\")]`",
                )
                .to_compile_error()
                .into();
            };
            let parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
            let metas = match syn::parse::Parser::parse(parser, list.tokens.clone().into()) {
                Ok(m) => m,
                Err(e) => return e.to_compile_error().into(),
            };
            for m in metas {
                if !m.path.is_ident("rename") {
                    let path = &m.path;
                    return syn::Error::new(
                        m.path.span(),
                        format!(
                            "#[dotnet_entity]: unknown field attribute key `{}`; expected `rename`",
                            quote! { #path }
                        ),
                    )
                    .to_compile_error()
                    .into();
                }
                match str_lit_value(&m.value) {
                    Ok(s) => prop_name = s,
                    Err(e) => return e.to_compile_error().into(),
                }
            }
        }

        let prop_name_lit = LitStr::new(&prop_name, fspan);
        let fty = &f.ty;
        let vis = &f.vis;
        retyped_fields.push(quote! {
            #vis #fident: ::mycorrhiza::linq::Field<#struct_name, #fty>
        });
        ctor_inits.push(quote! {
            #fident: ::mycorrhiza::linq::Field::new(#namespace_expr, #class_name_lit, #assembly_expr, #prop_name_lit)
        });
    }

    // Preserve the original struct's own attributes (other than the `#[dotnet(...)]` ones this macro
    // itself consumes) and visibility/generics, retyping only the fields.
    let other_attrs: Vec<_> = input
        .attrs
        .iter()
        .filter(|a| !a.path().is_ident("dotnet"))
        .collect();
    let vis = &input.vis;
    let generics = &input.generics;

    let expanded = quote! {
        #(#other_attrs)*
        #vis struct #struct_name #generics {
            #(#retyped_fields),*
        }

        impl #struct_name {
            /// Construct this entity's field descriptors — a real, visible, explicitly-called
            /// constructor (NOT a hidden singleton): `let #struct_name = ..; person.field.method(..)`.
            /// Stays a `const fn` (`Field::new` already is one) so it costs nothing and remains usable
            /// in const contexts. Generated by `#[dotnet_entity]`; see its doc comment for the full
            /// design.
            #[must_use]
            #vis const fn new() -> Self {
                #struct_name {
                    #(#ctor_inits),*
                }
            }
        }

        impl ::std::default::Default for #struct_name {
            /// Delegates to [`#struct_name::new`] — the ecosystem-standard spelling for callers who
            /// prefer `Person::default()` over `Person::new()`. Not itself `const` (`Default::default`
            /// isn't const in stable Rust); use `::new()` directly in a const context.
            fn default() -> Self {
                Self::new()
            }
        }
    };
    expanded.into()
}
