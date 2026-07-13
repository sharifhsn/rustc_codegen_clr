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
    Expr, FnArg, ImplItem, ItemEnum, ItemFn, ItemImpl, ItemStruct, ItemTrait, Lit, LitBool, LitStr,
    MetaNameValue, ReturnType, Token, TraitItem, Type, parse_macro_input, punctuated::Punctuated,
    spanned::Spanned,
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
            return (
                spec[..open].to_string(),
                Some(inner[open + 1..].to_string()),
            );
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

// ============================================================================
// `extends = "..."` — allowlisted base classes for `#[dotnet_class]`.
// ============================================================================

/// Base classes proven safe for `#[dotnet_class(extends = "...")]`, by real end-to-end test (compiles,
/// loads, runs, no crash) — not by inspection. Unlike the `attr(...)` denylist above, this is an
/// ALLOWlist, and deliberately so: a malformed custom-attribute blob fails safely (a catchable
/// reflection exception), but subclassing an arbitrary CLR base class does not. CoreCLR's loader has
/// private expectations about a base class's field layout, vtable slot count, and constructor
/// contract that a Rust caller has no way to inspect or prove correct from the type signature alone —
/// getting it wrong crashes the *loader*, not just the call (this is exactly what T0-2 root-caused:
/// see `cilly::ir::class::ClassDef::set_extends`'s doc and `cargo_tests/cd_bgservice`'s investigation).
/// So each entry here was proven individually, the way `BackgroundService` was proven in
/// `cargo_tests/cd_bgservice/rustlib_bgtest` (subclass + override `ExecuteAsync` + run under a real
/// `IHostBuilder` lifecycle, no crash) before being added. `System.Object` is always safe (it's the
/// universal base every class already implicitly extends) and isn't optional.
///
/// To subclass anything else without adding it here first, set `ALLOW_UNVERIFIED_BASE=1` in the
/// environment at Rust compile time — an explicit, loud, debug-only opt-out (matching this project's
/// `config!`-macro env-var culture: `OPTIMIZE_CIL`, `ALLOW_MISCOMPILATIONS`, etc.), not a public
/// unsafe API, since the underlying invariant genuinely cannot be discharged by the caller.
const EXTENDS_ALLOWLIST: &[&str] = &[
    "[System.Runtime]System.Object",
    "[Microsoft.Extensions.Hosting.Abstractions]Microsoft.Extensions.Hosting.BackgroundService",
];

/// `Some(reason)` if `base` isn't allowlisted and `ALLOW_UNVERIFIED_BASE` isn't set; `None` if the
/// extends is OK to proceed (either allowlisted, or the escape hatch is active).
fn unverified_base_reason(base: &str) -> Option<String> {
    if EXTENDS_ALLOWLIST.contains(&base) {
        return None;
    }
    if std::env::var("ALLOW_UNVERIFIED_BASE").is_ok() {
        return None;
    }
    Some(format!(
        "#[dotnet_class]: `{base}` is not on the proven-safe `extends` allowlist ({EXTENDS_ALLOWLIST:?}). \
         Subclassing an arbitrary CLR base class risks a CoreCLR loader crash (private layout/vtable \
         expectations the backend cannot verify from the Rust side) rather than a catchable error — see \
         `EXTENDS_ALLOWLIST`'s doc in dotnet_macros. Either prove this base class safe end-to-end and add \
         it to the allowlist, or set `ALLOW_UNVERIFIED_BASE=1` to bypass this check at your own risk."
    ))
}

// ============================================================================
// `attr(...)` — general `#[dotnet_class(attr(...))]` custom-attribute surface.
// ============================================================================

/// The same denylist `src/comptime.rs`'s `denylisted_attr_reason` carries (kept as an independent
/// copy — this crate has no dependency on the backend — so a bad attribute type is rejected here,
/// at ordinary Rust compile time, with a real `syn::Error`, rather than surfacing only as a
/// backend panic during codegen). See that fn's doc for why this is a NAMESPACE-blanket deny
/// rather than an enumerated list: `System.Runtime.CompilerServices`/`System.Runtime.
/// InteropServices` is exactly where CoreCLR's own layout/marshalling/ABI-affecting attributes
/// live, so a BCL attribute added to either namespace later is denied by default.
const ATTR_DENYLIST_NAMESPACES: &[&str] = &[
    "System.Runtime.CompilerServices.",
    "System.Runtime.InteropServices.",
];

fn denylisted_attr_reason(full_type_name: &str) -> Option<String> {
    for ns in ATTR_DENYLIST_NAMESPACES {
        if full_type_name.starts_with(ns) {
            return Some(format!(
                "#[dotnet_class]: `{full_type_name}` cannot be attached via `attr(...)` — it is \
                 in `{ns}`, a namespace CoreCLR's own loader/JIT treats as runtime-semantic \
                 (layout- or calling-convention-affecting, e.g. InlineArrayAttribute/\
                 UnmanagedCallersOnlyAttribute/StructLayoutAttribute). This safety check exists \
                 because a malformed attribute TYPE reference fails safely (a catchable \
                 reflection exception), but a runtime-semantic attribute silently changing layout \
                 the backend depends on would not."
            ));
        }
    }
    None
}

/// One literal constructor/named argument inside an `attr(...)` entry — `args(...)`/`props(...)`
/// accept exactly these four Rust literal kinds (a plain string, bool, `i32`-range integer, or
/// `i64`-range integer), matching the shapes `cilly::class::CustomAttrArg` can express. No other
/// literal kind (float, char, byte-string, …) is accepted — see that enum's doc for the full
/// "well-formed by construction" rationale this mirrors at the macro-syntax level.
enum AttrArgLit {
    Str(String),
    Bool(bool),
    I32(i32),
    I64(i64),
}
impl AttrArgLit {
    /// `s:`/`b:`/`i:`/`l:` + the literal's text — the wire format `src/comptime.rs`'s
    /// `decode_custom_attr_spec` is the authoritative decoder for. Panics (via `assert!`, at
    /// macro-expansion time — never reachable with user-controlled content because the literal
    /// value itself, not user runtime input, is what's encoded) if the text contains one of the
    /// reserved delimiter control characters; see `validate_no_delims`, called before this at
    /// parse time, for the real (spanned) diagnostic.
    fn encode(&self) -> String {
        match self {
            AttrArgLit::Str(s) => format!("s:{s}"),
            AttrArgLit::Bool(b) => format!("b:{b}"),
            AttrArgLit::I32(i) => format!("i:{i}"),
            AttrArgLit::I64(i) => format!("l:{i}"),
        }
    }
}

/// Reserved wire-format delimiters (see `AttrSpec::encode`'s doc) — a string literal (the only
/// arg kind that can contain arbitrary text) containing one of these is rejected at parse time
/// with a real diagnostic, rather than silently desyncing the packed spec's fields.
const RESERVED_DELIMS: [char; 3] = ['\u{1E}', '\u{1D}', '\u{1C}'];

fn validate_no_delims(s: &str, span: proc_macro2::Span) -> syn::Result<()> {
    if s.contains(RESERVED_DELIMS) {
        return Err(syn::Error::new(
            span,
            "string literal contains a reserved control character (U+001C/U+001D/U+001E) used \
             internally to encode `attr(...)` — this is vanishingly unlikely to be intentional; \
             remove it",
        ));
    }
    Ok(())
}

fn lit_to_arg(lit: &syn::Lit) -> syn::Result<AttrArgLit> {
    match lit {
        syn::Lit::Str(s) => {
            validate_no_delims(&s.value(), s.span())?;
            Ok(AttrArgLit::Str(s.value()))
        }
        syn::Lit::Bool(b) => Ok(AttrArgLit::Bool(b.value)),
        syn::Lit::Int(i) => {
            if let Ok(v) = i.base10_parse::<i32>() {
                Ok(AttrArgLit::I32(v))
            } else {
                i.base10_parse::<i64>().map(AttrArgLit::I64)
            }
        }
        other => Err(syn::Error::new(
            other.span(),
            "expected a string, bool, or integer literal",
        )),
    }
}

fn expr_to_arg(expr: &syn::Expr) -> syn::Result<AttrArgLit> {
    if let syn::Expr::Lit(syn::ExprLit { lit, .. }) = expr {
        lit_to_arg(lit)
    } else {
        Err(syn::Error::new(
            expr.span(),
            "expected a string, bool, or integer literal",
        ))
    }
}

/// One `attr("[Assembly]Namespace.AttrType", args(1, "x", true), props(Name = "Foo", Order = 1))`
/// entry: the attribute's `.NET` type reference (same `"[Asm]Ns.Type"` spelling as `extends`/
/// `implements`), its positional constructor arguments in `args(...)` (empty/omitted for a
/// no-arg ctor), and its named PROPERTY arguments in `props(...)` (field-targeted named args are
/// not supported — see `cilly::class::CustomAttrDef`'s doc for why: virtually every real
/// attribute's named-arg surface is settable properties, not public fields).
struct AttrSpec {
    type_name: String,
    ctor_args: Vec<AttrArgLit>,
    named_args: Vec<(String, AttrArgLit)>,
}
impl AttrSpec {
    /// Packs this spec into the single `&'static str` `rustc_codegen_clr_add_custom_attr::<SPEC>`
    /// carries: `<asm>\x1E<type>\x1E<ctor_args>\x1E<named_args>`, where `ctor_args` is a `\x1D`-
    /// joined list of `AttrArgLit::encode()` outputs and `named_args` is a `\x1D`-joined list of
    /// `<name>\x1C<AttrArgLit::encode()>` entries. `\x1C`/`\x1D`/`\x1E` are ASCII-reserved
    /// (Unit/Group/Record Separator) control characters chosen specifically because ordinary
    /// source text can't contain them — `validate_no_delims` rejects any string literal that
    /// does, so this packing can never desynchronize. `src/comptime.rs`'s `decode_custom_attr_spec`
    /// is the authoritative decoder.
    fn encode(&self) -> String {
        let (asm, type_name) = split_dotnet_ref(&self.type_name);
        let ctor = self
            .ctor_args
            .iter()
            .map(AttrArgLit::encode)
            .collect::<Vec<_>>()
            .join("\u{1D}");
        let named = self
            .named_args
            .iter()
            .map(|(name, v)| format!("{name}\u{1C}{}", v.encode()))
            .collect::<Vec<_>>()
            .join("\u{1D}");
        format!("{asm}\u{1E}{type_name}\u{1E}{ctor}\u{1E}{named}")
    }
}
impl syn::parse::Parse for AttrSpec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let type_lit: LitStr = input.parse()?;
        let type_name = type_lit.value();
        validate_dotnet_ref(&type_name, type_lit.span())?;
        validate_no_delims(&type_name, type_lit.span())?;
        let mut ctor_args = Vec::new();
        let mut named_args = Vec::new();
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break; // trailing comma
            }
            let ident: syn::Ident = input.parse()?;
            let content;
            syn::parenthesized!(content in input);
            if ident == "args" {
                let lits = Punctuated::<syn::Lit, Token![,]>::parse_terminated(&content)?;
                for l in lits {
                    ctor_args.push(lit_to_arg(&l)?);
                }
            } else if ident == "props" {
                let nvs = Punctuated::<MetaNameValue, Token![,]>::parse_terminated(&content)?;
                for nv in nvs {
                    let name = nv
                        .path
                        .get_ident()
                        .ok_or_else(|| syn::Error::new(nv.path.span(), "expected a property name"))?
                        .to_string();
                    validate_no_delims(&name, nv.path.span())?;
                    named_args.push((name, expr_to_arg(&nv.value)?));
                }
            } else {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("attr(...): unknown key `{ident}`; expected `args` or `props`"),
                ));
            }
        }
        Ok(AttrSpec {
            type_name,
            ctor_args,
            named_args,
        })
    }
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

/// One `static_field(NAME: Type)` entry inside `#[dotnet_class(static_field(...), ...)]` — a
/// genuine `.NET` `static` field (see `rustc_codegen_clr_add_static_field_def`'s doc), distinct
/// from the struct's own (instance) fields. Repeatable, one call per field.
struct StaticFieldSpec {
    name: syn::Ident,
    ty: Type,
}
impl syn::parse::Parse for StaticFieldSpec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        Ok(StaticFieldSpec { name, ty })
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
    //      field_setters = bool, properties = bool, attr(...) (repeatable) ----
    let mut extends = "[System.Runtime]System.Object".to_string();
    let mut value_type = false;
    let mut default_ctor = false;
    let mut field_setters = false;
    let mut properties = false;
    // Managed interfaces this class implements, `;`-separated in one string (usually just one), e.g.
    // `implements = "[MyLib]MyLib.IService"` or `"[A]A.I1;[B]B.I2"`. See the interface `add_*` intrinsic.
    let mut implements: Vec<String> = Vec::new();
    // General custom attributes, one `attr(...)` entry per attribute — see `AttrSpec`'s doc.
    let mut attr_specs: Vec<AttrSpec> = Vec::new();
    // Static fields, one `static_field(NAME: Type)` entry each — see `StaticFieldSpec`'s doc.
    let mut static_field_specs: Vec<StaticFieldSpec> = Vec::new();
    // `base_ctor_args(Type1, Type2, ...)` — positional types the base class's `.ctor` requires, in
    // order. See `rustc_codegen_clr_add_base_ctor_arg`'s doc for why these are types only (no
    // values): a comptime class-shape entrypoint describes static metadata, it can't carry an
    // interpretable expression for "what value to pass" — the managed caller supplies the values.
    let mut base_ctor_arg_types: Vec<Type> = Vec::new();
    if !attr.is_empty() {
        let parser = Punctuated::<syn::Meta, Token![,]>::parse_terminated;
        let metas = match syn::parse::Parser::parse(parser, attr) {
            Ok(m) => m,
            Err(e) => return e.to_compile_error().into(),
        };
        for meta in metas {
            let m = match &meta {
                syn::Meta::NameValue(nv) => nv,
                syn::Meta::List(list) if list.path.is_ident("attr") => {
                    match list.parse_args::<AttrSpec>() {
                        Ok(spec) => {
                            // Check the BARE type name (namespace + type, no `[Assembly]`
                            // prefix) — `spec.type_name` still carries the bracketed assembly,
                            // which would never match `ATTR_DENYLIST_NAMESPACES`'s
                            // `starts_with` and silently let every denylisted attribute through.
                            let (_, bare_name) = split_dotnet_ref(&spec.type_name);
                            if let Some(reason) = denylisted_attr_reason(&bare_name) {
                                return syn::Error::new(list.span(), reason)
                                    .to_compile_error()
                                    .into();
                            }
                            attr_specs.push(spec);
                        }
                        Err(e) => return e.to_compile_error().into(),
                    }
                    continue;
                }
                syn::Meta::List(list) if list.path.is_ident("static_field") => {
                    match list.parse_args::<StaticFieldSpec>() {
                        Ok(spec) => static_field_specs.push(spec),
                        Err(e) => return e.to_compile_error().into(),
                    }
                    continue;
                }
                syn::Meta::List(list) if list.path.is_ident("base_ctor_args") => {
                    match list.parse_args_with(Punctuated::<Type, Token![,]>::parse_terminated) {
                        Ok(types) => base_ctor_arg_types.extend(types),
                        Err(e) => return e.to_compile_error().into(),
                    }
                    continue;
                }
                _ => {
                    return syn::Error::new(
                        meta.span(),
                        "#[dotnet_class]: expected `key = \"...\"`, `attr(\"[Asm]Ns.Type\", \
                         args(...), props(...))`, `static_field(NAME: Type)`, or \
                         `base_ctor_args(Type, ...)`",
                    )
                    .to_compile_error()
                    .into();
                }
            };
            if m.path.is_ident("extends") {
                match str_lit_value(&m.value) {
                    Ok(s) => {
                        if let Err(e) = validate_dotnet_ref(&s, m.value.span()) {
                            return e.to_compile_error().into();
                        }
                        if let Some(reason) = unverified_base_reason(&s) {
                            return syn::Error::new(m.value.span(), reason)
                                .to_compile_error()
                                .into();
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
            } else if m.path.is_ident("properties") {
                match bool_lit_value(&m.value) {
                    Ok(v) => properties = v,
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
                         `value_type`, `default_ctor`, `field_setters`, `properties`, \
                         `implements`, `attr(...)`, `static_field(...)`",
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
    // One `add_custom_attr::<"packed spec">` per `attr(...)` entry — see `AttrSpec::encode`'s doc
    // for the packed wire format `rustc_codegen_clr_add_custom_attr` decodes.
    let attr_calls: Vec<_> = attr_specs
        .iter()
        .map(|spec| {
            let packed = LitStr::new(&spec.encode(), span);
            quote! {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_custom_attr::<#packed>(class);
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

    // One `add_static_field_def::<FieldTy, "NAME">` per `static_field(NAME: Type)` entry.
    let static_field_calls = static_field_specs.iter().map(|s| {
        let fname_lit = LitStr::new(&s.name.to_string(), s.name.span());
        let fty = &s.ty;
        quote! {
            let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_static_field_def::<#fty, #fname_lit>(class);
        }
    });

    // One `add_base_ctor_arg::<Type>` per `base_ctor_args(...)` entry, in declared order.
    let base_ctor_arg_calls = base_ctor_arg_types.iter().map(|ty| {
        quote! {
            let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_base_ctor_arg::<#ty>(class);
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
    // `properties = true`: a `get_<Field>`/`set_<Field>` accessor pair per field, linked into a
    // real `.NET` property — a SEPARATE opt-in from `field_setters` (see
    // `rustc_codegen_clr_add_field_properties`'s doc for why: once linked as a property accessor,
    // C# rejects calling the method explicitly, so this must not touch `field_setters`' `read_*`/
    // `set_*` accessors, which existing consumers call directly).
    let properties_call = if properties {
        quote! { let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_field_properties(class); }
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
                // `HAS_TYPE_KIND_OPINION = true`: this IS the authoritative `#[dotnet_class]`
                // declaration — `#value_type` is the real `value_type = ...` attribute value, not
                // a re-opening placeholder (see `rustc_codegen_clr_new_typedef`'s doc).
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_new_typedef::<
                    #name_lit, #value_type, #super_asm_lit, #super_name_lit, true,
                >();
                #(#field_calls)*
                #(#static_field_calls)*
                #(#base_ctor_arg_calls)*
                #(#interface_calls)*
                #(#attr_calls)*
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_primary_ctor(class);
                #default_ctor_call
                #field_setters_call
                #properties_call
                ::mycorrhiza::comptime::rustc_codegen_clr_finish_type(class);
            }
        }
    };
    expanded.into()
}

fn enum_discriminant(expr: &Expr) -> syn::Result<i128> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(value) => value.base10_parse(),
            _ => Err(syn::Error::new(
                expr.span(),
                "#[dotnet_enum]: discriminants must be integer literals",
            )),
        },
        Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => {
            let Expr::Lit(lit) = &*unary.expr else {
                return Err(syn::Error::new(
                    expr.span(),
                    "#[dotnet_enum]: negative discriminants must be integer literals",
                ));
            };
            let Lit::Int(value) = &lit.lit else {
                return Err(syn::Error::new(
                    expr.span(),
                    "#[dotnet_enum]: negative discriminants must be integer literals",
                ));
            };
            value.base10_parse::<i128>()?.checked_neg().ok_or_else(|| {
                syn::Error::new(expr.span(), "#[dotnet_enum]: discriminant is out of range")
            })
        }
        _ => Err(syn::Error::new(
            expr.span(),
            "#[dotnet_enum]: discriminants must be integer literals; implicit increments are supported",
        )),
    }
}

fn enum_repr(input: &ItemEnum) -> syn::Result<(String, Type, usize, i128, i128)> {
    for attr in &input.attrs {
        if attr.path().is_ident("repr") {
            let repr: syn::Ident = attr.parse_args()?;
            let name = repr.to_string();
            let (size, min, max) = match name.as_str() {
                "i8" => (1, i8::MIN as i128, i8::MAX as i128),
                "u8" => (1, 0, u8::MAX as i128),
                "i16" => (2, i16::MIN as i128, i16::MAX as i128),
                "u16" => (2, 0, u16::MAX as i128),
                "i32" => (4, i32::MIN as i128, i32::MAX as i128),
                "u32" => (4, 0, u32::MAX as i128),
                "i64" => (8, i64::MIN as i128, i64::MAX as i128),
                "u64" => (8, 0, u64::MAX as i128),
                _ => {
                    return Err(syn::Error::new(
                        repr.span(),
                        "#[dotnet_enum]: repr must be i8/u8/i16/u16/i32/u32/i64/u64",
                    ));
                }
            };
            return Ok((name, syn::parse_str(&repr.to_string())?, size, min, max));
        }
    }
    Err(syn::Error::new(
        input.ident.span(),
        "#[dotnet_enum]: add an explicit #[repr(i8/u8/i16/u16/i32/u32/i64/u64)]",
    ))
}

/// Export a fieldless Rust enum as a genuine CLR enum TypeDef.
///
/// ```ignore
/// #[dotnet_enum(name = "Example.Status")]
/// #[repr(i32)]
/// pub enum Status { Ready = 1, Done = 2 }
/// ```
#[proc_macro_attribute]
pub fn dotnet_enum(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemEnum);
    let parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
    let args = match syn::parse::Parser::parse(parser, attr) {
        Ok(args) => args,
        Err(error) => return error.to_compile_error().into(),
    };
    let mut managed_name = None;
    for arg in args {
        if !arg.path.is_ident("name") || managed_name.is_some() {
            return syn::Error::new(
                arg.path.span(),
                "#[dotnet_enum]: expected exactly one `name = \"Namespace.Type\"`",
            )
            .to_compile_error()
            .into();
        }
        let Expr::Lit(lit) = arg.value else {
            return syn::Error::new(
                arg.path.span(),
                "#[dotnet_enum]: `name` must be a string literal",
            )
            .to_compile_error()
            .into();
        };
        let Lit::Str(value) = lit.lit else {
            return syn::Error::new(
                lit.lit.span(),
                "#[dotnet_enum]: `name` must be a string literal",
            )
            .to_compile_error()
            .into();
        };
        if let Err(error) = validate_dotnet_ref(&value.value(), value.span()) {
            return error.to_compile_error().into();
        }
        if value.value().starts_with('[') {
            return syn::Error::new(value.span(), "#[dotnet_enum]: a Rust-defined enum belongs to the output assembly; omit an `[Assembly]` prefix").to_compile_error().into();
        }
        managed_name = Some(value);
    }
    let managed_name =
        managed_name.unwrap_or_else(|| LitStr::new(&input.ident.to_string(), input.ident.span()));
    let (repr_name, repr_ty, size, min, max) = match enum_repr(&input) {
        Ok(value) => value,
        Err(error) => return error.to_compile_error().into(),
    };
    let mut next = 0i128;
    let mut variant_values = Vec::new();
    for variant in &input.variants {
        if !matches!(variant.fields, syn::Fields::Unit) {
            return syn::Error::new(
                variant.fields.span(),
                "#[dotnet_enum]: CLR enum variants must be fieldless",
            )
            .to_compile_error()
            .into();
        }
        let value = match &variant.discriminant {
            Some((_, expr)) => match enum_discriminant(expr) {
                Ok(value) => value,
                Err(error) => return error.to_compile_error().into(),
            },
            None => next,
        };
        if value < min || value > max {
            return syn::Error::new(
                variant.ident.span(),
                format!("#[dotnet_enum]: discriminant {value} does not fit `{repr_name}`"),
            )
            .to_compile_error()
            .into();
        }
        variant_values.push((variant.ident.clone(), value));
        next = match value.checked_add(1) {
            Some(value) => value,
            None if variant.ident == input.variants.last().unwrap().ident => value,
            None => {
                return syn::Error::new(
                    variant.ident.span(),
                    "#[dotnet_enum]: implicit next discriminant overflows",
                )
                .to_compile_error()
                .into();
            }
        };
    }
    if variant_values.is_empty() {
        return syn::Error::new(
            input.ident.span(),
            "#[dotnet_enum]: at least one variant is required",
        )
        .to_compile_error()
        .into();
    }
    let mut spec = repr_name;
    for (name, value) in &variant_values {
        spec.push(';');
        spec.push_str(&name.to_string());
        spec.push('=');
        spec.push_str(&value.to_string());
    }
    let spec = LitStr::new(&spec, input.ident.span());
    let name = &input.ident;
    let handle = format_ident!("{}Handle", name);
    let entry_mod = format_ident!("__dotnet_enum_{}", name);
    let variants = variant_values
        .iter()
        .map(|(variant, value)| quote! { #value => ::core::option::Option::Some(Self::#variant) });
    let expanded = quote! {
        #input

        #[allow(non_camel_case_types, dead_code)]
        pub type #handle = ::mycorrhiza::intrinsics::RustcCLRInteropManagedStruct<"", #managed_name, #size>;

        impl #name {
            #[inline]
            pub fn value(self) -> #repr_ty { self as #repr_ty }

            #[inline]
            pub fn from_value(value: #repr_ty) -> ::core::option::Option<Self> {
                match value as i128 { #(#variants,)* _ => ::core::option::Option::None }
            }

            #[inline]
            pub fn to_handle(self) -> #handle {
                let value: #repr_ty = self as #repr_ty;
                unsafe { ::core::mem::transmute_copy(&value) }
            }

            #[inline]
            pub fn from_handle(value: #handle) -> ::core::option::Option<Self> {
                let value: #repr_ty = unsafe { ::core::mem::transmute_copy(&value) };
                Self::from_value(value)
            }
        }

        impl ::mycorrhiza::enums::DotNetExportEnum for #name {
            type Managed = #handle;
            fn into_managed(self) -> Self::Managed { self.to_handle() }
            fn try_from_managed(value: Self::Managed) -> ::core::option::Option<Self> { Self::from_handle(value) }
        }

        #[allow(non_snake_case, dead_code, unused_variables, internal_features)]
        mod #entry_mod {
            #[used]
            static PREVENT_DCE: fn() = rustc_codegen_clr_comptime_entrypoint;
            #[inline(never)]
            pub fn rustc_codegen_clr_comptime_entrypoint() {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_new_typedef::<#managed_name, true, "", "", true>();
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_set_enum::<#spec>(class);
                ::mycorrhiza::comptime::rustc_codegen_clr_finish_type(class);
            }
        }
    };
    expanded.into()
}

/// Declares a conventional managed data-transfer object.
///
/// This is deliberately only validating sugar for
/// `#[dotnet_class(default_ctor = true, properties = true)]`: it emits a managed class with a
/// parameterless constructor and ordinary read/write CLR properties. It does not imply or generate
/// serialization behavior.
fn dto_backing_name(name: &str) -> String {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    if first.is_ascii_uppercase() {
        format!("{}{}", first.to_ascii_lowercase(), chars.as_str())
    } else {
        name.to_owned()
    }
}

#[proc_macro_attribute]
pub fn dotnet_dto(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[dotnet_dto] does not accept arguments; use #[dotnet_class(...)] for custom class options",
        )
        .to_compile_error()
        .into();
    }

    // Validate here so diagnostics name the DTO surface before delegating to the established class
    // expansion. `dotnet_class` parses it again and remains the single implementation path.
    let mut parsed = parse_macro_input!(item as ItemStruct);
    if !matches!(parsed.fields, syn::Fields::Named(_)) {
        return syn::Error::new_spanned(
            parsed,
            "#[dotnet_dto] requires a struct with named fields",
        )
        .to_compile_error()
        .into();
    }
    // A DTO schema often spells fields in their exact public CLR PascalCase. Keeping that spelling
    // for the backing field would expose two same-named members (field + generated property), which
    // Roslyn reports as CS0229. Internally normalize only an initial ASCII capital to lower-camel;
    // `properties = true` capitalizes it again for the public property, preserving the schema name.
    // Existing idiomatic lowercase Rust fields are unchanged.
    for field in &mut parsed.fields {
        let Some(ident) = &mut field.ident else {
            continue;
        };
        let name = ident.to_string();
        let backing = dto_backing_name(&name);
        if backing != name {
            *ident = syn::Ident::new(&backing, ident.span());
        }
    }
    let dto_name = parsed.ident.clone();
    let dto_handle = format_ident!("{}Handle", dto_name);
    let dto_name_lit = LitStr::new(&dto_name.to_string(), dto_name.span());
    let fields = parsed.fields.iter().collect::<Vec<_>>();
    let field_names = fields
        .iter()
        .map(|field| field.ident.clone().expect("named fields were validated"))
        .collect::<Vec<_>>();
    let field_types = fields
        .iter()
        .map(|field| field.ty.clone())
        .collect::<Vec<_>>();
    let arg_types = (0..fields.len())
        .map(|index| format_ident!("Arg{index}"))
        .collect::<Vec<_>>();
    let ctor_magic = format_ident!("rustc_clr_interop_managed_ctor{}_", fields.len());
    let class_expansion = proc_macro2::TokenStream::from(dotnet_class(
        quote!(default_ctor = true, properties = true).into(),
        quote!(#parsed).into(),
    ));
    quote! {
        #class_expansion

        impl #dto_name {
            /// Construct the fully initialized managed DTO through its generated primary CLR
            /// constructor. The bridge arity is derived from the schema, so large DTOs do not need
            /// a hand-written setter sequence or a fixed ctor0..ctor3 helper ladder.
            #[allow(non_snake_case, dead_code, unused_variables, internal_features)]
            pub fn new_managed(#(#field_names: #field_types),*) -> #dto_handle {
                #[inline(never)]
                fn #ctor_magic<
                    const ASSEMBLY: &'static str,
                    const CLASS_PATH: &'static str,
                    const IS_VALUETYPE: bool,
                    #(#arg_types),*
                >(
                    #(#field_names: #arg_types),*
                ) -> ::mycorrhiza::intrinsics::RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH> {
                    loop { ::core::hint::spin_loop(); }
                }
                #ctor_magic::<"", #dto_name_lit, false, #(#field_types),*>(#(#field_names),*)
            }
        }
    }
    .into()
}

#[cfg(test)]
mod dotnet_export_name_tests {
    use super::{
        clr_member_id_type_name, dotnet_export_member_id, dto_backing_name,
        imported_delegate_param, parse_dotnet_export_args,
    };
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn accepts_an_ordinary_managed_name() {
        assert_eq!(
            parse_dotnet_export_args(quote!(name = "ParsePosition"))
                .unwrap()
                .name,
            Some("ParsePosition".to_string()),
        );
    }

    #[test]
    fn dto_pascal_schema_name_uses_a_distinct_lower_camel_backing_field() {
        assert_eq!(dto_backing_name("FirmNumber"), "firmNumber");
        assert_eq!(dto_backing_name("amount"), "amount");
    }

    #[test]
    fn preserves_the_legacy_no_argument_form() {
        assert_eq!(parse_dotnet_export_args(quote!()).unwrap().name, None);
    }

    #[test]
    fn rejects_non_csharp_identifier_names() {
        let error = parse_dotnet_export_args(quote!(name = "parse-position")).unwrap_err();
        assert!(error.to_string().contains("ASCII C# identifier"));
    }

    #[test]
    fn accepts_exception_policy_with_a_managed_name() {
        let args =
            parse_dotnet_export_args(quote!(name = "TryParse", error = "exception")).unwrap();
        assert_eq!(args.name.as_deref(), Some("TryParse"));
        assert!(args.error_exception);
    }

    #[test]
    fn accepts_registered_enum_types() {
        let parsed =
            parse_dotnet_export_args(quote!(name = "Roundtrip", enums(Status, Mode))).unwrap();
        assert_eq!(parsed.name.as_deref(), Some("Roundtrip"));
        assert_eq!(parsed.enum_types.len(), 2);
    }

    #[test]
    fn xml_member_ids_use_the_configured_managed_name() {
        assert_eq!(
            dotnet_export_member_id("ParsePosition", &["System.String".to_string()]),
            "M:MainModule.ParsePosition(System.String)"
        );
    }

    #[test]
    fn imported_delegate_member_ids_keep_the_constructed_clr_signature() {
        assert_eq!(
            clr_member_id_type_name(&parse_quote!(Func2<i32, i64, bool>)).as_deref(),
            Some("System.Func{System.Int32,System.Int64,System.Boolean}")
        );
    }

    #[test]
    fn imported_delegates_accept_supported_shapes_and_reject_owned_strings() {
        let supported = imported_delegate_param(&parse_quote!(Action2<i32, bool>))
            .expect("recognized delegate")
            .expect("primitive delegate should marshal");
        assert!(supported.to_rust.is_some());

        let managed_string =
            imported_delegate_param(&parse_quote!(Func1<mycorrhiza::system::MString, i32>))
                .expect("recognized delegate")
                .expect("managed string handle should marshal");
        assert!(managed_string.to_rust.is_some());

        let arity_three = imported_delegate_param(&parse_quote!(Func3<i32, i32, i32, i32>))
            .expect("recognized delegate")
            .expect("three-argument delegate should marshal");
        assert!(arity_three.to_rust.is_some());

        let error = imported_delegate_param(&parse_quote!(Func1<String, i32>))
            .expect("recognized delegate")
            .err()
            .expect("owned String callback boundary must be rejected");
        assert!(error.contains("explicit callback-boundary marshalling policy"));
    }
}

// ============================================================================
// #[dotnet_interface] — turn a Rust trait into a C#-consumable .NET interface.
// ============================================================================

/// One self-callable instance member of a `#[dotnet_interface]` trait, as seen by the
/// default-interface-method body rewriter ([`DimRewriter`]): the signature a `self.<name>(…)`
/// call inside a default body must be lowered against. Types are cloned VERBATIM from the trait
/// signature so the rewritten call's `MethodRef` signature is token-identical to the interface
/// member's own declared signature (in-assembly method resolution is by exact interning match).
struct DimCallee {
    /// Non-receiver parameter types, verbatim from the trait signature.
    arg_tys: Vec<Type>,
    /// Return type (`None` = unit), verbatim from the trait signature.
    ret_ty: Option<Type>,
    /// The member has a reference (`&`/`&mut`) parameter. Such members are declared as managed
    /// byrefs (`ref T`) on the interface, but a rewritten self-call would lower its argument as a
    /// raw pointer (`T*`) — a signature MISMATCH that would resolve to a dangling `MemberRef`
    /// instead of the interface's own `MethodDef`. No sound lowering exists today, so the
    /// rewriter rejects self-calls to these loudly. (This detection is SYNTACTIC — a type alias
    /// hiding the reference defeats it; the linker's interface missing-method backstop catches
    /// that case at build time.)
    byref: bool,
    /// The member declares method-level generic parameters (`fn Echo<T>(…)`). Its `arg_tys`/
    /// `ret_ty` are then spelled in terms of `T`, which has no meaning inside the lifted DIM
    /// free fn: cloning them into the rewritten call either fails to resolve (confusing E0425
    /// deep in generated code) or — worse — silently resolves to an in-scope CONCRETE type of
    /// the same name, interning a `MethodRef` that matches no interface member. The rewriter
    /// rejects self-calls to generic members loudly.
    generic: bool,
}

/// Rewrites a default interface method's Rust body so it can be lifted out of the trait into a
/// free fn (the DIM's real, codegen'd body): `self.<trait_method>(args…)` becomes
/// `this.instanceN::<"<Method>", ArgTys…, Ret>(args…)` — a `callvirt` through the interface
/// handle (see `call_managed`'s reference-type branch in `src/terminator/call.rs`), so genuine
/// virtual dispatch: an implementing class's own definition wins even when called from inside the
/// DIM. Any remaining bare `self` path is rewritten to `this` (the handle IS the receiver, and is
/// `Copy`). Everything `self`-shaped that has NO faithful lowering is recorded as an error —
/// unknown/supertrait methods, >2-argument calls (the `instanceN` ladder tops out at
/// `managed_call3_` = receiver + 2), turbofish, byref-parameter members. The AST visitor cannot
/// see into macro-invocation token streams (`println!("{}", self.x())`), so the caller must ALSO
/// run [`tokens_contain_self_ident`] over the rewritten body as a backstop.
struct DimRewriter<'a> {
    callees: &'a std::collections::HashMap<String, DimCallee>,
    errors: Vec<syn::Error>,
}

impl syn::visit_mut::VisitMut for DimRewriter<'_> {
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        // `self.M(args…)` — handled BEFORE the default child traversal: the receiver `self` is
        // consumed by this rewrite, and letting the default traversal see it first would turn it
        // into a bare `this` and break the pattern match.
        if let syn::Expr::MethodCall(mc) = expr {
            let receiver_is_self = matches!(
                &*mc.receiver,
                syn::Expr::Path(p) if p.qself.is_none() && p.path.is_ident("self")
            );
            if receiver_is_self {
                // The arguments may themselves contain self-calls / bare `self` — rewrite them
                // first (the receiver is deliberately NOT visited).
                for arg in mc.args.iter_mut() {
                    self.visit_expr_mut(arg);
                }
                let mname = mc.method.to_string();
                let Some(callee) = self.callees.get(&mname) else {
                    self.errors.push(syn::Error::new_spanned(
                        &mc.method,
                        format!(
                            "#[dotnet_interface]: this default body calls `self.{mname}(…)`, \
                             which is not an instance method of this trait — only self-calls on \
                             the trait's own instance members can be lowered to .NET interface \
                             dispatch (supertrait members aren't supported yet)"
                        ),
                    ));
                    return;
                };
                if callee.generic {
                    self.errors.push(syn::Error::new_spanned(
                        &mc.method,
                        format!(
                            "#[dotnet_interface]: default body cannot call `self.{mname}(…)` — \
                             `{mname}` is a generic method (`fn {mname}<T>(…)`), and its \
                             `T`-spelled signature has no meaning inside the lifted default \
                             body (the rewritten call would either fail to resolve or silently \
                             bind `T` to an unrelated in-scope type of the same name); self-calls \
                             to generic members are not supported"
                        ),
                    ));
                    return;
                }
                if mc.turbofish.is_some() {
                    self.errors.push(syn::Error::new_spanned(
                        &mc.method,
                        format!(
                            "#[dotnet_interface]: `self.{mname}::<…>(…)` — turbofish on a \
                             self-call in a default body is not supported (the rewritten call's \
                             signature comes verbatim from the trait declaration, so there is \
                             nothing a turbofish could parameterize)"
                        ),
                    ));
                    return;
                }
                if callee.byref {
                    self.errors.push(syn::Error::new_spanned(
                        &mc.method,
                        format!(
                            "#[dotnet_interface]: default body cannot call `self.{mname}(…)` — \
                             `{mname}` has a reference (`&mut T`) parameter, declared as a \
                             managed byref (`ref T`) on the interface, and a rewritten self-call \
                             would mismatch that signature (raw-pointer lowering); byref \
                             self-calls are not supported yet"
                        ),
                    ));
                    return;
                }
                if mc.args.len() != callee.arg_tys.len() {
                    self.errors.push(syn::Error::new_spanned(
                        &mc.method,
                        format!(
                            "#[dotnet_interface]: `self.{mname}(…)` passes {} argument(s) but \
                             the trait declares {} — argument count must match the trait \
                             signature exactly",
                            mc.args.len(),
                            callee.arg_tys.len()
                        ),
                    ));
                    return;
                }
                if callee.arg_tys.len() > 2 {
                    self.errors.push(syn::Error::new_spanned(
                        &mc.method,
                        format!(
                            "#[dotnet_interface]: `self.{mname}(…)` takes {} arguments, but \
                             self-calls in a default body support at most 2 (the mycorrhiza \
                             `instanceN` call ladder tops out at receiver + 2 — extending it is \
                             a mechanical follow-up)",
                            callee.arg_tys.len()
                        ),
                    ));
                    return;
                }
                let method_lit = LitStr::new(&mname, mc.method.span());
                let instance_n = format_ident!("instance{}", callee.arg_tys.len());
                let arg_tys = &callee.arg_tys;
                let ret_tokens = match &callee.ret_ty {
                    Some(t) => quote! { #t },
                    None => quote! { () },
                };
                let args = std::mem::take(&mut mc.args);
                *expr = syn::parse_quote!(
                    this.#instance_n::<#method_lit, #(#arg_tys,)* #ret_tokens>(#args)
                );
                return;
            }
        }
        syn::visit_mut::visit_expr_mut(self, expr);
        // Any remaining bare `self` (e.g. passed as an argument) becomes the handle — sound
        // because the handle IS the receiver and is `Copy`.
        if let syn::Expr::Path(p) = expr {
            if p.qself.is_none() && p.path.is_ident("self") {
                *expr = syn::parse_quote!(this);
            }
        }
    }
}

/// Recursively scans a token stream for any ident spelled `self` or `Self` — the load-bearing
/// BACKSTOP behind [`DimRewriter`]: the AST visitor cannot see into macro-invocation bodies
/// (`println!("{}", self.x())`) or shapes it doesn't model, and a residual `self` in the lifted
/// fn would produce a confusing rustc error deep in macro-generated code (or worse). Anything
/// this finds after rewriting is rejected loudly by the caller.
fn tokens_contain_self_ident(ts: proc_macro2::TokenStream) -> bool {
    ts.into_iter().any(|tt| match tt {
        proc_macro2::TokenTree::Ident(i) => i == "self" || i == "Self",
        proc_macro2::TokenTree::Group(g) => tokens_contain_self_ident(g.stream()),
        _ => false,
    })
}

/// If `ty` is EXACTLY a bare generic-parameter path — a single unqualified path segment with no
/// arguments whose ident is one of the trait's declared type parameters — returns that
/// parameter's declaration index (`T`->0, `U`->1, …). This is the ONLY position a generic
/// parameter may occupy in a `#[dotnet_interface]` member signature: the carrier rewrites it to
/// the `RustcCLRInteropTypeGeneric<N>` marker, which the backend lowers to `ELEMENT_TYPE_VAR N`
/// (the `!N` the member's .NET signature needs). Composite uses (`&T`, `Vec<T>`, `(T,)`, …) have
/// no faithful lowering and are rejected via [`first_generic_param_mention`].
fn bare_generic_param(
    ty: &syn::Type,
    params: &std::collections::HashMap<String, usize>,
) -> Option<usize> {
    let syn::Type::Path(tp) = ty else { return None };
    if tp.qself.is_some() || tp.path.segments.len() != 1 {
        return None;
    }
    let seg = tp.path.segments.first()?;
    if !seg.arguments.is_empty() {
        return None;
    }
    params.get(&seg.ident.to_string()).copied()
}

/// Token-scans `ts` for any ident that names one of the trait's declared generic parameters,
/// returning the first hit. Run AFTER [`bare_generic_param`] replacement: any survivor means the
/// parameter appears inside a composite type (`&T`, `Vec<T>`, `[T; N]`, a nested generic, …) —
/// shapes with no faithful `ELEMENT_TYPE_VAR` lowering — and the member must be rejected loudly
/// rather than emitting a signature that silently treats `T` as an unrelated Rust type.
fn first_generic_param_mention(
    ts: proc_macro2::TokenStream,
    params: &std::collections::HashMap<String, usize>,
) -> Option<String> {
    ts.into_iter().find_map(|tt| match tt {
        proc_macro2::TokenTree::Ident(i) => {
            let name = i.to_string();
            params.contains_key(&name).then_some(name)
        }
        proc_macro2::TokenTree::Group(g) => first_generic_param_mention(g.stream(), params),
        _ => None,
    })
}

/// `#[dotnet_interface]` on a Rust `trait` emits a genuine ECMA-335 `interface` `TypeDef` (via the
/// PE writer on the default `DIRECT_PE=1` path) whose members are the trait's methods, each an
/// abstract (no-body) instance method. A C# consumer can then implement it (`class Foo :
/// IMyInterface { … }`) and use it polymorphically, and a Rust `#[dotnet_class]` can implement it
/// via `implements = "IMyInterface"`.
///
/// A trait method taking `&self` (or `&mut self`) as its first parameter becomes an **instance**
/// member — the receiver becomes the interface's implicit `this`. A trait fn with NO `self`
/// receiver becomes a **`static abstract`** member (.NET 7+ *static virtual members in
/// interfaces*, the `INumber<T>` generic-math shape): C# implements it as `public static …` and
/// dispatches generically via `T.Member(…)` under a `where T : IFace` constraint. Both kinds mix
/// freely in one trait. Parameters and return types map straight through:
///
/// ```ignore
/// #[dotnet_interface]
/// pub trait ISpeaker {
///     fn Speak(&self);              // C#: void Speak();
///     fn Volume(&self) -> i32;      // C#: int Volume();
///     fn Make() -> i32;             // C#: static abstract int Make();
/// }
/// ```
///
/// A `static abstract` member is declaration-only surface from the Rust side: Rust code cannot
/// *call* it (that would need a `constrained.` generic call the backend doesn't emit) — the C#
/// consumer implements and dispatches it.
///
/// **Default interface methods (DIM)**: an *instance* trait method WITH a default body becomes a
/// genuine .NET default interface method (CoreCLR 3.0+) — a virtual, non-abstract member with a
/// real IL body on the interface itself. A C# class that omits the member inherits the default;
/// a class that defines it wins over the default (ordinary virtual dispatch):
///
/// ```ignore
/// #[dotnet_interface]
/// pub trait ICalc {
///     fn Base(&self) -> i32;                       // C#: int Base();  (abstract — must implement)
///     fn Doubled(&self) -> i32 { self.Base() * 2 } // C#: DIM — `self.Base()` dispatches virtually,
///                                                  // so the implementing class's Base() is called
/// }
/// ```
///
/// The default body is ordinary Rust, restricted to shapes with a faithful .NET-dispatch lowering:
/// `self.<trait_method>(…)` calls on the trait's OWN instance members (rewritten to a `callvirt`
/// through the interface handle — at most 2 arguments, no byref-parameter members) plus self-free
/// Rust code. Any other use of `self`/`Self` — a macro body like `println!("{}", self.x())`,
/// calls on supertrait members, `Self::`-qualified paths — is rejected with a compile error rather
/// than emitting a subtly-wrong body. Default bodies are not supported on `static` (receiver-less)
/// members, `#[dotnet_event]` members, or methods with reference (`&T`/`&mut T`) parameters.
///
/// A trait fn marked `#[dotnet_event]` declares a `.NET` **event on the interface** instead of a
/// plain method: the fn name is the event name, its single non-receiver parameter is the delegate
/// type subscribers must match, and the abstract `add_<Name>`/`remove_<Name>` accessor pair (plus
/// the `Event`/`EventMap`/`MethodSemantics` metadata rows) is synthesized from that one
/// declaration. C# then implements it field-like (`class X : IButton { public event Action
/// Clicked; }`) and subscribes through the interface reference:
///
/// ```ignore
/// #[dotnet_interface]
/// pub trait IButton {
///     fn Id(&self) -> i32;
///     #[dotnet_event]
///     fn Clicked(&self, handler: ActionHandle);   // C#: event Action Clicked;
/// }
/// ```
///
/// (Note this deliberately differs from the class-side `#[dotnet_event("Name")]` pair form: class
/// accessors have two distinct Rust bodies; interface accessors have none, so a single declaration
/// makes a missing/mismatched half impossible by construction.)
///
/// A trait fn marked `#[dotnet_property]` declares a **.NET property accessor** on the interface:
/// the fn MUST be named `get_<Prop>` (the getter — `&self`, no other parameters, returns the
/// property's value) or `set_<Prop>` (the setter — `&mut self`/`&self` plus exactly ONE by-value
/// parameter, no return). The pair (or a lone getter, for a get-only property) becomes ONE
/// §II.22.34 `Property` row named `<Prop>` with `MethodSemantics` `Getter`/`Setter` rows over the
/// abstract accessors, so C# sees and implements `int Volume { get; set; }` — including
/// field-like auto-properties — and `typeof(I).GetProperty("Volume")` works:
///
/// ```ignore
/// #[dotnet_interface]
/// pub trait IVolume {
///     #[dotnet_property]
///     fn get_Volume(&self) -> i32;          // C#: int Volume { get; … }
///     #[dotnet_property]
///     fn set_Volume(&mut self, value: i32); //     …          { …; set; }
///     #[dotnet_property]
///     fn get_Name(&self) -> MString;        // C#: string Name { get; } (get-only)
/// }
/// ```
///
/// Loudly rejected (compile errors, never silently-wrong metadata): a marked fn not named
/// `get_*`/`set_*`; a getter with parameters or without a return type (indexers/parameterized
/// properties — C# `this[…]` — are not supported); a setter with any arity but one or with a
/// return type; a setter whose value is passed by reference (`&T`/`&mut T` — a property's value
/// travels by value); `#[dotnet_out]` on an accessor parameter; a `set_<Prop>` with no matching
/// `get_<Prop>` (write-only properties); combining `#[dotnet_property]` with `#[dotnet_event]`,
/// a default body, generic method parameters, or a static (receiver-less) member; a property
/// whose VALUE type is a trait generic parameter (`-> T`); and an UNMARKED trait fn whose name
/// collides with a property's reserved accessor slot (a plain `fn set_Volume` next to a get-only
/// `Volume` property). A getter/setter TYPE disagreement is caught by the backend's comptime
/// pass with a clean error naming the property. Rust-side callers still see plain
/// `get_*`/`set_*` trait methods — the property surface is for the C# consumer.
///
/// **Interface inheritance**: the trait's supertrait list IS the .NET base-interface list — each
/// supertrait becomes an `InterfaceImpl` row on this interface's own `TypeDef` (§II.10.1.3 — that,
/// not `Extends`, is how ECMA-335 models `interface IDerived : IBase`), so C# sees the inheritance
/// and the CLR computes the transitive closure (`impl is IBase` holds through `IDerived`):
///
/// ```ignore
/// #[dotnet_interface]
/// pub trait IPet: IAnimal + ILoud {   // C#: interface IPet : IAnimal, ILoud
///     fn Cuteness(&self) -> i32;
/// }
/// ```
///
/// **Contract**: every supertrait must itself be a `#[dotnet_interface]` trait compiled into the
/// same final assembly (the .NET name is the supertrait's last path segment). Anything else —
/// `Clone`, `Send`, a plain Rust trait — fails LOUDLY at export/link time with an error naming
/// both types (the macro cannot know which idents are .NET interfaces). For an EXTERNAL .NET base
/// interface use the attribute form, same `[Assembly]Ns.Name` convention as `#[dotnet_class]`:
///
/// ```ignore
/// #[dotnet_interface(implements = "[System.Runtime]System.IDisposable")]
/// pub trait IManagedResource { fn Poke(&self); }
/// ```
///
/// **Generic interfaces**: a plain type parameter list maps to a genuine generic .NET interface
/// definition — the metadata name carries the CLS backtick-arity suffix (`IBox`1`), one
/// `GenericParam` row per parameter, and a bare `T` in a member's parameter/return position
/// becomes `ELEMENT_TYPE_VAR` (C#'s `T`). C# implements any instantiation (`class IntBox :
/// `IBox<int>`), and the parameterized ``IBoxHandle<T>`` alias lets Rust signatures reference an
/// instantiation (e.g. ``IBoxHandle<i32>`` = `IBox<int>` in a `#[dotnet_export]` fn):
///
/// ```ignore
/// #[dotnet_interface]
/// pub trait IBox<T> {
///     fn Get(&self) -> T;           // C#: T Get();
///     fn Put(&mut self, value: T);  // C#: void Put(T value);
///     fn Count(&self) -> i32;       // C#: int Count();  (non-generic members mix freely)
/// }
/// ```
///
/// Loudly rejected on a generic trait (compile errors, never silently-wrong metadata): lifetime
/// or const parameters, parameter defaults (`<T = i32>`), bounds/`where` clauses (no
/// `GenericParamConstraint` emission), `T` anywhere except a bare parameter/return position
/// (`&T`, `Vec<T>`, `(T,)`, …), default method bodies, and a generic parameter as an event's
/// delegate type.
///
/// **Generic methods**: a plain type-parameter list on an *instance* trait method maps to a
/// genuine generic .NET method DEFINITION — the member's signature carries `SIG_GENERIC` + a
/// `GenParamCount`, one method-owned `GenericParam` row is emitted per parameter (§II.22.20),
/// and a bare `T` in a parameter/return position becomes `ELEMENT_TYPE_MVAR` (C#'s method-level
/// `T`, `!!N`). C# implements and calls it as an ordinary generic interface method, including
/// via reflection (`GetMethod("Echo").MakeGenericMethod(typeof(int))`). Method- and trait-level
/// parameters mix freely on a generic trait (`trait IBox<T> { fn Pick<U>(&self, a: T, b: U) ->
/// U; }` — `U Pick<U>(T a, U b)`):
///
/// ```ignore
/// #[dotnet_interface]
/// pub trait IConverter {
///     fn Echo<T>(&self, value: T) -> T;             // C#: T Echo<T>(T value);
///     fn First<K, V>(&self, key: K, value: V) -> K; // C#: K First<K, V>(K key, V value);
/// }
/// ```
///
/// Loudly rejected on a generic method (same rules as the trait-level list): bounds
/// (`fn f<T: Clone>`), `where` clauses, lifetime/const parameters, defaults, and a parameter
/// used anywhere except a bare parameter/return position. Also rejected: generic parameters on
/// a `#[dotnet_event]` member, on a method with a default body (the lifted DIM body would need
/// a generic IL body), and on a static (receiver-less) member (generic `static abstract` isn't
/// emitted yet). Like a `static abstract`, a generic member is declaration-only surface from
/// the Rust side: Rust code cannot *call* it through the handle yet (the trait is a declaration
/// vehicle); the C# consumer implements and dispatches it.
///
/// **`ref`/`out` parameters**: a `&mut T` (thin, sized `T`) parameter maps to a managed byref —
/// C# sees `ref T` — and marking it `#[dotnet_out]` additionally stamps `ParamAttributes.Out`
/// so C# sees `out T`:
///
/// ```ignore
/// #[dotnet_interface]
/// pub trait IRefCell {
///     fn Fill(&self, slot: &mut i32);                  // C#: void Fill(ref int slot);
///     fn FillOut(&self, #[dotnet_out] slot: &mut i32); // C#: void FillOut(out int slot);
/// }
/// ```
///
/// The mapping is byref-only for `&mut`: shared `&T` parameters are rejected (C# `in T` would
/// need `modreq(InAttribute)`), as are reference RETURNS and `&mut` to unsized types
/// (`&mut str`/`&mut [T]`/`&mut dyn`, which have no managed-byref equivalent — alias-hidden cases
/// are caught by the backend). Raw pointers keep their meaning: `*mut T`/`*const T` still emit
/// C# `T*` (the unsafe escape hatch). NOTE: the byref mapping applies to `#[dotnet_interface]`
/// members only — a Rust `#[dotnet_class]`/`#[dotnet_methods]` implementor still lowers `&mut T`
/// to `T*`, so a Rust class named in `implements =` for a byref-parameter interface fails LOUDLY
/// at CLR type load (`TypeLoadException` naming the unimplemented member); implement such
/// interfaces from C# for now (see `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md`).
///
/// The macro also emits an `<Name>Handle` managed-handle alias (a Rust-side reference to the
/// interface type). The trait itself is re-emitted unchanged — it is a declaration vehicle only
/// (nothing needs to `impl` it in Rust; managed types satisfy the interface by name+signature).
#[proc_macro_attribute]
pub fn dotnet_interface(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as ItemTrait);
    let trait_name = input.ident.clone();
    let span = trait_name.span();
    let handle_ident = format_ident!("{}Handle", trait_name);
    let entry_mod = format_ident!("__dotnet_interface_{}", trait_name);

    // ---- attribute args: implements = "[Assembly]Ns.IExternal" (`;`-separated for several) ----
    // External .NET base interfaces, same `[Assembly]Ns.Name` convention as `#[dotnet_class]`'s
    // `implements`. Same-assembly Rust bases use the supertrait list instead (below).
    let mut extern_bases: Vec<(String, String)> = Vec::new();
    if !attr.is_empty() {
        let parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
        let metas = match syn::parse::Parser::parse(parser, attr) {
            Ok(m) => m,
            Err(e) => return e.to_compile_error().into(),
        };
        for m in metas {
            if m.path.is_ident("implements") {
                let s = match str_lit_value(&m.value) {
                    Ok(s) => s,
                    Err(e) => return e.to_compile_error().into(),
                };
                for spec in s.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                    if let Err(e) = validate_dotnet_ref(spec, m.value.span()) {
                        return e.to_compile_error().into();
                    }
                    if spec.contains('<') {
                        return syn::Error::new(
                            m.value.span(),
                            format!(
                                "#[dotnet_interface]: generic base interfaces \
                                 (`{spec}`) are not supported yet"
                            ),
                        )
                        .to_compile_error()
                        .into();
                    }
                    extern_bases.push(split_dotnet_ref(spec));
                }
            } else {
                let path = &m.path;
                return syn::Error::new(
                    m.path.span(),
                    format!(
                        "#[dotnet_interface]: unknown attribute key `{}`; expected `implements`",
                        quote! { #path }
                    ),
                )
                .to_compile_error()
                .into();
            }
        }
    }

    // ---- generic type parameters (`trait IBox<T>`) -> a genuine generic .NET interface ----
    // Plain type parameters map to ECMA-335 `GenericParam` rows (§II.22.20) on the interface's
    // own TypeDef; every OTHER generic shape is rejected LOUDLY — `GenericParamConstraint`
    // (0x2C) rows are not emitted, so a bound/where-clause would be silently dropped metadata
    // (a lie to the C# consumer), and lifetimes/const params have no .NET representation.
    let mut type_params: Vec<syn::Ident> = Vec::new();
    for gp in &input.generics.params {
        match gp {
            syn::GenericParam::Type(tp) => {
                if tp.default.is_some() {
                    return syn::Error::new(
                        tp.span(),
                        "#[dotnet_interface]: type-parameter defaults (`<T = i32>`) are not \
                         supported — .NET generic parameters have no default arguments",
                    )
                    .to_compile_error()
                    .into();
                }
                if !tp.bounds.is_empty() {
                    return syn::Error::new(
                        tp.bounds.span(),
                        "#[dotnet_interface]: type-parameter bounds (`<T: Clone>`) are not \
                         supported — `GenericParamConstraint` metadata is not emitted, so a \
                         bound would be silently dropped rather than enforced on the C# side",
                    )
                    .to_compile_error()
                    .into();
                }
                type_params.push(tp.ident.clone());
            }
            syn::GenericParam::Lifetime(lt) => {
                return syn::Error::new(
                    lt.span(),
                    "#[dotnet_interface]: lifetime parameters (`trait IFoo<'a>`) are not \
                     supported — .NET generics have no lifetime concept",
                )
                .to_compile_error()
                .into();
            }
            syn::GenericParam::Const(cp) => {
                return syn::Error::new(
                    cp.span(),
                    "#[dotnet_interface]: const parameters (`<const N: usize>`) are not \
                     supported — .NET generics are type-parameter-only",
                )
                .to_compile_error()
                .into();
            }
        }
    }
    if let Some(wc) = &input.generics.where_clause {
        return syn::Error::new(
            wc.span(),
            "#[dotnet_interface]: `where` clauses are not supported — \
             `GenericParamConstraint` metadata is not emitted, so a constraint would be \
             silently dropped rather than enforced on the C# side",
        )
        .to_compile_error()
        .into();
    }
    // Declaration-order index of each generic parameter (`T`->0, `U`->1, …) — the `!N`
    // (`ELEMENT_TYPE_VAR`) position a bare `T` in a member signature lowers to.
    let param_index: std::collections::HashMap<String, usize> = type_params
        .iter()
        .enumerate()
        .map(|(i, id)| (id.to_string(), i))
        .collect();
    // The interface's .NET metadata name carries the CLS backtick-arity suffix (`IBox`1`) —
    // what Roslyn requires to surface the type as `IBox<T>`.
    let dotnet_name = if type_params.is_empty() {
        trait_name.to_string()
    } else {
        format!("{trait_name}`{}", type_params.len())
    };
    let name_lit = LitStr::new(&dotnet_name, span);
    // Supertraits: each becomes a .NET base interface (`interface IDerived : IBase` — an
    // `InterfaceImpl` row on this interface's TypeDef). Only a plain, non-generic trait path is
    // representable; every other bound shape is rejected loudly. The .NET name is the LAST path
    // segment, and the supertrait must itself be a `#[dotnet_interface]` trait in the same
    // assembly — an unresolvable name (e.g. `: Clone`) fails loudly at export/link time with an
    // error naming it (the macro cannot know which idents are .NET interfaces).
    let mut base_calls = Vec::new();
    for bound in &input.supertraits {
        match bound {
            syn::TypeParamBound::Trait(tb) => {
                if !matches!(tb.modifier, syn::TraitBoundModifier::None) {
                    return syn::Error::new(
                        bound.span(),
                        "#[dotnet_interface]: `?Trait` supertrait bounds are not supported",
                    )
                    .to_compile_error()
                    .into();
                }
                if tb.lifetimes.is_some() {
                    return syn::Error::new(
                        bound.span(),
                        "#[dotnet_interface]: higher-ranked (`for<…>`) supertrait bounds are \
                         not supported",
                    )
                    .to_compile_error()
                    .into();
                }
                let seg = tb
                    .path
                    .segments
                    .last()
                    .expect("a trait bound path always has at least one segment");
                if !seg.arguments.is_empty() {
                    return syn::Error::new(
                        bound.span(),
                        "#[dotnet_interface]: generic supertraits (`trait IDerived: IBase<T>`) \
                         are not supported yet",
                    )
                    .to_compile_error()
                    .into();
                }
                base_calls.push((String::new(), seg.ident.to_string()));
            }
            syn::TypeParamBound::Lifetime(_) => {
                return syn::Error::new(
                    bound.span(),
                    "#[dotnet_interface]: lifetime bounds are not supported on \
                     #[dotnet_interface] traits",
                )
                .to_compile_error()
                .into();
            }
            other => {
                return syn::Error::new(
                    other.span(),
                    "#[dotnet_interface]: unsupported supertrait bound — only plain, non-generic \
                     trait paths (`trait IDerived: IBase`) are supported",
                )
                .to_compile_error()
                .into();
            }
        }
    }
    base_calls.extend(extern_bases);
    let base_iface_calls: Vec<_> = base_calls
        .iter()
        .map(|(asm_name, iface_name)| {
            let asm_lit = LitStr::new(asm_name, span);
            let iface_lit = LitStr::new(iface_name, span);
            quote! {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_interface_impl::<
                    #asm_lit, #iface_lit,
                >(class);
            }
        })
        .collect();

    // ---- Pre-pass: the trait's self-callable instance-member surface, for default-body
    // (`self.<method>(…)`) rewriting. Built from the UNMODIFIED items before the main loop
    // mutates them; events and static (receiver-less) members are excluded — a default body
    // cannot dispatch to either.
    let mut dim_callees: std::collections::HashMap<String, DimCallee> =
        std::collections::HashMap::new();
    for it in &input.items {
        let TraitItem::Fn(m) = it else { continue };
        if m.attrs.iter().any(|a| a.path().is_ident("dotnet_event")) {
            continue;
        }
        if !matches!(m.sig.inputs.first(), Some(FnArg::Receiver(_))) {
            continue;
        }
        let mut arg_tys = Vec::new();
        let mut byref = false;
        for arg in m.sig.inputs.iter().skip(1) {
            if let FnArg::Typed(pt) = arg {
                if matches!(&*pt.ty, Type::Reference(_)) {
                    byref = true;
                }
                arg_tys.push((*pt.ty).clone());
            }
        }
        let ret_ty = match &m.sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => Some((**ty).clone()),
        };
        let generic = !m.sig.generics.params.is_empty();
        dim_callees.insert(
            m.sig.ident.to_string(),
            DimCallee {
                arg_tys,
                ret_ty,
                byref,
                generic,
            },
        );
    }

    // One signature-carrier fn + one `add_abstract_method_def` call per trait method — or, for a
    // `#[dotnet_event]` member, ONE shared carrier (add/remove have identical signatures) + the
    // abstract `add_<Name>`/`remove_<Name>` accessor pair with their event-binding marks — or,
    // for a method WITH a default body, a lifted real fn + an `add_default_method_def` call (a
    // .NET default interface method).
    let mut carriers = Vec::new();
    let mut method_calls = Vec::new();
    // Every .NET member name this interface declares (plain methods AND synthesized event
    // accessors). A duplicate would silently emit two identically-named `MethodDef`s — reject it
    // loudly instead (the synthesized `add_`/`remove_` names are the non-obvious collision).
    let mut member_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // The `get_`/`set_` halves each `#[dotnet_property]` declares, keyed by property name in
    // DECLARATION order (a Vec, not a HashMap — validation errors below must fire
    // deterministically). Each half records the accessor fn's span for error placement. Validated
    // as a whole after the member loop: a setter without a getter (write-only property) and an
    // UNMARKED member squatting on a property's name or reserved accessor names are rejected.
    let mut property_members: Vec<(
        String,
        (Option<proc_macro2::Span>, Option<proc_macro2::Span>),
    )> = Vec::new();
    for it in &mut input.items {
        let TraitItem::Fn(m) = it else {
            return syn::Error::new(
                it.span(),
                "#[dotnet_interface]: only `fn` members are supported in the trait",
            )
            .to_compile_error()
            .into();
        };
        // `async fn` has NO faithful interface lowering: the signature carrier is built from the
        // declared inputs/output only, so the emitted member would be a *synchronous* `void`/`T`
        // shape while the author expects a `Task`-returning one — reject loudly (the same axis
        // `#[dotnet_export]` rejects; the Task/async bridge in `mycorrhiza::task` is a separate,
        // explicit surface). `const fn` likewise has no .NET meaning.
        if let Some(a) = &m.sig.asyncness {
            return syn::Error::new(
                a.span(),
                "#[dotnet_interface]: `async fn` interface members are not supported — the \
                 emitted .NET member would be a synchronous signature (`void`/`T`), not a \
                 `Task`; declare a synchronous member, or use the mycorrhiza Task/async bridge \
                 explicitly",
            )
            .to_compile_error()
            .into();
        }
        if let Some(c) = &m.sig.constness {
            return syn::Error::new(
                c.span(),
                "#[dotnet_interface]: `const fn` interface members are not supported — .NET \
                 interface members have no const-evaluation concept",
            )
            .to_compile_error()
            .into();
        }
        // `#[dotnet_event]` on a trait fn: the fn NAME is the event name, and the abstract
        // `add_<Name>`/`remove_<Name>` accessor pair is synthesized from this single declaration
        // (deliberately different from the class-side `#[dotnet_event("Name")]` pair form: class
        // accessors have two distinct Rust bodies, interface accessors have none — one declaration
        // makes a missing/mismatched half impossible by construction).
        let mut is_event = false;
        for attr in &m.attrs {
            if attr.path().is_ident("dotnet_event") {
                if !matches!(attr.meta, syn::Meta::Path(_)) {
                    return syn::Error::new(
                        attr.span(),
                        "#[dotnet_interface]: on an interface, `#[dotnet_event]` takes no argument \
                         — the fn name is the event name and the `add_`/`remove_` accessor pair is \
                         synthesized from this single declaration (the class-side \
                         `#[dotnet_event(\"Name\")]` pair form doesn't apply here)",
                    )
                    .to_compile_error()
                    .into();
                }
                is_event = true;
            }
        }
        // Strip the marker so the re-emitted trait doesn't reference an unregistered attribute —
        // the same idiom as `#[dotnet_methods]`'s per-method markers.
        m.attrs.retain(|a| !a.path().is_ident("dotnet_event"));
        // `#[dotnet_property]` on a `get_<Prop>`/`set_<Prop>` trait fn: the fn becomes an ordinary
        // abstract accessor `MethodDef` (registered exactly like a plain method below), PLUS a
        // `rustc_codegen_clr_mark_last_abstract_property_get`/`_set` call binding it into a
        // §II.22.34 `Property` row named `<Prop>`. See the macro's top doc for the full contract.
        let mut property_name: Option<(String, bool)> = None;
        for attr in &m.attrs {
            if attr.path().is_ident("dotnet_property") {
                if !matches!(attr.meta, syn::Meta::Path(_)) {
                    return syn::Error::new(
                        attr.span(),
                        "#[dotnet_interface]: `#[dotnet_property]` takes no argument",
                    )
                    .to_compile_error()
                    .into();
                }
                let fname_str = m.sig.ident.to_string();
                let (prop, is_getter) = if let Some(rest) = fname_str.strip_prefix("get_") {
                    (rest, true)
                } else if let Some(rest) = fname_str.strip_prefix("set_") {
                    (rest, false)
                } else {
                    return syn::Error::new(
                        m.sig.ident.span(),
                        format!(
                            "#[dotnet_interface]: `#[dotnet_property]` fn `{fname_str}` must be \
                             named `get_<Prop>` or `set_<Prop>`"
                        ),
                    )
                    .to_compile_error()
                    .into();
                };
                if prop.is_empty() {
                    return syn::Error::new(
                        m.sig.ident.span(),
                        "#[dotnet_interface]: `#[dotnet_property]` accessor is missing the \
                         property name after `get_`/`set_`",
                    )
                    .to_compile_error()
                    .into();
                }
                property_name = Some((prop.to_string(), is_getter));
            }
        }
        m.attrs.retain(|a| !a.path().is_ident("dotnet_property"));
        if property_name.is_some() && is_event {
            return syn::Error::new(
                m.sig.ident.span(),
                "#[dotnet_interface]: `#[dotnet_property]` cannot be combined with \
                 `#[dotnet_event]`",
            )
            .to_compile_error()
            .into();
        }
        // A default body turns the member into a .NET *default interface method* (DIM) — handled
        // in the emission branch below, after the shared parameter validation.
        let has_default = m.default.is_some();
        // On a GENERIC trait a default body has no sound lowering: the lifted DIM is a free fn
        // where the trait's type parameters are not in scope (a `T`-typed value has no Rust
        // representation there — the carrier-side `RustcCLRInteropTypeGeneric<N>` marker is a
        // signature-only ZST, never a runtime value), and even a `T`-free body's `self.<m>(…)`
        // rewrite would clone `T`-spelled types into the free fn. Reject loudly.
        if has_default && !type_params.is_empty() {
            return syn::Error::new(
                m.span(),
                "#[dotnet_interface]: default method bodies are not supported on generic \
                 interfaces (`trait IFoo<T>`) yet — declare the member without a body and \
                 implement it on the C# side",
            )
            .to_compile_error()
            .into();
        }
        if has_default && is_event {
            return syn::Error::new(
                m.span(),
                "#[dotnet_interface]: a `#[dotnet_event]` declaration must have no body — the \
                 accessor pair is synthesized, there is nothing a default body could attach to",
            )
            .to_compile_error()
            .into();
        }
        if has_default && property_name.is_some() {
            return syn::Error::new(
                m.span(),
                "#[dotnet_interface]: `#[dotnet_property]` accessors cannot have a default body \
                 — declare them without a body and implement on the C# side",
            )
            .to_compile_error()
            .into();
        }
        // ---- method-level generic parameters (`fn Echo<T>(&self, value: T) -> T`) -> a genuine
        // generic .NET method DEFINITION: the member's signature blob carries `SIG_GENERIC` + a
        // `GenParamCount`, one METHOD-owned ECMA-335 `GenericParam` row (§II.22.20) is emitted
        // per parameter, and a bare `T` in a parameter/return position becomes
        // `ELEMENT_TYPE_MVAR` (`!!N` — C#'s method-level `T`). Same acceptance rules as the
        // trait-level list above: plain, unbounded, undefaulted type parameters only — every
        // other shape is rejected LOUDLY (`GenericParamConstraint` rows are not emitted, so a
        // bound would be silently-dropped metadata; lifetimes/consts have no .NET form).
        let mut method_type_params: Vec<syn::Ident> = Vec::new();
        for gp in &m.sig.generics.params {
            match gp {
                syn::GenericParam::Type(tp) => {
                    if tp.default.is_some() {
                        return syn::Error::new(
                            tp.span(),
                            "#[dotnet_interface]: type-parameter defaults (`fn f<T = i32>`) are \
                             not supported — .NET generic parameters have no default arguments",
                        )
                        .to_compile_error()
                        .into();
                    }
                    if !tp.bounds.is_empty() {
                        return syn::Error::new(
                            tp.bounds.span(),
                            "#[dotnet_interface]: type-parameter bounds (`fn f<T: Clone>`) are \
                             not supported on interface methods — `GenericParamConstraint` \
                             metadata is not emitted, so a bound would be silently dropped \
                             rather than enforced on the C# side; unconstrained type parameters \
                             only",
                        )
                        .to_compile_error()
                        .into();
                    }
                    method_type_params.push(tp.ident.clone());
                }
                syn::GenericParam::Lifetime(lt) => {
                    return syn::Error::new(
                        lt.span(),
                        "#[dotnet_interface]: lifetime parameters (`fn f<'a>`) are not supported \
                         on interface methods — .NET generics have no lifetime concept",
                    )
                    .to_compile_error()
                    .into();
                }
                syn::GenericParam::Const(cp) => {
                    return syn::Error::new(
                        cp.span(),
                        "#[dotnet_interface]: const parameters (`fn f<const N: usize>`) are not \
                         supported on interface methods — .NET generics are type-parameter-only",
                    )
                    .to_compile_error()
                    .into();
                }
            }
        }
        if let Some(wc) = &m.sig.generics.where_clause {
            return syn::Error::new(
                wc.span(),
                "#[dotnet_interface]: `where` clauses are not supported on interface methods — \
                 `GenericParamConstraint` metadata is not emitted, so a constraint would be \
                 silently dropped rather than enforced on the C# side",
            )
            .to_compile_error()
            .into();
        }
        if is_event && !method_type_params.is_empty() {
            return syn::Error::new(
                m.sig.generics.span(),
                "#[dotnet_interface]: a `#[dotnet_event]` declaration cannot take generic \
                 parameters — the synthesized `add_`/`remove_` accessors have a fixed \
                 `void (DelegateType)` shape",
            )
            .to_compile_error()
            .into();
        }
        if has_default && !method_type_params.is_empty() {
            return syn::Error::new(
                m.sig.generics.span(),
                "#[dotnet_interface]: default method bodies are not supported on generic \
                 methods (`fn f<T>(…) { … }`) — the lifted DIM body would need a generic IL \
                 body, which this backend doesn't emit; declare the member without a body and \
                 implement it on the C# side",
            )
            .to_compile_error()
            .into();
        }
        // Declaration-order index of each METHOD generic parameter (`T`->0, `U`->1, …) — the
        // `!!N` (`ELEMENT_TYPE_MVAR`) position a bare `T` lowers to. Disjoint from the
        // trait-level `param_index` by construction: rustc rejects a method generic parameter
        // shadowing the trait's (E0403) before this macro's output ever compiles.
        let method_param_index: std::collections::HashMap<String, usize> = method_type_params
            .iter()
            .enumerate()
            .map(|(i, id)| (id.to_string(), i))
            .collect();
        // The union of both generic namespaces, for the residual-mention scan: after the bare
        // substitutions below, ANY surviving mention of either kind of parameter is a composite
        // use with no faithful lowering.
        let all_param_index: std::collections::HashMap<String, usize> = param_index
            .iter()
            .chain(method_param_index.iter())
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        // Owned clone (not `&m.sig.ident`): the parameter loop below needs `&mut m.sig.inputs`
        // to strip `#[dotnet_out]` markers in place, which a live shared borrow would block.
        let fn_ident = m.sig.ident.clone();
        let fname_lit = LitStr::new(&fn_ident.to_string(), fn_ident.span());
        let carrier_ident = if is_event {
            format_ident!("__iface_sig_event_{}", fn_ident)
        } else {
            format_ident!("__iface_sig_{}", fn_ident)
        };

        // Build the carrier's parameter list. A leading `&self`/`self` receiver marks an INSTANCE
        // member: it becomes an explicit `_this: <Name>Handle` (the interface's own managed handle
        // — the receiver an instance method carries at signature-input 0). NO receiver marks a
        // **`static abstract`** member (.NET 7+ static virtual members in interfaces): the carrier
        // takes the parameter list verbatim, with no `_this` at all. Every non-receiver parameter
        // is kept verbatim in both cases.
        let is_static = !matches!(m.sig.inputs.first(), Some(FnArg::Receiver(_)));
        if property_name.is_some() && !method_type_params.is_empty() {
            return syn::Error::new(
                m.sig.generics.span(),
                "#[dotnet_interface]: `#[dotnet_property]` accessors cannot be generic",
            )
            .to_compile_error()
            .into();
        }
        if property_name.is_some() && is_static {
            return syn::Error::new(
                m.sig.ident.span(),
                "#[dotnet_interface]: `#[dotnet_property]` accessors must take `&self`/`&mut \
                 self` — static (receiver-less) properties are not supported",
            )
            .to_compile_error()
            .into();
        }
        if is_static && !method_type_params.is_empty() {
            return syn::Error::new(
                m.sig.generics.span(),
                "#[dotnet_interface]: generic parameters are not supported on static \
                 (receiver-less) interface members yet — a generic `static abstract` \
                 (`static abstract T Create<T>()`) is a metadata shape this backend doesn't \
                 emit; take `&self` or drop the generic parameters",
            )
            .to_compile_error()
            .into();
        }
        if has_default && is_static {
            return syn::Error::new(
                m.span(),
                "#[dotnet_interface]: default bodies are only supported on instance (`&self`) \
                 members — a `static` (receiver-less) member with a body would be a *non-abstract \
                 static virtual*, a metadata shape this backend doesn't emit yet",
            )
            .to_compile_error()
            .into();
        }
        let mut carrier_inputs: Punctuated<FnArg, Token![,]> = Punctuated::new();
        if !is_static {
            // On a GENERIC trait the public `<Name>Handle<T,…>` alias is itself parameterized —
            // a carrier can't name it without arguments, so carriers use the module-private
            // NON-generic `__IfaceDefHandle` alias instead (declared in the entry module below,
            // pointing at the backtick-arity .NET name). The receiver is signature-input 0,
            // which every exporter STRIPS before encoding (`export.rs` Pass 3 / `method_token`),
            // so its exact shape is inert — it only needs to lower to a `ClassRef` of the
            // interface's own def so the comptime layer can thread it.
            let recv: FnArg = if type_params.is_empty() {
                syn::parse_quote!(_this: #handle_ident)
            } else {
                syn::parse_quote!(_this: __IfaceDefHandle)
            };
            carrier_inputs.push(recv);
        }
        // 1-based positions (among the C#-visible, receiver-stripped parameters — exactly the
        // emitted `Param` row Sequence numbers) of `#[dotnet_out]`-marked `&mut T` parameters.
        let mut out_sequences: Vec<u16> = Vec::new();
        let mut param_seq: u16 = 0;
        for arg in m.sig.inputs.iter_mut().skip(usize::from(!is_static)) {
            match arg {
                FnArg::Typed(pt) => {
                    param_seq += 1;
                    // `#[dotnet_out]` parameter marker: BYREF + `ParamAttributes.Out` => C# `out`.
                    // Detect, validate its form, then STRIP it from the trait we re-emit AND from
                    // the carrier clone below (it is not a real registered attribute — leaving it
                    // in would be a "cannot find attribute" error; the same strip idiom as
                    // `#[dotnet_event]` above).
                    let mut is_out = false;
                    for attr in &pt.attrs {
                        if attr.path().is_ident("dotnet_out") {
                            if !matches!(attr.meta, syn::Meta::Path(_)) {
                                return syn::Error::new(
                                    attr.span(),
                                    "#[dotnet_interface]: `#[dotnet_out]` takes no arguments — \
                                     write `#[dotnet_out] name: &mut T`",
                                )
                                .to_compile_error()
                                .into();
                            }
                            is_out = true;
                        }
                    }
                    pt.attrs.retain(|a| !a.path().is_ident("dotnet_out"));
                    // Reference-typed parameters: `&mut T` maps to a managed byref (C# `ref T` /
                    // `out T`); everything else reference-shaped is rejected loudly.
                    if let syn::Type::Reference(r) = &*pt.ty {
                        if has_default {
                            return syn::Error::new(
                                pt.ty.span(),
                                "#[dotnet_interface]: reference parameters (`&T`/`&mut T`) are \
                                 not supported on a method with a default body — the lifted DIM \
                                 body compiles with raw-pointer lowering, which would not match \
                                 the interface's managed-byref (`ref T`) surface; pass by value \
                                 or drop the default body",
                            )
                            .to_compile_error()
                            .into();
                        }
                        if is_event {
                            return syn::Error::new(
                                pt.ty.span(),
                                format!(
                                    "#[dotnet_interface]: event `{fn_ident}`'s parameter must be \
                                     the subscriber delegate handle by value, not a reference"
                                ),
                            )
                            .to_compile_error()
                            .into();
                        }
                        if r.mutability.is_none() {
                            return syn::Error::new(
                                pt.ty.span(),
                                "#[dotnet_interface]: shared-reference parameters (`&T`) are not \
                                 supported on interface methods — C# `in T` would need \
                                 `modreq(InAttribute)`. Use `&mut T` (C# `ref T`), pass by value, \
                                 or use a raw pointer (`*const T` => C# `T*`)",
                            )
                            .to_compile_error()
                            .into();
                        }
                    }
                    if is_out {
                        if is_event {
                            return syn::Error::new(
                                pt.span(),
                                format!(
                                    "#[dotnet_interface]: `#[dotnet_out]` is not valid on event \
                                     `{fn_ident}`'s parameter — event accessor signatures are \
                                     synthesized"
                                ),
                            )
                            .to_compile_error()
                            .into();
                        }
                        if is_static {
                            return syn::Error::new(
                                pt.span(),
                                "#[dotnet_interface]: `#[dotnet_out]` is not supported on static \
                                 (receiver-less) interface members yet — use plain `&mut T` \
                                 (C# `ref T`) instead",
                            )
                            .to_compile_error()
                            .into();
                        }
                        if !matches!(&*pt.ty, syn::Type::Reference(r) if r.mutability.is_some()) {
                            return syn::Error::new(
                                pt.span(),
                                "#[dotnet_interface]: `#[dotnet_out]` is only valid on a \
                                 `&mut T` parameter (spelled literally — an alias hiding the \
                                 reference is not accepted)",
                            )
                            .to_compile_error()
                            .into();
                        }
                        out_sequences.push(param_seq);
                    }
                    // GENERIC trait and/or GENERIC method: a parameter typed EXACTLY as a bare
                    // generic parameter (`x: T`) becomes the matching signature marker in the
                    // carrier — the METHOD's own parameters take priority-by-namespace (they are
                    // disjoint from the trait's, see `method_param_index`'s comment) and map to
                    // `RustcCLRInteropMethodGeneric<N>` (lowered to `ELEMENT_TYPE_MVAR N`, the
                    // member's `!!N` position); the TRAIT's map to `RustcCLRInteropTypeGeneric
                    // <N>` (`ELEMENT_TYPE_VAR N`, `!N`). Any OTHER use of either kind of
                    // parameter (`&T`, `Vec<T>`, `(T,)`, …) has no faithful lowering and is
                    // rejected loudly below (after these replacements, any surviving mention IS
                    // such a use). The re-emitted trait keeps the original spelling — only the
                    // carrier clone is rewritten.
                    let mut carrier_pt = pt.clone();
                    if !all_param_index.is_empty() {
                        if let Some(idx) = bare_generic_param(&carrier_pt.ty, &method_param_index) {
                            // `is_event && !method_type_params.is_empty()` was rejected above —
                            // a method-generic marker can't reach an event's carrier.
                            let idx_lit = proc_macro2::Literal::usize_unsuffixed(idx);
                            carrier_pt.ty = Box::new(syn::parse_quote!(
                                ::mycorrhiza::intrinsics::RustcCLRInteropMethodGeneric<#idx_lit>
                            ));
                        } else if let Some(idx) = bare_generic_param(&carrier_pt.ty, &param_index) {
                            if is_event {
                                return syn::Error::new(
                                    pt.ty.span(),
                                    format!(
                                        "#[dotnet_interface]: event `{fn_ident}`'s parameter \
                                         must be a concrete delegate handle — a generic \
                                         parameter (`{}`) cannot be an event's delegate type",
                                        type_params[idx]
                                    ),
                                )
                                .to_compile_error()
                                .into();
                            }
                            let idx_lit = proc_macro2::Literal::usize_unsuffixed(idx);
                            carrier_pt.ty = Box::new(syn::parse_quote!(
                                ::mycorrhiza::intrinsics::RustcCLRInteropTypeGeneric<#idx_lit>
                            ));
                        } else if let Some(bad) = first_generic_param_mention(
                            quote::ToTokens::to_token_stream(&carrier_pt.ty),
                            &all_param_index,
                        ) {
                            return syn::Error::new(
                                pt.ty.span(),
                                format!(
                                    "#[dotnet_interface]: generic parameter `{bad}` may only \
                                     appear as a BARE parameter or return type on an interface \
                                     member (`x: {bad}` / `-> {bad}`) — composite uses \
                                     (`&{bad}`, `Vec<{bad}>`, tuples, nested generics, …) have \
                                     no .NET signature lowering here"
                                ),
                            )
                            .to_compile_error()
                            .into();
                        }
                    }
                    carrier_inputs.push(FnArg::Typed(carrier_pt));
                }
                FnArg::Receiver(r) => {
                    return syn::Error::new(
                        r.span(),
                        "#[dotnet_interface]: `self` is only allowed as the first parameter",
                    )
                    .to_compile_error()
                    .into();
                }
            }
        }
        // Byref returns (`fn f(&self) -> &mut i32`) have no supported mapping (C# `ref` returns
        // are a different metadata shape than we emit) — reject loudly rather than emit `T*`.
        if let ReturnType::Type(_, ty) = &m.sig.output {
            if matches!(&**ty, syn::Type::Reference(_)) {
                return syn::Error::new(
                    ty.span(),
                    "#[dotnet_interface]: reference returns (`-> &T` / `-> &mut T`) are not \
                     supported on interface methods",
                )
                .to_compile_error()
                .into();
            }
        }
        let output = &m.sig.output;
        // The CARRIER's return type: a bare `-> T` naming a METHOD generic parameter becomes
        // the `RustcCLRInteropMethodGeneric<N>` marker (`!!N`); one naming a TRAIT parameter
        // becomes `RustcCLRInteropTypeGeneric<N>` (`!N`) — same rewrite + same loud
        // composite-use reject as the parameter loop above; everything else passes through
        // verbatim.
        let carrier_output: ReturnType = match &m.sig.output {
            ReturnType::Default => ReturnType::Default,
            ReturnType::Type(arrow, ty) => {
                if let Some(idx) = bare_generic_param(ty, &method_param_index) {
                    let idx_lit = proc_macro2::Literal::usize_unsuffixed(idx);
                    ReturnType::Type(
                        *arrow,
                        Box::new(syn::parse_quote!(
                            ::mycorrhiza::intrinsics::RustcCLRInteropMethodGeneric<#idx_lit>
                        )),
                    )
                } else if let Some(idx) = bare_generic_param(ty, &param_index) {
                    let idx_lit = proc_macro2::Literal::usize_unsuffixed(idx);
                    ReturnType::Type(
                        *arrow,
                        Box::new(syn::parse_quote!(
                            ::mycorrhiza::intrinsics::RustcCLRInteropTypeGeneric<#idx_lit>
                        )),
                    )
                } else if let Some(bad) = first_generic_param_mention(
                    quote::ToTokens::to_token_stream(ty),
                    &all_param_index,
                ) {
                    return syn::Error::new(
                        ty.span(),
                        format!(
                            "#[dotnet_interface]: generic parameter `{bad}` may only appear as \
                             a BARE parameter or return type on an interface member (`x: {bad}` \
                             / `-> {bad}`) — composite uses (`&{bad}`, `Vec<{bad}>`, tuples, \
                             nested generics, …) have no .NET signature lowering here"
                        ),
                    )
                    .to_compile_error()
                    .into();
                } else {
                    ReturnType::Type(*arrow, ty.clone())
                }
            }
        };

        // `#[dotnet_property]` accessor shape: a getter takes ONLY `&self` and returns the
        // property's value; a setter takes `&self`/`&mut self` plus exactly ONE BY-VALUE
        // parameter (no other .NET property setter shape exists in C#) and returns nothing.
        // Bookkept in `property_members` (declaration order, NOT a HashMap — the write-only
        // check below must fire deterministically) for the post-loop write-only-property check;
        // a duplicate half (two getters/two setters for the same property) is rejected here.
        if let Some((prop_name, is_getter)) = &property_name {
            if *is_getter {
                if carrier_inputs.len() != 1 {
                    return syn::Error::new(
                        m.sig.span(),
                        format!(
                            "#[dotnet_interface]: property getter `{fn_ident}` must take only \
                             `&self` — no other parameters"
                        ),
                    )
                    .to_compile_error()
                    .into();
                }
                if matches!(m.sig.output, ReturnType::Default) {
                    return syn::Error::new(
                        m.sig.span(),
                        format!(
                            "#[dotnet_interface]: property getter `{fn_ident}` must return the \
                             property's value"
                        ),
                    )
                    .to_compile_error()
                    .into();
                }
            } else {
                if carrier_inputs.len() != 2 {
                    return syn::Error::new(
                        m.sig.span(),
                        format!(
                            "#[dotnet_interface]: property setter `{fn_ident}` must take exactly \
                             one parameter besides the receiver — the property's new value"
                        ),
                    )
                    .to_compile_error()
                    .into();
                }
                if !matches!(m.sig.output, ReturnType::Default) {
                    return syn::Error::new(
                        m.sig.output.span(),
                        format!(
                            "#[dotnet_interface]: property setter `{fn_ident}` must not return a \
                             value"
                        ),
                    )
                    .to_compile_error()
                    .into();
                }
                // The general per-parameter loop above allows a `&mut T` parameter through
                // (it becomes a C# `ref`/`out` parameter on an ordinary method) — but no C#
                // property setter can be `ref`/`out`-valued, so reject it here explicitly rather
                // than silently emitting an unimplementable property.
                if let Some(FnArg::Typed(pt)) = m.sig.inputs.iter().nth(1) {
                    if matches!(&*pt.ty, syn::Type::Reference(_)) {
                        return syn::Error::new(
                            pt.ty.span(),
                            format!(
                                "#[dotnet_interface]: property setter `{fn_ident}`'s value \
                                 parameter must be passed by value, not by reference"
                            ),
                        )
                        .to_compile_error()
                        .into();
                    }
                }
            }
            let slot_idx = property_members
                .iter()
                .position(|(name, _)| name == prop_name);
            let idx = slot_idx.unwrap_or_else(|| {
                property_members.push((prop_name.clone(), (None, None)));
                property_members.len() - 1
            });
            let (getter_span, setter_span) = &mut property_members[idx].1;
            if *is_getter {
                if getter_span.is_some() {
                    return syn::Error::new(
                        fn_ident.span(),
                        format!("#[dotnet_interface]: property `{prop_name}` declares two getters"),
                    )
                    .to_compile_error()
                    .into();
                }
                *getter_span = Some(fn_ident.span());
            } else {
                if setter_span.is_some() {
                    return syn::Error::new(
                        fn_ident.span(),
                        format!("#[dotnet_interface]: property `{prop_name}` declares two setters"),
                    )
                    .to_compile_error()
                    .into();
                }
                *setter_span = Some(fn_ident.span());
            }
        }

        // Record the .NET member names this item declares, rejecting duplicates loudly.
        let mut declare_member = |name: String, what: String| -> Option<TokenStream> {
            if let Some(prev) = member_names.insert(name.clone(), what.clone()) {
                let cur = &member_names[&name];
                return Some(
                    syn::Error::new(
                        fn_ident.span(),
                        format!(
                            "#[dotnet_interface]: interface member name `{name}` is declared \
                             twice: {prev} and {cur} — rename one of them"
                        ),
                    )
                    .to_compile_error()
                    .into(),
                );
            }
            None
        };

        if is_event {
            // Interface events are INSTANCE members: their accessors dispatch through an object
            // reference (`obj.Clicked += …`). A "static abstract event" is technically legal in
            // .NET 7+ metadata but has no consumer story here — reject it loudly rather than
            // synthesize accessors of the wrong kind.
            if is_static {
                return syn::Error::new(
                    fn_ident.span(),
                    format!(
                        "#[dotnet_interface]: event `{fn_ident}` must take `&self` as its first \
                         parameter — static abstract events are not supported"
                    ),
                )
                .to_compile_error()
                .into();
            }
            // Event shape guards: exactly ONE non-receiver parameter (the subscriber delegate)
            // and no return value — the synthesized `add_`/`remove_` accessors are
            // `void (DelegateType)`, so anything else cannot be represented.
            if carrier_inputs.len() != 2 {
                return syn::Error::new(
                    m.sig.span(),
                    format!(
                        "#[dotnet_interface]: event `{fn_ident}` must take exactly one parameter \
                         besides `&self` — the subscriber delegate, e.g. \
                         `fn {fn_ident}(&self, handler: ActionHandle)`"
                    ),
                )
                .to_compile_error()
                .into();
            }
            if !matches!(m.sig.output, ReturnType::Default) {
                return syn::Error::new(
                    m.sig.output.span(),
                    format!(
                        "#[dotnet_interface]: event `{fn_ident}` must not declare a return type — \
                         event accessors are `void`"
                    ),
                )
                .to_compile_error()
                .into();
            }
            let event_name = fn_ident.to_string();
            let add_lit = LitStr::new(&format!("add_{event_name}"), fn_ident.span());
            let remove_lit = LitStr::new(&format!("remove_{event_name}"), fn_ident.span());
            for accessor in [format!("add_{event_name}"), format!("remove_{event_name}")] {
                if let Some(err) = declare_member(
                    accessor.clone(),
                    format!("the `{accessor}` accessor synthesized for event `{event_name}`"),
                ) {
                    return err;
                }
            }
            // ONE shared signature carrier: `add` and `remove` have identical signatures, and
            // `rustc_codegen_clr_add_abstract_method_def` only READS the carrier's signature
            // (never aliases/codegens it), so a single carrier serves both accessors.
            carriers.push(quote! {
                fn #carrier_ident(#carrier_inputs) { ::core::unimplemented!() }
            });
            method_calls.push(quote! {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_abstract_method_def::<
                    #add_lit, _,
                >(class, #carrier_ident);
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_mark_last_method_event_add::<
                    #fname_lit,
                >(class);
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_abstract_method_def::<
                    #remove_lit, _,
                >(class, #carrier_ident);
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_mark_last_method_event_remove::<
                    #fname_lit,
                >(class);
            });
        } else if has_default {
            // ---- Default interface method (DIM). The body is LIFTED out of the trait into a
            // real, codegen'd free fn: `self` becomes the explicit `this` handle, and every
            // `self.<trait_method>(…)` call is rewritten to a `callvirt` through that handle
            // (see `DimRewriter`). The interface member is then a virtual, NON-abstract
            // `MethodDef` aliasing this fn — same pipeline as a `#[dotnet_class]` virtual, just
            // owned by an interface `TypeDef`. The trait itself is re-emitted verbatim (original
            // body intact), so Rust-side trait semantics are unchanged.
            if let Some(err) = declare_member(fn_ident.to_string(), format!("method `{fn_ident}`"))
            {
                return err;
            }
            let dim_ident = format_ident!("__iface_dim_{}", fn_ident);
            // The lifted fn's parameters: the carrier list with the receiver renamed `_this` →
            // `this` (the DIM body actually USES it).
            let mut dim_inputs = carrier_inputs.clone();
            *dim_inputs
                .first_mut()
                .expect("instance member: carrier_inputs always starts with the receiver") =
                syn::parse_quote!(this: #handle_ident);
            let mut body = m.default.clone().expect("has_default was just checked");
            let mut rewriter = DimRewriter {
                callees: &dim_callees,
                errors: Vec::new(),
            };
            syn::visit_mut::VisitMut::visit_block_mut(&mut rewriter, &mut body);
            if let Some(err) = rewriter.errors.into_iter().reduce(|mut a, b| {
                a.combine(b);
                a
            }) {
                return err.to_compile_error().into();
            }
            // BACKSTOP: the AST rewrite cannot see into macro-invocation token streams
            // (`println!("{}", self.x())`) or shapes it doesn't model — any `self`/`Self` ident
            // still in the rewritten body means an unsupported use, rejected loudly here rather
            // than as a confusing rustc error deep inside macro-generated code.
            if tokens_contain_self_ident(quote::ToTokens::to_token_stream(&body)) {
                return syn::Error::new(
                    m.span(),
                    format!(
                        "#[dotnet_interface]: `{fn_ident}`'s default body uses `self`/`Self` in \
                         a form that can't be lowered to .NET interface dispatch — only \
                         `self.<trait_method>(…)` calls (at most 2 arguments) and passing `self` \
                         to handle-taking helpers are supported; `self` inside a macro invocation \
                         (e.g. `println!(\"{{}}\", self.x())`) must be hoisted to a `let` binding \
                         first"
                    ),
                )
                .to_compile_error()
                .into();
            }
            // `#[used]` fn-pointer anchor: the lifted fn has a REAL body the mono-collector must
            // codegen (nothing calls it — the entrypoint only NAMES it and is interpreted, not
            // codegen'd; without this the `AliasFor` edge would dangle). Exact idiom of
            // `#[dotnet_methods]`' KEEP anchors.
            let in_tys: Vec<Type> = dim_inputs
                .iter()
                .map(|arg| match arg {
                    FnArg::Typed(pt) => (*pt.ty).clone(),
                    FnArg::Receiver(_) => unreachable!("dim_inputs holds no receiver"),
                })
                .collect();
            let out_tokens = match output {
                ReturnType::Default => quote! { () },
                ReturnType::Type(_, ty) => quote! { #ty },
            };
            let keep_ident = format_ident!("KEEP_DIM_{}", fn_ident);
            carriers.push(quote! {
                fn #dim_ident(#dim_inputs) #output #body
                #[used]
                static #keep_ident: fn(#(#in_tys),*) -> #out_tokens = #dim_ident;
            });
            method_calls.push(quote! {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_default_method_def::<
                    #fname_lit, _,
                >(class, #dim_ident);
            });
        } else {
            if let Some(err) = declare_member(fn_ident.to_string(), format!("method `{fn_ident}`"))
            {
                return err;
            }
            carriers.push(quote! {
                fn #carrier_ident(#carrier_inputs) #carrier_output { ::core::unimplemented!() }
            });
            if is_static {
                // No `self` receiver -> a `static abstract` interface member (.NET 7+ static
                // virtual members in interfaces). The carrier has no `_this`; its signature is
                // the member's C#-visible parameter list verbatim.
                method_calls.push(quote! {
                    let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_static_abstract_method_def::<
                        #fname_lit, _,
                    >(class, #carrier_ident);
                });
            } else if !method_type_params.is_empty() {
                // A generic method DEFINITION: the declared type-parameter names ride as a
                // `;`-separated list (declaration order — the same `;`-list convention as
                // `set_type_generics`) so the backend emits `SIG_GENERIC` + method-owned
                // `GenericParam` rows. Instance-only by the guards above (no events, no
                // defaults, no statics).
                let gparams_csv = method_type_params
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(";");
                let gparams_lit = LitStr::new(&gparams_csv, fn_ident.span());
                method_calls.push(quote! {
                    let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_generic_abstract_method_def::<
                        #fname_lit, #gparams_lit, _,
                    >(class, #carrier_ident);
                });
            } else {
                method_calls.push(quote! {
                    let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_abstract_method_def::<
                        #fname_lit, _,
                    >(class, #carrier_ident);
                });
                // `#[dotnet_property]`: bind the abstract member just registered above as a
                // property getter/setter (`rustc_codegen_clr_mark_last_abstract_property_get`/
                // `_set` — the same "mark the LAST member" contract as `#[dotnet_out]` below).
                // Guarded above to only reach here as a plain (non-static, non-generic) instance
                // member.
                if let Some((prop_name, is_getter)) = &property_name {
                    let prop_lit = LitStr::new(prop_name, fn_ident.span());
                    if *is_getter {
                        method_calls.push(quote! {
                            let class = ::mycorrhiza::comptime::rustc_codegen_clr_mark_last_abstract_property_get::<
                                #prop_lit,
                            >(class);
                        });
                    } else {
                        method_calls.push(quote! {
                            let class = ::mycorrhiza::comptime::rustc_codegen_clr_mark_last_abstract_property_set::<
                                #prop_lit,
                            >(class);
                        });
                    }
                }
            }
            // `#[dotnet_out]` positions ride as a CSV of 1-based (receiver-stripped) parameter
            // positions in a `&'static str` const generic, marking the member the
            // `add_abstract_method_def`/`add_generic_abstract_method_def` call directly above
            // just registered (the same "mark the LAST member" contract as
            // `#[dotnet_override]`/`#[dotnet_event]`; both intrinsics push to the same
            // `abstract_methods` list the marker mutates). Always empty for a static member —
            // the parameter loop rejects `#[dotnet_out]` there.
            if !out_sequences.is_empty() {
                let csv = out_sequences
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                let out_lit = LitStr::new(&csv, fn_ident.span());
                method_calls.push(quote! {
                    let class = ::mycorrhiza::comptime::rustc_codegen_clr_mark_last_abstract_method_out_params::<
                        #out_lit,
                    >(class);
                });
            }
        }
    }

    // A `#[dotnet_property]` setter with no matching getter is a write-only property — C# has
    // no idiomatic write-only-property surface, so reject it loudly here (a macro-level
    // frontstop; `src/comptime.rs`'s `finish_type` carries the same check as a backstop for a
    // hand-rolled entrypoint).
    for (prop_name, (getter_span, setter_span)) in &property_members {
        if getter_span.is_none() {
            let span = setter_span.expect("a property entry always has at least one accessor");
            return syn::Error::new(
                span,
                format!(
                    "#[dotnet_interface]: property `{prop_name}` has a `set_{prop_name}` \
                     accessor but no `get_{prop_name}` — write-only properties are not \
                     supported; add `#[dotnet_property] fn get_{prop_name}(&self) -> ...`"
                ),
            )
            .to_compile_error()
            .into();
        }
    }

    // The public managed-handle alias. Non-generic: a plain `RustcCLRInteropManagedClass`
    // reference (unchanged surface). Generic: a PARAMETERIZED alias over
    // `RustcCLRInteropManagedGeneric` — `IBoxHandle<i32>` lowers to the instantiated
    // `ClassRef("IBox`1", None, [int32])`, usable in `#[dotnet_export]` signatures etc. (the
    // trailing comma makes the type-argument tuple `(T,)` well-formed at arity 1).
    let handle_alias = if type_params.is_empty() {
        quote! {
            /// Managed handle to this Rust-defined .NET interface (Rust-side references; C#
            /// refers to the interface by its plain name).
            #[allow(non_camel_case_types, dead_code)]
            pub type #handle_ident =
                ::mycorrhiza::intrinsics::RustcCLRInteropManagedClass<"", #name_lit>;
        }
    } else {
        quote! {
            /// Managed handle to an INSTANTIATION of this Rust-defined generic .NET interface
            /// (e.g. `IBoxHandle<i32>` = `IBox<int>`). C# refers to the interface by its plain
            /// generic name.
            #[allow(non_camel_case_types, dead_code)]
            pub type #handle_ident<#(#type_params),*> =
                ::mycorrhiza::intrinsics::RustcCLRInteropManagedGeneric<
                    "", #name_lit, (#(#type_params,)*),
                >;
        }
    };
    // Generic traits: (a) carriers need a NON-generic receiver alias (the public handle alias
    // is parameterized — see the receiver comment above); (b) the entrypoint declares the
    // parameter names via `set_type_generics` (`;`-separated, declaration order).
    let iface_def_handle_alias = if type_params.is_empty() {
        quote! {}
    } else {
        quote! {
            /// The open interface definition's own handle — carrier-receiver use only (stripped
            /// as signature-input 0 by every exporter).
            #[allow(non_camel_case_types, dead_code)]
            type __IfaceDefHandle =
                ::mycorrhiza::intrinsics::RustcCLRInteropManagedClass<"", #name_lit>;
        }
    };
    let set_generics_call = if type_params.is_empty() {
        quote! {}
    } else {
        let names_csv = type_params
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(";");
        let names_lit = LitStr::new(&names_csv, span);
        quote! {
            let class = ::mycorrhiza::comptime::rustc_codegen_clr_set_type_generics::<
                #names_lit,
            >(class);
        }
    };

    let expanded = quote! {
        // The trait, unchanged — a declaration vehicle only.
        #input

        #handle_alias

        #[allow(non_snake_case, dead_code, unused_variables, internal_features, clippy::diverging_sub_expression)]
        mod #entry_mod {
            use super::*;
            #iface_def_handle_alias
            // Signature-only carriers (named ONLY in the interpreted entrypoint below, so the
            // mono-collector never codegens them — an abstract interface member has no body),
            // plus, for default interface methods, the LIFTED real bodies with their `#[used]`
            // KEEP anchors (those MUST be codegen'd — the interface member aliases them).
            #(#carriers)*
            // The comptime interpreter only *reads* this fn's MIR; a `#[used]` root keeps it (and
            // the interface) from being dropped as dead code.
            #[used]
            static PREVENT_DCE: fn() = rustc_codegen_clr_comptime_entrypoint;
            #[inline(never)]
            pub fn rustc_codegen_clr_comptime_entrypoint() {
                // An interface has no base type -> empty superclass args. `HAS_TYPE_KIND_OPINION
                // = true`: an interface is always registered fresh by exactly one entrypoint (no
                // `#[dotnet_methods]`-style re-opening), so `false` here is a real, authoritative
                // opinion, not a placeholder.
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_new_typedef::<
                    #name_lit, false, "", "", true,
                >();
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_mark_interface(class);
                // Generic interface (`trait IFoo<T>`): declare the parameter names — one
                // ECMA-335 `GenericParam` row each on this TypeDef.
                #set_generics_call
                // Base interfaces (supertraits + external `implements = "…"`): one
                // `InterfaceImpl` row each on this interface's TypeDef (§II.10.1.3).
                #(#base_iface_calls)*
                #(#method_calls)*
                ::mycorrhiza::comptime::rustc_codegen_clr_finish_type(class);
            }
        }
    };
    expanded.into()
}

// ============================================================================
// #[dotnet_methods] — attach static / instance methods to a `#[dotnet_class]` type.
// ============================================================================

/// The full C# spec (§14.10.2) / ECMA-335 set of CLR operator-overload method names — the exact
/// same list `src/comptime.rs::CLR_OPERATOR_METHOD_NAMES` carries (duplicated here deliberately,
/// matching the precedent `ATTR_DENYLIST_NAMESPACES` already sets for `dotnet_class`'s custom-attr
/// denylist: this crate has no dependency on the backend, so the two lists are kept in sync by
/// hand rather than shared). Used to force operator-named methods to dispatch as STATIC (see
/// `is_operator_name` below) regardless of their first parameter's shape.
const CLR_OPERATOR_METHOD_NAMES: &[&str] = &[
    "op_Decrement",
    "op_Increment",
    "op_UnaryNegation",
    "op_UnaryPlus",
    "op_LogicalNot",
    "op_OnesComplement",
    "op_True",
    "op_False",
    "op_Addition",
    "op_Subtraction",
    "op_Multiply",
    "op_Division",
    "op_Modulus",
    "op_ExclusiveOr",
    "op_BitwiseAnd",
    "op_BitwiseOr",
    "op_LeftShift",
    "op_RightShift",
    "op_UnsignedRightShift",
    "op_Equality",
    "op_GreaterThan",
    "op_LessThan",
    "op_Inequality",
    "op_GreaterThanOrEqual",
    "op_LessThanOrEqual",
    "op_Implicit",
    "op_Explicit",
];

/// `#[dotnet_methods]` on an inherent `impl <Name> { … }` block attaches the block's functions to the
/// managed class `<Name>` that a `#[dotnet_class] struct <Name>` declared. It emits a *second* comptime
/// entrypoint that re-opens `<Name>` (the backend's `finish_type` is idempotent — it reuses the
/// already-registered class and just appends these methods) and adds one method per `fn`:
///
///   * a `fn` whose FIRST parameter is `<Name>Handle` (the managed-handle alias `#[dotnet_class]` emits)
///     becomes a **virtual instance** method `<Name>.method(this, …)` — C# calls `obj.method(…)`;
///   * any other `fn` becomes a **static** method `<Name>.method(…)` — C# calls `<Name>.method(…)`;
///   * a fn named after a CLR operator (`op_Addition`, `op_Equality`, …, see
///     `CLR_OPERATOR_METHOD_NAMES`) is ALWAYS static and gets `SpecialName` stamped, so C# binds
///     the real `+`/`==`/etc. syntax to it — even though its params look instance-shaped (a binary
///     operator's operands are typically both `<Name>Handle`).
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
    // Generated marshalling shims (see the `needs_shim` block below) — free fns emitted at the SAME
    // top level as `#input`/`mod #entry_mod` (not inside either), so `entry_mod`'s `use super::*;`
    // brings them into scope by name, and each shim's body can plainly call `#self_ty::#fn_ident`.
    let mut shim_items = Vec::new();
    for it in &mut input.items {
        let ImplItem::Fn(f) = it else {
            return syn::Error::new(
                it.span(),
                "#[dotnet_methods]: only `fn` items are supported in the impl block",
            )
            .to_compile_error()
            .into();
        };
        // `async fn` has no faithful lowering here: the method is aliased (or shimmed) with its
        // ordinary Rust signature, which for an `async fn` is a compiler-generated coroutine type,
        // not the `Task`/`Task<T>`-returning shape a managed caller would expect — same rejection
        // axis `#[dotnet_export]`/`#[dotnet_interface]` already apply (the `mycorrhiza::task`
        // Task/async bridge is the separate, explicit surface for this). Previously unchecked here,
        // so an `async fn` silently fell through to a confusing failure deep in codegen instead of
        // this clear compile-time error.
        if let Some(a) = &f.sig.asyncness {
            return syn::Error::new(
                a.span(),
                "#[dotnet_methods]: `async fn` methods are not supported — the emitted .NET \
                 member would alias the compiler-generated coroutine type, not a `Task`/`Task<T>`-\
                 returning method. Build a `Task`/`TaskT<T>` yourself (see `mycorrhiza::task`) and \
                 return it from an ordinary (non-`async`) method instead.",
            )
            .to_compile_error()
            .into();
        }
        // `#[dotnet(name = "PascalCase")]` keeps the Rust implementation idiomatically
        // snake_case while choosing the public CLR member name. This mirrors `#[dotnet_export]`'s
        // name option but applies to methods attached to a generated class.
        let mut managed_method_name = f.sig.ident.to_string();
        let mut saw_dotnet_name = false;
        for attr in &f.attrs {
            if attr.path().is_ident("dotnet") {
                if saw_dotnet_name {
                    return syn::Error::new(
                        attr.span(),
                        "duplicate #[dotnet(...)] method attribute",
                    )
                    .to_compile_error()
                    .into();
                }
                let tokens = match &attr.meta {
                    syn::Meta::List(list) => list.tokens.clone(),
                    _ => {
                        return syn::Error::new(
                            attr.span(),
                            "#[dotnet(name = \"ManagedName\")]: expected a name argument",
                        )
                        .to_compile_error()
                        .into();
                    }
                };
                let args = match parse_dotnet_export_args(tokens) {
                    Ok(args) if !args.error_exception => args,
                    Ok(_) => {
                        return syn::Error::new(
                            attr.span(),
                            "#[dotnet_methods] only supports #[dotnet(name = \"ManagedName\")]",
                        )
                        .to_compile_error()
                        .into();
                    }
                    Err(error) => return error.to_compile_error().into(),
                };
                let Some(name) = args.name else {
                    return syn::Error::new(
                        attr.span(),
                        "#[dotnet(name = \"ManagedName\")]: name is required",
                    )
                    .to_compile_error()
                    .into();
                };
                managed_method_name = name;
                saw_dotnet_name = true;
            }
        }
        f.attrs
            .retain(|attribute| !attribute.path().is_ident("dotnet"));
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

        // `#[dotnet_event("Name")]` — links this method into a `.NET` event's `add_*`/`remove_*`
        // half (see `rustc_codegen_clr_mark_last_method_event_add`'s doc). Which half is decided
        // by the fn's own name prefix (`add_`/`remove_`), matching the real C# codegen convention
        // for events — not a separate attribute argument, since the method already has to be named
        // that way for the pair to link up on the .NET side regardless.
        let mut event_name: Option<String> = None;
        for attr in &f.attrs {
            if attr.path().is_ident("dotnet_event") {
                let spec = match attr.parse_args::<syn::LitStr>() {
                    Ok(lit) => lit.value(),
                    Err(_) => {
                        return syn::Error::new(
                            attr.span(),
                            "#[dotnet_event(\"Name\")]: expected a single string literal \
                             argument naming the event",
                        )
                        .to_compile_error()
                        .into();
                    }
                };
                event_name = Some(spec);
            }
        }
        f.attrs.retain(|a| !a.path().is_ident("dotnet_event"));
        let event_role = match &event_name {
            Some(_) => {
                let fn_name = managed_method_name.as_str();
                if fn_name.starts_with("add_") {
                    Some(true)
                } else if fn_name.starts_with("remove_") {
                    Some(false)
                } else {
                    return syn::Error::new(
                        f.sig.ident.span(),
                        "#[dotnet_event(\"Name\")]: the method name must start with `add_` or \
                         `remove_` (matching the real C# event codegen convention) so its role \
                         is unambiguous",
                    )
                    .to_compile_error()
                    .into();
                }
            }
            None => None,
        };

        let fn_ident = &f.sig.ident;
        let fname_lit = LitStr::new(&managed_method_name, fn_ident.span());

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
        // A CLR operator-overload name (`op_Addition`, `op_Equality`, …) is ALWAYS static in .NET —
        // never dispatch it as instance just because its first param happens to be `<Name>Handle`
        // (a binary operator's left-hand operand naturally has that shape, e.g. `op_Addition(a:
        // Vector2Handle, b: Vector2Handle)`). Checked BEFORE the handle-shape heuristic so it wins.
        let is_operator_name = CLR_OPERATOR_METHOD_NAMES.contains(&managed_method_name.as_str());
        let first_is_handle = !is_operator_name
            && matches!(
                f.sig.inputs.first(),
                Some(FnArg::Typed(pt)) if type_last_ident_is(&pt.ty, &handle_name)
            );

        // Run every param/return through the SAME marshalling table `#[dotnet_export]` uses
        // (`marshal_param`/`marshal_return`), so a `#[dotnet_methods]` instance/static method can
        // ALSO take idiomatic `&str`/`String`/`Option<T>`/`Vec<T>` instead of requiring the class
        // author to hand-marshal `MString`/`Nullable<T>`/a `RustVec<T>` handle themselves — the same
        // gap `#[dotnet_export]` closed for free functions.
        //
        // UNLIKE `#[dotnet_export]` (which hard-errors on any type it doesn't recognize — every
        // exported free function's types must cross the seam SOMEHOW), an unrecognized type here
        // falls back to PASSTHROUGH (unchanged) rather than a compile error: `#[dotnet_methods]` has
        // never restricted its type surface to an allowlist — the receiver (`<Name>Handle`) and any
        // other interop-representable type (another class's handle, a raw primitive, …) already work
        // today via direct aliasing, and must keep working exactly as before. So this is purely
        // ADDITIVE ergonomic sugar, never a narrowing.
        struct ParamPlan {
            /// Bare seam type (no name) — used for both the shim's parameter list and the `KEEP_`
            /// anchor's fn-pointer type.
            seam_ty: proc_macro2::TokenStream,
            pname: syn::Ident,
            /// `Some` if this param needs an in-conversion statement (e.g. `MString` → `String`).
            pre_call: Option<proc_macro2::TokenStream>,
            /// The expression passed to the real (original) method at the call site inside the shim.
            call_arg: proc_macro2::TokenStream,
            marshalled: bool,
        }
        let mut param_plans: Vec<ParamPlan> = Vec::new();
        for (idx, arg) in f.sig.inputs.iter().enumerate() {
            let FnArg::Typed(pt) = arg else {
                continue; // `self` already rejected above.
            };
            let pname = format_ident!("__arg{}", idx);
            match marshal_param(&pt.ty) {
                Ok(m) => {
                    let seam_ty = m.seam_ty.clone();
                    let call_arg = if m.to_rust.is_some() && matches!(&*pt.ty, Type::Reference(_)) {
                        quote! { &#pname }
                    } else {
                        quote! { #pname }
                    };
                    param_plans.push(ParamPlan {
                        seam_ty,
                        pre_call: m.to_rust.map(|conv| conv(&pname)),
                        call_arg,
                        pname,
                        marshalled: true,
                    });
                }
                Err(_) => {
                    let orig_ty = (*pt.ty).clone();
                    param_plans.push(ParamPlan {
                        seam_ty: quote! { #orig_ty },
                        pre_call: None,
                        call_arg: quote! { #pname },
                        pname,
                        marshalled: false,
                    });
                }
            }
        }
        let (ret_seam_ty, ret_seam_arrow, ret_expr, ret_marshalled) = match &f.sig.output {
            ReturnType::Default => (quote! { () }, quote! {}, quote! { __ret }, false),
            ReturnType::Type(_, ty) => match marshal_return(ty) {
                Ok(m) => {
                    let seam_ty = m.seam_ty.clone();
                    let expr = match m.from_rust {
                        Some(conv) => conv(&format_ident!("__ret")),
                        None => quote! { __ret },
                    };
                    (seam_ty.clone(), quote! { -> #seam_ty }, expr, true)
                }
                Err(_) => (quote! { #ty }, quote! { -> #ty }, quote! { __ret }, false),
            },
        };
        let needs_shim = param_plans.iter().any(|p| p.marshalled) || ret_marshalled;

        // The identity actually aliased into the managed method: either the original method
        // (unmarshalled — today's exact behavior, byte-identical codegen) or a generated shim that
        // converts seam-typed args to idiomatic ones, calls the original, and converts the result
        // back. Both are plain free-fn *values*, so everything downstream (the `KEEP_` anchor, the
        // `add_method_def`/`add_static_method_def` call) treats them uniformly.
        let alias_target = if needs_shim {
            let shim_ident = format_ident!("__dotnet_methods_shim_{}", fn_ident);
            let seam_params: Vec<_> = param_plans
                .iter()
                .map(|p| {
                    let (pname, ty) = (&p.pname, &p.seam_ty);
                    quote! { #pname: #ty }
                })
                .collect();
            let pre_call: Vec<_> = param_plans
                .iter()
                .filter_map(|p| p.pre_call.clone())
                .collect();
            let call_args: Vec<_> = param_plans.iter().map(|p| p.call_arg.clone()).collect();
            shim_items.push(quote! {
                #[inline(never)]
                #[allow(non_snake_case, clippy::too_many_arguments)]
                fn #shim_ident(#(#seam_params),*) #ret_seam_arrow {
                    #(#pre_call)*
                    let __ret = #self_ty::#fn_ident(#(#call_args),*);
                    #ret_expr
                }
            });
            quote! { #shim_ident }
        } else {
            quote! { #self_ty::#fn_ident }
        };

        // A `#[used]` anchor holding the reified fn pointer forces rustc's own mono-collector to
        // codegen the aliased fn (a plain `pub fn` on a cdylib type is otherwise pruned as
        // unreachable — nothing *calls* it directly; the comptime entrypoint only names it, and that
        // entrypoint is interpreted, not codegen'd — EXCEPT when a shim is generated: the shim itself
        // is what needs anchoring, and it naturally keeps the ORIGINAL method alive too, since the
        // shim's body genuinely calls it). Without this the managed method's `AliasFor` edge would
        // dangle (`method_def_from_ref` → None → "alias for an extern function" panic at typecheck
        // time). A `fn(...) -> ...` pointer static is used (not `*const ()`/`usize`) because only a
        // fn-pointer-typed const initializer is legal in const-eval (this is the same anchor shape the
        // older `dotnet_typedef!` used) — so its type must exactly match whatever's aliased, hence
        // being built from the (possibly shim-adjusted) seam types above rather than the original
        // signature unconditionally.
        let seam_in_types: Vec<_> = param_plans.iter().map(|p| p.seam_ty.clone()).collect();
        let keep_ident = format_ident!("KEEP_{}", fn_ident);
        keep_anchors.push(quote! {
            #[used]
            static #keep_ident: fn(#(#seam_in_types),*) -> #ret_seam_ty = #alias_target;
        });

        if first_is_handle {
            // Instance (virtual) method: signature includes the receiver as arg 0.
            method_calls.push(quote! {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_method_def::<
                    "pub", "virtual", #fname_lit, _,
                >(class, #alias_target);
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
            if let (Some(name), Some(is_add)) = (&event_name, event_role) {
                let name_lit = LitStr::new(name, fn_ident.span());
                let intrinsic = if is_add {
                    quote! { rustc_codegen_clr_mark_last_method_event_add }
                } else {
                    quote! { rustc_codegen_clr_mark_last_method_event_remove }
                };
                method_calls.push(quote! {
                    let class = ::mycorrhiza::comptime::#intrinsic::<#name_lit>(class);
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
            if event_name.is_some() {
                return syn::Error::new(
                    fn_ident.span(),
                    "#[dotnet_event]: only supported on an instance method (add_*/remove_* take \
                     the subscriber as a parameter) — a static method can't back a .NET event",
                )
                .to_compile_error()
                .into();
            }
            // Static method: signature verbatim, no receiver.
            method_calls.push(quote! {
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_add_static_method_def::<
                    #fname_lit, _,
                >(class, #alias_target);
            });
        }
    }

    let expanded = quote! {
        // The user's impl block, verbatim — the methods remain ordinary callable Rust functions.
        #input

        // Marshalling shims generated for any method with an ergonomic-sugar param/return type (see
        // `needs_shim` above) — plain free fns, private to this module.
        #(#shim_items)*

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
                // Re-open the class by name so `finish_type`'s idempotent registration finds the
                // already-registered `ClassDef` (from the `#[dotnet_class]` struct decl) and
                // appends these methods to it. `INHERITS` is deliberately EMPTY, not
                // `"System.Object"`: this `impl` block has no access to the original struct's
                // `#[dotnet_class(extends = "...")]` attribute, so it must declare "no opinion"
                // about the base class (`""` decodes to `superclass = None` in
                // `PendingClass`/`finish_type`, which then leaves the existing class's `extends`
                // untouched) rather than asserting a specific one. This used to hardcode
                // `"System.Object"` as if that were always a safe default -- it wasn't: on
                // codegen-unit orderings where THIS entrypoint's `new_typedef` call ran before the
                // struct decl's, that false "System.Object" opinion won permanently (the decl's
                // real `extends=` was silently dropped by the old merge-only-fields reuse path),
                // producing a `TypeDef` whose actual base was `System.Object` even though every
                // `.override`/base-ctor-chain call site still correctly named the real base --
                // fatal at CLR type-load time (see `cilly::ir::class::ClassDef::set_extends`'s doc
                // for the exact CoreCLR native-crash mechanism this caused, confirmed via
                // `cargo_tests/cd_bgservice`).
                //
                // The SAME "no real opinion" reasoning applies to `IS_VALUETYPE` (the `false`
                // above): this impl block also has no access to the original struct's
                // `#[dotnet_class(value_type = ...)]` attribute. Unlike `INHERITS`, a bare `bool`
                // can't spell "no opinion" on its own, so `HAS_TYPE_KIND_OPINION = false` marks
                // this `false` as a non-authoritative placeholder — `finish_type` looks the class
                // up by name alone (never by an is_valuetype-baked `ClassRef`) and only lets an
                // authoritative entrypoint's opinion stick (see
                // `cilly::ir::class::ClassDef::set_is_valuetype`'s doc for the mirrored hazard
                // this closes: a `value_type = true` class whose `#[dotnet_methods]` block ran
                // first used to silently register a second, phantom `is_valuetype = false`
                // `ClassDef` under the same name).
                let class = ::mycorrhiza::comptime::rustc_codegen_clr_new_typedef::<
                    #name_lit, false, "", "", false,
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
    /// The type the `#[unsafe(no_mangle)] extern "C"` shim uses at the seam (what C# sees).
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

/// A trailing generic type path as `(name, type arguments)`, rejecting lifetime/const arguments.
/// This is intentionally separate from [`single_generic_arg`]: imported delegate wrappers have
/// arities one through three, while the existing Option/Vec/Task helpers are clearer with the
/// stricter one-argument shape.
fn generic_type_args(ty: &Type) -> Option<(String, Vec<&Type>)> {
    let Type::Path(tp) = ty else { return None };
    if tp.qself.is_some() {
        return None;
    }
    let seg = tp.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let mut types = Vec::with_capacity(args.args.len());
    for arg in &args.args {
        let syn::GenericArgument::Type(ty) = arg else {
            return None;
        };
        types.push(ty);
    }
    Some((seg.ident.to_string(), types))
}

/// If `ty` is `Result<T, E>` (possibly path-qualified), return its two type arguments.
fn result_args(ty: &Type) -> Option<(&Type, &Type)> {
    let Type::Path(tp) = ty else { return None };
    if tp.qself.is_some() {
        return None;
    }
    let seg = tp.path.segments.last()?;
    if seg.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let mut args = args.args.iter();
    let syn::GenericArgument::Type(ok) = args.next()? else {
        return None;
    };
    let syn::GenericArgument::Type(err) = args.next()? else {
        return None;
    };
    if args.next().is_some() {
        return None;
    }
    Some((ok, err))
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

/// If `ty` is `mycorrhiza::nullable::Nullable<T>` (a real `System.Nullable<T>` handle — already
/// FFI-safe, see that module's doc), returns `T`. `None` otherwise. Matched by trailing ident only,
/// like [`task_t_inner`].
fn nullable_inner(ty: &Type) -> Option<&Type> {
    let (name, inner) = single_generic_arg(ty)?;
    (name == "Nullable").then_some(inner)
}

/// If `ty` is `mycorrhiza::intrinsics::RustcCLRInteropManagedArray<T, N>` (a real managed 1-D array
/// handle — already FFI-safe), returns `T`, requiring the const dimension argument `N` to be the
/// literal `1` (the only arity this marshals today). `None` if the type isn't this array handle at
/// all; `Err` (via the caller) if it names 2+ dimensions, so that case fails loudly with a clear
/// message rather than silently mismarshalling.
fn managed_array_elem(ty: &Type) -> Option<Result<&Type, String>> {
    let Type::Path(tp) = ty else { return None };
    if tp.qself.is_some() {
        return None;
    }
    let seg = tp.path.segments.last()?;
    if seg.ident != "RustcCLRInteropManagedArray" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let mut type_args = args.args.iter().filter_map(|a| match a {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    });
    let elem = type_args.next()?;
    let dims_ok = args.args.iter().any(|a| {
        matches!(
            a,
            syn::GenericArgument::Const(syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(n),
                ..
            })) if n.base10_digits() == "1"
        )
    });
    if dims_ok {
        Some(Ok(elem))
    } else {
        Some(Err(
            "#[dotnet_export]: RustcCLRInteropManagedArray<T, N> is only marshalled for N = 1 \
             (a 1-D array) today."
                .to_string(),
        ))
    }
}

/// Recognize the concrete delegate wrappers whose managed handles can cross into an exported Rust
/// API unchanged. The generated shim reconstructs the ergonomic wrapper with `from_handle`, after
/// which ordinary Rust code calls `.invoke(..)`. Primitive arguments/results and `MString` managed
/// handles cross unchanged. Owned Rust `String` and other values still need an explicit
/// callback-boundary marshalling policy rather than an unchecked promise.
fn imported_delegate_param(ty: &Type) -> Option<Result<Marshal, String>> {
    let (name, args) = generic_type_args(ty)?;
    let expected_arity = match name.as_str() {
        "Action1" | "Comparison" => 1,
        "Action2" | "Func1" => 2,
        "Action3" | "Func2" => 3,
        "Func3" => 4,
        _ => return None,
    };
    if args.len() != expected_arity {
        return Some(Err(format!(
            "#[dotnet_export]: `{name}` expects {expected_arity} type argument(s), found {}.",
            args.len()
        )));
    }
    for arg in &args {
        if type_last_ident_is(arg, "MString") {
            continue;
        }
        let Some(arg_name) = simple_path_ident(arg) else {
            return Some(Err(format!(
                "#[dotnet_export]: imported `{name}` delegate types must be passthrough primitives \
                 or `MString`; unsupported callback type `{}`.",
                quote! { #arg }
            )));
        };
        if !is_passthrough_primitive(&arg_name) {
            return Some(Err(format!(
                "#[dotnet_export]: imported `{name}` delegate types must be passthrough primitives \
                 or `MString`; `{arg_name}` needs an explicit callback-boundary marshalling policy."
            )));
        }
    }

    let args: Vec<Type> = args.into_iter().cloned().collect();
    let marshal = match (name.as_str(), args.as_slice()) {
        ("Action1", [a0]) => {
            let a0 = a0.clone();
            Marshal {
                seam_ty: quote! {
                    ::mycorrhiza::intrinsics::RustcCLRInteropManagedGeneric<
                        { "System.Private.CoreLib" }, { "System.Action" }, (#a0,)
                    >
                },
                to_rust: Some(Box::new(move |id| {
                    quote! { let #id = ::mycorrhiza::delegate::Action1::<#a0>::from_handle(#id); }
                })),
                from_rust: None,
                returns_managed_handle: false,
            }
        }
        ("Action2", [a0, a1]) => {
            let (a0, a1) = (a0.clone(), a1.clone());
            Marshal {
                seam_ty: quote! {
                    ::mycorrhiza::intrinsics::RustcCLRInteropManagedGeneric<
                        { "System.Private.CoreLib" }, { "System.Action" }, (#a0, #a1)
                    >
                },
                to_rust: Some(Box::new(move |id| {
                    quote! {
                        let #id = ::mycorrhiza::delegate::Action2::<#a0, #a1>::from_handle(#id);
                    }
                })),
                from_rust: None,
                returns_managed_handle: false,
            }
        }
        ("Action3", [a0, a1, a2]) => {
            let (a0, a1, a2) = (a0.clone(), a1.clone(), a2.clone());
            Marshal {
                seam_ty: quote! {
                    ::mycorrhiza::intrinsics::RustcCLRInteropManagedGeneric<
                        { "System.Private.CoreLib" }, { "System.Action" }, (#a0, #a1, #a2)
                    >
                },
                to_rust: Some(Box::new(move |id| {
                    quote! {
                        let #id = ::mycorrhiza::delegate::Action3::<#a0, #a1, #a2>::from_handle(#id);
                    }
                })),
                from_rust: None,
                returns_managed_handle: false,
            }
        }
        ("Func1", [a0, ret]) => {
            let (a0, ret) = (a0.clone(), ret.clone());
            Marshal {
                seam_ty: quote! {
                    ::mycorrhiza::intrinsics::RustcCLRInteropManagedGeneric<
                        { "System.Private.CoreLib" }, { "System.Func" }, (#a0, #ret)
                    >
                },
                to_rust: Some(Box::new(move |id| {
                    quote! {
                        let #id = ::mycorrhiza::delegate::Func1::<#a0, #ret>::from_handle(#id);
                    }
                })),
                from_rust: None,
                returns_managed_handle: false,
            }
        }
        ("Func2", [a0, a1, ret]) => {
            let (a0, a1, ret) = (a0.clone(), a1.clone(), ret.clone());
            Marshal {
                seam_ty: quote! {
                    ::mycorrhiza::intrinsics::RustcCLRInteropManagedGeneric<
                        { "System.Private.CoreLib" }, { "System.Func" }, (#a0, #a1, #ret)
                    >
                },
                to_rust: Some(Box::new(move |id| {
                    quote! {
                        let #id = ::mycorrhiza::delegate::Func2::<#a0, #a1, #ret>::from_handle(#id);
                    }
                })),
                from_rust: None,
                returns_managed_handle: false,
            }
        }
        ("Func3", [a0, a1, a2, ret]) => {
            let (a0, a1, a2, ret) = (a0.clone(), a1.clone(), a2.clone(), ret.clone());
            Marshal {
                seam_ty: quote! {
                    ::mycorrhiza::intrinsics::RustcCLRInteropManagedGeneric<
                        { "System.Private.CoreLib" }, { "System.Func" }, (#a0, #a1, #a2, #ret)
                    >
                },
                to_rust: Some(Box::new(move |id| {
                    quote! {
                        let #id = ::mycorrhiza::delegate::Func3::<#a0, #a1, #a2, #ret>::from_handle(#id);
                    }
                })),
                from_rust: None,
                returns_managed_handle: false,
            }
        }
        ("Comparison", [a0]) => {
            let a0 = a0.clone();
            Marshal {
                seam_ty: quote! {
                    ::mycorrhiza::intrinsics::RustcCLRInteropManagedGeneric<
                        { "System.Private.CoreLib" }, { "System.Comparison" }, (#a0,)
                    >
                },
                to_rust: Some(Box::new(move |id| {
                    quote! {
                        let #id = ::mycorrhiza::delegate::Comparison::<#a0>::from_handle(#id);
                    }
                })),
                from_rust: None,
                returns_managed_handle: false,
            }
        }
        _ => unreachable!("delegate name and arity were validated above"),
    };
    Some(Ok(marshal))
}

fn is_imported_delegate_type(ty: &Type) -> bool {
    generic_type_args(ty).is_some_and(|(name, _)| {
        matches!(
            name.as_str(),
            "Action1" | "Action2" | "Action3" | "Func1" | "Func2" | "Func3" | "Comparison"
        )
    })
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

    if let Some(delegate) = imported_delegate_param(ty) {
        return delegate;
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
            primitives, `bool`, `&str`, `String`, concrete `Action1`/`Action2`/`Action3`/\
            `Func1`/`Func2`/`Func3`/\
            `Comparison` delegates, `Option<T>`, and `Vec<T>` (delegate elements may be passthrough \
            primitives or `MString`; collection elements must be passthrough primitives)."
        ));
    }

    // `Option<T>` of a passthrough primitive `T` -> a real `System.Nullable<T>`/`T?` inbound. The
    // ergonomic mirror of `marshal_return`'s `Option<T>` arm: the caller writes idiomatic `Option<T>`,
    // not the wrapper type — the seam carries `Nullable<T>` (already FFI-safe, see that module's doc)
    // and the conversion (`NullableExt::to_option`) happens automatically. Only a primitive `T` is
    // supported: `Nullable<T>` requires `T: struct` on the .NET side, which the passthrough
    // primitives already map to 1:1; a non-primitive `T` has no defined `Nullable<T>` shape to
    // marshal into (yet — see that arm's doc for why this specific slice was chosen first).
    if let Some((name, inner)) = single_generic_arg(ty) {
        if name == "Option" {
            if let Some(inner_name) = simple_path_ident(inner) {
                if is_passthrough_primitive(&inner_name) {
                    let inner = inner.clone();
                    return Ok(Marshal {
                        seam_ty: quote! { ::mycorrhiza::nullable::Nullable<#inner> },
                        to_rust: Some(Box::new(move |id| {
                            quote! {
                                let #id: ::core::option::Option<#inner> =
                                    ::mycorrhiza::nullable::NullableExt::to_option(&#id);
                            }
                        })),
                        from_rust: None,
                        returns_managed_handle: false,
                    });
                }
                return Err(format!(
                    "#[dotnet_export]: unsupported `Option<{inner_name}>` parameter element type. \
                     Only the integer/float primitives and `bool` are supported as `Option<T>` \
                     parameters today."
                ));
            }
            return Err(format!(
                "#[dotnet_export]: unsupported `Option<{}>` parameter element type. Only the \
                 integer/float primitives and `bool` are supported as `Option<T>` parameters today.",
                quote! { #inner }
            ));
        }
        // `Vec<T>` of a passthrough primitive `T` -> a `RustVec<T>` handle inbound: the caller passes
        // the opaque `usize` handle a C#-side `RustVec<T>`/`RustBoxVec<T>` already carries, and this
        // reconstructs an owned `Vec<T>` by walking it via the SAME `rcl_vec_len`/`rcl_vec_get` core
        // functions the return-side `Vec<T>` arm below uses in the opposite direction (`rcl_vec_new`/
        // `rcl_vec_push`). Same requirement as that arm: the consuming crate must have called
        // `mycorrhiza::export_rust_containers!()` once at its crate root.
        if name == "Vec" {
            if let Some(elem_name) = simple_path_ident(inner) {
                if is_passthrough_primitive(&elem_name) {
                    let elem_ty = inner.clone();
                    return Ok(Marshal {
                        seam_ty: quote! { ::core::primitive::usize },
                        to_rust: Some(Box::new(move |id| {
                            quote! {
                                let #id: ::std::vec::Vec<#elem_ty> = {
                                    let __len: ::core::primitive::usize =
                                        unsafe { crate::rcl_vec_len(#id) };
                                    let mut __out: ::std::vec::Vec<#elem_ty> =
                                        ::std::vec::Vec::with_capacity(__len);
                                    for __i in 0..__len {
                                        let mut __elem: #elem_ty = ::core::default::Default::default();
                                        unsafe {
                                            crate::rcl_vec_get(
                                                #id,
                                                __i,
                                                (&mut __elem as *mut #elem_ty)
                                                    as *mut ::core::primitive::u8,
                                            );
                                        }
                                        __out.push(__elem);
                                    }
                                    __out
                                };
                            }
                        })),
                        from_rust: None,
                        returns_managed_handle: false,
                    });
                }
                return Err(format!(
                    "#[dotnet_export]: unsupported `Vec<{elem_name}>` parameter element type. Only \
                     the integer/float primitives and `bool` are supported as `Vec<T>` parameters \
                     today (this expects a `RustVec<T>` handle, whose C# side is `T : unmanaged`)."
                ));
            }
            return Err(format!(
                "#[dotnet_export]: unsupported `Vec<{}>` parameter element type. Only the \
                 integer/float primitives and `bool` are supported as `Vec<T>` parameters today.",
                quote! { #inner }
            ));
        }
    }

    Err(format!(
        "#[dotnet_export]: unsupported parameter type `{}`. Supported: the integer/float primitives, \
         `bool`, `&str`, `String`, concrete `Action1`/`Action2`/`Action3`/`Func1`/`Func2`/\
         `Func3`/`Comparison` delegates, `Option<T>`, and `Vec<T>` (delegate elements may be \
         passthrough primitives or `MString`; collection elements must be passthrough primitives).",
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
             `mycorrhiza::task::TaskT<T>`, `Option<T>`, and `Vec<T>` of a passthrough primitive `T`."
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

    // `mycorrhiza::nullable::Nullable<T>` — a real `System.Nullable<T>` value (a fixed-size inline
    // byte buffer, NOT a managed reference — see that module's doc), already FFI-safe across the
    // seam for any `T`. This is the answer to "how do I return an Option<T>": a bare Rust `Option<T>`
    // can't cross directly (its layout is whatever niche/tag encoding rustc picked for this specific
    // `T`, not Nullable<T>'s fixed `{bool,T}` layout), so an exported fn computes an `Option<T>`
    // internally and converts with `.into()` at the boundary (`Nullable<T>: From<Option<T>>`) before
    // returning. `returns_managed_handle: false` — unlike Task/TaskT<T>, this is an inline value type
    // with no gcref field, so it has none of the catch_unwind-hazard those two need.
    if nullable_inner(ty).is_some() {
        return Ok(Marshal {
            seam_ty: quote! { #ty },
            to_rust: None,
            from_rust: None,
            returns_managed_handle: false,
        });
    }

    // `Option<T>` of a passthrough primitive `T` — the ergonomic counterpart to the arm just above,
    // and the return-side mirror of `marshal_param`'s `Option<T>` arm: return idiomatic `Option<T>`
    // directly instead of manually building a `Nullable<T>` and `.into()`-ing it yourself (the arm
    // above is kept as-is for that explicit spelling, and for the day a non-primitive `T` is
    // supported). Converts via the same `Nullable<T>: From<Option<T>>` impl that arm's doc documents.
    if let Some((name, inner)) = single_generic_arg(ty) {
        if name == "Option" {
            if let Some(inner_name) = simple_path_ident(inner) {
                if is_passthrough_primitive(&inner_name) {
                    let inner = inner.clone();
                    return Ok(Marshal {
                        seam_ty: quote! { ::mycorrhiza::nullable::Nullable<#inner> },
                        to_rust: None,
                        from_rust: Some(Box::new(move |id| {
                            quote! {
                                ::core::convert::Into::<::mycorrhiza::nullable::Nullable<#inner>>::into(#id)
                            }
                        })),
                        returns_managed_handle: false,
                    });
                }
                return Err(format!(
                    "#[dotnet_export]: unsupported `Option<{inner_name}>` return element type. Only \
                     the integer/float primitives and `bool` are supported as `Option<T>` returns \
                     today."
                ));
            }
        }
    }

    // `mycorrhiza::intrinsics::RustcCLRInteropManagedArray<T, 1>` — a real managed 1-D array handle
    // (a single `object_ref: usize` under a `PhantomData<T>`, like Task/TaskT<T> above), already
    // FFI-safe across the seam. The fn constructs it itself via `rustc_clr_interop_managed_new_arr`/
    // `_set_elem` (both already Rust-callable, used in mycorrhiza::linq/dynamic today) and returns
    // the handle — C# receives a genuine `T[]`, not the opaque `RustVec<T>` indexer-wrapper the
    // `Vec<T>` arm below produces. `returns_managed_handle: true` — same reason as Task/TaskT<T>.
    if let Some(elem) = managed_array_elem(ty) {
        let _elem = elem?;
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
         `bool`, `&str`, `String`, `()`, `mycorrhiza::task::Task`, `mycorrhiza::task::TaskT<T>`, \
         `Option<T>`, and `Vec<T>` of a passthrough primitive `T`.",
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
            value:
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }),
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
fn clr_member_id_type_name(ty: &Type) -> Option<String> {
    if let Type::Reference(r) = ty {
        if r.mutability.is_none() {
            if let Type::Path(tp) = &*r.elem {
                if tp.qself.is_none() && tp.path.is_ident("str") {
                    return Some("System.String".to_string());
                }
            }
        }
        return None;
    }
    if let Some((name, args)) = generic_type_args(ty) {
        let expected = match name.as_str() {
            "Action1" | "Comparison" => 1,
            "Action2" | "Func1" => 2,
            "Action3" | "Func2" => 3,
            "Func3" => 4,
            _ => 0,
        };
        if expected != 0 && args.len() == expected {
            let args = args
                .into_iter()
                .map(clr_member_id_type_name)
                .collect::<Option<Vec<_>>>()?;
            let clr_name = match name.as_str() {
                "Action1" | "Action2" | "Action3" => "System.Action",
                "Func1" | "Func2" | "Func3" => "System.Func",
                "Comparison" => "System.Comparison",
                _ => unreachable!(),
            };
            return Some(format!("{clr_name}{{{}}}", args.join(",")));
        }
    }
    let name = simple_path_ident(ty)?;
    Some(
        match name.as_str() {
            "String" | "MString" => "System.String",
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
        }
        .to_string(),
    )
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
fn dotnet_export_member_id(managed_name: &str, params: &[String]) -> String {
    format!("M:MainModule.{}({})", managed_name, params.join(","))
}

fn emit_xmldoc_entry(managed_name: &str, params: &[String], doc: &str) {
    let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let Ok(crate_name) = std::env::var("CARGO_PKG_NAME") else {
        return;
    };
    let member_id = dotnet_export_member_id(managed_name, params);

    let dir = std::path::Path::new(&manifest_dir)
        .join("target")
        .join("dotnet_xmldoc");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file_path = dir.join(format!("{crate_name}.xmldoc.jsonl"));
    let entry = serde_json_line(&member_id, doc);
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
    {
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
    format!(
        "{{\"member\":\"{}\",\"summary\":\"{}\"}}",
        escape(member),
        escape(summary)
    )
}

/// Parse the optional `name = "ManagedName"` override accepted by `#[dotnet_export]`.
///
/// The value deliberately has a narrower contract than raw CLI metadata: it must be an ASCII C#
/// identifier, so every accepted value is callable as an ordinary C# member without reflection,
/// escaping, or generated-source quoting. The Rust function identifier is never changed.
#[derive(Default)]
struct DotnetExportArgs {
    name: Option<String>,
    error_exception: bool,
    enum_types: Vec<Type>,
}

impl std::fmt::Debug for DotnetExportArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DotnetExportArgs")
            .field("name", &self.name)
            .field("error_exception", &self.error_exception)
            .field("enum_type_count", &self.enum_types.len())
            .finish()
    }
}

fn parse_dotnet_export_args(attr: proc_macro2::TokenStream) -> syn::Result<DotnetExportArgs> {
    if attr.is_empty() {
        return Ok(DotnetExportArgs::default());
    }

    let parser = Punctuated::<syn::Meta, Token![,]>::parse_terminated;
    let entries = syn::parse::Parser::parse2(parser, attr)?;
    let mut parsed = DotnetExportArgs::default();
    for meta in entries {
        if let syn::Meta::List(list) = &meta {
            if list.path.is_ident("enums") {
                let types =
                    list.parse_args_with(Punctuated::<Type, Token![,]>::parse_terminated)?;
                if types.is_empty() {
                    return Err(syn::Error::new(
                        list.span(),
                        "#[dotnet_export]: `enums(...)` needs at least one enum type",
                    ));
                }
                for ty in types {
                    if parsed.enum_types.iter().any(|existing| {
                        quote! { #existing }.to_string() == quote! { #ty }.to_string()
                    }) {
                        return Err(syn::Error::new(
                            ty.span(),
                            "#[dotnet_export]: enum type listed more than once",
                        ));
                    }
                    parsed.enum_types.push(ty);
                }
                continue;
            }
        }
        let syn::Meta::NameValue(entry) = meta else {
            return Err(syn::Error::new(
                meta.span(),
                "#[dotnet_export]: expected `name = \"ManagedName\"`, `error = \"exception\"`, or `enums(Type, ...)`",
            ));
        };
        if entry.path.is_ident("name") {
            if parsed.name.is_some() {
                return Err(syn::Error::new(
                    entry.path.span(),
                    "#[dotnet_export]: `name` may be specified only once",
                ));
            }
            let Expr::Lit(expr_lit) = entry.value else {
                return Err(syn::Error::new(
                    entry.path.span(),
                    "#[dotnet_export]: `name` must be a string literal",
                ));
            };
            let Lit::Str(name) = expr_lit.lit else {
                return Err(syn::Error::new(
                    expr_lit.lit.span(),
                    "#[dotnet_export]: `name` must be a string literal",
                ));
            };
            let value = name.value();
            let mut chars = value.chars();
            let valid = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !valid {
                return Err(syn::Error::new(
                    name.span(),
                    "#[dotnet_export]: `name` must be a non-empty ASCII C# identifier",
                ));
            }
            parsed.name = Some(value);
        } else if entry.path.is_ident("error") {
            if parsed.error_exception {
                return Err(syn::Error::new(
                    entry.path.span(),
                    "#[dotnet_export]: `error` may be specified only once",
                ));
            }
            let Expr::Lit(expr_lit) = entry.value else {
                return Err(syn::Error::new(
                    entry.path.span(),
                    "#[dotnet_export]: `error` must be the string literal \"exception\"",
                ));
            };
            let Lit::Str(policy) = expr_lit.lit else {
                return Err(syn::Error::new(
                    expr_lit.lit.span(),
                    "#[dotnet_export]: `error` must be the string literal \"exception\"",
                ));
            };
            if policy.value() != "exception" {
                return Err(syn::Error::new(
                    policy.span(),
                    "#[dotnet_export]: unsupported error policy; expected `error = \"exception\"`",
                ));
            }
            parsed.error_exception = true;
        } else {
            return Err(syn::Error::new(
                entry.path.span(),
                "#[dotnet_export]: unknown attribute key; expected `name = \"ManagedName\"`, `error = \"exception\"`, or `enums(Type, ...)`",
            ));
        }
    }
    Ok(parsed)
}

fn registered_export_enum(ty: &Type, enum_types: &[Type]) -> bool {
    let Some(actual) = simple_path_ident(ty) else {
        return false;
    };
    enum_types
        .iter()
        .any(|candidate| simple_path_ident(candidate).is_some_and(|name| name == actual))
}

fn marshal_export_enum_param(ty: &Type) -> Marshal {
    let ty = ty.clone();
    Marshal {
        seam_ty: quote! { <#ty as ::mycorrhiza::enums::DotNetExportEnum>::Managed },
        to_rust: Some(Box::new(move |id| {
            quote! {
                let #id: #ty = match <#ty as ::mycorrhiza::enums::DotNetExportEnum>::try_from_managed(#id) {
                    ::core::option::Option::Some(value) => value,
                    ::core::option::Option::None => ::mycorrhiza::error::throw_msg(
                        concat!("invalid numeric value for exported enum `", stringify!(#ty), "`")
                    ),
                };
            }
        })),
        from_rust: None,
        returns_managed_handle: false,
    }
}

fn marshal_export_enum_return(ty: &Type) -> Marshal {
    let ty = ty.clone();
    Marshal {
        seam_ty: quote! { <#ty as ::mycorrhiza::enums::DotNetExportEnum>::Managed },
        to_rust: None,
        from_rust: Some(Box::new(move |id| {
            quote! {
                <#ty as ::mycorrhiza::enums::DotNetExportEnum>::into_managed(#id)
            }
        })),
        returns_managed_handle: false,
    }
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
/// and C# calls `MainModule.greet("x")`, getting back a `string`. To expose an idiomatic managed
/// name while preserving the Rust API, write `#[dotnet_export(name = "Greet")]`; C# then calls
/// `MainModule.Greet("x")` while Rust still calls `greet`. The macro leaves the original fn
/// untouched (still callable from Rust) and emits a hidden `#[unsafe(no_mangle)] extern "C"` **shim** that
/// crosses the managed seam: each supported argument/return type is marshalled to/from its
/// CIL-visible form. `&str`/`String` cross as a real managed `System.String` (so C# sees `string`,
/// not a pointer pair); the numeric/`bool` primitives pass through unchanged. Concrete
/// `Action1`/`Action2`/`Action3`/`Func1`/`Func2`/`Func3`/`Comparison` parameters with primitive or
/// managed-`MString` signatures cross as their real managed delegate types and are reconstructed as
/// invokable Rust wrappers.
///
/// The consuming `cdylib` must depend on `mycorrhiza`. Types outside the supported set produce a
/// clear compile error (marshalling is never faked).
#[proc_macro_attribute]
pub fn dotnet_export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let export_args = match parse_dotnet_export_args(attr.into()) {
        Ok(args) => args,
        Err(error) => return error.to_compile_error().into(),
    };
    let func = parse_macro_input!(item as ItemFn);
    let sig = &func.sig;
    let enum_types = &export_args.enum_types;

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
        return syn::Error::new(
            v.span(),
            "#[dotnet_export]: variadic functions cannot be exported",
        )
        .to_compile_error()
        .into();
    }

    let fn_name = sig.ident.clone();
    let (managed_name, export_attribute) = match export_args.name {
        Some(managed_name) => {
            let name_literal = LitStr::new(&managed_name, fn_name.span());
            (
                managed_name,
                quote! { #[unsafe(export_name = #name_literal)] },
            )
        }
        // Preserve the established no-argument expansion exactly, including the `no_mangle`
        // marker that older backend versions use to classify exports.
        None => (fn_name.to_string(), quote! { #[unsafe(no_mangle)] }),
    };
    let shim_mod = format_ident!("__dotnet_export_{}", fn_name);

    // Marshal each parameter. `receiver` (self) is rejected — only free functions are exportable.
    let mut seam_params = Vec::new(); // `#pname: #seam_ty` tokens for the shim signature.
    let mut pre_call = Vec::new(); // in-conversion statements (seam value → idiomatic Rust value).
    let mut call_args = Vec::new(); // expressions passed to the inner fn.
    let mut doc_param_types = Vec::new(); // CLR type names, for the XML-doc member-ID (see below).
    let mut doc_param_types_complete = true;
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
        let imported_delegate = is_imported_delegate_type(&pat_ty.ty);
        let marshal = if registered_export_enum(&pat_ty.ty, enum_types) {
            marshal_export_enum_param(&pat_ty.ty)
        } else {
            match marshal_param(&pat_ty.ty) {
                Ok(m) => m,
                Err(msg) => {
                    return syn::Error::new(pat_ty.ty.span(), msg)
                        .to_compile_error()
                        .into();
                }
            }
        };
        if let Some(clr_name) = clr_member_id_type_name(&pat_ty.ty) {
            doc_param_types.push(clr_name);
        } else {
            // Never emit a plausible-but-wrong member ID with a missing parameter. Unsupported
            // XML-doc shapes can simply omit the entry; consumers cannot match an incorrect one.
            doc_param_types_complete = false;
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
                } else if imported_delegate {
                    // A delegate wrapper contains a managed object reference. Moving it directly
                    // into `catch_unwind`'s closure makes rustc erase a managed-containing closure
                    // environment through `*mut u8`, which this backend correctly rejects as a
                    // ManagedPtrCast in unoptimized/debug MIR. Keep the value in an outer Option;
                    // the closure captures only `&mut Option<Wrapper>` and takes it once. This is
                    // the same GC-safe shape used below for managed return values.
                    let slot = format_ident!("__managed_param_slot_{}", idx);
                    pre_call.push(quote! {
                        let mut #slot = ::core::option::Option::Some(#pname);
                    });
                    call_args.push(quote! {
                        match #slot.take() {
                            ::core::option::Option::Some(__value) => __value,
                            ::core::option::Option::None => unreachable!(
                                "dotnet_export: managed delegate parameter consumed more than once"
                            ),
                        }
                    });
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
    if doc_param_types_complete {
        if let Some(doc) = scrape_doc_comment(&func.attrs) {
            emit_xmldoc_entry(&managed_name, &doc_param_types, &doc);
        }
    }

    // Marshal the return type. `returns_managed_handle` (Task/TaskT<T>) picks a different
    // `catch_unwind` shape below — see that field's doc comment on `Marshal` for why.
    let mut result_error_ty = None;
    let (seam_ret, ret_expr, ret_ty_for_slot, returns_managed_handle) = match &sig.output {
        ReturnType::Default => (quote! {}, quote! { __ret }, None, false), // `-> ()`; identity.
        ReturnType::Type(_, ty) => {
            let marshal_ty = if let Some((ok_ty, error_ty)) = result_args(ty) {
                if !export_args.error_exception {
                    return syn::Error::new(
                        ty.span(),
                        "#[dotnet_export]: `Result<T, E>` cannot cross the managed seam; opt in to mapping `Err(E)` to a C# exception with `#[dotnet_export(error = \"exception\")]`",
                    )
                    .to_compile_error()
                    .into();
                }
                result_error_ty = Some(error_ty.clone());
                ok_ty
            } else {
                if export_args.error_exception {
                    return syn::Error::new(
                        ty.span(),
                        "#[dotnet_export]: `error = \"exception\"` requires a `Result<T, E>` return type",
                    )
                    .to_compile_error()
                    .into();
                }
                ty
            };
            let marshal = if registered_export_enum(marshal_ty, enum_types) {
                marshal_export_enum_return(marshal_ty)
            } else {
                match marshal_return(marshal_ty) {
                    Ok(m) => m,
                    Err(msg) => {
                        return syn::Error::new(ty.span(), msg).to_compile_error().into();
                    }
                }
            };
            if result_error_ty.is_some() && marshal.returns_managed_handle {
                return syn::Error::new(
                    ty.span(),
                    "#[dotnet_export]: `error = \"exception\"` cannot be used with a managed-handle success type: the original Rust `Result<T, E>` would place a managed reference in overlapping enum storage before the generated shim can unwrap it",
                )
                .to_compile_error()
                .into();
            }
            let seam_ty = marshal.seam_ty;
            let ret_ident = format_ident!("__ret");
            let expr = match marshal.from_rust {
                Some(conv) => conv(&ret_ident),
                None => quote! { #ret_ident },
            };
            (
                quote! { -> #seam_ty },
                expr,
                Some(marshal_ty.clone()),
                marshal.returns_managed_handle,
            )
        }
    };

    let raw_call = quote! { super::#fn_name(#(#call_args),*) };
    let call = if result_error_ty.is_some() {
        quote! {
            match #raw_call {
                ::std::result::Result::Ok(__value) => __value,
                ::std::result::Result::Err(__error) => {
                    let __msg = ::std::format!("{}", __error);
                    ::mycorrhiza::error::throw_msg(&__msg)
                }
            }
        }
    } else {
        raw_call
    };

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
        ::mycorrhiza::error::throw_msg(&__msg)
    };

    // The inner call is wrapped in `catch_unwind`: a plain `extern "C"` is a `nounwind` ABI
    // boundary (the same rule as native Rust FFI — unwinding across it is UB), so an uncaught
    // panic would otherwise reach the true edge and the runtime hard-aborts the whole process
    // (`Environment.FailFast`) with no chance for the C# caller to recover. Catching it *inside*
    // the shim and re-raising as a genuine managed `System.Exception`
    // (`::mycorrhiza::error::throw_msg`) turns an unrecoverable process abort into an ordinary
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
            // `#ret_ty` (`Task`/`TaskT<T>`) carries a real .NET object reference, so it can't live
            // in `catch_unwind`'s own `Result<RetTy, _>` (a genuine gcref in the `Err` variant's
            // overlapping storage — `cilly`'s `ClassDef::layout_check` correctly refuses that, see
            // the module note above). The ORIGINAL fix for this routed the value out through a raw
            // pointer into a `MaybeUninit<RetTy>` slot instead — but `MaybeUninit`'s own write/read
            // (`as_mut_ptr`, and even the "safe" `write`/`assume_init`, which still lower to
            // `self as *mut Self as *mut T` internally) is itself a CIL `PtrCast` reinterpreting a
            // possibly-uninitialized location as a live gcref — exactly the hazard `Type::
            // contains_gcref`'s (correctly) deepened check now also catches. It slipped through for
            // the simplest `#[dotnet_export]` bodies only because rustc's own optimizer proved the
            // `MaybeUninit` dance redundant and elided the cast before our backend ever saw it; a
            // bigger fn body (e.g. one driving a real multi-`.await` coroutine through
            // `future_to_task`) keeps the cast in the final MIR and the verifier correctly rejects
            // it (`ManagedPtrCast`).
            //
            // Fix: use a plain `Option<RetTy>` out-slot instead of `MaybeUninit<RetTy>`. Writing
            // `*out = Some(__v)` is an ordinary tagged-enum construction (`aggregate.rs`), not a
            // storage reinterpretation, and reading it back is an ordinary `match` (a normal field
            // read off the `Some` variant) — neither lowers to a `PtrCast`, so `contains_gcref`
            // never enters the picture. The slot itself is captured by the closure as `&mut
            // Option<RetTy>` (a managed byref field in the closure's environment, not a `RetTy`
            // value), so it still never sits inside `catch_unwind`'s own `Result`.
            let mut __slot: ::core::option::Option<#ret_ty> = ::core::option::Option::None;
            match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                let __v = #call;
                __slot = ::core::option::Option::Some(__v);
            })) {
                ::std::result::Result::Ok(()) => {
                    // The closure above set `__slot` on its only non-unwinding path, which is
                    // exactly the path that reaches this arm.
                    let __ret: #ret_ty = match __slot {
                        ::core::option::Option::Some(__v) => __v,
                        ::core::option::Option::None => unreachable!(
                            "dotnet_export: catch_unwind returned Ok without the closure setting __slot"
                        ),
                    };
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

        // The generated seam shim exports `#managed_name` as its flat symbol. The backend marks
        // exported symbols `Access::Extern` (a DCE root) and emits them as public static methods
        // on the assembly's `MainModule`; the configured managed name is therefore exactly the
        // C# method name. With no attribute, `no_mangle` preserves the historical `#fn_name`
        // behavior. See
        // `body`'s construction above for the two `catch_unwind` shapes and why a second one is
        // needed.
        //
        // The shim itself is declared `extern "C-unwind"`, not plain `extern "C"`. This is NOT
        // optional: `throw_msg`'s `ExceptionDispatchInfo::Throw()` call performs a genuine CIL
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
            #export_attribute
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
            if !m.path.is_ident("namespace")
                && !m.path.is_ident("assembly")
                && !m.path.is_ident("name")
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
