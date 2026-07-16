//! The ECMA-335 metadata tables (§II.22) this backend needs, plus the `#~` stream container
//! that holds them (§II.24.2.6).
//!
//! Scope: exactly the tables `il_exporter` drives, per the inventory in
//! `docs/PE_EMISSION_PLAN.md` — Module, TypeRef, TypeDef, Field, MethodDef, Param,
//! InterfaceImpl, MemberRef, Constant, CustomAttribute, ClassLayout, FieldLayout, StandAloneSig, TypeSpec,
//! ModuleRef, ImplMap, FieldRVA, Assembly, AssemblyRef, MethodSpec. Ordinary static defaults remain
//! `FieldRVA` blobs; genuine enum literal fields use the ECMA `Constant` table.
//!
//! Pipeline: **populate** (the `add_*`/`*_ref` methods, called while walking the `Assembly`) →
//! **size** (row counts fix each table's row-index width; heap final sizes fix `HeapSizes`) →
//! **serialize** (`#~` stream bytes, `Valid`/`Sorted` bitmasks, then the four heap streams).
//! Row order within a table is insertion order except where §II.22 requires a *sorted* table
//! (`InterfaceImpl`, `ClassLayout`, `FieldLayout`, `FieldRVA`, `ImplMap`, `MethodSpec` is NOT
//! sorted) — sorting is a `serialize()`-time concern, not a population-time one, so implementers
//! can append rows in whatever order is convenient while walking the `Assembly`.

use super::heaps::{BlobHeap, GuidHeap, StringsHeap, UserStringHeap, write_compressed_u32};
use super::sig::{self, TypeDefOrRefResolver};
use crate::DotnetRuntime;
use crate::ir::{
    Assembly, ClassRef, Const, FieldDesc, Interned, MethodDefIdx, PInvokeCallConv, StaticFieldDesc,
    Type,
};
use std::collections::HashMap;

/// A metadata token (§II.22.1.8): high byte is the table id, low 3 bytes are the 1-based row
/// index (`rid`). `NIL` (rid 0) denotes "no row" (e.g. `Extends` on a class with no base type
/// other than the implicit `System.Object`/`ValueType`, which is instead handled by `extends`
/// being `Option`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token(pub u32);

impl Token {
    /// Table-id constants this backend emits (§II.22, one nibble-pair per table).
    pub const TABLE_MODULE: u32 = 0x00;
    pub const TABLE_TYPE_REF: u32 = 0x01;
    pub const TABLE_TYPE_DEF: u32 = 0x02;
    pub const TABLE_FIELD: u32 = 0x04;
    pub const TABLE_METHOD_DEF: u32 = 0x06;
    pub const TABLE_PARAM: u32 = 0x08;
    pub const TABLE_INTERFACE_IMPL: u32 = 0x09;
    pub const TABLE_MEMBER_REF: u32 = 0x0A;
    pub const TABLE_CONSTANT: u32 = 0x0B;
    pub const TABLE_CUSTOM_ATTRIBUTE: u32 = 0x0C;
    pub const TABLE_CLASS_LAYOUT: u32 = 0x0F;
    pub const TABLE_FIELD_LAYOUT: u32 = 0x10;
    pub const TABLE_STAND_ALONE_SIG: u32 = 0x11;
    pub const TABLE_EVENT_MAP: u32 = 0x12;
    pub const TABLE_EVENT: u32 = 0x14;
    pub const TABLE_PROPERTY_MAP: u32 = 0x15;
    pub const TABLE_PROPERTY: u32 = 0x17;
    pub const TABLE_METHOD_SEMANTICS: u32 = 0x18;
    pub const TABLE_METHOD_IMPL: u32 = 0x19;
    pub const TABLE_MODULE_REF: u32 = 0x1A;
    pub const TABLE_TYPE_SPEC: u32 = 0x1B;
    pub const TABLE_IMPL_MAP: u32 = 0x1C;
    pub const TABLE_FIELD_RVA: u32 = 0x1D;
    pub const TABLE_ASSEMBLY: u32 = 0x20;
    pub const TABLE_ASSEMBLY_REF: u32 = 0x23;
    pub const TABLE_GENERIC_PARAM: u32 = 0x2A;
    pub const TABLE_METHOD_SPEC: u32 = 0x2B;
    /// The `#US` (User String) heap's "table id" (§II.22.2) — not a real metadata table, but
    /// `ldstr` tokens are shaped identically (`0x70 << 24 | offset`), so [`Token`] represents
    /// them the same way.
    const TABLE_USER_STRING: u32 = 0x70;

    /// Builds a token from a table id (§II.22 table-id byte) and a 1-based row index.
    #[must_use]
    pub fn new(table: u32, rid: u32) -> Self {
        debug_assert!(
            table <= 0xFF,
            "table id {table:#x} doesn't fit a token byte"
        );
        Token((table << 24) | rid)
    }

    #[must_use]
    pub fn table(self) -> u32 {
        self.0 >> 24
    }

    #[must_use]
    pub fn rid(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }
}

/// A resolved `.NET` assembly reference target for [`MetadataBuilder::assembly_ref`]: either a
/// versioned BCL/framework assembly (mirrors `il_exporter`'s `.assembly extern '<name>' { .ver …
/// .publickeytoken = (…) }` for `bcl_public_key_token`-matched names) or a bare name-only
/// reference (mirrors the `else` arm, used for a consumer's own non-BCL library).
pub enum AssemblyRefTarget<'a> {
    /// A BCL/framework assembly: runtime-selected `.ver` triplet + the
    /// real public-key token for that assembly's signing family (see `bcl_public_key_token`) —
    /// NOT always the ECMA token; `Microsoft.Extensions.*`/`Microsoft.AspNetCore.*`/
    /// `Microsoft.EntityFrameworkCore*` carry a different one.
    Bcl {
        version: (u16, u16, u16, u16),
        token: [u8; 8],
    },
    /// A consumer-supplied assembly, referenced by simple name only — no version, no token.
    NameOnly,
    #[doc(hidden)]
    _Marker(std::marker::PhantomData<&'a ()>),
}

/// The fixed ECMA public-key token every real `System.*`/CoreLib assembly reference carries
/// (§II.22.5), matching `il_exporter`'s `B0 3F 5F 7F 11 D5 0A 3A` literal.
const ECMA_PUBLIC_KEY_TOKEN: [u8; 8] = [0xB0, 0x3F, 0x5F, 0x7F, 0x11, 0xD5, 0x0A, 0x3A];

/// The public-key token Microsoft's "extensions/aspnetcore" signing family carries — a DIFFERENT
/// key from [`ECMA_PUBLIC_KEY_TOKEN`], matching `il_exporter`'s `AD B9 79 38 29 DD AE 60` literal.
/// See `bcl_public_key_token`'s doc for how this was verified.
const EXTENSIONS_PUBLIC_KEY_TOKEN: [u8; 8] = [0xAD, 0xB9, 0x79, 0x38, 0x29, 0xDD, 0xAE, 0x60];

/// The maximum class-name length the CoreCLR `ilasm` accepts ("Full class name too long
/// (N characters, 1023 allowed)"); ported from `il_exporter::ILASM_MAX_CLASS_NAME` so `tables.rs`
/// applies the identical shortening at both TypeDef- and TypeRef-name construction time (no
/// def/ref skew, exactly as the textual exporter documents).
const ILASM_MAX_CLASS_NAME: usize = 1023;

/// Deterministically shorten an over-long class name so the CoreCLR `ilasm` accepts it — a local
/// port of `il_exporter::dotnet_class_name` (see that function's doc comment for the full
/// rationale; kept in sync by construction since both are pure functions of the same FNV-1a
/// scheme over the same input string).
fn dotnet_class_name(name: &str) -> std::borrow::Cow<'_, str> {
    if name.len() <= ILASM_MAX_CLASS_NAME {
        return std::borrow::Cow::Borrowed(name);
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    const HEAD: usize = 900;
    let mut head_end = HEAD.min(name.len());
    while head_end > 0 && !name.is_char_boundary(head_end) {
        head_end -= 1;
    }
    std::borrow::Cow::Owned(format!("{}__h{hash:016x}", &name[..head_end]))
}

// ---------------------------------------------------------------------------------------------
// Row types. Each row stores heap/coded-index values as plain u32 — width narrowing happens only
// at serialize() time (§II.24.2.6), so population never has to know final table sizes.
// ---------------------------------------------------------------------------------------------

struct ModuleRow {
    name: u32,
    mvid: u32,
}

struct TypeRefRow {
    resolution_scope: u32,
    namespace: u32,
    name: u32,
}

struct TypeDefRow {
    flags: u32,
    name: u32,
    namespace: u32,
    extends: u32,
    field_list: u32,
    method_list: u32,
}

struct FieldRow {
    flags: u16,
    name: u32,
    signature: u32,
}

struct MethodDefRow {
    rva: u32,
    impl_flags: u16,
    flags: u16,
    name: u32,
    signature: u32,
    param_list: u32,
}

/// Nullable-reference metadata attached while one `MethodDef` and its `Param` rows are emitted.
/// Parameter flags are receiver-stripped and parallel to `add_method`'s `param_names` slice.
#[derive(Clone, Copy)]
pub struct MethodNullability<'a> {
    pub context: u8,
    pub return_flag: Option<u8>,
    pub parameter_flags: &'a [Option<u8>],
}

struct ParamRow {
    flags: u16,
    sequence: u16,
    name: u32,
}

struct InterfaceImplRow {
    class: u32,
    interface: u32,
}

struct MemberRefRow {
    class: u32,
    name: u32,
    signature: u32,
}

/// §II.22.9 Constant. `parent` is a HasConstant coded index; enum literals target Field (tag 0).
struct ConstantRow {
    type_code: u8,
    parent: u32,
    value: u32,
}

struct CustomAttributeRow {
    parent: u32,
    ctor: u32,
    value: u32,
}

struct ClassLayoutRow {
    packing_size: u16,
    class_size: u32,
    parent: u32,
}

struct FieldLayoutRow {
    offset: u32,
    field: u32,
}

struct ModuleRefRow {
    name: u32,
}

struct ImplMapRow {
    mapping_flags: u16,
    member_forwarded: u32,
    import_name: u32,
    import_scope: u32,
}

struct FieldRvaRow {
    rva: u32,
    field: u32,
}

struct AssemblyRow {
    hash_alg_id: u32,
    major: u16,
    minor: u16,
    build: u16,
    revision: u16,
    flags: u32,
    public_key: u32,
    name: u32,
    culture: u32,
}

struct AssemblyRefRow {
    major: u16,
    minor: u16,
    build: u16,
    revision: u16,
    flags: u32,
    public_key_or_token: u32,
    name: u32,
    culture: u32,
    hash_value: u32,
}

struct TypeSpecRow {
    signature: u32,
}

struct MethodSpecRow {
    method: u32,
    instantiation: u32,
}

struct StandAloneSigRow {
    signature: u32,
}

/// §II.22.27 `MethodImpl` — an explicit body-overrides-declaration binding (`.override` in ilasm),
/// distinct from ordinary name+signature virtual binding. Sorted by `class`.
struct MethodImplRow {
    /// `Class` — simple `TypeDef` index of the class declaring the override.
    class: u32,
    /// `MethodBody` — `MethodDefOrRef` coded index of the overriding method (a `MethodDef` in
    /// `class`).
    method_body: u32,
    /// `MethodDeclaration` — `MethodDefOrRef` coded index of the base method being overridden
    /// (usually a `MemberRef` to an external base type's virtual).
    method_declaration: u32,
}

/// §II.22.12 `EventMap` — one row per type that declares events, pointing at that type's first
/// `Event` row (a table-position "run-start", exactly like `TypeDef.MethodList`). NOT sorted.
struct EventMapRow {
    /// `Parent` — simple `TypeDef` index of the event-declaring type.
    parent: u32,
    /// `EventList` — simple `Event` index of the first event this type owns (run continues to the
    /// next `EventMap`'s `EventList`, or the end of the `Event` table).
    event_list: u32,
}

/// §II.22.13 `Event` — a single event's name + delegate type. The `add`/`remove` accessor linkage
/// lives in `MethodSemantics`, not here.
struct EventRow {
    /// `EventFlags` (§II.23.1.4) — 0 for an ordinary event (no `SpecialName`/`RTSpecialName`).
    event_flags: u16,
    /// `Name` — `#Strings` offset.
    name: u32,
    /// `EventType` — `TypeDefOrRef` coded index of the delegate type subscribers must match.
    event_type: u32,
}

/// §II.22.35 `PropertyMap` — one row per type that declares properties, pointing at that type's
/// first `Property` row (a table-position "run-start", exactly like `EventMap.EventList` /
/// `TypeDef.MethodList`). NOT sorted (same §II.24.2.6 status as `EventMap`).
struct PropertyMapRow {
    /// `Parent` — simple `TypeDef` index of the property-declaring type.
    parent: u32,
    /// `PropertyList` — simple `Property` index of the first property this type owns (run
    /// continues to the next `PropertyMap`'s `PropertyList`, or the end of the `Property` table).
    property_list: u32,
}

/// §II.22.34 `Property` — a single property's name + `PropertySig` blob. The getter/setter
/// accessor linkage lives in `MethodSemantics`, not here. NOTE the spec's column named "Type" is
/// a `#Blob` index holding a §II.23.2.5 `PropertySig`, NOT a `TypeDefOrRef` — a spec misnomer.
struct PropertyRow {
    /// `Flags` (§II.23.1.14 `PropertyAttributes`) — 0 for an ordinary property (no
    /// `SpecialName`/`RTSpecialName`/`HasDefault`).
    flags: u16,
    /// `Name` — `#Strings` offset.
    name: u32,
    /// `Type` — `#Blob` offset of the `PropertySig` (see the struct doc's misnomer note).
    signature: u32,
}

/// §II.22.28 `MethodSemantics` — links a `MethodDef` (an `add_`/`remove_`/getter/setter body) to
/// the `Event`/`Property` it services. Sorted by `association`.
struct MethodSemanticsRow {
    /// `Semantics` (§II.23.1.12) — 0x8 `AddOn`, 0x10 `RemoveOn`, 0x20 `Fire`, 0x4 `Other`.
    semantics: u16,
    /// `Method` — simple `MethodDef` index of the accessor.
    method: u32,
    /// `Association` — `HasSemantics` coded index (0 = `Event`, 1 = `Property`).
    association: u32,
}

/// §II.22.20 `GenericParam` — one declared generic parameter of a generic type or method
/// DEFINITION (`interface IBox<T>` gets one row: `{Number: 0, Flags: 0, Owner: IBox`1's TypeDef,
/// Name: "T"}`). SORTED by `owner` (the coded `TypeOrMethodDef` index) then `number`
/// (§II.24.2.6's sorted-table list includes it — primary key Owner, secondary Number).
struct GenericParamRow {
    /// `Number` — the parameter's 0-based ordinal in the declaration order (`T`=0, `U`=1, …).
    number: u16,
    /// `Flags` (§II.23.1.7 `GenericParamAttributes`) — always 0 here: no variance
    /// (`in`/`out`) and no special constraints (`class`/`struct`/`new()`); constrained/variant
    /// parameters are a macro-level loud reject, never silently-dropped metadata.
    flags: u16,
    /// `Owner` — `TypeOrMethodDef` coded index (§II.24.2.6, 1 tag bit: TypeDef=0, MethodDef=1)
    /// of the generic type/method definition declaring this parameter.
    owner: u32,
    /// `Name` — `#Strings` offset of the declared parameter name (purely reflective metadata,
    /// but Roslyn surfaces it — `typeof(IBox<>).GetGenericArguments()[0].Name`).
    name: u32,
}

/// Deferred `FieldRVA` bookkeeping: the row itself can't be built until the layout pass
/// (`pe.rs`) hands back a real RVA for the queued blob, so `add_static_field` records enough to
/// materialize it later via [`MetadataBuilder::set_field_rva`].
struct PendingFieldRva {
    /// Row index (1-based) into `field_rva` this pending entry will occupy once resolved.
    row: usize,
    field_token: Token,
}

/// Owns the four metadata heaps and every table row this backend populates. One instance per
/// emitted assembly (mirrors one `ILExporter::export_to_write` call).
#[derive(Default)]
pub struct MetadataBuilder {
    pub strings: StringsHeap,
    pub blobs: BlobHeap,
    pub guids: GuidHeap,
    pub user_strings: UserStringHeap,

    module: Vec<ModuleRow>,
    type_ref: Vec<TypeRefRow>,
    type_def: Vec<TypeDefRow>,
    field: Vec<FieldRow>,
    method_def: Vec<MethodDefRow>,
    param: Vec<ParamRow>,
    interface_impl: Vec<InterfaceImplRow>,
    member_ref: Vec<MemberRefRow>,
    constant: Vec<ConstantRow>,
    custom_attribute: Vec<CustomAttributeRow>,
    class_layout: Vec<ClassLayoutRow>,
    field_layout: Vec<FieldLayoutRow>,
    standalone_sig: Vec<StandAloneSigRow>,
    module_ref: Vec<ModuleRefRow>,
    type_spec: Vec<TypeSpecRow>,
    impl_map: Vec<ImplMapRow>,
    field_rva: Vec<FieldRvaRow>,
    assembly: Vec<AssemblyRow>,
    assembly_ref: Vec<AssemblyRefRow>,
    method_spec: Vec<MethodSpecRow>,
    method_impl: Vec<MethodImplRow>,
    event_map: Vec<EventMapRow>,
    event: Vec<EventRow>,
    property_map: Vec<PropertyMapRow>,
    property: Vec<PropertyRow>,
    method_semantics: Vec<MethodSemanticsRow>,
    generic_param: Vec<GenericParamRow>,

    /// Interned `TypeRef` rows, keyed on (resolution_scope token bits, namespace, name) so
    /// repeated references to the same external type share one row.
    type_ref_cache: HashMap<(u32, Box<str>, Box<str>), Token>,
    /// Interned `MemberRef` rows, keyed on (class token, name, signature blob offset).
    member_ref_cache: HashMap<(u32, Box<str>, u32), Token>,
    /// Interned `ModuleRef` rows, keyed on the module (library) name.
    module_ref_cache: HashMap<Box<str>, Token>,
    /// Interned `StandAloneSig` rows, keyed on the signature blob offset (both `calli` sites and
    /// `.locals` signatures share this cache — a signature blob is self-describing, so two
    /// identical blobs used for different purposes can safely share one row).
    standalone_sig_cache: HashMap<u32, Token>,
    calli_sig_cache: HashMap<String, Token>,
    locals_sig_cache: HashMap<Vec<String>, Token>,
    /// Interned `TypeSpec` rows, keyed on the signature blob offset.
    type_spec_cache: HashMap<u32, Token>,

    /// `Assembly` `ClassRef` interned handle -> the `TypeDef`/`TypeRef` token already resolved
    /// for it, so [`TypeDefOrRefResolver::type_def_or_ref`] and [`TokenSink::type_token`] never
    /// create duplicate rows for the same open class shape.
    class_token_cache: HashMap<Interned<ClassRef>, Token>,
    /// `MethodDefIdx` -> the `MethodDef` row's token, populated by [`MetadataBuilder::add_method`]
    /// so [`TokenSink::method_token`] can look up an in-assembly method without a second table
    /// scan.
    method_def_cache: HashMap<MethodDefIdx, Token>,

    /// Deferred `FieldRVA` rows awaiting a real RVA from the `pe` layout pass.
    pending_field_rva: Vec<PendingFieldRva>,

    /// `EntryPointToken` (§II.25.3.3), set by [`MetadataBuilder::set_entry_point`].
    entry_point: Option<Token>,

    /// Tracks the currently-open `TypeDef` row (its 1-based rid) so `add_field`/`add_method`
    /// know which owner's field/method run they're extending, mirroring `il_exporter`'s
    /// per-class emission loop.
    current_type_def: Option<u32>,

    /// `true` for a `.dll` output, `false` for a `.exe` — mirrors `il_exporter`'s `is_lib` flag
    /// (`ILExporter::new`'s `is_lib` parameter). Gates whether BCL `AssemblyRef` rows get a real
    /// version+public-key-token (`il_exporter::export_to_write`'s `if self.is_lib { … }` block,
    /// mod.rs:70-106) or stay name-only/`0.0.0.0` (an executable emits no `.assembly extern`
    /// headers at all; `ilasm` then infers unversioned externs from type-use, and the CLR's
    /// executable-load path resolves those leniently). Defaults to `false` (exe) since that is
    /// every existing hand-built test's shape; `export_pe` sets this explicitly from
    /// `ExportOptions::is_dll` before any `AssemblyRef` gets created.
    ///
    /// Getting this wrong is not cosmetic: stamping a real BCL version (`8.0.0.0` etc.) on an
    /// executable's `AssemblyRef` rows makes `AssemblyLoadContext.InternalLoad`'s native binder
    /// try to resolve each referenced BCL assembly at that EXACT version via the app's
    /// `.runtimeconfig.json`/deps rollForward machinery — and fail before the managed loader (or
    /// even `System.Reflection.Metadata`'s lenient `PEReader`) ever runs, surfacing as
    /// `System.IO.FileLoadException … The access code is invalid. (0x8007000C)` on the assembly's
    /// OWN identity, not the mismatched reference — root-caused by comparing a real `ilasm`-built
    /// `.exe` for the same source (whose `AssemblyRef` rows are all `0.0.0.0`, confirmed via a
    /// from-scratch metadata reader) against this exporter's `8.0.0.0`-stamped output.
    is_lib: bool,

    /// Runtime surface used to version BCL/framework assembly references. Defaults to .NET 8 so
    /// hand-built tests preserve their historical output; production export sets it explicitly
    /// before creating any `AssemblyRef` rows.
    runtime: DotnetRuntime,

    /// Optional metadata projection for the compiler's internal `MainModule` sentinel.
    public_module_full_name: Option<Box<str>>,
}

/// The 64-bit `Valid`/`Sorted` bitmask position for each table id (§II.24.2.6: bit `N` set iff
/// table `N` has at least one row / must be emitted sorted). Table ids double as bit indices
/// since every id in this backend's inventory is `< 64`.
const SORTED_TABLES: &[u32] = &[
    Token::TABLE_CONSTANT,
    Token::TABLE_INTERFACE_IMPL,
    Token::TABLE_CLASS_LAYOUT,
    Token::TABLE_FIELD_LAYOUT,
    Token::TABLE_FIELD_RVA,
    Token::TABLE_IMPL_MAP,
    // §II.24.2.6: MethodSemantics is sorted by Association, MethodImpl by Class. EventMap and
    // Event are NOT sorted tables (and are absent from this list on purpose).
    Token::TABLE_METHOD_SEMANTICS,
    Token::TABLE_METHOD_IMPL,
    // §II.24.2.6: GenericParam is sorted by Owner (coded TypeOrMethodDef) then Number.
    Token::TABLE_GENERIC_PARAM,
];

// ---- §II.23.3 CustomAttrib element-type tags used by `MetadataBuilder::add_custom_attribute` ----
// These match the corresponding `ELEMENT_TYPE_*` codes (§II.23.1.16) for every arg shape
// `crate::ir::class::CustomAttrArg` can express; the custom-attribute blob format reuses them
// directly for primitive/string FixedArgs and NamedArg FieldOrPropTypes (no boxing needed since
// this backend never emits an `Object`-typed ctor/property parameter).
const ATTR_ELEM_BOOLEAN: u8 = 0x02;
const ATTR_ELEM_U1: u8 = 0x05;
const ATTR_ELEM_I4: u8 = 0x08;
const ATTR_ELEM_I8: u8 = 0x0A;
const ATTR_ELEM_STRING: u8 = 0x0E;
/// §II.23.3 `NamedArg` kind discriminators.
const ATTR_NAMED_FIELD: u8 = 0x53;
const ATTR_NAMED_PROPERTY: u8 = 0x54;

fn write_u16_le(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Encodes one `CustomAttrArg`'s value as a §II.23.3 `FixedArg` (this exact same shape is reused
/// for both a positional ctor `FixedArg` and a `NamedArg`'s trailing value — the two are
/// byte-identical, only the surrounding named-arg header differs). A UTF-8 `SerString`
/// (compressed length + raw bytes, §II.23.2) for strings; the primitive's raw little-endian bytes
/// otherwise. Every shape has a fixed, unambiguous length, so this can never desynchronize the
/// blob — see `CustomAttrDef`'s doc.
fn encode_custom_attr_fixed_arg(
    out: &mut Vec<u8>,
    v: &crate::ir::class::CustomAttrArg,
    asm: &Assembly,
) {
    match v {
        crate::ir::class::CustomAttrArg::Str(s) => {
            let text = asm[*s].to_string();
            write_compressed_u32(out, u32::try_from(text.len()).unwrap());
            out.extend_from_slice(text.as_bytes());
        }
        crate::ir::class::CustomAttrArg::Bool(b) => out.push(u8::from(*b)),
        crate::ir::class::CustomAttrArg::U8(value) => out.push(*value),
        crate::ir::class::CustomAttrArg::I32(i) => out.extend_from_slice(&i.to_le_bytes()),
        crate::ir::class::CustomAttrArg::I64(i) => out.extend_from_slice(&i.to_le_bytes()),
    }
}

impl MetadataBuilder {
    pub(super) fn set_public_module_full_name(&mut self, name: Option<&str>) {
        self.public_module_full_name = name.map(Into::into);
    }

    /// Builds a fresh, empty builder — already seeded with the mandatory `<Module>` pseudo-`TypeDef`
    /// (§II.22.37: "The first row of the TypeDef table represents the pseudo class that acts as
    /// the parent for functions and variables defined at module scope" — every real assembly's
    /// user-defined types start at `TypeDef` **row 2**, never row 1). Discovered the hard way
    /// during the Phase 1a E2E milestone: without this, the first `add_type_def` call (e.g. for
    /// `MainModule`) became TypeDef row 1 itself, and the CLR silently reinterpreted it AS the
    /// `<Module>` pseudo-type — its methods were then treated as ownerless "global methods" (no
    /// enclosing class at all, confirmed via `monodis`), and the real load failure surfaced many
    /// steps downstream as an opaque `BadImageFormatException: Index not found`.
    #[must_use]
    pub fn new() -> Self {
        let mut mb = Self::default();
        // `<Module>`: TypeAttributes = 0 (not-public — it's never referenced by name), no fields,
        // no methods (`field_list`/`method_list` = 1, the same "run start" convention every other
        // `add_type_def` caller gets — an empty run when nothing follows it), Extends = NIL (the
        // pseudo-type has no base type at all, unlike every real class).
        let name_off = mb.strings.intern("<Module>");
        let namespace_off = mb.strings.intern("");
        mb.type_def.push(TypeDefRow {
            flags: 0,
            name: name_off,
            namespace: namespace_off,
            extends: 0,
            field_list: 1,
            method_list: 1,
        });
        mb
    }

    /// Sets `MetadataBuilder::is_lib` — see that field's doc. Must be called before any
    /// `AssemblyRef` row is created (i.e. right after [`MetadataBuilder::new`]), since it only
    /// affects rows created from that point on; `export_pe` calls this first, before Pass 0.
    pub fn set_is_lib(&mut self, is_lib: bool) {
        self.is_lib = is_lib;
    }

    /// Sets the runtime surface used by subsequently created framework `AssemblyRef` rows.
    pub fn set_runtime(&mut self, runtime: DotnetRuntime) {
        self.runtime = runtime;
    }

    /// Interns an `AssemblyRef` row (§II.22.5), returning its token. `name` is the `.NET`
    /// assembly identity (e.g. `"System.Runtime"`); `target` selects the BCL-versioned vs.
    /// name-only shape per `il_exporter`'s `bcl_public_key_token` split.
    pub fn assembly_ref(&mut self, name: &str, target: AssemblyRefTarget<'_>) -> Token {
        let name_off = self.strings.intern(name);
        let (major, minor, build, revision, public_key_or_token) = match target {
            AssemblyRefTarget::Bcl {
                version: (maj, min, bui, rev),
                token,
            } => (maj, min, bui, rev, self.blobs.intern(&token)),
            AssemblyRefTarget::NameOnly => (0, 0, 0, 0, 0),
            AssemblyRefTarget::_Marker(_) => unreachable!("hidden marker variant"),
        };
        self.assembly_ref.push(AssemblyRefRow {
            major,
            minor,
            build,
            revision,
            // §II.23.1.4 `AssemblyFlags`: 0 (no retargetable/full-public-key bits needed here).
            flags: 0,
            public_key_or_token,
            name: name_off,
            culture: 0,
            hash_value: 0,
        });
        let rid = u32::try_from(self.assembly_ref.len()).unwrap();
        Token::new(Token::TABLE_ASSEMBLY_REF, rid)
    }

    /// Interns a `TypeRef` row (§II.22.38): a reference to a type defined in another module or
    /// assembly (`resolution_scope` is the token of the owning `AssemblyRef`/`ModuleRef`, or
    /// `None` for a nested/self-module reference). Mirrors `simple_class_ref`/`class_ref` in
    /// `il_exporter` (the `[assembly]'Name'` rendering).
    pub fn type_ref(
        &mut self,
        resolution_scope: Option<Token>,
        namespace: &str,
        name: &str,
    ) -> Token {
        let scope_bits = resolution_scope.map_or(0, |t| t.0);
        let key = (scope_bits, Box::from(namespace), Box::from(name));
        if let Some(&tok) = self.type_ref_cache.get(&key) {
            return tok;
        }
        let resolution_scope_coded = encode_resolution_scope(resolution_scope);
        let namespace_off = self.strings.intern(namespace);
        let name_off = self.strings.intern(name);
        self.type_ref.push(TypeRefRow {
            resolution_scope: resolution_scope_coded,
            namespace: namespace_off,
            name: name_off,
        });
        let rid = u32::try_from(self.type_ref.len()).unwrap();
        let tok = Token::new(Token::TABLE_TYPE_REF, rid);
        self.type_ref_cache.insert(key, tok);
        tok
    }

    /// Adds a `TypeDef` row (§II.22.37) for a class this assembly defines. `FieldList`/
    /// `MethodList` (§II.22.37's run-start columns) are stamped with "one past the current end
    /// of field/method rows" AT THIS CALL — correct only when this class's own fields/methods
    /// are added immediately afterward, with no other `add_type_def` call in between (the
    /// *insertion-order* invariant `add_field`/`add_method` assume, matching `il_exporter`'s
    /// per-class loop, which emits a class's `.class` block and its `.field`/method bodies as
    /// one contiguous unit).
    ///
    /// A caller that must create every `TypeDef` row UP FRONT (before any field/method exists —
    /// needed so a field's *type* can forward-reference a class def that appears later in
    /// iteration order, resolved via `MetadataBuilder::find_type_def`) gets a WRONG
    /// `field_list`/`method_list` here (every such row reads back as `1`, since no field/method
    /// row exists yet at any of those calls) and MUST re-stamp the correct run-start once that
    /// class's fields/methods are about to be appended, via
    /// [`MetadataBuilder::set_type_def_field_list`] / [`MetadataBuilder::set_type_def_method_list`].
    pub fn add_type_def(
        &mut self,
        namespace: &str,
        name: &str,
        is_valuetype: bool,
        extends: Option<Token>,
        pack: Option<u16>,
        size: Option<u32>,
        implements: &[Token],
    ) -> Token {
        let name = dotnet_class_name(name);
        let name_off = self.strings.intern(&name);
        let namespace_off = self.strings.intern(namespace);
        let extends_coded = extends.map_or(0, encode_type_def_or_ref_token);
        // §II.23.1.15 `TypeAttributes`: 0x1 = Public, 0x0 = NotPublic (private). Sealed
        // (0x100) mirrors `il_exporter`'s `sealed` valuetype rule; layout bits (0x0 = auto,
        // 0x10 = sequential/explicit — we only ever need explicit, 0x10) mirror `explicit`.
        // This API doesn't thread `il_exporter`'s per-class `Access` enum through (`add_type_def`
        // has no `access` parameter), so every TypeDef is conservatively marked Public — a safe
        // superset of `il_exporter`'s private/public split, since any cross-class access this
        // backend ever emits stays legal either way.
        let mut flags: u32 = 0x1; // public
        if is_valuetype {
            flags |= 0x100; // sealed
        }
        let has_explicit_layout = pack.is_some() || size.is_some();
        if has_explicit_layout {
            flags |= 0x10; // ExplicitLayout
        }
        let field_list = u32::try_from(self.field.len() + 1).unwrap();
        let method_list = u32::try_from(self.method_def.len() + 1).unwrap();
        self.type_def.push(TypeDefRow {
            flags,
            name: name_off,
            namespace: namespace_off,
            extends: extends_coded,
            field_list,
            method_list,
        });
        let rid = u32::try_from(self.type_def.len()).unwrap();
        let tok = Token::new(Token::TABLE_TYPE_DEF, rid);
        self.current_type_def = Some(rid);
        if has_explicit_layout {
            self.class_layout.push(ClassLayoutRow {
                packing_size: pack.unwrap_or(0),
                class_size: size.unwrap_or(0),
                parent: rid,
            });
        }
        for &interface in implements {
            self.interface_impl.push(InterfaceImplRow {
                class: rid,
                interface: encode_type_def_or_ref_token(interface),
            });
        }
        tok
    }

    /// Re-stamps a `TypeDef` row's `FieldList` run-start column (§II.22.37) to the CURRENT end
    /// of the `Field` table (i.e. `self.field.len() + 1` — "the next field row added belongs to
    /// this class"). For a caller that creates every `TypeDef` up front (see
    /// [`MetadataBuilder::add_type_def`]'s doc) and only later walks classes again to append
    /// their fields: call this immediately BEFORE adding `tok`'s class's fields, in the SAME
    /// per-class order `add_type_def` originally ran in — `FieldList` is a run-START pointer
    /// (the run's END is implicit: whatever the NEXT TypeDef row's `FieldList` says), so classes
    /// must still be visited in a consistent order or ranges overlap/gap incorrectly. A class
    /// with zero fields still needs this call (stamping the current cursor, an empty range) so
    /// the run boundary is correct for its NEIGHBORS even though it owns no rows itself.
    ///
    /// # Panics
    /// If `tok` is not a `TypeDef` token, or its row index is out of range.
    pub fn set_type_def_field_list(&mut self, tok: Token) {
        assert_eq!(
            tok.table(),
            Token::TABLE_TYPE_DEF,
            "not a TypeDef token: {tok:?}"
        );
        let idx = usize::try_from(tok.rid()).unwrap() - 1;
        self.type_def[idx].field_list = u32::try_from(self.field.len() + 1).unwrap();
    }

    /// The `MethodDef`-table analogue of [`MetadataBuilder::set_type_def_field_list`] — re-stamps
    /// `MethodList` (§II.22.37) to `self.method_def.len() + 1`. See that method's doc for the
    /// full run-pointer contract and the "call before this class's OWN methods, zero-method
    /// classes included" ordering requirement.
    ///
    /// # Panics
    /// If `tok` is not a `TypeDef` token, or its row index is out of range.
    pub fn set_type_def_method_list(&mut self, tok: Token) {
        assert_eq!(
            tok.table(),
            Token::TABLE_TYPE_DEF,
            "not a TypeDef token: {tok:?}"
        );
        let idx = usize::try_from(tok.rid()).unwrap() - 1;
        self.type_def[idx].method_list = u32::try_from(self.method_def.len() + 1).unwrap();
    }

    /// Adds a `private explicit ansi sealed` `TypeDef` (§II.22.37) sized to exactly `size` bytes
    /// with `.pack 1` — the "blob-sized valuetype" shape a const-data buffer's synthetic
    /// `__rcl_const_blob_{size}` carrier type needs (see `docs/PE_EMISSION_PLAN.md`'s FieldRVA-
    /// sizing lesson: a `FieldRVA` field must be typed to its blob's exact byte width, or
    /// NativeAOT's ILC keeps only 1 byte of the blob — commit 4b487f7). Mirrors `il_exporter`'s
    /// literal text for this exact construct: `.class private explicit ansi sealed
    /// '__rcl_const_blob_{n}' extends [System.Runtime]System.ValueType {{ .pack 1 .size {n} }}`
    /// (`il_exporter/mod.rs:120`).
    ///
    /// Deliberately a separate method rather than a new parameter on [`MetadataBuilder::add_type_def`]:
    /// that method's existing "every TypeDef is conservatively marked Public" contract is
    /// documented and depended on by every other caller (9 call sites as of Phase 1b), so this adds
    /// the one new (private, sealed, explicit-layout, fixed-size) shape as its own entry point
    /// instead of threading a visibility flag through everything that doesn't need it.
    pub fn add_blob_sized_valuetype(&mut self, name: &str, extends: Token, size: u32) -> Token {
        let name = dotnet_class_name(name);
        let name_off = self.strings.intern(&name);
        let namespace_off = self.strings.intern("");
        let extends_coded = encode_type_def_or_ref_token(extends);
        // §II.23.1.15 `TypeAttributes`: 0x0 NotPublic (private) | 0x100 Sealed | 0x10 ExplicitLayout.
        let flags: u32 = 0x100 | 0x10;
        let field_list = u32::try_from(self.field.len() + 1).unwrap();
        let method_list = u32::try_from(self.method_def.len() + 1).unwrap();
        self.type_def.push(TypeDefRow {
            flags,
            name: name_off,
            namespace: namespace_off,
            extends: extends_coded,
            field_list,
            method_list,
        });
        let rid = u32::try_from(self.type_def.len()).unwrap();
        let tok = Token::new(Token::TABLE_TYPE_DEF, rid);
        self.current_type_def = Some(rid);
        self.class_layout.push(ClassLayoutRow {
            packing_size: 1,
            class_size: size,
            parent: rid,
        });
        tok
    }

    /// Encodes a field signature (§II.23.2.4) whose type is a bare `valuetype` reference to a
    /// `TypeDef`/`TypeRef` `token` — `SIG_FIELD (0x06) ET_VALUETYPE (0x11) <coded TypeDefOrRef
    /// index>`. Interns the blob and returns its `#Blob` heap offset, ready for
    /// [`MetadataBuilder::add_static_field`]'s `signature_blob` parameter.
    ///
    /// A standalone helper rather than routing through [`sig::encode_field_sig`] /
    /// [`sig::TypeDefOrRefResolver`]: that path resolves a `ClassRef` through `type_def_or_ref`,
    /// which decides TypeDef-vs-TypeRef by checking `Assembly::class_ref_to_def` — but the
    /// `__rcl_const_blob_{n}` carrier types [`MetadataBuilder::add_blob_sized_valuetype`] creates
    /// are metadata-only rows with no matching `Assembly`-level `ClassDef`/`ClassRef` (mirrors
    /// `il_exporter`, which never allocates a Rust-side `ClassRef` for them either — they only
    /// ever exist as raw IL text). This writer already has the `TypeDef` `Token` in hand (the
    /// return value of `add_blob_sized_valuetype`), so it skips the resolver entirely.
    pub fn field_sig_for_valuetype_token(&mut self, token: Token) -> u32 {
        let mut blob = Vec::new();
        blob.push(sig::SIG_FIELD);
        const ET_VALUETYPE: u8 = 0x11;
        blob.push(ET_VALUETYPE);
        write_compressed_u32(&mut blob, encode_type_def_or_ref_token(token));
        self.blobs.intern(&blob)
    }

    /// Adds an instance `Field` row (§II.22.15) to the most recently added `TypeDef`.
    /// `offset` mirrors `ClassDef::fields()`'s `Option<u32>` (`.field [N] …`) and populates a
    /// `FieldLayout` row (§II.22.16) when present.
    pub fn add_field(&mut self, name: &str, signature_blob: u32, offset: Option<u32>) -> Token {
        // §II.23.1.5 `FieldAttributes`: the low 3 bits are `FieldAccessMask`
        // (CompilerControlled=0x0, Private=0x1, FamANDAssem=0x2, Assembly=0x3, Family=0x4,
        // FamORAssem=0x5, **Public=0x6**) — NOT the `TypeAttributes::VisibilityMask` numbering
        // (where `0x1` happens to mean Public) this constant was previously copy-pasted from.
        // `0x1` here is actually `Private`, which a real CoreCLR JIT enforces at field-access
        // time (unlike `ilasm`, which apparently never got exercised with cross-class field
        // access in the differential suite) — surfaced as `FieldAccessException: Attempt by
        // method '…' to access field '….x' failed` once fields started landing in their real
        // owning `TypeDef` instead of accidentally aliasing the caller's own class (see the
        // `FieldList`/`MethodList` run-pointer fix this same commit makes). Public (0x6) is a
        // safe superset here since this writer never emits cross-class private field coupling.
        let flags: u16 = 0x6;
        let name_off = self.strings.intern(name);
        self.field.push(FieldRow {
            flags,
            name: name_off,
            signature: signature_blob,
        });
        let rid = u32::try_from(self.field.len()).unwrap();
        let tok = Token::new(Token::TABLE_FIELD, rid);
        if let Some(offset) = offset {
            self.field_layout
                .push(FieldLayoutRow { offset, field: rid });
        }
        tok
    }

    /// Adds the special instance field every CLR enum must carry.
    pub fn add_enum_value_field(&mut self, signature_blob: u32) -> Token {
        let name = self.strings.intern("value__");
        self.field.push(FieldRow {
            // Public | SpecialName | RTSpecialName (§II.23.1.5).
            flags: 0x6 | 0x0200 | 0x0400,
            name,
            signature: signature_blob,
        });
        Token::new(Token::TABLE_FIELD, u32::try_from(self.field.len()).unwrap())
    }

    /// Adds a public static literal enum member and its metadata Constant row.
    pub fn add_enum_literal_field(
        &mut self,
        name: &str,
        signature_blob: u32,
        value: Const,
    ) -> Token {
        let name = self.strings.intern(name);
        self.field.push(FieldRow {
            // Public | Static | Literal | HasDefault (§II.23.1.5).
            flags: 0x6 | 0x0010 | 0x0040 | 0x8000,
            name,
            signature: signature_blob,
        });
        let rid = u32::try_from(self.field.len()).unwrap();
        let (type_code, bytes): (u8, Vec<u8>) = match value {
            Const::I8(v) => (0x04, v.to_le_bytes().to_vec()),
            Const::U8(v) => (0x05, v.to_le_bytes().to_vec()),
            Const::I16(v) => (0x06, v.to_le_bytes().to_vec()),
            Const::U16(v) => (0x07, v.to_le_bytes().to_vec()),
            Const::I32(v) => (0x08, v.to_le_bytes().to_vec()),
            Const::U32(v) => (0x09, v.to_le_bytes().to_vec()),
            Const::I64(v) => (0x0A, v.to_le_bytes().to_vec()),
            Const::U64(v) => (0x0B, v.to_le_bytes().to_vec()),
            other => panic!("unsupported CLR enum literal constant {other:?}"),
        };
        let value = self.blobs.intern(&bytes);
        self.constant.push(ConstantRow {
            type_code,
            parent: rid << 2, // HasConstant: Field tag = 0.
            value,
        });
        Token::new(Token::TABLE_FIELD, rid)
    }

    /// Adds a `static` `Field` row. `rva_data` mirrors `il_exporter`'s FieldRVA statics (the
    /// `.data cil I_N = bytearray (…)` + `.field … at I_N` pair, lines ~107-127): when present,
    /// the bytes are queued for placement in `.sdata` by the `pe` layout pass and a `FieldRVA`
    /// row (§II.22.18) is added once that placement assigns a real RVA (see
    /// [`MetadataBuilder::set_field_rva`]). `is_thread_static` triggers
    /// [`MetadataBuilder::thread_static_attribute`] on the returned token, mirroring
    /// `StaticFieldDef::is_tls`. `is_const` sets `FieldAttributes::InitOnly` (§II.23.1.5, 0x20),
    /// mirroring `StaticFieldDef::is_const` / `il_exporter`'s `initonly` keyword (mod.rs:224,328)
    /// — note this is the .NET *InitOnly* semantic (settable once, typically from a `.cctor`), NOT
    /// the metadata `Constant` table (§II.22.9, compile-time-substituted literals) — `il_exporter`
    /// never emits a `Constant` row for these either, so this mirrors that scope exactly.
    pub fn add_static_field(
        &mut self,
        name: &str,
        signature_blob: u32,
        rva_data: Option<Vec<u8>>,
        is_thread_static: bool,
        is_const: bool,
    ) -> Token {
        // §II.23.1.5 `FieldAttributes`: Public (0x6, see `add_field`'s doc for why not 0x1) |
        // Static (0x10) | InitOnly (0x20, when `is_const`).
        let mut flags: u16 = 0x6 | 0x10;
        if is_const {
            flags |= 0x20;
        }
        if rva_data.is_some() {
            flags |= 0x100; // HasFieldRVA
        }
        let name_off = self.strings.intern(name);
        self.field.push(FieldRow {
            flags,
            name: name_off,
            signature: signature_blob,
        });
        let rid = u32::try_from(self.field.len()).unwrap();
        let tok = Token::new(Token::TABLE_FIELD, rid);
        if rva_data.is_some() {
            // Placeholder row; `set_field_rva` overwrites `rva` once the layout pass runs.
            // Recorded positionally via `pending_field_rva` so `serialize()` can assert every
            // queued blob was eventually resolved.
            self.field_rva.push(FieldRvaRow { rva: 0, field: rid });
            self.pending_field_rva.push(PendingFieldRva {
                row: self.field_rva.len(),
                field_token: tok,
            });
        }
        if is_thread_static {
            self.thread_static_attribute(tok);
        }
        tok
    }

    /// Adds a `MethodDef` row (§II.22.26) to the most recently added `TypeDef`, plus its `Param`
    /// rows (§II.22.33, one per named argument, plus Sequence 0 when return metadata is needed).
    /// Argument rows mirror `MethodDef::arg_names()`. The body RVA
    /// is unknown until `body.rs` assembles bytes and `pe.rs` lays them out, so it starts at 0
    /// and must be patched via [`MetadataBuilder::set_method_body_rva`] before `serialize()`.
    ///
    /// `out_params` — 1-based Param **Sequence** numbers (positions among `param_names`, which are
    /// already receiver-stripped) whose `Param` row gets `ParamAttributes.Out` (0x0002,
    /// §II.23.1.13). Paired with an `ELEMENT_TYPE_BYREF` type in `signature_blob` this is what
    /// makes C# see `out T`; a BYREF param whose row stays `Flags == 0` reads back as `ref T`
    /// (byte-matching csc, which sets no other bit and no modreq for `out`). Empty for every
    /// method today except `#[dotnet_interface]` members carrying `#[dotnet_out]`
    /// (`MethodDef::out_params`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_method(
        &mut self,
        name: &str,
        signature_blob: u32,
        param_names: &[Option<&str>],
        out_params: &[u16],
        is_static: bool,
        is_virtual: bool,
        is_ctor: bool,
        pinvoke: Option<(&str, Option<&str>, PInvokeCallConv, bool)>,
        aggressive_inline: bool,
        nullability: Option<MethodNullability<'_>>,
    ) -> Token {
        self.add_method_with_access(
            name,
            crate::Access::Public,
            signature_blob,
            param_names,
            out_params,
            is_static,
            is_virtual,
            is_ctor,
            pinvoke,
            aggressive_inline,
            nullability,
        )
    }

    /// Accessibility-aware counterpart to [`Self::add_method`]. Most synthetic metadata helpers
    /// intentionally emit public methods and use the convenience wrapper; assembly export routes
    /// real [`MethodDef`](crate::MethodDef) accessibility through this entry point.
    #[allow(clippy::too_many_arguments)]
    pub fn add_method_with_access(
        &mut self,
        name: &str,
        access: crate::Access,
        signature_blob: u32,
        param_names: &[Option<&str>],
        out_params: &[u16],
        is_static: bool,
        is_virtual: bool,
        is_ctor: bool,
        pinvoke: Option<(&str, Option<&str>, PInvokeCallConv, bool)>,
        aggressive_inline: bool,
        nullability: Option<MethodNullability<'_>>,
    ) -> Token {
        // §II.23.1.10 `MethodAttributes`: the low 3 bits are `MemberAccessMask`, numbered
        // identically to `FieldAttributes::FieldAccessMask` (see `add_field`'s doc) —
        // CompilerControlled=0x0, Private=0x1, FamANDAssem=0x2, Assembly=0x3, Family=0x4,
        // FamORAssem=0x5, **Public=0x6**. 0x10 Static, 0x40 Virtual, **0x100 NewSlot** (paired with
        // Virtual so it doesn't try to override a base slot), 0x0800 SpecialName | 0x1000
        // RTSpecialName (both together, ctors only), 0x2000 PInvokeImpl.
        //
        // BUG FIX (was `0x0400`, which is `Abstract`, NOT `NewSlot` — a real off-by-one-hex-digit
        // confusion between adjacent flag bits): this previously stamped `Abstract` on EVERY
        // virtual method this exporter ever emitted, real body or not. §II.22.26 requires a
        // non-abstract method's `RVA` be nonzero (it has a body); a method that is BOTH `Abstract`
        // and has a nonzero `RVA` is the exact malformed shape CoreCLR's native type loader rejects
        // outright with `TypeLoadException: Abstract method with non-zero RVA` the moment the
        // declaring type is loaded — before any managed reflection API (which never surfaced this,
        // since `System.Reflection.Metadata` treats `Attributes`/`RelativeVirtualAddress` as inert
        // data, not a loader-enforced invariant) or the IL body itself is ever touched. Root-caused
        // via `UnmanagedThreadStart::Start` (`cilly/src/ir/builtins/thread.rs`) in the `pal_threads`
        // battery target: a real virtual method with a real body, whose class only gets touched
        // (and only then type-loaded, tripping this check) once `thread::spawn` first runs — this
        // is why simpler probes with no virtual dispatch never hit it, and why the failure surfaced
        // as "abstract method" arbitrarily far from the actual defect site in the stack trace.
        let mut flags: u16 = match access {
            crate::Access::Private => 0x1,
            crate::Access::Assembly | crate::Access::InternalExtern => 0x3,
            crate::Access::Extern | crate::Access::Public => 0x6,
        };
        if is_static {
            flags |= 0x10;
        }
        if is_virtual {
            flags |= 0x40 | 0x0100;
        }
        if is_ctor {
            flags |= 0x1000 | 0x0800;
        }
        if pinvoke.is_some() {
            flags |= 0x2000;
        }
        // §II.23.1.11 `MethodImplAttributes`: 0x0 Managed/IL, except a `pinvokeimpl` method
        // which is unmanaged-forwarded and marked `PreserveSig` (0x80) to match `il_exporter`'s
        // `pinvokeimpl` + `preservesig` pairing (native calling convention, no HRESULT wrapping).
        // `0x100` is `AggressiveInlining` — mirrors `il_exporter`'s JIT hint (mod.rs:462-471) for
        // small, single-block, handler-free bodies; see `add_method`'s caller (`export.rs`) for
        // the heuristic that computes `aggressive_inline`. This was a real, documented parity gap
        // (`pdb.rs`'s module doc, "Phase-0 probe" section, gap (a)): under `DIRECT_PE=1` no
        // `AggressiveInlining` bit was ever written, so RyuJIT could not inline tiny leaf helpers
        // (e.g. the `cast_f64_u32`-style saturating float->int cast helpers) into hot callers —
        // pure JIT hint, cannot affect correctness.
        let mut impl_flags: u16 = if pinvoke.is_some() { 0x80 } else { 0x0 };
        if aggressive_inline {
            impl_flags |= 0x100;
        }
        let name_off = self.strings.intern(name);
        let param_list = u32::try_from(self.param.len() + 1).unwrap();
        self.method_def.push(MethodDefRow {
            rva: 0,
            impl_flags,
            flags,
            name: name_off,
            signature: signature_blob,
            param_list,
        });
        let rid = u32::try_from(self.method_def.len()).unwrap();
        let tok = Token::new(Token::TABLE_METHOD_DEF, rid);
        if let Some(nullability) = nullability {
            assert!(
                matches!(nullability.context, 1 | 2),
                "nullable context must be 1 or 2"
            );
            assert_eq!(
                nullability.parameter_flags.len(),
                param_names.len(),
                "nullable parameter flags must be parallel to receiver-stripped Param rows"
            );
            self.nullable_metadata_attribute(tok, nullability.context, true);
            if let Some(flag) = nullability.return_flag {
                self.param.push(ParamRow {
                    flags: 0,
                    sequence: 0,
                    name: 0,
                });
                let return_param =
                    Token::new(Token::TABLE_PARAM, u32::try_from(self.param.len()).unwrap());
                self.nullable_metadata_attribute(return_param, flag, false);
            }
        }
        for (i, pname) in param_names.iter().enumerate() {
            let sequence = u16::try_from(i + 1).unwrap();
            let name_off = pname.map_or(0, |n| self.strings.intern(n));
            // §II.23.1.13 `ParamAttributes`: 0x0002 `Out` — the only flag this exporter ever sets
            // (see `add_method`'s doc; `In`/`Optional` are never needed for the shapes we emit).
            let flags = if out_params.contains(&sequence) {
                0x0002
            } else {
                0
            };
            self.param.push(ParamRow {
                flags,
                sequence,
                name: name_off,
            });
            if let Some(nullable_flag) =
                nullability.and_then(|metadata| metadata.parameter_flags[i])
            {
                let param =
                    Token::new(Token::TABLE_PARAM, u32::try_from(self.param.len()).unwrap());
                self.nullable_metadata_attribute(param, nullable_flag, false);
            }
        }
        if let Some((lib, entry_point, call_conv, preserve_errno)) = pinvoke {
            let module_ref = self.intern_module_ref(lib);
            // §II.23.1.7 `PInvokeAttributes`: 0x1 NoMangle | 0x4 CharSetAnsi | call convention |
            // (0x40 SupportsLastError when `preserve_errno`) — mirrors `il_exporter`'s
            // `pinvokeimpl("<lib>" <convention> [lasterr])` rendering.
            let convention = match call_conv {
                PInvokeCallConv::Winapi => 0x100,
                PInvokeCallConv::Cdecl => 0x200,
                PInvokeCallConv::Stdcall => 0x300,
                PInvokeCallConv::Thiscall => 0x400,
                PInvokeCallConv::Fastcall => 0x500,
            };
            let mut mapping_flags: u16 = 0x1 | 0x4 | convention;
            if preserve_errno {
                mapping_flags |= 0x40;
            }
            let import_name = self.strings.intern(entry_point.unwrap_or(name));
            self.impl_map.push(ImplMapRow {
                mapping_flags,
                member_forwarded: encode_member_forwarded(tok),
                import_name,
                import_scope: module_ref.rid(),
            });
        }
        tok
    }

    /// Emit compiler-recognized nullable metadata on a method or Param row.
    fn nullable_metadata_attribute(&mut self, parent: Token, flag: u8, context: bool) -> Token {
        assert!(
            matches!(flag, 1 | 2),
            "nullable metadata flag must be 1 or 2"
        );
        let scope = self.system_runtime_assembly_ref();
        let attribute_name = if context {
            "NullableContextAttribute"
        } else {
            "NullableAttribute"
        };
        let type_ref = self.type_ref(
            Some(scope),
            "System.Runtime.CompilerServices",
            attribute_name,
        );
        let ctor_signature = {
            let mut signature = Vec::new();
            signature.push(sig::SIG_HASTHIS);
            write_compressed_u32(&mut signature, 1);
            signature.push(0x01); // ELEMENT_TYPE_VOID
            signature.push(ATTR_ELEM_U1);
            self.blobs.intern(&signature)
        };
        let ctor = self.member_ref(type_ref, ".ctor", ctor_signature);
        let value = self.blobs.intern(&[0x01, 0x00, flag, 0x00, 0x00]);
        self.custom_attribute.push(CustomAttributeRow {
            parent: encode_has_custom_attribute(parent),
            ctor: encode_custom_attribute_type(ctor),
            value,
        });
        Token::new(
            Token::TABLE_CUSTOM_ATTRIBUTE,
            u32::try_from(self.custom_attribute.len()).unwrap(),
        )
    }

    /// Finds-or-creates the `ModuleRef` row for `lib` (§II.22.31), used by `pinvokeimpl` methods.
    fn intern_module_ref(&mut self, lib: &str) -> Token {
        if let Some(&tok) = self.module_ref_cache.get(lib) {
            return tok;
        }
        let name_off = self.strings.intern(lib);
        self.module_ref.push(ModuleRefRow { name: name_off });
        let rid = u32::try_from(self.module_ref.len()).unwrap();
        let tok = Token::new(Token::TABLE_MODULE_REF, rid);
        self.module_ref_cache.insert(Box::from(lib), tok);
        tok
    }

    /// Records that `method` (an `Assembly`-level `MethodDefIdx`) was emitted as the `MethodDef`
    /// row `tok`, so [`TokenSink::method_token`] resolves later in-assembly calls to `method`
    /// directly instead of (incorrectly) synthesizing a `MemberRef` to itself. Callers driving the
    /// populate pass (e.g. `export::export_pe`) call this once per method immediately after
    /// [`MetadataBuilder::add_method`], mirroring how [`MetadataBuilder::add_type_def`]'s caller
    /// is expected to add a class's methods only after its own `TypeDef` row exists.
    pub fn register_method_def(&mut self, method: MethodDefIdx, tok: Token) {
        self.method_def_cache.insert(method, tok);
    }

    /// Looks up the `MethodDef` token previously recorded via
    /// [`MetadataBuilder::register_method_def`], if any.
    #[must_use]
    pub fn method_def_token(&self, method: MethodDefIdx) -> Option<Token> {
        self.method_def_cache.get(&method).copied()
    }

    /// Patches the body RVA of a previously-added `MethodDef` row (§II.22.26 `RVA` column) once
    /// `pe.rs`'s layout pass has placed the assembled body bytes in `.text`.
    pub fn set_method_body_rva(&mut self, method: Token, rva: u32) {
        assert_eq!(
            method.table(),
            Token::TABLE_METHOD_DEF,
            "not a MethodDef token"
        );
        let idx = usize::try_from(method.rid()).unwrap() - 1;
        self.method_def[idx].rva = rva;
    }

    /// Interns a `MemberRef` row (§II.22.25): a reference to a field or method defined in
    /// another `TypeRef`/`TypeSpec`/`MethodDef` (`class` is that owner's coded index token).
    /// Used for every BCL call (`Console.WriteLine`, …) and cross-assembly field access.
    pub fn member_ref(&mut self, class: Token, name: &str, signature_blob: u32) -> Token {
        let key = (class.0, Box::from(name), signature_blob);
        if let Some(&tok) = self.member_ref_cache.get(&key) {
            return tok;
        }
        let class_coded = encode_member_ref_parent(class);
        let name_off = self.strings.intern(name);
        self.member_ref.push(MemberRefRow {
            class: class_coded,
            name: name_off,
            signature: signature_blob,
        });
        let rid = u32::try_from(self.member_ref.len()).unwrap();
        let tok = Token::new(Token::TABLE_MEMBER_REF, rid);
        self.member_ref_cache.insert(key, tok);
        tok
    }

    /// Test-only: the `#Blob` offset of a `MemberRef` row's `Signature` column, by RID. Lets
    /// `body.rs`'s regression tests decode a real resolved-through-`method_token` `MethodRefSig`
    /// blob without the `member_ref` field itself needing to be `pub` outside `tables.rs`.
    #[cfg(test)]
    pub(crate) fn member_ref_signature_for_test(&self, token: Token) -> u32 {
        assert_eq!(token.table(), Token::TABLE_MEMBER_REF);
        self.member_ref[(token.rid() - 1) as usize].signature
    }

    /// Interns a `TypeSpec` row (§II.22.39): a signature-encoded type too complex for a
    /// `TypeDef`/`TypeRef` token alone (generic instantiations, arrays, pointers used as a
    /// standalone type operand — e.g. `ldelem`/`newarr` on `List<T>`). `blob` is a pre-encoded
    /// `sig::encode_type` signature (NOT a field/method/locals-wrapped one, per §II.23.2.14).
    pub fn type_spec(&mut self, blob: u32) -> Token {
        if let Some(&tok) = self.type_spec_cache.get(&blob) {
            return tok;
        }
        self.type_spec.push(TypeSpecRow { signature: blob });
        let rid = u32::try_from(self.type_spec.len()).unwrap();
        let tok = Token::new(Token::TABLE_TYPE_SPEC, rid);
        self.type_spec_cache.insert(blob, tok);
        tok
    }

    /// Interns a `MethodSpec` row (§II.22.29): a generic-method instantiation (`method<T,…>` in
    /// `il_exporter`'s rendering). `method` is the generic `MethodDef`/`MemberRef` token;
    /// `instantiation_blob` is a `sig::encode_method_spec_sig` blob. NOT deduplicated: unlike
    /// `MemberRef`/`TypeRef`, repeated `MethodSpec` rows for the same instantiation are harmless
    /// (each call site may legitimately want its own row) and §II.22.29 does not require the
    /// table to be sorted or unique.
    pub fn method_spec(&mut self, method: Token, instantiation_blob: u32) -> Token {
        let method_coded = encode_method_def_or_ref(method);
        self.method_spec.push(MethodSpecRow {
            method: method_coded,
            instantiation: instantiation_blob,
        });
        let rid = u32::try_from(self.method_spec.len()).unwrap();
        Token::new(Token::TABLE_METHOD_SPEC, rid)
    }

    /// Interns a `StandAloneSig` row (§II.22.36): either a `calli` call-site signature or a
    /// method body's `.locals` signature (both are bare signature blobs with no owning row).
    pub fn standalone_sig(&mut self, signature_blob: u32) -> Token {
        if let Some(&tok) = self.standalone_sig_cache.get(&signature_blob) {
            return tok;
        }
        self.standalone_sig.push(StandAloneSigRow {
            signature: signature_blob,
        });
        let rid = u32::try_from(self.standalone_sig.len()).unwrap();
        let tok = Token::new(Token::TABLE_STAND_ALONE_SIG, rid);
        self.standalone_sig_cache.insert(signature_blob, tok);
        tok
    }

    /// Adds an `InterfaceImpl` row (§II.22.23) directly. Normally populated as a side effect of
    /// [`MetadataBuilder::add_type_def`]'s `implements` argument; exposed separately for the rare
    /// case a caller needs to add one after the fact.
    pub fn interface_impl(&mut self, class: Token, interface: Token) -> Token {
        assert_eq!(
            class.table(),
            Token::TABLE_TYPE_DEF,
            "InterfaceImpl.Class must be a TypeDef"
        );
        self.interface_impl.push(InterfaceImplRow {
            class: class.rid(),
            interface: encode_type_def_or_ref_token(interface),
        });
        let rid = u32::try_from(self.interface_impl.len()).unwrap();
        Token::new(Token::TABLE_INTERFACE_IMPL, rid)
    }

    /// Adds a `MethodImpl` row (§II.22.27) — an explicit body-overrides-declaration binding, the
    /// metadata `.override` in ilasm produces. `class` is the overriding class's `TypeDef`;
    /// `body` is the overriding method (a `MethodDef` in that class); `declaration` is the base
    /// method being overridden (a `MethodDef` or `MemberRef` — both are `MethodDefOrRef` members).
    /// Retrofits the given `TypeDef` (must be the token `add_type_def` just returned) into a
    /// genuine ECMA-335 `interface` (§II.23.1.15): sets `Interface` (0x20) + `Abstract` (0x80),
    /// clears `Sealed` (an interface is never sealed), and NILs its `Extends` (interfaces have no
    /// base type — a non-NIL extends on an `Interface`-flagged type is a load-time rejection). The
    /// caller must have passed `extends = None` to `add_type_def`; this only enforces/asserts it.
    pub fn mark_type_def_interface(&mut self, tok: Token) {
        debug_assert_eq!(tok.table(), Token::TABLE_TYPE_DEF);
        let row = &mut self.type_def[tok.rid() as usize - 1];
        row.flags |= 0x20 | 0x80; // Interface | Abstract
        row.flags &= !0x100; // never Sealed
        row.extends = 0; // NIL
    }

    /// Clears the `NewSlot` (0x0100) flag on the given `MethodDef` (must be the token `add_method`
    /// just returned). An explicit base-class override REUSES the inherited vtable slot rather than
    /// allocating a new one — matching `il_exporter`'s `virtual instance` (no `newslot` keyword)
    /// for `#[dotnet_override]` methods. Paired with a `MethodImpl` row (`add_method_impl`).
    pub fn mark_method_reuse_slot(&mut self, method: Token) {
        debug_assert_eq!(method.table(), Token::TABLE_METHOD_DEF);
        self.method_def[method.rid() as usize - 1].flags &= !0x0100;
    }

    /// Marks the given `MethodDef` `Abstract` (0x0400) — no body, `RVA` stays 0 (§II.22.26). Used
    /// for interface members (see `il_exporter`'s `newslot abstract virtual`). The method must have
    /// been added with `is_virtual = true` (so it already carries `Virtual | NewSlot`) and must
    /// never have a body assembled for it (see `export_pe`'s Pass 4 skip).
    pub fn mark_method_abstract(&mut self, method: Token) {
        debug_assert_eq!(method.table(), Token::TABLE_METHOD_DEF);
        self.method_def[method.rid() as usize - 1].flags |= 0x0400;
    }

    /// Marks the given `MethodDef` a **static abstract** interface member (.NET 7+ static virtual
    /// members in interfaces — the `INumber<T>` generic-math shape). Roslyn ground truth (net8
    /// csc, verified via a `System.Reflection.Metadata` dump of a compiled
    /// `static abstract int Make();`): flags = `0x4D6` = `Public | Static | Virtual | HideBySig |
    /// Abstract` — `Virtual` **without** `NewSlot` (an INSTANCE abstract member is
    /// `Virtual | NewSlot | Abstract` instead, and the CoreCLR static-virtual loader path is
    /// stricter about the difference). The signature stays `SIG_DEFAULT` (no `HASTHIS`), exactly
    /// what `add_method`'s static path already encoded; RVA stays 0 (§II.22.26 — no body). The
    /// method must have been added with `is_static = true, is_virtual = false` (so it carries
    /// `Public | Static = 0x16` and no `NewSlot` to clear).
    pub fn mark_method_static_abstract(&mut self, method: Token) {
        debug_assert_eq!(method.table(), Token::TABLE_METHOD_DEF);
        let row = &mut self.method_def[method.rid() as usize - 1];
        debug_assert!(
            row.flags & 0x10 != 0,
            "must have been added with is_static = true"
        );
        debug_assert_eq!(row.flags & 0x100, 0, "a static virtual must NOT be NewSlot");
        row.flags |= 0x40 | 0x80 | 0x400; // Virtual | HideBySig | Abstract
    }

    /// ORs `SpecialName` (0x0800, §II.23.1.10) into the given `MethodDef`'s flags. §II.10.4 /
    /// §II.22.13 require an event's `add_*`/`remove_*` accessors to carry `SpecialName` (Roslyn
    /// emits its own accessors that way — e.g. an interface event's accessors are
    /// `Public|HideBySig|NewSlot|SpecialName|Abstract|Virtual`); [`MetadataBuilder::add_event`]
    /// stamps it on both accessors. NOT `RTSpecialName` (0x1000) — that is reserved for
    /// runtime-recognized names (`.ctor`/`.cctor`, see `add_method`'s ctor flag pair).
    pub fn mark_method_special_name(&mut self, method: Token) {
        debug_assert_eq!(method.table(), Token::TABLE_METHOD_DEF);
        self.method_def[method.rid() as usize - 1].flags |= 0x0800;
    }

    /// Adds a `GenericParam` row (§II.22.20) declaring generic parameter number `number` (0-based)
    /// named `name` on the generic type/method definition `owner` (a `TypeDef` or `MethodDef`
    /// token). Flags are always 0 — no variance, no special constraints (constrained parameters
    /// are rejected loudly at the macro level; `GenericParamConstraint` (0x2C) is not emitted).
    /// Rows are sorted by (coded Owner, Number) at serialize time (`write_generic_param_rows`),
    /// so callers may emit in any order — though the natural Pass-1 emission (ascending TypeDef
    /// rid, ascending number) is already sorted.
    pub fn add_generic_param(&mut self, owner: Token, number: u16, name: &str) -> Token {
        let name_off = self.strings.intern(name);
        self.generic_param.push(GenericParamRow {
            number,
            flags: 0,
            owner: encode_type_or_method_def(owner),
            name: name_off,
        });
        let rid = u32::try_from(self.generic_param.len()).unwrap();
        Token::new(Token::TABLE_GENERIC_PARAM, rid)
    }

    pub fn add_method_impl(&mut self, class: Token, body: Token, declaration: Token) {
        assert_eq!(
            class.table(),
            Token::TABLE_TYPE_DEF,
            "MethodImpl.Class must be a TypeDef"
        );
        self.method_impl.push(MethodImplRow {
            class: class.rid(),
            method_body: encode_method_def_or_ref(body),
            method_declaration: encode_method_def_or_ref(declaration),
        });
    }

    /// Adds one event (§II.22.13 `Event` + §II.22.28 `MethodSemantics` for its accessors) to the
    /// most-recently-opened `TypeDef`'s event run. The FIRST event added to a given class also
    /// creates that class's single §II.22.12 `EventMap` row (run-start into the `Event` table) —
    /// so all of a class's events must be added contiguously, before any other class's, exactly
    /// like the `Field`/`Method` run-start discipline (see [`MetadataBuilder::add_field`]).
    ///
    /// `class` is the declaring `TypeDef`; `name` the event name; `event_type` the delegate's
    /// `TypeDefOrRef` token; `add`/`remove` the accessor `MethodDef` tokens (already added via
    /// [`MetadataBuilder::add_method`]).
    pub fn add_event(
        &mut self,
        class: Token,
        name: &str,
        event_type: Token,
        add: Token,
        remove: Token,
    ) {
        assert_eq!(
            class.table(),
            Token::TABLE_TYPE_DEF,
            "Event owner must be a TypeDef"
        );
        // §II.10.4/§II.22.13: event accessors shall be `SpecialName` — how Roslyn (and reflection's
        // accessor filtering) distinguish `add_X`/`remove_X` accessor pairs from ordinary methods
        // (load-bearing for a C# consumer *implementing* an interface event: without it csc treats
        // the accessors as ordinary unimplemented abstract members).
        self.mark_method_special_name(add);
        self.mark_method_special_name(remove);
        // First event for this class -> open its EventMap run at the next Event row.
        let next_event_rid = u32::try_from(self.event.len() + 1).unwrap();
        if self.event_map.iter().any(|r| r.parent == class.rid()) {
            // The class already has an EventMap run. §II.22.12 runs are CONTIGUOUS slices of the
            // Event table delimited by the NEXT EventMap row's `event_list`, so appending another
            // Event row is only sound while this class's run is still the OPEN TAIL run — i.e.
            // the most recently pushed EventMap row is this class's. Anything else means a caller
            // interleaved classes (A, B, A again): the new Event row would land inside another
            // class's run and reflection/csc would silently attribute it to that class. Fail
            // loudly instead of writing a silently-wrong assembly.
            assert_eq!(
                self.event_map.last().map(|r| r.parent),
                Some(class.rid()),
                "add_event: non-contiguous event addition — TypeDef rid {} already has a closed \
                 EventMap run (another class's events were added after its), so event `{name}` \
                 cannot be appended to it. All of a class's events must be added contiguously.",
                class.rid(),
            );
        } else {
            self.event_map.push(EventMapRow {
                parent: class.rid(),
                event_list: next_event_rid,
            });
        }
        let name_off = self.strings.intern(name);
        self.event.push(EventRow {
            event_flags: 0,
            name: name_off,
            event_type: encode_type_def_or_ref_token(event_type),
        });
        let event_rid = u32::try_from(self.event.len()).unwrap();
        let association = encode_has_semantics(Token::new(Token::TABLE_EVENT, event_rid));
        // §II.23.1.12 MethodSemantics.Semantics: 0x8 AddOn, 0x10 RemoveOn.
        self.method_semantics.push(MethodSemanticsRow {
            semantics: 0x8,
            method: add.rid(),
            association,
        });
        self.method_semantics.push(MethodSemanticsRow {
            semantics: 0x10,
            method: remove.rid(),
            association,
        });
    }

    /// Adds one property (§II.22.34 `Property` + §II.22.28 `MethodSemantics` for its accessors)
    /// to the given `TypeDef`'s property run. The FIRST property added to a given class also
    /// creates that class's single §II.22.35 `PropertyMap` row (run-start into the `Property`
    /// table) — so all of a class's properties must be added contiguously, before any other
    /// class's, exactly like [`MetadataBuilder::add_event`]'s run-start discipline (this method
    /// fails loudly on interleaving, same as `add_event`).
    ///
    /// `class` is the declaring `TypeDef`; `name` the property name; `sig_blob` a §II.23.2.5
    /// `PropertySig` `#Blob` offset (see [`super::sig::encode_property_sig`]); `getter`/`setter`
    /// the accessor `MethodDef` tokens (already added via [`MetadataBuilder::add_method`]) — at
    /// least one must be present. Both accessors get `SpecialName` (0x0800) stamped, matching
    /// Roslyn's own `get_*`/`set_*` emission (csc's `PEPropertySymbol` reads properties back, so
    /// matching the reference compiler's accessor flags removes the only consumer-tolerance
    /// unknown — same reasoning as `add_event`'s accessor stamping).
    ///
    /// # Panics
    /// If `class` is not a `TypeDef` token, if both accessors are `None`, or on a
    /// non-contiguous per-class property run (see above).
    pub fn add_property(
        &mut self,
        class: Token,
        name: &str,
        sig_blob: u32,
        getter: Option<Token>,
        setter: Option<Token>,
    ) -> Token {
        assert_eq!(
            class.table(),
            Token::TABLE_TYPE_DEF,
            "Property owner must be a TypeDef"
        );
        assert!(
            getter.is_some() || setter.is_some(),
            "add_property: property `{name}` has no accessors — nothing for MethodSemantics to \
             associate"
        );
        for accessor in [getter, setter].into_iter().flatten() {
            self.mark_method_special_name(accessor);
        }
        // First property for this class -> open its PropertyMap run at the next Property row.
        let next_property_rid = u32::try_from(self.property.len() + 1).unwrap();
        if self.property_map.iter().any(|r| r.parent == class.rid()) {
            // §II.22.35 runs are CONTIGUOUS slices of the Property table delimited by the NEXT
            // PropertyMap row's `property_list` — appending is only sound while this class's run
            // is still the OPEN TAIL run. Anything else means a caller interleaved classes and
            // the new Property row would silently land inside another class's run. Fail loudly
            // instead of writing a silently-wrong assembly (the exact `add_event` idiom).
            assert_eq!(
                self.property_map.last().map(|r| r.parent),
                Some(class.rid()),
                "add_property: non-contiguous property addition — TypeDef rid {} already has a \
                 closed PropertyMap run (another class's properties were added after its), so \
                 property `{name}` cannot be appended to it. All of a class's properties must be \
                 added contiguously.",
                class.rid(),
            );
        } else {
            self.property_map.push(PropertyMapRow {
                parent: class.rid(),
                property_list: next_property_rid,
            });
        }
        let name_off = self.strings.intern(name);
        self.property.push(PropertyRow {
            flags: 0,
            name: name_off,
            signature: sig_blob,
        });
        let property_rid = u32::try_from(self.property.len()).unwrap();
        let property = Token::new(Token::TABLE_PROPERTY, property_rid);
        let association = encode_has_semantics(property);
        // §II.23.1.12 MethodSemantics.Semantics: 0x2 Getter, 0x1 Setter.
        if let Some(g) = getter {
            self.method_semantics.push(MethodSemanticsRow {
                semantics: 0x2,
                method: g.rid(),
                association,
            });
        }
        if let Some(s) = setter {
            self.method_semantics.push(MethodSemanticsRow {
                semantics: 0x1,
                method: s.rid(),
                association,
            });
        }
        property
    }

    /// Attach an explicit compiler-recognized nullable-reference flag to a Property row.
    pub fn mark_property_nullable(&mut self, property: Token, flag: u8) {
        assert_eq!(property.table(), Token::TABLE_PROPERTY);
        self.nullable_metadata_attribute(property, flag, false);
    }

    /// Emits the dedicated bare `[ThreadStaticAttribute]`
    /// (`System.ThreadStaticAttribute::.ctor()`) on a static field,
    /// mirroring `il_exporter`'s `.custom instance void
    /// [System.Runtime]System.ThreadStaticAttribute::.ctor() = (01 00 00 00)` (the fixed 4-byte
    /// `01 00 00 00` prolog+zero-named-args blob, §II.23.3). `field` is the owning `Field` row's
    /// token (`HasCustomAttribute` coded index, §II.24.2.6).
    ///
    /// The `ThreadStaticAttribute::.ctor()` `MemberRef` is created lazily and cached, keyed by a
    /// reserved `System.Runtime` `TypeRef` — repeated calls (one per TLS field) reuse a single
    /// `MemberRef` row, matching how `il_exporter` re-renders the identical textual ctor
    /// reference without any row-sharing concern (text has no such notion; here it matters).
    pub fn thread_static_attribute(&mut self, field: Token) -> Token {
        assert_eq!(field.table(), Token::TABLE_FIELD, "expected a Field token");
        let ctor = self.thread_static_ctor_ref();
        // Fixed prolog (0x0001) + zero named-args count (0x0000) — §II.23.3's "no arguments, no
        // named args" custom-attribute blob, matching `il_exporter`'s literal `01 00 00 00`.
        let value = self.blobs.intern(&[0x01, 0x00, 0x00, 0x00]);
        self.custom_attribute.push(CustomAttributeRow {
            parent: encode_has_custom_attribute(field),
            ctor: encode_custom_attribute_type(ctor),
            value,
        });
        let rid = u32::try_from(self.custom_attribute.len()).unwrap();
        Token::new(Token::TABLE_CUSTOM_ATTRIBUTE, rid)
    }

    /// The general `CustomAttribute` (§II.21/§II.22.10/§II.23.3) emitter — everything
    /// `thread_static_attribute` above hardcodes for one specific attribute, generalized to any
    /// [`crate::ir::class::CustomAttrDef`]: resolves the attribute's TYPE via the same
    /// `ClassRef`→`TypeRef`/`TypeDef` machinery `extends`/`implements` already use
    /// (`MetadataBuilder::class_ref_token`), finds-or-creates the matching `.ctor`
    /// `MemberRef` (a HASTHIS signature shaped by `attr.ctor_args()`'s element types), and
    /// assembles a well-formed `CustomAttrib` blob (prolog `0x0001`, one `FixedArg` per ctor arg
    /// in order, `NumNamed`, then one explicitly field- or property-targeted `NamedArg`) per
    /// §II.23.3. `parent` is
    /// the target row's own token (`HasCustomAttribute` coded index, §II.24.2.6): TypeDef,
    /// MethodDef, Field, Property, or Param (including return Sequence 0).
    ///
    /// Every arg shape [`crate::ir::class::CustomAttrArg`] can express (`Str`/`Bool`/`I32`/`I64`)
    /// encodes to a FIXED number of bytes with no length ambiguity, so this function can never
    /// produce a malformed blob by construction — see `CustomAttrDef`'s doc for the full safety
    /// argument (this is why the surface is a safe API, not `unsafe`).
    pub fn add_custom_attribute(
        &mut self,
        asm: &mut Assembly,
        parent: Token,
        attr: &crate::ir::class::CustomAttrDef,
    ) -> Token {
        let attr_type_tok = self.class_ref_token(asm, attr.attr_type());
        let arg_kind = |v: &crate::ir::class::CustomAttrArg| -> u8 {
            match v {
                crate::ir::class::CustomAttrArg::Str(_) => ATTR_ELEM_STRING,
                crate::ir::class::CustomAttrArg::Bool(_) => ATTR_ELEM_BOOLEAN,
                crate::ir::class::CustomAttrArg::U8(_) => ATTR_ELEM_U1,
                crate::ir::class::CustomAttrArg::I32(_) => ATTR_ELEM_I4,
                crate::ir::class::CustomAttrArg::I64(_) => ATTR_ELEM_I8,
            }
        };
        // ---- ctor MemberRef: HASTHIS, one param per ctor arg, VOID return ----
        let mut sig = Vec::new();
        sig.push(sig::SIG_HASTHIS);
        write_compressed_u32(&mut sig, u32::try_from(attr.ctor_args().len()).unwrap());
        sig.push(0x01); // ELEMENT_TYPE_VOID
        for a in attr.ctor_args() {
            sig.push(arg_kind(a));
        }
        let sig_off = self.blobs.intern(&sig);
        let ctor = self.member_ref(attr_type_tok, ".ctor", sig_off);

        // ---- CustomAttrib blob (§II.23.3) ----
        let mut blob = Vec::new();
        write_u16_le(&mut blob, 0x0001); // Prolog
        for a in attr.ctor_args() {
            encode_custom_attr_fixed_arg(&mut blob, a, asm);
        }
        write_u16_le(&mut blob, u16::try_from(attr.named_args().len()).unwrap());
        for named in attr.named_args() {
            let val = named.value();
            blob.push(match named.kind() {
                crate::ir::class::CustomAttrNamedArgKind::Field => ATTR_NAMED_FIELD,
                crate::ir::class::CustomAttrNamedArgKind::Property => ATTR_NAMED_PROPERTY,
            });
            blob.push(arg_kind(val));
            let name_str = asm[named.name()].to_string();
            write_compressed_u32(&mut blob, u32::try_from(name_str.len()).unwrap());
            blob.extend_from_slice(name_str.as_bytes());
            encode_custom_attr_fixed_arg(&mut blob, val, asm);
        }
        let value = self.blobs.intern(&blob);
        self.custom_attribute.push(CustomAttributeRow {
            parent: encode_has_custom_attribute(parent),
            ctor: encode_custom_attribute_type(ctor),
            value,
        });
        let rid = u32::try_from(self.custom_attribute.len()).unwrap();
        Token::new(Token::TABLE_CUSTOM_ATTRIBUTE, rid)
    }

    /// Attach structured attributes to a MethodDef and its return/argument Param rows. This is
    /// called immediately after `add_method`, before another method can begin its ParamList run.
    pub fn add_method_custom_attributes(
        &mut self,
        asm: &mut Assembly,
        method: Token,
        method_attributes: &[crate::ir::class::CustomAttrDef],
        return_attributes: &[crate::ir::class::CustomAttrDef],
        parameter_attributes: &[Vec<crate::ir::class::CustomAttrDef>],
    ) {
        assert_eq!(method.table(), Token::TABLE_METHOD_DEF);
        for attribute in method_attributes {
            self.add_custom_attribute(asm, method, attribute);
        }

        let method_row = &self.method_def[usize::try_from(method.rid() - 1).unwrap()];
        let param_start = usize::try_from(method_row.param_list - 1).unwrap();
        assert!(param_start <= self.param.len());

        let token_for_sequence = |this: &mut Self, sequence: u16| {
            let existing = this.param[param_start..]
                .iter()
                .position(|row| row.sequence == sequence)
                .map(|offset| param_start + offset);
            let index = if let Some(index) = existing {
                index
            } else {
                assert_eq!(
                    sequence, 0,
                    "argument Param rows must already exist when attributes are attached"
                );
                this.param.push(ParamRow {
                    flags: 0,
                    sequence: 0,
                    name: 0,
                });
                this.param.len() - 1
            };
            Token::new(Token::TABLE_PARAM, u32::try_from(index + 1).unwrap())
        };

        if !return_attributes.is_empty() {
            let return_param = token_for_sequence(self, 0);
            for attribute in return_attributes {
                self.add_custom_attribute(asm, return_param, attribute);
            }
        }
        for (index, attributes) in parameter_attributes.iter().enumerate() {
            if attributes.is_empty() {
                continue;
            }
            let sequence = u16::try_from(index + 1).expect("method parameter index exceeds u16");
            let parameter = token_for_sequence(self, sequence);
            for attribute in attributes {
                self.add_custom_attribute(asm, parameter, attribute);
            }
        }
    }

    /// Finds-or-creates the `MemberRef` to `System.ThreadStaticAttribute::.ctor()`, resolving
    /// through a `System.Runtime`-scoped `TypeRef` (no `AssemblyRef` row is created here — the
    /// caller is expected to have already registered `System.Runtime` via
    /// [`MetadataBuilder::assembly_ref`] for any assembly that uses TLS fields; if not yet
    /// present this creates a name-only-scoped `TypeRef`, matching how `type_ref`'s
    /// `resolution_scope` is simply whatever token the caller supplies).
    fn thread_static_ctor_ref(&mut self) -> Token {
        let scope = self.system_runtime_assembly_ref();
        let type_ref = self.type_ref(Some(scope), "System", "ThreadStaticAttribute");
        let sig = {
            let mut out = Vec::new();
            out.push(sig::SIG_HASTHIS);
            write_compressed_u32(&mut out, 0);
            out.push(0x01); // ELEMENT_TYPE_VOID
            self.blobs.intern(&out)
        };
        self.member_ref(type_ref, ".ctor", sig)
    }

    /// Finds-or-creates the `System.Runtime` `AssemblyRef` row used to resolve
    /// `ThreadStaticAttribute` (a BCL type). Cached separately from the generic
    /// `assembly_ref`/`type_ref` interning caches since it has a fixed identity.
    fn system_runtime_assembly_ref(&mut self) -> Token {
        const NAME: &str = "System.Runtime";
        for (i, row) in self.assembly_ref.iter().enumerate() {
            if self.strings_eq(row.name, NAME) {
                return Token::new(Token::TABLE_ASSEMBLY_REF, u32::try_from(i + 1).unwrap());
            }
        }
        // Uses the tuple form since `AssemblyRefRow`'s columns are raw `u16`s (§II.22.5), not the
        // `"8:0:0:0"` string the textual exporter interpolates into IL.
        //
        // Gated on `self.is_lib` exactly like `il_exporter`'s `if self.is_lib { … }` (mod.rs:70):
        // an executable gets a name-only (`0.0.0.0`) reference — mirrors ilasm's own inferred-extern
        // default for an `.exe`'s implicit `[[assembly]Type` uses — while a `.dll` gets the real BCL
        // version+token so a C# compiler can bind it directly (CS0012 otherwise). See
        // `MetadataBuilder::is_lib`'s doc for the concrete `FileLoadException` this fixes.
        let target = if self.is_lib {
            AssemblyRefTarget::Bcl {
                version: self.runtime.assembly_ver_tuple(),
                token: ECMA_PUBLIC_KEY_TOKEN,
            }
        } else {
            AssemblyRefTarget::NameOnly
        };
        self.assembly_ref(NAME, target)
    }

    fn strings_eq(&self, off: u32, s: &str) -> bool {
        let bytes = self.strings.as_bytes();
        let start = off as usize;
        let end = bytes[start..].iter().position(|&b| b == 0).unwrap() + start;
        &bytes[start..end] == s.as_bytes()
    }

    /// Records the `MethodDef` token `serialize()` must stamp into the CLI header's
    /// `EntryPointToken` (§II.25.3.3), mirroring `il_exporter`'s "method literally named
    /// `entrypoint`" convention (`ENTRYPOINT` in `asm.rs`).
    pub fn set_entry_point(&mut self, method: Token) {
        self.entry_point = Some(method);
    }

    /// Returns the previously-recorded `EntryPointToken`, if any.
    #[must_use]
    pub fn entry_point(&self) -> Option<Token> {
        self.entry_point
    }

    /// Records the RVA of a `FieldRVA` blob once `pe.rs`'s layout pass has placed it in
    /// `.sdata`, and materializes the deferred `FieldRVA` row (§II.22.18) for `field` (added via
    /// [`MetadataBuilder::add_static_field`]'s `rva_data`).
    pub fn set_field_rva(&mut self, field: Token, rva: u32) {
        let entry = self
            .pending_field_rva
            .iter()
            .find(|p| p.field_token == field)
            .unwrap_or_else(|| {
                panic!(
                    "no pending FieldRVA for {field:?} — was add_static_field called with rva_data?"
                )
            });
        self.field_rva[entry.row - 1].rva = rva;
    }

    /// Populate → size → serialize (see module docs). Produces the complete BSJB metadata root
    /// (§II.24.2.1): magic `BSJB`, version info, a single `#~` stream (§II.24.2.6 — row counts,
    /// `Valid`/`Sorted` 64-bit table-presence bitmasks, `HeapSizes` byte controlling 2- vs 4-byte
    /// heap-index widths, then every populated table's rows in table-id order) followed by
    /// `#Strings`/`#US`/`#GUID`/`#Blob` stream bodies. Coded-index widths (`TypeDefOrRef`,
    /// `HasConstant`, `HasCustomAttribute`, `MethodDefOrRef`, `MemberRefParent`,
    /// `ResolutionScope`, …) are computed here from final row counts per §II.24.2.6's "the
    /// large-index-bit" rule. Every stream is padded to a 4-byte boundary (§II.24.2).
    #[must_use]
    pub fn serialize(&self) -> Vec<u8> {
        let sizes = self.row_counts();
        let widths = Widths::compute(
            &sizes,
            &self.strings,
            &self.blobs,
            &self.guids,
            &self.user_strings,
        );

        let tables_bytes = self.serialize_tables(&sizes, &widths);
        let strings_bytes = pad4(self.strings.as_bytes());
        let us_bytes = pad4(self.user_strings.as_bytes());
        let guid_bytes = pad4(self.guids.as_bytes());
        let blob_bytes = pad4(self.blobs.as_bytes());

        let tables_stream = pad4(&tables_bytes);

        // §II.24.2.1: metadata root header.
        let mut out = Vec::new();
        out.extend_from_slice(b"BSJB"); // magic
        out.extend_from_slice(&1u16.to_le_bytes()); // MajorVersion
        out.extend_from_slice(&1u16.to_le_bytes()); // MinorVersion
        out.extend_from_slice(&0u32.to_le_bytes()); // Reserved

        const VERSION: &str = "v4.0.30319";
        let mut version_bytes = VERSION.as_bytes().to_vec();
        version_bytes.push(0);
        while version_bytes.len() % 4 != 0 {
            version_bytes.push(0);
        }
        out.extend_from_slice(&(version_bytes.len() as u32).to_le_bytes()); // Length
        out.extend_from_slice(&version_bytes);

        out.extend_from_slice(&0u16.to_le_bytes()); // Flags
        // Streams: #~, #Strings, #US, #GUID, #Blob — matches ilasm's conventional ordering.
        let streams: [(&str, &[u8]); 5] = [
            ("#~", &tables_stream),
            ("#Strings", &strings_bytes),
            ("#US", &us_bytes),
            ("#GUID", &guid_bytes),
            ("#Blob", &blob_bytes),
        ];
        out.extend_from_slice(&(streams.len() as u16).to_le_bytes()); // NumberOfStreams

        // Stream headers (§II.24.2.2): Offset (from metadata root start), Size, Name (4-aligned,
        // NUL-terminated). Offsets are computed after every header's own size is known, so this
        // is a two-pass layout: first compute header bytes, then patch in offsets.
        let mut headers = Vec::new();
        let mut header_names = Vec::new();
        for (name, _) in &streams {
            let mut name_bytes = name.as_bytes().to_vec();
            name_bytes.push(0);
            while name_bytes.len() % 4 != 0 {
                name_bytes.push(0);
            }
            header_names.push(name_bytes);
        }
        let header_total_len: usize = header_names.iter().map(|n| 8 + n.len()).sum();
        let mut running = out.len() + header_total_len;
        for ((_, bytes), name_bytes) in streams.iter().zip(&header_names) {
            headers.extend_from_slice(&(running as u32).to_le_bytes());
            headers.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            headers.extend_from_slice(name_bytes);
            running += bytes.len();
        }
        out.extend_from_slice(&headers);
        for (_, bytes) in &streams {
            out.extend_from_slice(bytes);
        }
        out
    }

    /// The number of `MethodDef` rows added so far — the count [`super::pdb::PdbBuilder::new`]
    /// pre-sizes its per-method slot vector with (one `MethodDebugInformation` row must exist for
    /// EVERY `MethodDef` row, per the Portable PDB spec's "PDB Stream" section, in the same RID
    /// order the type-system table uses — see that constructor's doc).
    #[must_use]
    pub fn method_def_row_count(&self) -> usize {
        self.method_def.len()
    }

    /// `(table id, row count)` pairs for every non-empty type-system table this builder has
    /// populated, in table-id order — exactly the shape [`super::pdb::TypeSystemRowCounts::rows`]
    /// needs for the standalone PDB's `#Pdb` stream (`ReferencedTypeSystemTables` mask + per-table
    /// row counts, per the Portable PDB spec). Reuses the same `(Token::TABLE_*, count)` pairing
    /// `Self::serialize_tables` computes from `Self::row_counts`,
    /// so this can never drift out of sync with what `serialize()` actually wrote to the `.dll`'s
    /// own `#~` stream.
    #[must_use]
    pub fn type_system_row_counts(&self) -> Vec<(u32, u32)> {
        let sizes = self.row_counts();
        [
            (Token::TABLE_MODULE, sizes.module),
            (Token::TABLE_TYPE_REF, sizes.type_ref),
            (Token::TABLE_TYPE_DEF, sizes.type_def),
            (Token::TABLE_FIELD, sizes.field),
            (Token::TABLE_METHOD_DEF, sizes.method_def),
            (Token::TABLE_PARAM, sizes.param),
            (Token::TABLE_INTERFACE_IMPL, sizes.interface_impl),
            (Token::TABLE_MEMBER_REF, sizes.member_ref),
            (Token::TABLE_CONSTANT, sizes.constant),
            (Token::TABLE_CUSTOM_ATTRIBUTE, sizes.custom_attribute),
            (Token::TABLE_CLASS_LAYOUT, sizes.class_layout),
            (Token::TABLE_FIELD_LAYOUT, sizes.field_layout),
            (Token::TABLE_STAND_ALONE_SIG, sizes.standalone_sig),
            (Token::TABLE_EVENT_MAP, sizes.event_map),
            (Token::TABLE_EVENT, sizes.event),
            (Token::TABLE_PROPERTY_MAP, sizes.property_map),
            (Token::TABLE_PROPERTY, sizes.property),
            (Token::TABLE_METHOD_SEMANTICS, sizes.method_semantics),
            (Token::TABLE_METHOD_IMPL, sizes.method_impl),
            (Token::TABLE_MODULE_REF, sizes.module_ref),
            (Token::TABLE_TYPE_SPEC, sizes.type_spec),
            (Token::TABLE_IMPL_MAP, sizes.impl_map),
            (Token::TABLE_FIELD_RVA, sizes.field_rva),
            (Token::TABLE_ASSEMBLY, sizes.assembly),
            (Token::TABLE_ASSEMBLY_REF, sizes.assembly_ref),
            (Token::TABLE_GENERIC_PARAM, sizes.generic_param),
            (Token::TABLE_METHOD_SPEC, sizes.method_spec),
        ]
        .into_iter()
        .filter(|&(_, count)| count > 0)
        .map(|(table, count)| (table, u32::try_from(count).expect("row count exceeds u32")))
        .collect()
    }

    fn row_counts(&self) -> RowCounts {
        RowCounts {
            module: self.module.len(),
            type_ref: self.type_ref.len(),
            type_def: self.type_def.len(),
            field: self.field.len(),
            method_def: self.method_def.len(),
            param: self.param.len(),
            interface_impl: self.interface_impl.len(),
            member_ref: self.member_ref.len(),
            constant: self.constant.len(),
            custom_attribute: self.custom_attribute.len(),
            class_layout: self.class_layout.len(),
            field_layout: self.field_layout.len(),
            standalone_sig: self.standalone_sig.len(),
            module_ref: self.module_ref.len(),
            type_spec: self.type_spec.len(),
            impl_map: self.impl_map.len(),
            field_rva: self.field_rva.len(),
            assembly: self.assembly.len(),
            assembly_ref: self.assembly_ref.len(),
            method_spec: self.method_spec.len(),
            method_impl: self.method_impl.len(),
            event_map: self.event_map.len(),
            event: self.event.len(),
            property_map: self.property_map.len(),
            property: self.property.len(),
            method_semantics: self.method_semantics.len(),
            generic_param: self.generic_param.len(),
        }
    }

    fn serialize_tables(&self, sizes: &RowCounts, widths: &Widths) -> Vec<u8> {
        // Every table this backend can ever emit, in ascending table-id order (§II.24.2.6
        // requires tables be written in table-id order regardless of population order).
        let table_rowcounts: [(u32, usize); 27] = [
            (Token::TABLE_MODULE, sizes.module),
            (Token::TABLE_TYPE_REF, sizes.type_ref),
            (Token::TABLE_TYPE_DEF, sizes.type_def),
            (Token::TABLE_FIELD, sizes.field),
            (Token::TABLE_METHOD_DEF, sizes.method_def),
            (Token::TABLE_PARAM, sizes.param),
            (Token::TABLE_INTERFACE_IMPL, sizes.interface_impl),
            (Token::TABLE_MEMBER_REF, sizes.member_ref),
            (Token::TABLE_CONSTANT, sizes.constant),
            (Token::TABLE_CUSTOM_ATTRIBUTE, sizes.custom_attribute),
            (Token::TABLE_CLASS_LAYOUT, sizes.class_layout),
            (Token::TABLE_FIELD_LAYOUT, sizes.field_layout),
            (Token::TABLE_STAND_ALONE_SIG, sizes.standalone_sig),
            (Token::TABLE_EVENT_MAP, sizes.event_map),
            (Token::TABLE_EVENT, sizes.event),
            (Token::TABLE_PROPERTY_MAP, sizes.property_map),
            (Token::TABLE_PROPERTY, sizes.property),
            (Token::TABLE_METHOD_SEMANTICS, sizes.method_semantics),
            (Token::TABLE_METHOD_IMPL, sizes.method_impl),
            (Token::TABLE_MODULE_REF, sizes.module_ref),
            (Token::TABLE_TYPE_SPEC, sizes.type_spec),
            (Token::TABLE_IMPL_MAP, sizes.impl_map),
            (Token::TABLE_FIELD_RVA, sizes.field_rva),
            (Token::TABLE_ASSEMBLY, sizes.assembly),
            (Token::TABLE_ASSEMBLY_REF, sizes.assembly_ref),
            (Token::TABLE_GENERIC_PARAM, sizes.generic_param),
            (Token::TABLE_METHOD_SPEC, sizes.method_spec),
        ];

        let mut valid: u64 = 0;
        let mut sorted: u64 = 0;
        for &(id, count) in &table_rowcounts {
            if count > 0 {
                valid |= 1u64 << id;
                // Only a table that actually has rows (and IS one of the tables this backend
                // always emits pre-sorted, see the write_* methods below) sets its Sorted bit —
                // an absent table contributes nothing to either bitmask.
                if SORTED_TABLES.contains(&id) {
                    sorted |= 1u64 << id;
                }
            }
        }

        let mut out = Vec::new();
        out.extend_from_slice(&0u32.to_le_bytes()); // Reserved
        out.push(2); // MajorVersion
        out.push(0); // MinorVersion
        out.push(widths.heap_sizes); // HeapSizes
        out.push(1); // Reserved (always 1)
        out.extend_from_slice(&valid.to_le_bytes());
        out.extend_from_slice(&sorted.to_le_bytes());
        for &(_, count) in &table_rowcounts {
            if count > 0 {
                out.extend_from_slice(&(count as u32).to_le_bytes());
            }
        }

        // Row bytes, one table at a time, table-id order. Sorted tables are emitted pre-sorted
        // (population order doesn't matter to callers — see module docs).
        self.write_module_rows(&mut out, widths);
        self.write_type_ref_rows(&mut out, widths);
        self.write_type_def_rows(&mut out, widths);
        self.write_field_rows(&mut out, widths);
        self.write_method_def_rows(&mut out, widths);
        self.write_param_rows(&mut out, widths);
        self.write_interface_impl_rows(&mut out, widths);
        self.write_member_ref_rows(&mut out, widths);
        self.write_constant_rows(&mut out, widths);
        self.write_custom_attribute_rows(&mut out, widths);
        self.write_class_layout_rows(&mut out, widths);
        self.write_field_layout_rows(&mut out, widths);
        self.write_standalone_sig_rows(&mut out, widths);
        self.write_event_map_rows(&mut out, widths);
        self.write_event_rows(&mut out, widths);
        self.write_property_map_rows(&mut out, widths);
        self.write_property_rows(&mut out, widths);
        self.write_method_semantics_rows(&mut out, widths);
        self.write_method_impl_rows(&mut out, widths);
        self.write_module_ref_rows(&mut out, widths);
        self.write_type_spec_rows(&mut out, widths);
        self.write_impl_map_rows(&mut out, widths);
        self.write_field_rva_rows(&mut out, widths);
        self.write_assembly_rows(&mut out, widths);
        self.write_assembly_ref_rows(&mut out, widths);
        self.write_generic_param_rows(&mut out, widths);
        self.write_method_spec_rows(&mut out, widths);
        out
    }

    fn write_module_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.module {
            out.extend_from_slice(&0u16.to_le_bytes()); // Generation
            write_heap_idx(out, row.name, w.str_wide);
            write_heap_idx(out, row.mvid, w.guid_wide); // Mvid
            write_heap_idx(out, 0, w.guid_wide); // EncId
            write_heap_idx(out, 0, w.guid_wide); // EncBaseId
        }
    }

    fn write_type_ref_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.type_ref {
            write_coded_idx(out, row.resolution_scope, w.resolution_scope_wide);
            write_heap_idx(out, row.name, w.str_wide);
            write_heap_idx(out, row.namespace, w.str_wide);
        }
    }

    fn write_type_def_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.type_def {
            out.extend_from_slice(&row.flags.to_le_bytes());
            write_heap_idx(out, row.name, w.str_wide);
            write_heap_idx(out, row.namespace, w.str_wide);
            write_coded_idx(out, row.extends, w.type_def_or_ref_wide);
            write_simple_idx(out, row.field_list, w.field_wide);
            write_simple_idx(out, row.method_list, w.method_def_wide);
        }
    }

    fn write_field_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.field {
            out.extend_from_slice(&row.flags.to_le_bytes());
            write_heap_idx(out, row.name, w.str_wide);
            write_heap_idx(out, row.signature, w.blob_wide);
        }
    }

    fn write_method_def_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.method_def {
            out.extend_from_slice(&row.rva.to_le_bytes());
            out.extend_from_slice(&row.impl_flags.to_le_bytes());
            out.extend_from_slice(&row.flags.to_le_bytes());
            write_heap_idx(out, row.name, w.str_wide);
            write_heap_idx(out, row.signature, w.blob_wide);
            write_simple_idx(out, row.param_list, w.param_wide);
        }
    }

    fn write_param_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.param {
            out.extend_from_slice(&row.flags.to_le_bytes());
            out.extend_from_slice(&row.sequence.to_le_bytes());
            write_heap_idx(out, row.name, w.str_wide);
        }
    }

    fn write_interface_impl_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&InterfaceImplRow> = self.interface_impl.iter().collect();
        // Sorted by Class (§II.22.23) — a simple TypeDef row index, so a plain numeric sort is
        // the spec's total order.
        rows.sort_by_key(|r| r.class);
        debug_assert!(rows.windows(2).all(|w| w[0].class <= w[1].class));
        for row in rows {
            write_simple_idx(out, row.class, w.type_def_wide);
            write_coded_idx(out, row.interface, w.type_def_or_ref_wide);
        }
    }

    fn write_member_ref_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.member_ref {
            write_coded_idx(out, row.class, w.member_ref_parent_wide);
            write_heap_idx(out, row.name, w.str_wide);
            write_heap_idx(out, row.signature, w.blob_wide);
        }
    }

    fn write_constant_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&ConstantRow> = self.constant.iter().collect();
        rows.sort_by_key(|row| row.parent);
        for row in rows {
            out.push(row.type_code);
            out.push(0); // Padding
            write_coded_idx(out, row.parent, w.has_constant_wide);
            write_heap_idx(out, row.value, w.blob_wide);
        }
    }

    fn write_event_map_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        // §II.22.12: NOT a sorted table — insertion order (which is class-def iteration order, so
        // Parent is de-facto ascending anyway) is fine.
        for row in &self.event_map {
            write_simple_idx(out, row.parent, w.type_def_wide);
            write_simple_idx(out, row.event_list, w.event_wide);
        }
    }

    fn write_event_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.event {
            out.extend_from_slice(&row.event_flags.to_le_bytes());
            write_heap_idx(out, row.name, w.str_wide);
            write_coded_idx(out, row.event_type, w.type_def_or_ref_wide);
        }
    }

    fn write_property_map_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        // §II.22.35: NOT a sorted table — insertion order (class-def iteration order, so Parent
        // is de-facto ascending anyway), exactly like `write_event_map_rows`.
        for row in &self.property_map {
            write_simple_idx(out, row.parent, w.type_def_wide);
            write_simple_idx(out, row.property_list, w.property_wide);
        }
    }

    fn write_property_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.property {
            out.extend_from_slice(&row.flags.to_le_bytes());
            write_heap_idx(out, row.name, w.str_wide);
            write_heap_idx(out, row.signature, w.blob_wide);
        }
    }

    fn write_method_semantics_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&MethodSemanticsRow> = self.method_semantics.iter().collect();
        // Sorted by Association (§II.22.28) — the coded HasSemantics index. `sort_by_key` is
        // stable, so the add-then-remove insertion order is preserved within one event.
        rows.sort_by_key(|r| r.association);
        for row in rows {
            out.extend_from_slice(&row.semantics.to_le_bytes());
            write_simple_idx(out, row.method, w.method_def_wide);
            write_coded_idx(out, row.association, w.has_semantics_wide);
        }
    }

    fn write_method_impl_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&MethodImplRow> = self.method_impl.iter().collect();
        // Sorted by Class (§II.22.27) — a simple TypeDef row index.
        rows.sort_by_key(|r| r.class);
        debug_assert!(rows.windows(2).all(|w| w[0].class <= w[1].class));
        for row in rows {
            write_simple_idx(out, row.class, w.type_def_wide);
            write_coded_idx(out, row.method_body, w.method_def_or_ref_wide);
            write_coded_idx(out, row.method_declaration, w.method_def_or_ref_wide);
        }
    }

    fn write_custom_attribute_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&CustomAttributeRow> = self.custom_attribute.iter().collect();
        // Sorted by Parent (§II.22.10) — the coded HasCustomAttribute index.
        rows.sort_by_key(|r| r.parent);
        for row in rows {
            write_coded_idx(out, row.parent, w.has_custom_attribute_wide);
            write_coded_idx(out, row.ctor, w.custom_attribute_type_wide);
            write_heap_idx(out, row.value, w.blob_wide);
        }
    }

    fn write_class_layout_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&ClassLayoutRow> = self.class_layout.iter().collect();
        // Sorted by Parent (§II.22.8).
        rows.sort_by_key(|r| r.parent);
        debug_assert!(rows.windows(2).all(|w| w[0].parent <= w[1].parent));
        for row in rows {
            out.extend_from_slice(&row.packing_size.to_le_bytes());
            out.extend_from_slice(&row.class_size.to_le_bytes());
            write_simple_idx(out, row.parent, w.type_def_wide);
        }
    }

    fn write_field_layout_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&FieldLayoutRow> = self.field_layout.iter().collect();
        // Sorted by Field (§II.22.16).
        rows.sort_by_key(|r| r.field);
        debug_assert!(rows.windows(2).all(|w| w[0].field <= w[1].field));
        for row in rows {
            out.extend_from_slice(&row.offset.to_le_bytes());
            write_simple_idx(out, row.field, w.field_wide);
        }
    }

    fn write_standalone_sig_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.standalone_sig {
            write_heap_idx(out, row.signature, w.blob_wide);
        }
    }

    fn write_module_ref_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.module_ref {
            write_heap_idx(out, row.name, w.str_wide);
        }
    }

    fn write_type_spec_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.type_spec {
            write_heap_idx(out, row.signature, w.blob_wide);
        }
    }

    fn write_impl_map_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&ImplMapRow> = self.impl_map.iter().collect();
        // Sorted by MemberForwarded (§II.22.22) — the coded index.
        rows.sort_by_key(|r| r.member_forwarded);
        for row in rows {
            out.extend_from_slice(&row.mapping_flags.to_le_bytes());
            write_coded_idx(out, row.member_forwarded, w.member_forwarded_wide);
            write_heap_idx(out, row.import_name, w.str_wide);
            write_simple_idx(out, row.import_scope, w.module_ref_wide);
        }
    }

    fn write_field_rva_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&FieldRvaRow> = self.field_rva.iter().collect();
        // Sorted by Field (§II.22.18).
        rows.sort_by_key(|r| r.field);
        debug_assert!(rows.windows(2).all(|w| w[0].field <= w[1].field));
        for row in rows {
            out.extend_from_slice(&row.rva.to_le_bytes());
            write_simple_idx(out, row.field, w.field_wide);
        }
    }

    fn write_assembly_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.assembly {
            out.extend_from_slice(&row.hash_alg_id.to_le_bytes());
            out.extend_from_slice(&row.major.to_le_bytes());
            out.extend_from_slice(&row.minor.to_le_bytes());
            out.extend_from_slice(&row.build.to_le_bytes());
            out.extend_from_slice(&row.revision.to_le_bytes());
            out.extend_from_slice(&row.flags.to_le_bytes());
            write_heap_idx(out, row.public_key, w.blob_wide);
            write_heap_idx(out, row.name, w.str_wide);
            write_heap_idx(out, row.culture, w.str_wide);
        }
    }

    fn write_assembly_ref_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.assembly_ref {
            out.extend_from_slice(&row.major.to_le_bytes());
            out.extend_from_slice(&row.minor.to_le_bytes());
            out.extend_from_slice(&row.build.to_le_bytes());
            out.extend_from_slice(&row.revision.to_le_bytes());
            out.extend_from_slice(&row.flags.to_le_bytes());
            write_heap_idx(out, row.public_key_or_token, w.blob_wide);
            write_heap_idx(out, row.name, w.str_wide);
            write_heap_idx(out, row.culture, w.str_wide);
            write_heap_idx(out, row.hash_value, w.blob_wide);
        }
    }

    fn write_generic_param_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        let mut rows: Vec<&GenericParamRow> = self.generic_param.iter().collect();
        // Sorted by Owner (the coded TypeOrMethodDef index) then Number (§II.24.2.6). Pass 1's
        // natural emission order (ascending TypeDef rid, ascending number) is already sorted —
        // the sort here is belt-and-braces, the debug_assert the tripwire.
        rows.sort_by_key(|r| (r.owner, r.number));
        debug_assert!(
            rows.windows(2)
                .all(|w| (w[0].owner, w[0].number) <= (w[1].owner, w[1].number))
        );
        for row in rows {
            out.extend_from_slice(&row.number.to_le_bytes());
            out.extend_from_slice(&row.flags.to_le_bytes());
            write_coded_idx(out, row.owner, w.type_or_method_def_wide);
            write_heap_idx(out, row.name, w.str_wide);
        }
    }

    fn write_method_spec_rows(&self, out: &mut Vec<u8>, w: &Widths) {
        for row in &self.method_spec {
            write_coded_idx(out, row.method, w.method_def_or_ref_wide);
            write_heap_idx(out, row.instantiation, w.blob_wide);
        }
    }

    /// Sets (or replaces) the `Module` table's single row (§II.22.30). Not part of the pinned
    /// contract surface, but needed internally: `serialize()` requires a `Module` row to exist
    /// for a well-formed image, and the MVID must be derivable deterministically from content
    /// (see [`MetadataBuilder::finish_module`]).
    fn set_module(&mut self, name: &str, mvid: [u8; 16]) {
        let name_off = self.strings.intern(name);
        let mvid_idx = self.guids.push(mvid);
        if let Some(row) = self.module.first_mut() {
            row.name = name_off;
            row.mvid = mvid_idx;
        } else {
            self.module.push(ModuleRow {
                name: name_off,
                mvid: mvid_idx,
            });
        }
    }

    /// Finalizes the `Module` table row (§II.22.30) with a deterministic MVID: a 16-byte value
    /// derived by hashing `assembly_name` with FNV-1a (no randomness — required by
    /// `docs/PE_EMISSION_PLAN.md`'s determinism constraint). Must be called once, before
    /// `serialize()`, by whatever driver assembles the full pipeline (not exercised by the
    /// `add_*` methods above, since the module name is a whole-assembly property decided once).
    pub fn finish_module(&mut self, assembly_name: &str) {
        let mvid = deterministic_mvid(assembly_name);
        self.set_module(assembly_name, mvid);
    }

    /// Adds the single `Assembly` table row (§II.22.2) identifying this assembly itself (as
    /// opposed to `AssemblyRef` rows, which reference OTHER assemblies). Optional: only a
    /// library needs to be identifiable by name (see `il_exporter`'s `is_lib` split); an
    /// executable may skip this and leave the `Assembly` table empty, exactly like
    /// `il_exporter`'s `.assembly _{}` placeholder for executables carries no externally visible
    /// version identity.
    pub fn set_assembly(&mut self, name: &str, version: (u16, u16, u16, u16)) -> Token {
        let name_off = self.strings.intern(name);
        self.assembly.push(AssemblyRow {
            // §II.23.1.1: 0x8004 = SHA1.
            hash_alg_id: 0x8004,
            major: version.0,
            minor: version.1,
            build: version.2,
            revision: version.3,
            flags: 0,
            public_key: 0,
            name: name_off,
            culture: 0,
        });
        Token::new(Token::TABLE_ASSEMBLY, 1)
    }
}

/// A 16-byte MVID deterministically derived from `name` via 64-bit FNV-1a, expanded to fill all
/// 16 bytes (two independent FNV passes with different seeds, matching how a hash-derived UUID
/// commonly avoids an all-zero half). No timestamps, no OS randomness — see
/// `docs/PE_EMISSION_PLAN.md`'s determinism constraint.
fn deterministic_mvid(name: &str) -> [u8; 16] {
    fn fnv1a(seed: u64, data: &[u8]) -> u64 {
        let mut hash = seed;
        for &b in data {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
    let lo = fnv1a(0xcbf2_9ce4_8422_2325, name.as_bytes());
    let hi = fnv1a(0x9e37_79b9_7f4a_7c15, name.as_bytes());
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&lo.to_le_bytes());
    out[8..].copy_from_slice(&hi.to_le_bytes());
    out
}

fn pad4(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Coded indices (§II.24.2.6). Each coded index packs a small "tag" (selecting which table the
// row belongs to) into the low bits and the 1-based row index in the remaining high bits:
// `(rid << tag_bits) | tag`. The number of tag bits and the tag-to-table mapping are fixed by
// the spec per coded-index kind (Table II.24.2.6's "TypeDefOrRef", "HasCustomAttribute", …).
// ---------------------------------------------------------------------------------------------

/// `TypeDefOrRef` (2 tag bits): 0 = TypeDef, 1 = TypeRef, 2 = TypeSpec.
fn encode_type_def_or_ref_token(token: Token) -> u32 {
    let tag = match token.table() {
        Token::TABLE_TYPE_DEF => 0,
        Token::TABLE_TYPE_REF => 1,
        Token::TABLE_TYPE_SPEC => 2,
        other => panic!("{other:#x} is not a TypeDefOrRef member"),
    };
    (token.rid() << 2) | tag
}

/// `ResolutionScope` (2 tag bits): 0 = Module, 1 = ModuleRef, 2 = AssemblyRef, 3 = TypeRef.
/// `None` (a self/nested-module reference) encodes as the NIL coded index `0`.
fn encode_resolution_scope(token: Option<Token>) -> u32 {
    let Some(token) = token else { return 0 };
    let tag = match token.table() {
        Token::TABLE_MODULE => 0,
        Token::TABLE_MODULE_REF => 1,
        Token::TABLE_ASSEMBLY_REF => 2,
        Token::TABLE_TYPE_REF => 3,
        other => panic!("{other:#x} is not a ResolutionScope member"),
    };
    (token.rid() << 2) | tag
}

/// `MemberRefParent` (3 tag bits): 0 = TypeDef, 1 = TypeRef, 2 = ModuleRef, 3 = MethodDef,
/// 4 = TypeSpec.
fn encode_member_ref_parent(token: Token) -> u32 {
    let tag = match token.table() {
        Token::TABLE_TYPE_DEF => 0,
        Token::TABLE_TYPE_REF => 1,
        Token::TABLE_MODULE_REF => 2,
        Token::TABLE_METHOD_DEF => 3,
        Token::TABLE_TYPE_SPEC => 4,
        other => panic!("{other:#x} is not a MemberRefParent member"),
    };
    (token.rid() << 3) | tag
}

/// `MethodDefOrRef` (1 tag bit): 0 = MethodDef, 1 = MemberRef.
fn encode_method_def_or_ref(token: Token) -> u32 {
    let tag = match token.table() {
        Token::TABLE_METHOD_DEF => 0,
        Token::TABLE_MEMBER_REF => 1,
        other => panic!("{other:#x} is not a MethodDefOrRef member"),
    };
    (token.rid() << 1) | tag
}

/// `HasCustomAttribute` (5 tag bits, §II.24.2.6's largest coded index — 22 target tables). This
/// The backend attaches attributes to fields, types, methods, parameters, and properties; the
/// encoder accepts each emitted parent kind using the spec's canonical tag ordering.
fn encode_has_custom_attribute(token: Token) -> u32 {
    // §II.24.2.6's canonical `HasCustomAttribute` tag order: MethodDef=0, Field=1, TypeRef=2,
    // TypeDef=3, Param=4, InterfaceImpl=5, MemberRef=6, Module=7, Permission=8, Property=9,
    // Event=10, StandAloneSig=11, ModuleRef=12, TypeSpec=13, Assembly=14, AssemblyRef=15, …
    // PRIOR BUG (found wiring the general `CustomAttribute` emitter — `add_custom_attribute` —
    // to a `TypeDef` parent for the first time): this match used to assign TypeRef=3/TypeDef=4/
    // Param=5/InterfaceImpl=6/MemberRef=7/Module=8 — each off by one from the spec, shifted up to
    // (accidentally) leave a gap at tag 2. It was never caught because the ONLY parent kind ever
    // passed through here before was `Field` (tag 1, correct by construction), so `.field
    // ThreadStaticAttribute` rows happened to decode fine; a `TypeDef` parent decoded as `Param`
    // instead (`monodis --customattr` showing `Param: <field token>` for a class-level attribute
    // was the tell). `TypeSpec`(13)/`Assembly`(14)/`AssemblyRef`(15) were already correct since
    // this backend never emits a custom attribute on any of the tags in between, so the gap never
    // surfaced there either.
    let tag = match token.table() {
        Token::TABLE_METHOD_DEF => 0,
        Token::TABLE_FIELD => 1,
        Token::TABLE_TYPE_REF => 2,
        Token::TABLE_TYPE_DEF => 3,
        Token::TABLE_PARAM => 4,
        Token::TABLE_INTERFACE_IMPL => 5,
        Token::TABLE_MEMBER_REF => 6,
        Token::TABLE_MODULE => 7,
        Token::TABLE_PROPERTY => 9,
        Token::TABLE_ASSEMBLY => 14,
        Token::TABLE_ASSEMBLY_REF => 15,
        Token::TABLE_TYPE_SPEC => 13,
        other => panic!("{other:#x} is not a HasCustomAttribute member this backend emits"),
    };
    (token.rid() << 5) | tag
}

/// `CustomAttributeType` (3 tag bits): 2 = MethodDef, 3 = MemberRef (the only two the spec
/// defines as valid attribute-constructor targets).
fn encode_custom_attribute_type(token: Token) -> u32 {
    let tag = match token.table() {
        Token::TABLE_METHOD_DEF => 2,
        Token::TABLE_MEMBER_REF => 3,
        other => panic!("{other:#x} is not a CustomAttributeType member"),
    };
    (token.rid() << 3) | tag
}

/// `MemberForwarded` (1 tag bit): 0 = Field, 1 = MethodDef.
fn encode_member_forwarded(token: Token) -> u32 {
    let tag = match token.table() {
        Token::TABLE_FIELD => 0,
        Token::TABLE_METHOD_DEF => 1,
        other => panic!("{other:#x} is not a MemberForwarded member"),
    };
    (token.rid() << 1) | tag
}

/// `TypeOrMethodDef` (1 tag bit): 0 = TypeDef, 1 = MethodDef. Targets a `GenericParam.Owner`
/// (§II.24.2.6). This backend only emits type-owned generic parameters today (generic *method*
/// definitions don't exist anywhere in the IR), but the encoder accepts both members so a future
/// generic-method-def feature isn't blocked here.
fn encode_type_or_method_def(token: Token) -> u32 {
    let tag = match token.table() {
        Token::TABLE_TYPE_DEF => 0,
        Token::TABLE_METHOD_DEF => 1,
        other => panic!("{other:#x} is not a TypeOrMethodDef member"),
    };
    (token.rid() << 1) | tag
}

/// `HasSemantics` (1 tag bit): 0 = Event, 1 = Property. Targets a `MethodSemantics.Association`.
fn encode_has_semantics(token: Token) -> u32 {
    let tag = match token.table() {
        Token::TABLE_EVENT => 0,
        Token::TABLE_PROPERTY => 1,
        other => panic!("{other:#x} is not a HasSemantics member"),
    };
    (token.rid() << 1) | tag
}

// ---------------------------------------------------------------------------------------------
// Row-count-dependent widths (§II.24.2.6).
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct RowCounts {
    module: usize,
    type_ref: usize,
    type_def: usize,
    field: usize,
    method_def: usize,
    param: usize,
    interface_impl: usize,
    member_ref: usize,
    constant: usize,
    custom_attribute: usize,
    class_layout: usize,
    field_layout: usize,
    standalone_sig: usize,
    module_ref: usize,
    type_spec: usize,
    impl_map: usize,
    field_rva: usize,
    assembly: usize,
    assembly_ref: usize,
    method_spec: usize,
    method_impl: usize,
    event_map: usize,
    event: usize,
    property_map: usize,
    property: usize,
    method_semantics: usize,
    generic_param: usize,
}

/// Whether a *simple* index (one target table, no tag bits) needs 4 bytes: §II.24.2.6, "iff the
/// number of rows in the target table exceeds 2^16".
fn simple_wide(rows: usize) -> bool {
    rows > 0xFFFF
}

/// Whether a coded index with `tag_bits` needs 4 bytes: §II.24.2.6, "iff the number of rows in
/// the largest target table is equal to or greater than 2^(16 - tag_bits)".
///
/// This is a `>=`, not a `>`: the coded value packs `(rid << tag_bits) | tag`, so a table with
/// exactly `2^(16-tag_bits)` rows already produces a coded value of `(2^(16-tag_bits) << tag_bits)
/// == 0x1_0000` for its highest row — one bit too many for a `u16` — so the column must already
/// be wide AT that row count, not only past it. Ground-truthed against
/// `System.Reflection.Metadata.Ecma335.MetadataBuilder` (.NET 8): the reference writer emits a
/// wide `TypeDefOrRef` (`Extends`) column once the largest target table reaches exactly 16384
/// (`2^14`) rows.
fn coded_wide(tag_bits: u32, max_rows: usize) -> bool {
    max_rows >= (1usize << (16 - tag_bits))
}

struct Widths {
    heap_sizes: u8,
    str_wide: bool,
    guid_wide: bool,
    blob_wide: bool,
    #[allow(dead_code)] // symmetry with the other *_wide fields; #US never appears in a table row
    us_wide: bool,

    type_def_wide: bool,
    field_wide: bool,
    method_def_wide: bool,
    param_wide: bool,
    module_ref_wide: bool,
    event_wide: bool,
    property_wide: bool,

    type_def_or_ref_wide: bool,
    resolution_scope_wide: bool,
    member_ref_parent_wide: bool,
    has_constant_wide: bool,
    method_def_or_ref_wide: bool,
    has_custom_attribute_wide: bool,
    custom_attribute_type_wide: bool,
    member_forwarded_wide: bool,
    has_semantics_wide: bool,
    type_or_method_def_wide: bool,
}

impl Widths {
    fn compute(
        sizes: &RowCounts,
        strings: &StringsHeap,
        blobs: &BlobHeap,
        guids: &GuidHeap,
        user_strings: &UserStringHeap,
    ) -> Self {
        let str_wide = strings.as_bytes().len() > 0xFFFF;
        let blob_wide = blobs.as_bytes().len() > 0xFFFF;
        let guid_wide = guids.as_bytes().len() > 0xFFFF;
        let us_wide = user_strings.as_bytes().len() > 0xFFFF;

        // §II.24.2.6 `HeapSizes` byte: bit 0 = #Strings wide, bit 1 = #GUID wide, bit 2 = #Blob
        // wide.
        let mut heap_sizes = 0u8;
        if str_wide {
            heap_sizes |= 0x1;
        }
        if guid_wide {
            heap_sizes |= 0x2;
        }
        if blob_wide {
            heap_sizes |= 0x4;
        }

        let type_def_or_ref_max = sizes.type_def.max(sizes.type_ref).max(sizes.type_spec);
        let resolution_scope_max = sizes
            .module
            .max(sizes.module_ref)
            .max(sizes.assembly_ref)
            .max(sizes.type_ref);
        let member_ref_parent_max = sizes
            .type_def
            .max(sizes.type_ref)
            .max(sizes.module_ref)
            .max(sizes.method_def)
            .max(sizes.type_spec);
        let method_def_or_ref_max = sizes.method_def.max(sizes.member_ref);
        let has_constant_max = sizes.field.max(sizes.param).max(sizes.property);
        // Conservatively includes every table this backend can target with HasCustomAttribute
        // (22 possible target tables per spec; only a handful are ever nonzero here).
        let has_custom_attribute_max = sizes
            .method_def
            .max(sizes.field)
            .max(sizes.type_ref)
            .max(sizes.type_def)
            .max(sizes.param)
            .max(sizes.interface_impl)
            .max(sizes.member_ref)
            .max(sizes.module)
            .max(sizes.assembly)
            .max(sizes.assembly_ref)
            .max(sizes.type_spec)
            // `GenericParam` is `HasCustomAttribute` tag 19 (§II.24.2.6). This backend never
            // attributes one, but the coded-index WIDTH universe is computed from the row counts
            // of every table in the target list — readers do the same computation, so omitting a
            // populated target table here would make our narrow encoding disagree with their
            // width expectation once `GenericParam` outgrows the 2^(16-5) threshold.
            .max(sizes.generic_param)
            // Same rule for the remaining `HasCustomAttribute` target tables this backend can
            // populate: `Event` (tag 10), `StandAloneSig` (tag 11), `ModuleRef` (tag 12) and
            // `MethodSpec` (tag 21). None is ever attributed here either, but each counts toward
            // the width computation — `MethodSpec` in particular grows per generic-method
            // *instantiation* (the WF-9 `call_gmethod` path), so it can cross the 2^11 threshold
            // while every other table stays small. `Property` (tag 9) counts too now that
            // `add_property` populates it. The tables NOT listed (DeclSecurity, File,
            // ExportedType, ManifestResource, GenericParamConstraint) have no writer in this
            // backend at all, so their row counts are structurally zero.
            .max(sizes.event)
            .max(sizes.property)
            .max(sizes.standalone_sig)
            .max(sizes.module_ref)
            .max(sizes.method_spec);
        let custom_attribute_type_max = sizes.method_def.max(sizes.member_ref);
        let member_forwarded_max = sizes.field.max(sizes.method_def);
        // `HasSemantics` targets Event + Property (§II.24.2.6).
        let has_semantics_max = sizes.event.max(sizes.property);
        // `TypeOrMethodDef` (GenericParam.Owner) targets TypeDef + MethodDef.
        let type_or_method_def_max = sizes.type_def.max(sizes.method_def);

        Self {
            heap_sizes,
            str_wide,
            guid_wide,
            blob_wide,
            us_wide,
            type_def_wide: simple_wide(sizes.type_def),
            field_wide: simple_wide(sizes.field),
            method_def_wide: simple_wide(sizes.method_def),
            param_wide: simple_wide(sizes.param),
            module_ref_wide: simple_wide(sizes.module_ref),
            event_wide: simple_wide(sizes.event),
            property_wide: simple_wide(sizes.property),
            type_def_or_ref_wide: coded_wide(2, type_def_or_ref_max),
            resolution_scope_wide: coded_wide(2, resolution_scope_max),
            member_ref_parent_wide: coded_wide(3, member_ref_parent_max),
            has_constant_wide: coded_wide(2, has_constant_max),
            method_def_or_ref_wide: coded_wide(1, method_def_or_ref_max),
            has_custom_attribute_wide: coded_wide(5, has_custom_attribute_max),
            custom_attribute_type_wide: coded_wide(3, custom_attribute_type_max),
            member_forwarded_wide: coded_wide(1, member_forwarded_max),
            has_semantics_wide: coded_wide(1, has_semantics_max),
            type_or_method_def_wide: coded_wide(1, type_or_method_def_max),
        }
    }
}

fn write_heap_idx(out: &mut Vec<u8>, value: u32, wide: bool) {
    if wide {
        out.extend_from_slice(&value.to_le_bytes());
    } else {
        out.extend_from_slice(&(value as u16).to_le_bytes());
    }
}

fn write_simple_idx(out: &mut Vec<u8>, value: u32, wide: bool) {
    write_heap_idx(out, value, wide);
}

fn write_coded_idx(out: &mut Vec<u8>, value: u32, wide: bool) {
    write_heap_idx(out, value, wide);
}

/// [`MetadataBuilder`] is the [`TypeDefOrRefResolver`] the `sig` encoder calls into: it resolves
/// a `ClassRef` to a `TypeDefOrRef` coded index by finding-or-creating the matching `TypeDef`
/// (defined in this assembly) or `TypeRef` (external) row.
///
/// Per the task spec: keyed on (resolution scope, name) of the OPEN type (generics stripped —
/// `sig::encode_type`'s `encode_class` already wraps instantiations in `GENERICINST` itself, so
/// by the time this is called for a generic `ClassRef` its `generics` list is irrelevant to which
/// row it resolves to).
impl TypeDefOrRefResolver for MetadataBuilder {
    fn type_def_or_ref(&mut self, cref: Interned<ClassRef>, asm: &mut Assembly) -> u32 {
        if let Some(&tok) = self.class_token_cache.get(&cref) {
            return encode_type_def_or_ref_token(tok);
        }
        let class_ref = asm.class_ref(cref).clone();
        let raw_name = asm[class_ref.name()].to_string();
        let tok = if asm.class_ref_to_def(cref).is_some() {
            // Defined in this assembly: the corresponding TypeDef row must already have been
            // added by `add_type_def` (population walks class defs before any signature needs
            // to resolve one) — if not, this is a caller-ordering bug, not a spec question.
            self.find_type_def(&raw_name).unwrap_or_else(|| {
                panic!(
                    "ClassRef {raw_name:?} resolves to a class def not yet added via add_type_def"
                )
            })
        } else if let Some(def_id) = find_open_generic_def(asm, cref) {
            // An in-assembly INSTANTIATED generic reference (e.g. `IBox`1<int32>` where `IBox`1`
            // is this assembly's own `#[dotnet_interface] trait IBox<T>`): the interned handle
            // never maps to a def (defs register under the OPEN shape, empty generics), so the
            // plain branch above misses it — and the external fallback below would mint a bogus
            // module-scope `TypeRef` with a DOUBLED arity postfix (`IBox`1`1`). Resolve to the
            // open definition's own TypeDef row instead; `sig::encode_class` has already opened
            // the `GENERICINST` wrapper around this coded index, so the open TypeDef is exactly
            // the §II.23.2.12 "open type" position. Gated on `asm() == None` + non-empty
            // generics + a registered matching def, so no external type can ever take this path.
            let def_name = asm[asm[def_id].name()].to_string();
            self.find_type_def(&def_name).unwrap_or_else(|| {
                panic!(
                    "open generic def {def_name:?} (instantiated as {raw_name:?}) not yet added \
                     via add_type_def"
                )
            })
        } else {
            // External: a TypeRef, scoped by its declaring assembly's AssemblyRef (or a
            // name-only module-scope reference when `asm()` is None — matches
            // `il_exporter::class_ref`'s `if let Some(assembly) = cref.asm()` split).
            let scope = class_ref.asm().map(|asm_name_id| {
                let name = &asm[asm_name_id];
                self.find_or_create_assembly_ref(name)
            });
            // A generic external type's real metadata `Name` carries a `` `N `` arity postfix
            // (e.g. `Vector128`1`, confirmed against a real CoreCLR System.Runtime.Intrinsics.dll
            // via `.class extern forwarder System.Runtime.Intrinsics.Vector128`1`) — `il_exporter`
            // builds the identical string via its own `generic_postfix` (`format!("`{}",
            // cref.generics().len())`) baked into the quoted IL name that ilasm then parses back
            // apart itself. This writer has no assembler to do that for it, so the postfix must be
            // appended by hand before the name is split/interned.
            let full_name = if class_ref.generics().is_empty() {
                raw_name.to_string()
            } else {
                format!("{raw_name}`{}", class_ref.generics().len())
            };
            let (namespace, name) = split_namespace(&full_name);
            self.type_ref(scope, namespace, name)
        };
        self.class_token_cache.insert(cref, tok);
        encode_type_def_or_ref_token(tok)
    }
}

/// If `cref` is an INSTANTIATED reference to a generic type this assembly itself DEFINES —
/// `asm() == None` (no external assembly), non-empty `generics`, and an OPEN `ClassRef` of the
/// same name maps to a registered class def — returns that open definition's `ClassDefIdx`.
/// Returns `None` for every other shape (non-generic, external, or genuinely-unknown), leaving
/// the caller's pre-existing resolution behavior untouched.
///
/// Two candidate open names are tried: the reference's own name verbatim (the
/// `#[dotnet_interface]`-emitted `IBoxHandle<T>` alias already spells the CLS backtick-arity name
/// `IBox`1`), and — when the name carries no backtick — the `` `arity ``-postfixed form (the
/// `#[dotnet_class(implements = "IBox<…>")]` surface spells the bare `IBox`). SOUNDNESS: only a
/// def registered in THIS assembly can ever match (external types carry `asm() == Some(_)` and
/// are gated out up front), and a matched def whose declared arity disagrees with the
/// instantiation's argument count fails loudly rather than emitting a `GENERICINST` the CLR
/// would reject far less legibly at load time.
pub(super) fn find_open_generic_def(
    asm: &mut Assembly,
    cref: Interned<ClassRef>,
) -> Option<crate::ir::class::ClassDefIdx> {
    let class_ref = asm.class_ref(cref).clone();
    if class_ref.asm().is_some() || class_ref.generics().is_empty() {
        return None;
    }
    let arity = class_ref.generics().len();
    let raw_name = asm[class_ref.name()].to_string();
    let mut candidates = vec![raw_name.clone()];
    if !raw_name.contains('`') {
        candidates.push(format!("{raw_name}`{arity}"));
    }
    for candidate in candidates {
        let name_id = asm.alloc_string(candidate);
        let open = ClassRef::new(name_id, None, class_ref.is_valuetype(), [].into());
        let open_id = asm.alloc_class_ref(open);
        if let Some(def_id) = asm.class_ref_to_def(open_id) {
            let declared = asm[def_id].generics();
            assert_eq!(
                declared as usize, arity,
                "generic type `{raw_name}` is instantiated with {arity} argument(s) but its \
                 definition declares {declared} generic parameter(s)"
            );
            return Some(def_id);
        }
    }
    None
}

impl MetadataBuilder {
    /// Resolves `cref` to a token usable as a STANDALONE metadata reference — a `MemberRef`'s
    /// `Class` parent (§II.22.25 `MemberRefParent`), or an instruction operand's declaring type
    /// (`newobj`/`castclass`/…, via [`TokenSink::type_token`]). Unlike
    /// [`TypeDefOrRefResolver::type_def_or_ref`] (used ONLY inside a signature blob, where
    /// `sig::encode_class` wraps a generic instantiation's arguments in `GENERICINST` itself —
    /// see that impl's doc), a bare `TypeDef`/`TypeRef` token naming just the OPEN generic shape
    /// (`Dictionary\`2`, arguments erased) is not a valid concrete-type reference on its own: a
    /// real CoreCLR rejects it with `TypeLoadException: Could not load type '…\`2' from assembly
    /// '_'` (the runtime treats an uninstantiated open-generic operand as needing the CALLER's
    /// own module to supply a matching TypeDef, since there is no instantiation to bind — this
    /// was a real regression caught wiring `DIRECT_PE=1` into the linker, on `Dictionary<K,V>`
    /// statics initialized from a `.cctor`). A generic `cref` must resolve to a `TypeSpec`
    /// (§II.22.39) carrying the FULL `GENERICINST` blob instead — mirrors what `ilasm` builds
    /// under the hood whenever `il_exporter`'s textual `class 'Name'<T,…>` appears as a
    /// `newobj`/`MemberRef` operand.
    pub(super) fn class_ref_token(
        &mut self,
        asm: &mut Assembly,
        cref: Interned<ClassRef>,
    ) -> Token {
        if asm[cref].generics().is_empty() {
            let coded = self.type_def_or_ref(cref, asm);
            decode_type_def_or_ref(coded)
        } else {
            let mut blob = Vec::new();
            sig::encode_type(Type::ClassRef(cref), asm, self, &mut blob);
            let off = self.blobs.intern(&blob);
            self.type_spec(off)
        }
    }
}

impl MetadataBuilder {
    /// Finds a previously-added `TypeDef` row by its (already-shortened) name.
    ///
    /// **Must split on the last `.` exactly like [`add_type_def`](Self::add_type_def) does**, via
    /// the same [`split_namespace`] `TypeRef` already uses. This was a real bug (found wiring
    /// `cd_interop`'s C#-consumer battery target, not caught by any prior A/B round because every
    /// earlier check either read metadata generically — `System.Reflection.Metadata`, `ilverify`,
    /// this crate's own reader-based unit tests — none of which care whether `Namespace` is `""`
    /// or split out, or exercised `dotnet run`/`Assembly.Load`, which resolves types by TOKEN, not
    /// by name — only Roslyn's COMPILE-TIME reference resolution (`csc`/`dotnet build` against a
    /// `<Reference>`) looks a type up by `Namespace`+`Name`): a prior version of this fn assumed
    /// "namespace is always emitted empty by this backend, mirrors il_exporter" — false. `ilasm`
    /// itself, given `.class 'cd_interop.Point' …`, DOES split on the last `.` into
    /// `TypeDef.Namespace="cd_interop"`/`TypeDef.Name="Point"` (confirmed by decoding a real
    /// ilasm-built `cd_interop.dll` byte-for-byte) — `il_exporter` just never has to do that split
    /// ITSELF because it hands ilasm one opaque quoted string and lets the assembler's own name
    /// parser do it. This writer, bypassing ilasm, must replicate that split by hand or every
    /// namespaced Rust-exported type becomes uncompilable-against from C# (`CS0246: The type or
    /// namespace name 'cd_interop' could not be found`) even though it loads and runs fine at
    /// runtime (token-based resolution never notices the empty `Namespace`).
    fn find_type_def(&self, raw_name: &str) -> Option<Token> {
        let raw_name = if raw_name == crate::ir::asm::MAIN_MODULE {
            self.public_module_full_name.as_deref().unwrap_or(raw_name)
        } else {
            raw_name
        };
        // Split BEFORE shortening (not after) to match `add_type_def`'s own contract exactly: it
        // shortens only the (already-split-by-the-caller) `name` argument, never the combined
        // dotted string — splitting a POST-shortened (hash-suffixed) string here would risk
        // disagreeing with `add_type_def` for any name near the 1023-char cutoff.
        let (namespace, name) = split_namespace(raw_name);
        let shortened = dotnet_class_name(name);
        for (i, row) in self.type_def.iter().enumerate() {
            if self.strings_eq(row.name, &shortened) && self.strings_eq(row.namespace, namespace) {
                return Some(Token::new(
                    Token::TABLE_TYPE_DEF,
                    u32::try_from(i + 1).unwrap(),
                ));
            }
        }
        None
    }

    /// Finds-or-creates an `AssemblyRef` row for `name`, applying the same BCL-vs-consumer split
    /// as `il_exporter`'s `bcl_public_key_token` (this local port avoids depending on
    /// `il_exporter`, per the hard constraint that `pe_exporter` may not import it). Public so
    /// other `pe_exporter` modules (e.g. `export::export_pe`, bootstrapping a
    /// `System.Object`/`System.ValueType` `TypeRef`) share the same deduplicated row instead of
    /// calling the always-inserts [`MetadataBuilder::assembly_ref`] directly and creating
    /// duplicate rows for repeated bootstrap references.
    pub fn find_or_create_assembly_ref(&mut self, name: &str) -> Token {
        for (i, row) in self.assembly_ref.iter().enumerate() {
            if self.strings_eq(row.name, name) {
                return Token::new(Token::TABLE_ASSEMBLY_REF, u32::try_from(i + 1).unwrap());
            }
        }
        let target = if let (Some(token), true) = (bcl_public_key_token(name), self.is_lib) {
            // Same explicitly selected runtime as `system_runtime_assembly_ref`. Also gated on
            // `self.is_lib` for the same reason — see `MetadataBuilder::is_lib`'s doc.
            AssemblyRefTarget::Bcl {
                version: self.runtime.assembly_ver_tuple(),
                token,
            }
        } else {
            AssemblyRefTarget::NameOnly
        };
        self.assembly_ref(name, target)
    }
}

/// The public-key token an external assembly's `AssemblyRef` row should carry, or `None` if it's a
/// consumer-supplied assembly that should be referenced by simple name only (no version/token).
///
/// Local port of `il_exporter::bcl_public_key_token` (kept private/duplicated rather than
/// imported, per the hard constraint that `pe_exporter` code must not depend on `il_exporter`; see
/// that function's doc comment for the full rationale and how the two non-ECMA tokens were
/// verified — this used to be a single `is_bcl_assembly(name) -> bool` that wrongly treated every
/// `Microsoft`-prefixed name as CoreLib-signed).
fn bcl_public_key_token(name: &str) -> Option<[u8; 8]> {
    if name.starts_with("System") || matches!(name, "mscorlib" | "netstandard" | "WindowsBase") {
        return Some(ECMA_PUBLIC_KEY_TOKEN);
    }
    const EXTENSIONS_FAMILY_PREFIXES: &[&str] = &[
        "Microsoft.AspNetCore",
        "Microsoft.Extensions",
        "Microsoft.EntityFrameworkCore",
        "Microsoft.OpenApi",
        "Microsoft.JSInterop",
        "Microsoft.Net.Http.Headers",
    ];
    if EXTENSIONS_FAMILY_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return Some(EXTENSIONS_PUBLIC_KEY_TOKEN);
    }
    None
}

/// Splits an EXTERNAL type's dotted name into a metadata `TypeNamespace`/`TypeName` pair
/// (§II.22.38) at the last `.` — e.g. `"System.Console"` -> `("System", "Console")`. Needed for
/// every `TypeRef` this writer creates to a real BCL type: `il_exporter`'s textual
/// `[System.Console]System.Console::WriteLine` syntax lets ilasm itself perform this split when it
/// assembles the `TypeRef` row (the text form doesn't distinguish `Namespace`+`Name` from a single
/// dotted `Name`, but the *binary* metadata row genuinely has two separate columns, and a real BCL
/// type's `TypeDef` in `System.Console.dll`/`System.Private.CoreLib.dll` has `Namespace="System"`,
/// `Name="Console"` — a `TypeRef` with `Namespace=""`, `Name="System.Console"` matches NO type,
/// which is a `TypeLoadException`, not a `BadImageFormatException`, so it surfaces well past the
/// PE/CLI-header layer). Confirmed against a real CoreCLR-`ilasm`-produced reference image during
/// the Phase 1a E2E milestone (`TypeRef` row for `System.Console` there has `Namespace="System"`,
/// `Name="Console"`).
///
/// Used for BOTH `TypeRef` (external, §II.22.38 `TypeNamespace`) AND `TypeDef` (this assembly's
/// own classes, §II.22.37 `TypeNamespace`) — an EARLIER version of this doc claimed `TypeDef`
/// "intentionally stays UNSPLIT" on the theory that `il_exporter` never gives its own mangled Rust
/// type names a namespace concept either. That reasoning doesn't survive contact with a real
/// ilasm-produced image: `il_exporter`'s `.class 'MangledName' { … }` IS one quoted identifier
/// with no `.` interpreted specially *in the IL text*, but `ilasm` itself still splits that string
/// on its last `.` when it writes the BINARY `TypeDef` row — confirmed byte-for-byte against a
/// real `cd_interop.dll`: `.class 'cd_interop.Point'` becomes `TypeDef.Namespace="cd_interop"` /
/// `TypeDef.Name="Point"`, not `Namespace=""`/`Name="cd_interop.Point"`. Leaving `TypeDef`
/// unsplit loads and runs fine (CLR token-based method/field resolution never looks at
/// `Namespace`), but makes the type uncompilable-against from C#: Roslyn's reference resolution
/// looks types up by `Namespace`+`Name`, so an unsplit `cd_interop.Point` is invisible to
/// `csc`/`dotnet build`, surfacing as `CS0246: The type or namespace name 'cd_interop' could not
/// be found` — see `MetadataBuilder::find_type_def` and `export::export_pe`'s Pass 1 for the two
/// call sites this split had to be threaded into together.
pub(super) fn split_namespace(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(idx) => (&name[..idx], &name[idx + 1..]),
        None => ("", name),
    }
}

/// The token queries `body.rs` needs while assembling instruction bytes: every operand that is a
/// metadata token in the IL stream (`call`/`callvirt`/`newobj`/`ldfld`/`ldsfld`/`calli`/`ldstr`/
/// generic-method-instantiation calls/`.locals` signatures) goes through one of these methods
/// rather than touching table internals directly, so `body.rs` never depends on `tables.rs`'s
/// row-storage representation. Implemented by [`MetadataBuilder`].
pub trait TokenSink {
    /// Resolves a method reference to its token: a `MethodDef` token if `method` is defined in
    /// this assembly, otherwise a `MemberRef` token (interning one on first use) — mirrors
    /// `il_exporter`'s `class_ref`/`partitioned_class` + method-name lookup. When `generic_args`
    /// is non-empty the result is wrapped in a `MethodSpec` token instead (§II.22.29), matching
    /// `il_exporter`'s `method<T,…>` call-site rendering.
    fn method_token(
        &mut self,
        asm: &mut Assembly,
        method: MethodDefIdx,
        generic_args: &[Type],
    ) -> Token;

    /// Resolves a field reference to its token: a `Field` token if the owning class is defined
    /// in this assembly, otherwise a `MemberRef` token.
    fn field_token(&mut self, asm: &mut Assembly, field: Interned<FieldDesc>) -> Token;

    /// Resolves a *static* field reference to its token (separate from [`Self::field_token`]
    /// because instance and static fields are interned in different `Assembly` maps —
    /// `FieldDesc` vs. `StaticFieldDesc` — mirroring the `ldsfld`/`stsfld` vs. `ldfld`/`stfld`
    /// split in `il_exporter::export_node`).
    fn static_field_token(&mut self, asm: &mut Assembly, field: Interned<StaticFieldDesc>)
    -> Token;

    /// Interns `s` in the `#US` heap and returns the `ldstr` token (§II.22.2's *User String*
    /// token: table id `0x70`, not one of the ordinary metadata tables).
    fn user_string_token(&mut self, s: &str) -> Token;

    /// Interns a `StandAloneSig` row for a `calli` call-site signature and returns its token.
    fn calli_sig_token(&mut self, asm: &mut Assembly, semantic_key: &str, sig_blob: &[u8])
    -> Token;

    /// Interns a `StandAloneSig` row for a method body's `.locals` signature and returns its
    /// token (fat method headers store this in the `LocalVarSigTok` field, §II.25.4.3).
    fn locals_sig_token(&mut self, asm: &mut Assembly, locals: &[Type]) -> Token;

    /// Resolves a `Type` to the token an instruction operand needs (`newobj`'s class,
    /// `castclass`/`isinst`/`box`/`unbox.any`'s type operand, `newarr`'s element type, …):
    /// `TypeDef` if defined in this assembly, `TypeRef` if external, or `TypeSpec` if `tpe` needs
    /// a full signature encoding (generic instantiation, array, pointer — anything
    /// `sig::encode_type` doesn't collapse to a bare class reference).
    fn type_token(&mut self, asm: &mut Assembly, tpe: Type) -> Token;
}

impl TokenSink for MetadataBuilder {
    fn method_token(
        &mut self,
        asm: &mut Assembly,
        method: MethodDefIdx,
        generic_args: &[Type],
    ) -> Token {
        let base = if let Some(&tok) = self.method_def_cache.get(&method) {
            tok
        } else {
            // Not (yet) known as an in-assembly MethodDef row: resolve as a MemberRef against
            // its declaring ClassRef, mirroring `il_exporter`'s BCL-call rendering.
            let method_ref = asm[*method].clone();
            let class_tok = self.class_ref_token(asm, method_ref.class());
            let name = asm[method_ref.name()].to_string();
            let sig = method_ref.sig();
            let mut blob = Vec::new();
            let fnsig = asm[sig].clone();
            let is_static = method_ref.kind() == crate::ir::cilnode::MethodKind::Static;
            let mut convention = if is_static {
                sig::SIG_DEFAULT
            } else {
                sig::SIG_HASTHIS
            };
            // §II.23.2.2 `MethodRefSig`: when this call site is a generic-method instantiation
            // (`generic_args` non-empty — the caller wraps the result in a `MethodSpec`,
            // §II.22.29), the base MemberRef this MethodSpec points at must ITSELF carry the
            // `GENERIC` convention bit (0x10) plus the method's own generic-parameter COUNT (not
            // the instantiation's argument types — those live only in the MethodSpec's
            // instantiation blob). Its parameter/return positions reference that arity via
            // `ET_MVAR` (`Type::PlatformGeneric(_, CallGeneric)`, already correctly encoded by
            // `sig::encode_type`) — e.g. `Queryable.Count<T>(this IQueryable<T> source)` becomes
            // `int32 Count<GENPARAMCOUNT=1>(class IQueryable`1<!!0>)`, matching `il_exporter`'s
            // `call int32 …Queryable::'Count'<int32>(class …IQueryable`1<!!0>)` call-site
            // rendering byte-for-byte at the semantic level (ilasm's own assembler adds this flag
            // for a `<…>`-suffixed call; a hand-rolled writer must add it explicitly). Omitting
            // this produces a MemberRefSig CoreCLR cannot bind to any real method overload —
            // regression caught wiring `DIRECT_PE=1` into cd_linq_expr's `IntQuery::count`
            // (`MissingMethodException: Method not found: 'Int32
            // System.Linq.Queryable.Count(System.Linq.IQueryable`1<!!0>)'` — note the missing
            // `<T>` in the CLR's own error text, confirming it read this as a NON-generic
            // 1-arg method and correctly failed to find one).
            let generic_param_count = if generic_args.is_empty() {
                0
            } else {
                convention |= sig::SIG_GENERIC;
                u32::try_from(generic_args.len()).unwrap()
            };
            // Strip the implicit receiver (`this`) `fnsig.inputs()[0]` carries for every
            // non-static kind before encoding — see `export_pe`'s Pass 3 (`export.rs`) doc
            // comment on the identical fix for `MethodDef` signatures; a `MethodRef`'s stored
            // `FnSig` carries the SAME "receiver at index 0" convention (mirrors `il_exporter`'s
            // `&sig.inputs()[1..]` skip at its own `MethodRef` call-site rendering, mod.rs:796).
            let encode_sig = if is_static {
                fnsig
            } else {
                crate::ir::FnSig::new(fnsig.inputs()[1..].to_vec(), *fnsig.output())
            };
            sig::encode_method_sig(
                convention,
                generic_param_count,
                &encode_sig,
                asm,
                self,
                &mut blob,
            );
            let sig_off = self.blobs.intern(&blob);
            self.member_ref(class_tok, &name, sig_off)
        };
        if generic_args.is_empty() {
            base
        } else {
            let mut blob = Vec::new();
            sig::encode_method_spec_sig(generic_args, asm, self, &mut blob);
            let inst_off = self.blobs.intern(&blob);
            self.method_spec(base, inst_off)
        }
    }

    fn field_token(&mut self, asm: &mut Assembly, field: Interned<FieldDesc>) -> Token {
        let desc = asm[field];
        let owner_in_asm = asm.class_ref_to_def(desc.owner()).is_some();
        if owner_in_asm {
            let raw_name = asm[desc.owner()].name();
            let raw_name = asm[raw_name].to_string();
            let field_name = asm[desc.name()].to_string();
            if let Some(tok) = self.find_field(&raw_name, &field_name) {
                return tok;
            }
        }
        let class_tok = self.class_ref_token(asm, desc.owner());
        let name = asm[desc.name()].to_string();
        let mut blob = Vec::new();
        sig::encode_field_sig(desc.tpe(), asm, self, &mut blob);
        let sig_off = self.blobs.intern(&blob);
        self.member_ref(class_tok, &name, sig_off)
    }

    fn static_field_token(
        &mut self,
        asm: &mut Assembly,
        field: Interned<StaticFieldDesc>,
    ) -> Token {
        let desc = asm[field];
        let owner_in_asm = asm.class_ref_to_def(desc.owner()).is_some();
        if owner_in_asm {
            let raw_name = asm[desc.owner()].name();
            let raw_name = asm[raw_name].to_string();
            let field_name = asm[desc.name()].to_string();
            if let Some(tok) = self.find_field(&raw_name, &field_name) {
                return tok;
            }
        }
        let class_tok = self.class_ref_token(asm, desc.owner());
        let name = asm[desc.name()].to_string();
        let mut blob = Vec::new();
        sig::encode_field_sig(desc.tpe(), asm, self, &mut blob);
        let sig_off = self.blobs.intern(&blob);
        self.member_ref(class_tok, &name, sig_off)
    }

    fn user_string_token(&mut self, s: &str) -> Token {
        let off = self.user_strings.intern(s);
        // §II.22.2: the User String token's table id is the fixed value 0x70.
        Token::new(Token::TABLE_USER_STRING, off)
    }

    fn calli_sig_token(
        &mut self,
        _asm: &mut Assembly,
        semantic_key: &str,
        sig_blob: &[u8],
    ) -> Token {
        if let Some(&token) = self.calli_sig_cache.get(semantic_key) {
            return token;
        }
        let off = self.blobs.intern(sig_blob);
        let token = self.standalone_sig(off);
        self.calli_sig_cache.insert(semantic_key.to_owned(), token);
        token
    }

    fn locals_sig_token(&mut self, asm: &mut Assembly, locals: &[Type]) -> Token {
        let semantic_key: Vec<_> = locals.iter().map(|local| local.mangle(asm)).collect();
        if let Some(&token) = self.locals_sig_cache.get(&semantic_key) {
            return token;
        }
        let mut blob = Vec::new();
        sig::encode_locals_sig(locals, asm, self, &mut blob);
        let off = self.blobs.intern(&blob);
        let token = self.standalone_sig(off);
        self.locals_sig_cache.insert(semantic_key, token);
        token
    }

    fn type_token(&mut self, asm: &mut Assembly, tpe: Type) -> Token {
        match tpe {
            // See `class_ref_token`'s doc: a generic `ClassRef` needs the `TypeSpec` treatment,
            // not the bare open-shape `TypeDefOrRef` coded index.
            Type::ClassRef(cref) => self.class_ref_token(asm, cref),
            other => {
                let mut blob = Vec::new();
                sig::encode_type(other, asm, self, &mut blob);
                let off = self.blobs.intern(&blob);
                self.type_spec(off)
            }
        }
    }
}

impl MetadataBuilder {
    /// Finds a previously-added instance/static `Field` row owned by a `TypeDef` named
    /// `owner_name` (the raw, un-shortened name — matched via [`dotnet_class_name`] exactly like
    /// [`Self::find_type_def`]). This is a best-effort linear scan tying a `Field` row back to
    /// its owning `TypeDef`'s field run (`FieldList`..next `TypeDef.FieldList`), used so repeat
    /// lookups of the same in-assembly field return the already-added row instead of no row at
    /// all (this writer never creates a `MemberRef` to its own `TypeDef`'s field).
    fn find_field(&self, owner_name: &str, field_name: &str) -> Option<Token> {
        let shortened_owner = dotnet_class_name(owner_name);
        let type_idx = self
            .type_def
            .iter()
            .position(|row| self.strings_eq(row.name, &shortened_owner))?;
        let field_start = self.type_def[type_idx].field_list as usize; // 1-based
        let field_end = self
            .type_def
            .get(type_idx + 1)
            .map_or(self.field.len() + 1, |next| next.field_list as usize);
        for rid in field_start..field_end {
            if let Some(row) = self.field.get(rid - 1) {
                if self.strings_eq(row.name, field_name) {
                    return Some(Token::new(Token::TABLE_FIELD, u32::try_from(rid).unwrap()));
                }
            }
        }
        None
    }
}

/// Decodes a `TypeDefOrRef` coded index (as produced by
/// [`TypeDefOrRefResolver::type_def_or_ref`]) back into a [`Token`] — the inverse of
/// [`encode_type_def_or_ref_token`], needed because `sig::TypeDefOrRefResolver::type_def_or_ref`
/// returns a raw coded `u32` (the shape the signature encoder embeds directly into a blob), but
/// [`TokenSink::type_token`]/[`TokenSink::method_token`] need the *token* shape (the shape an
/// instruction operand embeds directly) for the same resolved row.
fn decode_type_def_or_ref(coded: u32) -> Token {
    let tag = coded & 0x3;
    let rid = coded >> 2;
    let table = match tag {
        0 => Token::TABLE_TYPE_DEF,
        1 => Token::TABLE_TYPE_REF,
        2 => Token::TABLE_TYPE_SPEC,
        _ => unreachable!("2-bit tag"),
    };
    Token::new(table, rid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Access, Float, Int, cilnode::MethodKind};

    #[test]
    fn token_encodes_table_and_rid() {
        let t = Token::new(Token::TABLE_TYPE_DEF, 3);
        assert_eq!(t.0, 0x0200_0003);
        assert_eq!(t.table(), Token::TABLE_TYPE_DEF);
        assert_eq!(t.rid(), 3);
    }

    #[test]
    fn user_string_token_uses_the_0x70_table_id() {
        let mut mb = MetadataBuilder::new();
        let t = mb.user_string_token("hi");
        assert_eq!(t.table(), 0x70);
    }

    #[test]
    fn metadata_builder_default_heaps_are_empty() {
        let mb = MetadataBuilder::new();
        // The `#Strings` heap is NOT empty: `new()` seeds the mandatory `<Module>` pseudo-`TypeDef`
        // row (§II.22.37 — see `MetadataBuilder::new`'s doc comment), which interns "<Module>".
        assert_eq!(mb.strings.as_bytes(), b"\0<Module>\0");
        assert_eq!(mb.blobs.as_bytes(), &[0]);
        assert_eq!(mb.guids.as_bytes(), &[] as &[u8]);
        assert_eq!(mb.user_strings.as_bytes(), b"\0");
    }

    #[test]
    fn metadata_builder_new_seeds_the_module_pseudo_type_as_type_def_row_1() {
        // §II.22.37: "The first row of the TypeDef table represents the pseudo class that acts
        // as the parent for functions and variables defined at module scope." A real class added
        // afterwards must land on row 2, never row 1 — landing on row 1 makes the CLR treat its
        // methods as ownerless "global methods" (confirmed via `monodis` during development; see
        // `MetadataBuilder::new`'s doc comment for the full story).
        let mut mb = MetadataBuilder::new();
        let tok = mb.add_type_def("", "MainModule", false, None, None, None, &[]);
        assert_eq!(
            tok.rid(),
            2,
            "the first real class def must be TypeDef row 2, not row 1"
        );
    }

    #[test]
    fn coded_index_roundtrip_type_def_or_ref() {
        for (table, tag) in [
            (Token::TABLE_TYPE_DEF, 0u32),
            (Token::TABLE_TYPE_REF, 1),
            (Token::TABLE_TYPE_SPEC, 2),
        ] {
            let tok = Token::new(table, 7);
            let coded = encode_type_def_or_ref_token(tok);
            assert_eq!(coded, (7 << 2) | tag);
            assert_eq!(decode_type_def_or_ref(coded), tok);
        }
    }

    #[test]
    fn deterministic_mvid_is_pure_function_of_name() {
        let a = deterministic_mvid("my_crate");
        let b = deterministic_mvid("my_crate");
        let c = deterministic_mvid("other_crate");
        assert_eq!(a, b, "same name must hash to the same MVID every time");
        assert_ne!(a, c);
        assert_eq!(a.len(), 16);
    }

    /// The PE writer emits a `ClassDef::with_interface` class as a genuine ECMA-335 `interface`
    /// `TypeDef` (§II.23.1.15: `Interface`+`Abstract` flags, NIL `Extends`) and its
    /// `MethodDef::with_abstract` member with `Abstract` set and RVA=0 (§II.22.26). Structural
    /// readback of the actual serialized bytes `export_pe` produces (the CoreCLR-loadability of
    /// this exact shape is separately proven via the `il_exporter` path + a hand-verified C#
    /// `Parrot : ISpeaker` consumer — see `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md` finding #2).
    #[test]
    fn interface_type_def_and_abstract_method_are_emitted_by_pe_writer() {
        use crate::ir::{ClassDef, MethodDef, MethodImpl, Type};

        let mut asm = crate::ir::Assembly::default();
        let iname = asm.alloc_string("ISpeaker");
        let cdef = ClassDef::new(
            iname,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        )
        .with_interface();
        let cidx = asm.class_def(cdef).unwrap();
        let self_ty = Type::ClassRef(*cidx);
        let mname = asm.alloc_string("Speak");
        let msig = asm.sig([self_ty], Type::Void);
        let mdef = MethodDef::new(
            Access::Public,
            cidx,
            mname,
            msig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None],
        )
        .with_abstract();
        asm.new_method(mdef);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_iface".to_string(),
            public_module_full_name: None,
            module_name: "pe_iface.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        // `MetadataReader` parses a bare BSJB metadata root; slice it out of the full PE image
        // (the root is contiguous, and the reader indexes by offset relative to its start, so
        // trailing PE bytes after the metadata are harmless).
        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());

        // --- ISpeaker's TypeDef row (rid 2 — `<Module>` occupies rid 1). Flags are the first 4
        // bytes of the row, independent of heap-index widths.
        let td_start = reader.table_offset(Token::TABLE_TYPE_DEF, &header);
        let td_w = MetadataReader::row_width(Token::TABLE_TYPE_DEF, &header);
        let iface_row = td_start + td_w; // rid 2 == index 1
        let flags = u32_at(iface_row);
        assert_ne!(
            flags & 0x20,
            0,
            "TypeDef must have Interface (0x20) flag; flags={flags:#x}"
        );
        assert_ne!(
            flags & 0x80,
            0,
            "TypeDef must have Abstract (0x80) flag; flags={flags:#x}"
        );
        assert_eq!(
            flags & 0x100,
            0,
            "interface TypeDef must NOT be Sealed (0x100); flags={flags:#x}"
        );
        // Extends sits after Flags(4) + Name + Namespace (two `#Strings` indices).
        let str_w = if header.heap_sizes & 0x1 != 0 { 4 } else { 2 };
        let extends_off = iface_row + 4 + 2 * str_w;
        let extends = if str_w == 4 {
            u32_at(extends_off)
        } else {
            u16_at(extends_off) as u32
        };
        assert_eq!(
            extends, 0,
            "interface TypeDef must have NIL Extends; got {extends:#x}"
        );

        // --- Speak's MethodDef row (rid 1, the only method). RVA is the first 4 bytes; Flags is a
        // u16 at offset 6 (after RVA(4) + ImplFlags(2)), both in the fixed-width prefix.
        let md_start = reader.table_offset(Token::TABLE_METHOD_DEF, &header);
        assert_eq!(u32_at(md_start), 0, "abstract method must have RVA=0");
        let m_flags = u16_at(md_start + 6);
        assert_ne!(
            m_flags & 0x400,
            0,
            "abstract method must have Abstract (0x400) flag; flags={m_flags:#x}"
        );
        assert_ne!(
            m_flags & 0x40,
            0,
            "abstract method must still be Virtual (0x40); flags={m_flags:#x}"
        );
    }

    /// §II.22.12 `EventMap` runs are contiguous Event-table slices delimited by the NEXT row's
    /// `event_list`, so re-opening a class's events after ANOTHER class started its run would
    /// silently file the late Event row under the other class. [`MetadataBuilder::add_event`]
    /// must fail loudly on that misuse instead of masking it.
    #[test]
    #[should_panic(expected = "non-contiguous event addition")]
    fn add_event_rejects_non_contiguous_per_class_event_runs() {
        let mut mb = MetadataBuilder::new();
        let class_a = mb.add_type_def("", "A", false, None, None, None, &[]);
        let class_b = mb.add_type_def("", "B", false, None, None, None, &[]);
        // Six accessor methods (an add/remove pair per event). The signature blob's shape is
        // irrelevant here — `add_event` only flips accessor flags and appends event rows.
        let sig = mb.blobs.intern(&[0x20, 0x00, 0x01]);
        let m: Vec<Token> = (0..6)
            .map(|i| {
                mb.add_method(
                    &format!("acc{i}"),
                    sig,
                    &[],
                    &[],
                    false,
                    true,
                    false,
                    None,
                    false,
                    None,
                )
            })
            .collect();
        let delegate = class_a; // any TypeDefOrRef-encodable token works as the event type
        mb.add_event(class_a, "E1", delegate, m[0], m[1]);
        mb.add_event(class_b, "E2", delegate, m[2], m[3]);
        // Class A's run is closed (B's is the open tail) — this must panic, not silently append
        // an Event row that decodes as belonging to B.
        mb.add_event(class_a, "E3", delegate, m[4], m[5]);
    }

    /// A **`static abstract`** interface member (.NET 7+ static virtual members in interfaces —
    /// the `INumber<T>` generic-math shape): the PE writer must emit its `MethodDef` with Roslyn's
    /// exact flags `0x4D6` = `Public|Static|Virtual|HideBySig|Abstract` (critically **no
    /// `NewSlot`** — an INSTANCE abstract member has it, a static virtual must not, per a
    /// `System.Reflection.Metadata` dump of real net8 csc output), RVA=0, and a `SIG_DEFAULT`
    /// (no-`HASTHIS`) signature blob. Structural readback of `export_pe`'s actual bytes, same
    /// pattern as `interface_type_def_and_abstract_method_are_emitted_by_pe_writer`.
    #[test]
    fn static_abstract_interface_member_is_emitted_by_pe_writer() {
        use crate::ir::{ClassDef, MethodDef, MethodImpl, Type};

        let mut asm = crate::ir::Assembly::default();
        let iname = asm.alloc_string("IParse");
        let cdef = ClassDef::new(
            iname,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        )
        .with_interface();
        let cidx = asm.class_def(cdef).unwrap();
        // `static abstract int Make();` — NO receiver input (a static sig carries none).
        let mname = asm.alloc_string("Make");
        let msig = asm.sig([], Type::Int(Int::I32));
        let mdef = MethodDef::new(
            Access::Public,
            cidx,
            mname,
            msig,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![],
        )
        .with_abstract();
        asm.new_method(mdef);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_static_iface".to_string(),
            public_module_full_name: None,
            module_name: "pe_static_iface.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());

        // --- Make's MethodDef row (rid 1, the only method). Row layout: RVA(4) + ImplFlags(2) +
        // Flags(2) + Name(str) + Signature(blob) + ParamList(simple).
        let md_start = reader.table_offset(Token::TABLE_METHOD_DEF, &header);
        assert_eq!(
            u32_at(md_start),
            0,
            "static abstract member must have RVA=0"
        );
        assert_eq!(u16_at(md_start + 4), 0, "ImplFlags must be 0");
        let m_flags = u16_at(md_start + 6);
        assert_eq!(
            m_flags, 0x4D6,
            "static abstract member flags must byte-match Roslyn's \
             Public|Static|Virtual|HideBySig|Abstract (0x4D6, no NewSlot); flags={m_flags:#x}"
        );

        // --- The signature blob: `SIG_DEFAULT` (0x00, NO `HASTHIS`), 0 params, ELEMENT_TYPE_I4.
        let str_w = if header.heap_sizes & 0x1 != 0 {
            4usize
        } else {
            2
        };
        let blob_w = if header.heap_sizes & 0x4 != 0 {
            4usize
        } else {
            2
        };
        let sig_col = md_start + 8 + str_w;
        let sig_off = if blob_w == 4 {
            u32_at(sig_col)
        } else {
            u32::from(u16_at(sig_col))
        } as usize;
        let blob_heap = reader.stream("#Blob");
        // Small blob (< 0x80 bytes): a single compressed-length byte, then the blob data.
        let blob_len = blob_heap[sig_off] as usize;
        let sig_blob = &blob_heap[sig_off + 1..sig_off + 1 + blob_len];
        assert_eq!(
            sig_blob,
            &[0x00, 0x00, 0x08],
            "sig must be SIG_DEFAULT (no HASTHIS), 0 params, ELEMENT_TYPE_I4"
        );
    }

    /// A **property on an interface** (`#[dotnet_property]` inside `#[dotnet_interface]` —
    /// `ClassDef::add_property`): the PE writer must emit (a) one §II.22.35 `PropertyMap` row
    /// (Parent = the interface TypeDef, PropertyList = 1, a run-start), (b) one §II.22.34
    /// `Property` row per property with Flags=0 and a §II.23.2.5 `PropertySig` blob
    /// (`PROPERTY|HASTHIS, 0 params, <type>`), (c) `MethodSemantics` rows with Getter=0x2/
    /// Setter=0x1 whose `Association` carries the `HasSemantics` Property tag (1), and (d)
    /// accessor `MethodDef`s flagged `Virtual|NewSlot|Abstract|SpecialName` with RVA=0.
    /// Structural readback of `export_pe`'s actual bytes, same pattern as
    /// `interface_type_def_and_abstract_method_are_emitted_by_pe_writer`. Includes a GET-ONLY
    /// second property (setter absent — exactly one MethodSemantics row).
    #[test]
    fn interface_property_emits_property_map_property_and_method_semantics_rows() {
        use crate::ir::class::PropertyDef;
        use crate::ir::{ClassDef, MethodDef, MethodImpl, Type};

        let mut asm = crate::ir::Assembly::default();
        let iname = asm.alloc_string("ISpeaker");
        let cdef = ClassDef::new(
            iname,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        )
        .with_interface();
        let cidx = asm.class_def(cdef).unwrap();
        let self_ty = Type::ClassRef(*cidx);

        // `int Volume { get; set; }` — abstract get_Volume/set_Volume accessors (MethodDef rids
        // 1 and 2) + one PropertyDef linking them.
        let get_name = asm.alloc_string("get_Volume");
        let get_sig = asm.sig([self_ty], Type::Int(Int::I32));
        let get_def = MethodDef::new(
            Access::Public,
            cidx,
            get_name,
            get_sig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None],
        )
        .with_abstract();
        let get_mref = asm.alloc_methodref(get_def.ref_to());
        asm.new_method(get_def);
        let set_name = asm.alloc_string("set_Volume");
        let set_sig = asm.sig([self_ty, Type::Int(Int::I32)], Type::Void);
        let set_def = MethodDef::new(
            Access::Public,
            cidx,
            set_name,
            set_sig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None, None],
        )
        .with_abstract();
        let set_mref = asm.alloc_methodref(set_def.ref_to());
        asm.new_method(set_def);
        // `string Name { get; }` — a GET-ONLY managed-typed property (MethodDef rid 3).
        let getname_name = asm.alloc_string("get_Name");
        let getname_sig = asm.sig([self_ty], Type::PlatformString);
        let getname_def = MethodDef::new(
            Access::Public,
            cidx,
            getname_name,
            getname_sig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None],
        )
        .with_abstract();
        let getname_mref = asm.alloc_methodref(getname_def.ref_to());
        asm.new_method(getname_def);

        let vol_name = asm.alloc_string("Volume");
        asm.class_mut(cidx).add_property(PropertyDef::new(
            vol_name,
            Type::Int(Int::I32),
            Some(get_mref),
            Some(set_mref),
        ));
        let nm_name = asm.alloc_string("Name");
        asm.class_mut(cidx).add_property(PropertyDef::new(
            nm_name,
            Type::PlatformString,
            Some(getname_mref),
            None,
        ));

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_iface_prop".to_string(),
            public_module_full_name: None,
            module_name: "pe_iface_prop.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());
        let count = |id: u32| {
            header
                .counts
                .iter()
                .find(|&&(t, _)| t == id)
                .map_or(0, |&(_, c)| c)
        };

        // Both new tables are Valid; neither is in the Sorted bitmask (§II.24.2.6 — same
        // NOT-sorted status as EventMap/Event); MethodSemantics keeps its Sorted bit.
        assert_eq!(count(Token::TABLE_PROPERTY_MAP), 1);
        assert_eq!(count(Token::TABLE_PROPERTY), 2);
        assert_eq!(count(Token::TABLE_METHOD_SEMANTICS), 3);
        assert_eq!(header.sorted & (1 << Token::TABLE_PROPERTY_MAP), 0);
        assert_eq!(header.sorted & (1 << Token::TABLE_PROPERTY), 0);
        assert_ne!(header.sorted & (1 << Token::TABLE_METHOD_SEMANTICS), 0);

        // Everything in this tiny image is narrow (2-byte) indices.
        let str_w = if header.heap_sizes & 0x1 != 0 {
            4usize
        } else {
            2
        };
        let blob_w = if header.heap_sizes & 0x4 != 0 {
            4usize
        } else {
            2
        };
        assert_eq!(
            (str_w, blob_w),
            (2, 2),
            "tiny image should have narrow heaps"
        );

        // --- PropertyMap row: Parent = ISpeaker's TypeDef rid (2 — `<Module>` is rid 1),
        // PropertyList = 1 (the run starts at the first Property row).
        let pm_start = reader.table_offset(Token::TABLE_PROPERTY_MAP, &header);
        assert_eq!(
            u16_at(pm_start),
            2,
            "PropertyMap.Parent must be ISpeaker's TypeDef rid"
        );
        assert_eq!(
            u16_at(pm_start + 2),
            1,
            "PropertyMap.PropertyList must open the run at 1"
        );

        // --- Property rows: Flags=0, Name, PropertySig blob.
        let p_start = reader.table_offset(Token::TABLE_PROPERTY, &header);
        let p_w = MetadataReader::row_width(Token::TABLE_PROPERTY, &header);
        let blob_heap = reader.stream("#Blob");
        let prop_blob = |row: usize| {
            let sig_off = u16_at(p_start + row * p_w + 2 + str_w) as usize;
            let len = blob_heap[sig_off] as usize; // small blob: 1-byte compressed length
            &blob_heap[sig_off + 1..sig_off + 1 + len]
        };
        assert_eq!(u16_at(p_start), 0, "Property.Flags must be 0");
        assert_eq!(
            reader.strings_at(u16_at(p_start + 2).into()),
            "Volume",
            "Property row 1 must be named Volume"
        );
        assert_eq!(
            prop_blob(0),
            &[0x28, 0x00, 0x08],
            "Volume's PropertySig must be PROPERTY|HASTHIS, 0 params, ELEMENT_TYPE_I4"
        );
        assert_eq!(reader.strings_at(u16_at(p_start + p_w + 2).into()), "Name");
        assert_eq!(
            prop_blob(1),
            &[0x28, 0x00, 0x0E],
            "Name's PropertySig must be PROPERTY|HASTHIS, 0 params, ELEMENT_TYPE_STRING"
        );

        // --- MethodSemantics rows (sorted by Association; both Volume rows share association
        // `(1 << 1) | 1 = 3`, Name's is `(2 << 1) | 1 = 5`): Getter(0x2)+Setter(0x1) for Volume
        // (canonical MethodDef rids 2/3), Getter for Name (rid 1). The stable sort preserves the
        // getter-then-setter insertion order within one property.
        let ms_start = reader.table_offset(Token::TABLE_METHOD_SEMANTICS, &header);
        let ms_w = MetadataReader::row_width(Token::TABLE_METHOD_SEMANTICS, &header);
        let sem_row = |i: usize| {
            (
                u16_at(ms_start + i * ms_w),
                u16_at(ms_start + i * ms_w + 2),
                u16_at(ms_start + i * ms_w + 4),
            )
        };
        assert_eq!(
            sem_row(0),
            (0x2, 2, 3),
            "Getter(get_Volume) associated to Property rid 1"
        );
        assert_eq!(
            sem_row(1),
            (0x1, 3, 3),
            "Setter(set_Volume) associated to Property rid 1"
        );
        assert_eq!(
            sem_row(2),
            (0x2, 1, 5),
            "Getter(get_Name) associated to Property rid 2"
        );

        // --- Accessor MethodDef rows: RVA=0 and Public|Virtual|NewSlot|Abstract|SpecialName.
        let md_start = reader.table_offset(Token::TABLE_METHOD_DEF, &header);
        let md_w = MetadataReader::row_width(Token::TABLE_METHOD_DEF, &header);
        for rid in 1..=3usize {
            let row = md_start + (rid - 1) * md_w;
            assert_eq!(u32_at(row), 0, "accessor rid {rid} must have RVA=0");
            let flags = u16_at(row + 6);
            assert_eq!(
                flags,
                0x6 | 0x40 | 0x100 | 0x400 | 0x800,
                "accessor rid {rid} must be Public|Virtual|NewSlot|Abstract|SpecialName; \
                 flags={flags:#x}"
            );
        }
    }

    /// §II.22.35 `PropertyMap` runs are contiguous Property-table slices — interleaving two
    /// classes' property additions must fail loudly, exactly like
    /// `add_event_rejects_non_contiguous_per_class_event_runs`.
    #[test]
    #[should_panic(expected = "non-contiguous property addition")]
    fn add_property_rejects_non_contiguous_per_class_property_runs() {
        let mut mb = MetadataBuilder::new();
        let class_a = mb.add_type_def("", "A", false, None, None, None, &[]);
        let class_b = mb.add_type_def("", "B", false, None, None, None, &[]);
        let msig = mb.blobs.intern(&[0x20, 0x00, 0x08]);
        let psig = mb.blobs.intern(&[0x28, 0x00, 0x08]);
        let m: Vec<Token> = (0..3)
            .map(|i| {
                mb.add_method(
                    &format!("get_P{i}"),
                    msig,
                    &[],
                    &[],
                    false,
                    true,
                    false,
                    None,
                    false,
                    None,
                )
            })
            .collect();
        mb.add_property(class_a, "P0", psig, Some(m[0]), None);
        mb.add_property(class_b, "P1", psig, Some(m[1]), None);
        // Class A's run is closed (B's is the open tail) — must panic.
        mb.add_property(class_a, "P2", psig, Some(m[2]), None);
    }

    /// A **generic interface definition** (`#[dotnet_interface] trait IBox<T>` —
    /// `ClassDef::with_type_generic_names`): the PE writer must emit (a) the backtick-arity
    /// TypeDef name (`IBox`1`), (b) exactly one `GenericParam` row (§II.22.20) with
    /// `{Number=0, Flags=0, Owner=coded TypeOrMethodDef of the TypeDef, Name="T"}`, (c) the
    /// `Sorted` bitmask bit for table 0x2A, and (d) member signatures whose `T` positions are
    /// `ELEMENT_TYPE_VAR 0` with NO `SIG_GENERIC` convention bit (that is generic-METHOD-only,
    /// §II.23.2.1). Structural readback of `export_pe`'s actual bytes, same pattern as
    /// `interface_type_def_and_abstract_method_are_emitted_by_pe_writer`.
    #[test]
    fn generic_interface_type_def_emits_generic_param_rows() {
        use crate::ir::tpe::GenericKind;
        use crate::ir::{ClassDef, MethodDef, MethodImpl, Type};

        let mut asm = crate::ir::Assembly::default();
        let iname = asm.alloc_string("IBox`1");
        let tname = asm.alloc_string("T");
        let cdef = ClassDef::new(
            iname,
            false,
            1,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        )
        .with_interface()
        .with_type_generic_names(vec![tname]);
        let cidx = asm.class_def(cdef).unwrap();
        let self_ty = Type::ClassRef(*cidx);
        // `T Roundtrip(T value);` — the generic parameter in BOTH param and return position.
        let t_var = Type::PlatformGeneric(0, GenericKind::TypeGeneric);
        let mname = asm.alloc_string("Roundtrip");
        let msig = asm.sig([self_ty, t_var], t_var);
        let mdef = MethodDef::new(
            Access::Public,
            cidx,
            mname,
            msig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None, None],
        )
        .with_abstract();
        asm.new_method(mdef);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_generic_iface".to_string(),
            public_module_full_name: None,
            module_name: "pe_generic_iface.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());
        let count = |id: u32| {
            header
                .counts
                .iter()
                .find(|&&(t, _)| t == id)
                .map_or(0, |&(_, c)| c)
        };
        let str_w = if header.heap_sizes & 0x1 != 0 {
            4usize
        } else {
            2
        };
        let blob_w = if header.heap_sizes & 0x4 != 0 {
            4usize
        } else {
            2
        };
        let str_at = |o: usize| -> u32 {
            if str_w == 4 {
                u32_at(o)
            } else {
                u32::from(u16_at(o))
            }
        };

        // (d) IBox`1's TypeDef row (rid 2 — `<Module>` is rid 1): the Name column (after the
        // 4-byte Flags) must be the backtick-arity string, with an Interface-flagged row.
        let td_start = reader.table_offset(Token::TABLE_TYPE_DEF, &header);
        let td_w = MetadataReader::row_width(Token::TABLE_TYPE_DEF, &header);
        let iface_row = td_start + td_w; // rid 2
        let flags = u32_at(iface_row);
        assert_ne!(
            flags & 0x20,
            0,
            "TypeDef must have Interface (0x20); flags={flags:#x}"
        );
        assert_eq!(reader.strings_at(str_at(iface_row + 4)), "IBox`1");

        // (b) Exactly one GenericParam row: Number(2) + Flags(2) + Owner(coded TypeOrMethodDef,
        // 1 tag bit; TypeDef rid 2 -> (2 << 1) | 0) + Name -> "T".
        assert_eq!(count(Token::TABLE_GENERIC_PARAM), 1);
        let gp_start = reader.table_offset(Token::TABLE_GENERIC_PARAM, &header);
        assert_eq!(
            u16_at(gp_start),
            0,
            "Number must be 0 (the first declared parameter)"
        );
        assert_eq!(
            u16_at(gp_start + 2),
            0,
            "Flags must be 0 (no variance/constraints)"
        );
        let tomd_max = count(Token::TABLE_TYPE_DEF).max(count(Token::TABLE_METHOD_DEF));
        let owner_wide = tomd_max >= (1usize << 15);
        let owner = if owner_wide {
            u32_at(gp_start + 4)
        } else {
            u32::from(u16_at(gp_start + 4))
        };
        assert_eq!(
            owner,
            (2 << 1) | 0,
            "Owner must be coded TypeOrMethodDef(TypeDef rid 2)"
        );
        let owner_w = if owner_wide { 4 } else { 2 };
        assert_eq!(reader.strings_at(str_at(gp_start + 4 + owner_w)), "T");

        // (c) The `Sorted` bitmask must claim table 0x2A (GenericParam is a sorted table).
        assert_ne!(
            header.sorted & (1u64 << Token::TABLE_GENERIC_PARAM),
            0,
            "Sorted bit for GenericParam (0x2A) must be set"
        );

        // (a) Roundtrip's MethodDefSig blob: HASTHIS (0x20, and critically NOT SIG_GENERIC 0x10
        // — that bit is for generic METHOD definitions), 1 param, return ET_VAR 0, param
        // ET_VAR 0 — the receiver is implicit, never a parameter.
        let md_start = reader.table_offset(Token::TABLE_METHOD_DEF, &header);
        let sig_col = md_start + 8 + str_w;
        let sig_off = if blob_w == 4 {
            u32_at(sig_col)
        } else {
            u32::from(u16_at(sig_col))
        } as usize;
        let blob_heap = reader.stream("#Blob");
        let blob_len = blob_heap[sig_off] as usize;
        let sig_blob = &blob_heap[sig_off + 1..sig_off + 1 + blob_len];
        assert_eq!(
            sig_blob,
            &[0x20, 0x01, 0x13, 0x00, 0x13, 0x00],
            "sig must be HASTHIS (no SIG_GENERIC), 1 param, ET_VAR 0 return, ET_VAR 0 param"
        );
    }

    /// A **generic method definition** on a non-generic interface (`#[dotnet_interface]`'s
    /// `fn Echo<T>(&self, value: T) -> T` — `MethodDef::with_generic_params`): the PE writer
    /// must emit (a) exactly one METHOD-owned `GenericParam` row (§II.22.20) with
    /// `{Number=0, Flags=0, Owner=coded TypeOrMethodDef(MethodDef, tag 1), Name="T"}`,
    /// (b) a `MethodDefSig` blob whose convention is `HASTHIS|SIG_GENERIC` (0x30) followed by a
    /// compressed `GenParamCount` of 1 (§II.23.2.1), with the `T` positions encoded
    /// `ELEMENT_TYPE_MVAR 0` (`!!0` — `GenericKind::CallGeneric`, per `sig.rs`'s
    /// naming-crossover note), and (c) the abstract-member flags of any other interface member.
    /// The method-owner dual of `generic_interface_type_def_emits_generic_param_rows` above.
    #[test]
    fn generic_method_def_emits_method_owned_generic_param_row() {
        use crate::ir::tpe::GenericKind;
        use crate::ir::{ClassDef, MethodDef, MethodImpl, Type};

        let mut asm = crate::ir::Assembly::default();
        let iname = asm.alloc_string("IConverter");
        let cdef = ClassDef::new(
            iname,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        )
        .with_interface();
        let cidx = asm.class_def(cdef).unwrap();
        let self_ty = Type::ClassRef(*cidx);
        // `T Echo<T>(T value);` — the method's own parameter in BOTH param and return position.
        let t_mvar = Type::PlatformGeneric(0, GenericKind::CallGeneric);
        let mname = asm.alloc_string("Echo");
        let tname = asm.alloc_string("T");
        let msig = asm.sig([self_ty, t_mvar], t_mvar);
        let mdef = MethodDef::new(
            Access::Public,
            cidx,
            mname,
            msig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None, None],
        )
        .with_abstract()
        .with_generic_params(vec![tname]);
        asm.new_method(mdef);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_generic_method".to_string(),
            public_module_full_name: None,
            module_name: "pe_generic_method.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());
        let count = |id: u32| {
            header
                .counts
                .iter()
                .find(|&&(t, _)| t == id)
                .map_or(0, |&(_, c)| c)
        };
        let str_w = if header.heap_sizes & 0x1 != 0 {
            4usize
        } else {
            2
        };
        let blob_w = if header.heap_sizes & 0x4 != 0 {
            4usize
        } else {
            2
        };
        let str_at = |o: usize| -> u32 {
            if str_w == 4 {
                u32_at(o)
            } else {
                u32::from(u16_at(o))
            }
        };

        // (a) Exactly one GenericParam row, owned by Echo's MethodDef (rid 1 — the only method
        // in this assembly): Owner = (1 << 1) | 1 (coded TypeOrMethodDef, MethodDef tag).
        assert_eq!(count(Token::TABLE_GENERIC_PARAM), 1);
        let gp_start = reader.table_offset(Token::TABLE_GENERIC_PARAM, &header);
        assert_eq!(
            u16_at(gp_start),
            0,
            "Number must be 0 (the first declared parameter)"
        );
        assert_eq!(
            u16_at(gp_start + 2),
            0,
            "Flags must be 0 (no variance/constraints)"
        );
        let tomd_max = count(Token::TABLE_TYPE_DEF).max(count(Token::TABLE_METHOD_DEF));
        let owner_wide = tomd_max >= (1usize << 15);
        let owner = if owner_wide {
            u32_at(gp_start + 4)
        } else {
            u32::from(u16_at(gp_start + 4))
        };
        assert_eq!(
            owner,
            (1 << 1) | 1,
            "Owner must be coded TypeOrMethodDef(MethodDef rid 1)"
        );
        let owner_w = if owner_wide { 4 } else { 2 };
        assert_eq!(reader.strings_at(str_at(gp_start + 4 + owner_w)), "T");

        // (c) Echo's MethodDef row (rid 1): abstract virtual interface-member flags, and (b) its
        // sig blob: HASTHIS|SIG_GENERIC (0x30), GenParamCount 1, 1 param, return ET_MVAR 0,
        // param ET_MVAR 0 — the receiver stays implicit, exactly like a non-generic member.
        let md_start = reader.table_offset(Token::TABLE_METHOD_DEF, &header);
        let flags = u16_at(md_start + 6);
        assert_ne!(
            flags & 0x400,
            0,
            "MethodDef must be Abstract (0x400); flags={flags:#x}"
        );
        assert_ne!(
            flags & 0x40,
            0,
            "MethodDef must be Virtual (0x40); flags={flags:#x}"
        );
        let sig_col = md_start + 8 + str_w;
        let sig_off = if blob_w == 4 {
            u32_at(sig_col)
        } else {
            u32::from(u16_at(sig_col))
        } as usize;
        let blob_heap = reader.stream("#Blob");
        let blob_len = blob_heap[sig_off] as usize;
        let sig_blob = &blob_heap[sig_off + 1..sig_off + 1 + blob_len];
        assert_eq!(
            sig_blob,
            &[0x30, 0x01, 0x01, 0x1E, 0x00, 0x1E, 0x00],
            "sig must be HASTHIS|SIG_GENERIC, GenParamCount 1, 1 param, ET_MVAR 0 return, \
             ET_MVAR 0 param"
        );
    }

    /// An in-assembly INSTANTIATED reference to this assembly's own generic interface (e.g. an
    /// `IBoxHandle<i32>` parameter — `ClassRef("IBox`1", None, [int32])`): the signature encoder
    /// must resolve the open-type position to the definition's own **TypeDef** token via
    /// `find_open_generic_def`, NOT mint a dangling module-scope `TypeRef` with a doubled arity
    /// postfix (`IBox`1`1` — the pre-existing external-fallback behavior for unknown generic
    /// refs). Expected blob: `GENERICINST CLASS <TypeDef-coded> 1 I4` (§II.23.2.12).
    #[test]
    fn instantiated_in_assembly_generic_interface_resolves_to_its_type_def() {
        use crate::ir::{BasicBlock, CILRoot, ClassDef, ClassRef, MethodDef, MethodImpl, Type};

        let mut asm = crate::ir::Assembly::default();
        let iname = asm.alloc_string("IBox`1");
        let tname = asm.alloc_string("T");
        let cdef = ClassDef::new(
            iname,
            false,
            1,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        )
        .with_interface()
        .with_type_generic_names(vec![tname]);
        let cidx = asm.class_def(cdef).unwrap();
        // `static void UseBox(IBox<int> box)` — a plain static (interfaces may carry static
        // non-virtual methods with bodies), so the interface stays the only TypeDef (rid 2,
        // deterministic — class-def iteration is hash-ordered with 2+ classes).
        let inst = asm.alloc_class_ref(ClassRef::new(
            iname,
            None,
            false,
            [Type::Int(Int::I32)].into(),
        ));
        let mname = asm.alloc_string("UseBox");
        let msig = asm.sig([Type::ClassRef(inst)], Type::Void);
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let mdef = MethodDef::new(
            Access::Public,
            cidx,
            mname,
            msig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![None],
        );
        asm.new_method(mdef);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_generic_iface_inst".to_string(),
            public_module_full_name: None,
            module_name: "pe_generic_iface_inst.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());
        let count = |id: u32| {
            header
                .counts
                .iter()
                .find(|&&(t, _)| t == id)
                .map_or(0, |&(_, c)| c)
        };
        let str_w = if header.heap_sizes & 0x1 != 0 {
            4usize
        } else {
            2
        };
        let blob_w = if header.heap_sizes & 0x4 != 0 {
            4usize
        } else {
            2
        };

        // The tell-tale failure shape would be a module-scope TypeRef named `IBox`1`1` — the
        // fixed resolver never creates ANY TypeRef for this reference. This minimal image
        // (interface-only, no extends bootstrap) carries NO TypeRef rows at all; guard the scan
        // since `table_offset` panics on an absent table.
        if count(Token::TABLE_TYPE_REF) > 0 {
            let tr_start = reader.table_offset(Token::TABLE_TYPE_REF, &header);
            let tr_w = MetadataReader::row_width(Token::TABLE_TYPE_REF, &header);
            let scope_max = count(Token::TABLE_MODULE)
                .max(count(Token::TABLE_MODULE_REF))
                .max(count(Token::TABLE_ASSEMBLY_REF))
                .max(count(Token::TABLE_TYPE_REF));
            let scope_w: usize = if scope_max >= (1usize << 14) { 4 } else { 2 };
            for rid0 in 0..count(Token::TABLE_TYPE_REF) {
                let name_col = tr_start + rid0 * tr_w + scope_w;
                let name_off = if str_w == 4 {
                    u32_at(name_col)
                } else {
                    u32::from(u16_at(name_col))
                };
                assert!(
                    !reader.strings_at(name_off).contains('`'),
                    "no TypeRef may carry a backtick arity here — the instantiated in-assembly \
                     generic must resolve to the open def's TypeDef, got TypeRef {:?}",
                    reader.strings_at(name_off)
                );
            }
        }

        // UseBox's MethodDefSig blob: SIG_DEFAULT (static), 1 param, void return, then
        // `GENERICINST CLASS <coded TypeDef rid 2 = (2<<2)|0 = 0x08> argc=1 ET_I4`.
        let md_start = reader.table_offset(Token::TABLE_METHOD_DEF, &header);
        let sig_col = md_start + 8 + str_w;
        let sig_off = if blob_w == 4 {
            u32_at(sig_col)
        } else {
            u32::from(u16_at(sig_col))
        } as usize;
        let blob_heap = reader.stream("#Blob");
        let blob_len = blob_heap[sig_off] as usize;
        let sig_blob = &blob_heap[sig_off + 1..sig_off + 1 + blob_len];
        assert_eq!(
            sig_blob,
            &[0x00, 0x01, 0x01, 0x15, 0x12, 0x08, 0x01, 0x08],
            "sig must encode GENERICINST CLASS <TypeDef IBox`1> <int32>"
        );
    }

    /// A **default interface method** (DIM, CoreCLR 3.0+ — `#[dotnet_interface]` trait fn with a
    /// default body): a virtual, NON-abstract `MethodDef` with a real IL body on the interface
    /// `TypeDef`. The PE writer must emit it with `Virtual` (0x40) and `NewSlot` (0x100) set,
    /// `Abstract` (0x400) CLEAR, and a non-zero RVA (Roslyn's DIM shape is 0x1C6 =
    /// `Public|Virtual|HideBySig|NewSlot`; we emit 0x146 — HideBySig omission is proven-tolerated
    /// for our abstract members and events) — while an abstract sibling on the same interface
    /// keeps `Abstract` set and RVA=0. Structural readback of `export_pe`'s actual bytes, same
    /// pattern as `interface_type_def_and_abstract_method_are_emitted_by_pe_writer`.
    #[test]
    fn default_interface_method_gets_body_and_stays_nonabstract() {
        use crate::ir::{
            BasicBlock, CILNode, CILRoot, ClassDef, Const, MethodDef, MethodImpl, Type,
        };

        let mut asm = crate::ir::Assembly::default();
        let iname = asm.alloc_string("ICalc");
        let cdef = ClassDef::new(
            iname,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        )
        .with_interface();
        let cidx = asm.class_def(cdef).unwrap();
        let self_ty = Type::ClassRef(*cidx);

        // Row 1: `int Base();` — an ordinary ABSTRACT member (the sibling that must stay RVA=0).
        let base_name = asm.alloc_string("Base");
        let msig = asm.sig([self_ty], Type::Int(Int::I32));
        let base_def = MethodDef::new(
            Access::Public,
            cidx,
            base_name,
            msig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None],
        )
        .with_abstract();
        asm.new_method(base_def);

        // Row 2: `int Fixed() => 7;` — the DIM: Virtual, NOT abstract, with a real body.
        let dim_name = asm.alloc_string("Fixed");
        let seven = asm.alloc_node(CILNode::Const(Box::new(Const::I32(7))));
        let ret = asm.alloc_root(CILRoot::Ret(seven));
        let dim_def = MethodDef::new(
            Access::Public,
            cidx,
            dim_name,
            msig,
            MethodKind::Virtual,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![None],
        );
        asm.new_method(dim_def);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_dim".to_string(),
            public_module_full_name: None,
            module_name: "pe_dim.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());

        // MethodDef rows: RVA(4) + ImplFlags(2) + Flags(2) + …; rid 1 = Base, rid 2 = Fixed
        // (insertion order).
        let md_start = reader.table_offset(Token::TABLE_METHOD_DEF, &header);
        let md_w = MetadataReader::row_width(Token::TABLE_METHOD_DEF, &header);

        // Abstract sibling: RVA=0, Abstract set — unharmed by the DIM next to it.
        assert_eq!(u32_at(md_start), 0, "abstract sibling must keep RVA=0");
        let base_flags = u16_at(md_start + 6);
        assert_ne!(
            base_flags & 0x400,
            0,
            "abstract sibling must keep Abstract (0x400); flags={base_flags:#x}"
        );

        // The DIM: a real body (RVA != 0), Virtual|NewSlot, NOT Abstract.
        let dim_row = md_start + md_w;
        assert_ne!(
            u32_at(dim_row),
            0,
            "DIM must have a non-zero RVA (a real IL body)"
        );
        let dim_flags = u16_at(dim_row + 6);
        assert_eq!(
            dim_flags & 0x400,
            0,
            "DIM must NOT be Abstract (0x400); flags={dim_flags:#x}"
        );
        assert_ne!(
            dim_flags & 0x40,
            0,
            "DIM must be Virtual (0x40); flags={dim_flags:#x}"
        );
        assert_ne!(
            dim_flags & 0x100,
            0,
            "DIM must be NewSlot (0x100); flags={dim_flags:#x}"
        );
    }

    /// A `ref`/`out` parameter on an abstract interface member (`#[dotnet_interface]` + `&mut T`
    /// / `#[dotnet_out]`): the PE writer must encode the parameter as `ELEMENT_TYPE_BYREF` (0x10)
    /// in the `MethodDefSig` blob (§II.23.2.10 `Param ::= [BYREF] Type`) and stamp
    /// `ParamAttributes.Out` (0x0002, §II.23.1.13) on the `Param` row named by
    /// `MethodDef::out_params` — the exact metadata shape csc reads back as `out T` (BYREF with
    /// `Flags == 0` reads back as `ref T`). Structural readback of `export_pe`'s actual bytes,
    /// same pattern as `interface_type_def_and_abstract_method_are_emitted_by_pe_writer`.
    #[test]
    fn byref_out_param_on_abstract_member_is_emitted_by_pe_writer() {
        use crate::ir::{ClassDef, MethodDef, MethodImpl, Type};

        let mut asm = crate::ir::Assembly::default();
        let iname = asm.alloc_string("IRefCell");
        let cdef = ClassDef::new(
            iname,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        )
        .with_interface();
        let cidx = asm.class_def(cdef).unwrap();
        let self_ty = Type::ClassRef(*cidx);
        // `void FillOut(out int slot);` — receiver + one byref-int param flagged `[out]`.
        let byref_i32 = asm.nref(Type::Int(Int::I32));
        let mname = asm.alloc_string("FillOut");
        let msig = asm.sig([self_ty, byref_i32], Type::Void);
        let mdef = MethodDef::new(
            Access::Public,
            cidx,
            mname,
            msig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None, None],
        )
        .with_abstract()
        .with_out_params(vec![1]);
        asm.new_method(mdef);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_byref_out".to_string(),
            public_module_full_name: None,
            module_name: "pe_byref_out.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());

        // --- FillOut's MethodDef row (rid 1, the only method). Row layout: RVA(4) + ImplFlags(2)
        // + Flags(2) + Name(str) + Signature(blob) + ParamList(simple).
        let md_start = reader.table_offset(Token::TABLE_METHOD_DEF, &header);
        assert_eq!(u32_at(md_start), 0, "abstract member must have RVA=0");
        let str_w = if header.heap_sizes & 0x1 != 0 {
            4usize
        } else {
            2
        };
        let blob_w = if header.heap_sizes & 0x4 != 0 {
            4usize
        } else {
            2
        };
        let sig_col = md_start + 8 + str_w;
        let sig_off = if blob_w == 4 {
            u32_at(sig_col)
        } else {
            u32::from(u16_at(sig_col))
        } as usize;
        let blob_heap = reader.stream("#Blob");
        // Small blob (< 0x80 bytes): a single compressed-length byte, then the blob data.
        let blob_len = blob_heap[sig_off] as usize;
        let sig_blob = &blob_heap[sig_off + 1..sig_off + 1 + blob_len];
        assert_eq!(
            sig_blob,
            &[0x20, 0x01, 0x01, 0x10, 0x08],
            "sig must be HASTHIS (0x20), 1 param, ELEMENT_TYPE_VOID ret (0x01), then \
             ELEMENT_TYPE_BYREF (0x10) ELEMENT_TYPE_I4 (0x08)"
        );

        // --- The Param row (rid 1, the only param): Flags(2) + Sequence(2) + Name(str).
        let p_start = reader.table_offset(Token::TABLE_PARAM, &header);
        assert_eq!(
            u16_at(p_start),
            0x0002,
            "the `#[dotnet_out]` Param row must carry ParamAttributes.Out (0x0002)"
        );
        assert_eq!(u16_at(p_start + 2), 1, "Param Sequence must be 1");
    }

    /// An EVENT declared on an interface (`#[dotnet_interface]` + `#[dotnet_event]`): the PE
    /// writer must emit the interface's abstract `add_*`/`remove_*` accessor `MethodDef`s
    /// (Abstract|Virtual|SpecialName, RVA=0) plus the §II.22.12/13/28 `EventMap`/`Event`/
    /// `MethodSemantics` rows binding them — structural readback of `export_pe`'s actual bytes,
    /// same pattern as `interface_type_def_and_abstract_method_are_emitted_by_pe_writer`.
    #[test]
    fn interface_event_rows_are_emitted_by_pe_writer() {
        use crate::ir::class::EventDef;
        use crate::ir::{ClassDef, ClassRef, MethodDef, MethodImpl, Type};

        let mut asm = crate::ir::Assembly::default();
        let iname = asm.alloc_string("IButton");
        let cdef = ClassDef::new(
            iname,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        )
        .with_interface();
        let cidx = asm.class_def(cdef).unwrap();
        let self_ty = Type::ClassRef(*cidx);
        // The delegate type subscribers must match — an EXTERNAL class ref (becomes a TypeRef).
        let action_name = asm.alloc_string("System.Action");
        let sysrt = asm.alloc_string("System.Runtime");
        let action = asm.alloc_class_ref(ClassRef::new(action_name, Some(sysrt), false, [].into()));
        let delegate_ty = Type::ClassRef(action);
        let acc_sig = asm.sig([self_ty, delegate_ty], Type::Void);
        // Both accessors: abstract virtual instance members (no bodies, RVA stays 0).
        let add_name = asm.alloc_string("add_Clicked");
        let add_def = MethodDef::new(
            Access::Public,
            cidx,
            add_name,
            acc_sig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None, None],
        )
        .with_abstract();
        let add_mref = asm.alloc_methodref(add_def.ref_to());
        asm.new_method(add_def);
        let remove_name = asm.alloc_string("remove_Clicked");
        let remove_def = MethodDef::new(
            Access::Public,
            cidx,
            remove_name,
            acc_sig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            vec![None, None],
        )
        .with_abstract();
        let remove_mref = asm.alloc_methodref(remove_def.ref_to());
        asm.new_method(remove_def);
        let ev_name = asm.alloc_string("Clicked");
        asm.class_mut(cidx)
            .add_event(EventDef::new(ev_name, delegate_ty, add_mref, remove_mref));

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_iface_event".to_string(),
            public_module_full_name: None,
            module_name: "pe_iface_event.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());
        let count = |id: u32| {
            header
                .counts
                .iter()
                .find(|&&(t, _)| t == id)
                .map_or(0, |&(_, c)| c)
        };
        let str_w = if header.heap_sizes & 0x1 != 0 { 4 } else { 2 };
        let idx_at = |o: usize, wide: bool| {
            if wide {
                u32_at(o)
            } else {
                u32::from(u16_at(o))
            }
        };

        // --- The accessor MethodDef rows (rids 1 and 2 — the only methods in the assembly).
        // RVA is the first 4 bytes; Flags is a u16 at offset 6 (after RVA(4) + ImplFlags(2)).
        let md_start = reader.table_offset(Token::TABLE_METHOD_DEF, &header);
        let md_w = MetadataReader::row_width(Token::TABLE_METHOD_DEF, &header);
        assert_eq!(
            count(Token::TABLE_METHOD_DEF),
            2,
            "exactly the two accessors"
        );
        for (rid0, which) in [(0usize, "add_Clicked"), (1usize, "remove_Clicked")] {
            let row = md_start + rid0 * md_w;
            assert_eq!(u32_at(row), 0, "{which}: abstract accessor must have RVA=0");
            let flags = u16_at(row + 6);
            assert_ne!(
                flags & 0x400,
                0,
                "{which}: Abstract (0x400) missing; flags={flags:#x}"
            );
            assert_ne!(
                flags & 0x40,
                0,
                "{which}: Virtual (0x40) missing; flags={flags:#x}"
            );
            assert_ne!(
                flags & 0x800,
                0,
                "{which}: event accessors must be SpecialName (0x800, §II.10.4); flags={flags:#x}"
            );
        }

        // --- EventMap: one row, parent = IButton's TypeDef (rid 2 — `<Module>` is rid 1),
        // run-start = Event rid 1. Columns: Parent (TypeDef simple index) + EventList (Event
        // simple index), both narrow here.
        assert_eq!(count(Token::TABLE_EVENT_MAP), 1);
        let em_start = reader.table_offset(Token::TABLE_EVENT_MAP, &header);
        let td_wide = count(Token::TABLE_TYPE_DEF) > 0xFFFF;
        let ev_wide = count(Token::TABLE_EVENT) > 0xFFFF;
        let em_parent = idx_at(em_start, td_wide);
        assert_eq!(
            em_parent, 2,
            "EventMap.Parent must be IButton's TypeDef rid"
        );
        let em_list = idx_at(em_start + if td_wide { 4 } else { 2 }, ev_wide);
        assert_eq!(
            em_list, 1,
            "EventMap.EventList must open the run at Event rid 1"
        );

        // --- Event: one row: EventFlags(u16)=0, Name -> "Clicked", EventType = TypeDefOrRef
        // coded index with the TypeRef tag (1) — the System.Action delegate is external.
        assert_eq!(count(Token::TABLE_EVENT), 1);
        let e_start = reader.table_offset(Token::TABLE_EVENT, &header);
        assert_eq!(u16_at(e_start), 0, "EventFlags must be 0 (ordinary event)");
        let name_off = idx_at(e_start + 2, str_w == 4);
        assert_eq!(reader.strings_at(name_off), "Clicked");
        let tdor_max = count(Token::TABLE_TYPE_DEF)
            .max(count(Token::TABLE_TYPE_REF))
            .max(count(Token::TABLE_TYPE_SPEC));
        let tdor_wide = tdor_max >= (1usize << 14);
        let event_type = idx_at(e_start + 2 + str_w, tdor_wide);
        assert_eq!(
            event_type & 0x3,
            1,
            "EventType must carry the TypeRef tag (delegate is external)"
        );

        // --- MethodSemantics: two rows, sorted by Association (both share the one Event, so
        // insertion order add-then-remove is preserved): Semantics(u16) + Method (MethodDef
        // simple index) + Association (HasSemantics coded: Event rid 1, tag 0 -> 0b10 == 2).
        assert_eq!(count(Token::TABLE_METHOD_SEMANTICS), 2);
        let ms_start = reader.table_offset(Token::TABLE_METHOD_SEMANTICS, &header);
        let ms_w = MetadataReader::row_width(Token::TABLE_METHOD_SEMANTICS, &header);
        let md_wide = count(Token::TABLE_METHOD_DEF) > 0xFFFF;
        for (i, (sem, acc_rid, which)) in [
            (0usize, (0x8u16, 1u32, "AddOn->add_Clicked")),
            (1, (0x10, 2, "RemoveOn->remove_Clicked")),
        ] {
            let row = ms_start + i * ms_w;
            assert_eq!(u16_at(row), sem, "{which}: wrong Semantics value");
            let method = idx_at(row + 2, md_wide);
            assert_eq!(method, acc_rid, "{which}: wrong accessor MethodDef rid");
            let assoc = idx_at(row + 2 + if md_wide { 4 } else { 2 }, false);
            assert_eq!(
                assoc, 2,
                "{which}: Association must be Event rid 1 (coded 0b10)"
            );
        }
    }

    /// INTERFACE INHERITANCE (`#[dotnet_interface] trait IDerived: IBase`): ECMA-335 models
    /// `interface IDerived : IBase` as an `InterfaceImpl` row (§II.22.23) on IDerived's own
    /// interface `TypeDef` (its `Extends` stays NIL, §II.10.1.3). Both types live in the SAME
    /// assembly, so the row's `Interface` coded index must resolve to IBase's `TypeDef` (tag 0),
    /// not a `TypeRef` — this exercises `export_pe`'s Pass 1.5 (all TypeDef rows exist before any
    /// `implements` is resolved, making the same-assembly forward reference order-independent).
    #[test]
    fn interface_inheritance_emits_interface_impl_on_the_interface_type_def() {
        use crate::ir::ClassDef;

        let mut asm = crate::ir::Assembly::default();
        let base_name = asm.alloc_string("IBase");
        let base_idx = asm
            .class_def(
                ClassDef::new(
                    base_name,
                    false,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Public,
                    None,
                    None,
                    true,
                )
                .with_interface(),
            )
            .unwrap();
        let derived_name = asm.alloc_string("IDerived");
        let derived_idx = asm
            .class_def(
                ClassDef::new(
                    derived_name,
                    false,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Public,
                    None,
                    None,
                    true,
                )
                .with_interface(),
            )
            .unwrap();
        // The base-interface reference is the SAME-ASSEMBLY ClassRef IBase's own def registered.
        asm.class_mut(derived_idx).add_interface(*base_idx);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_iface_inherit".to_string(),
            public_module_full_name: None,
            module_name: "pe_iface_inherit.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = super::super::export::export_pe(&mut asm, &options);

        let bsjb = image
            .windows(4)
            .position(|w| w == b"BSJB")
            .expect("PE image must contain a BSJB metadata root");
        let reader = MetadataReader::parse(&image[bsjb..]);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let u16_at = |o: usize| u16::from_le_bytes(row_bytes[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(row_bytes[o..o + 4].try_into().unwrap());
        let count = |id: u32| {
            header
                .counts
                .iter()
                .find(|&&(t, _)| t == id)
                .map_or(0, |&(_, c)| c)
        };
        let idx_at = |o: usize, wide: bool| {
            if wide {
                u32_at(o)
            } else {
                u32::from(u16_at(o))
            }
        };
        let str_w = if header.heap_sizes & 0x1 != 0 {
            4usize
        } else {
            2
        };

        // Map TypeDef rids to names (`class_def_ids` is a hash-order snapshot, so whether IBase
        // or IDerived gets the lower rid is arbitrary — resolve by name, don't assume).
        let td_start = reader.table_offset(Token::TABLE_TYPE_DEF, &header);
        let td_w = MetadataReader::row_width(Token::TABLE_TYPE_DEF, &header);
        let td_count = count(Token::TABLE_TYPE_DEF);
        let td_name = |rid: u32| {
            let row = td_start + (rid as usize - 1) * td_w;
            reader.strings_at(idx_at(row + 4, str_w == 4))
        };
        let rid_of = |name: &str| {
            (1..=td_count as u32)
                .find(|&rid| td_name(rid) == name)
                .unwrap_or_else(|| panic!("no TypeDef named {name}"))
        };
        let base_rid = rid_of("IBase");
        let derived_rid = rid_of("IDerived");

        // Exactly ONE InterfaceImpl row: Class = IDerived's TypeDef rid, Interface = a
        // TypeDefOrRef coded index with the TypeDef tag (0) decoding to IBase's rid.
        assert_eq!(count(Token::TABLE_INTERFACE_IMPL), 1);
        let ii_start = reader.table_offset(Token::TABLE_INTERFACE_IMPL, &header);
        let td_wide = td_count > 0xFFFF;
        let class = idx_at(ii_start, td_wide);
        assert_eq!(
            class, derived_rid,
            "InterfaceImpl.Class must be IDerived's TypeDef rid"
        );
        let tdor_max = count(Token::TABLE_TYPE_DEF)
            .max(count(Token::TABLE_TYPE_REF))
            .max(count(Token::TABLE_TYPE_SPEC));
        let tdor_wide = tdor_max >= (1usize << 14);
        let iface = idx_at(ii_start + if td_wide { 4 } else { 2 }, tdor_wide);
        assert_eq!(
            iface & 0x3,
            0,
            "Interface must carry the TypeDef tag (same assembly)"
        );
        assert_eq!(
            iface >> 2,
            base_rid,
            "Interface must decode to IBase's TypeDef rid"
        );

        // Both TypeDefs are genuine interfaces; IDerived's Extends stays NIL despite the base.
        for rid in [base_rid, derived_rid] {
            let row = td_start + (rid as usize - 1) * td_w;
            let flags = u32_at(row);
            assert_ne!(flags & 0x20, 0, "{}: Interface flag missing", td_name(rid));
            let extends = idx_at(row + 4 + 2 * str_w, tdor_wide);
            assert_eq!(
                extends,
                0,
                "{}: interface Extends must be NIL",
                td_name(rid)
            );
        }
    }

    /// Pass 1.5's fail-loudly validation: a SAME-ASSEMBLY, non-generic `implements` target that
    /// resolves to no registered class def (e.g. `#[dotnet_interface] trait X: Clone` — `Clone`
    /// is not a .NET interface) must be an export-time panic naming both types, never a dangling
    /// module-scope `TypeRef`.
    #[test]
    #[should_panic(expected = "not a type defined in this assembly")]
    fn same_assembly_implements_of_an_unregistered_type_panics() {
        use crate::ir::{ClassDef, ClassRef};

        let mut asm = crate::ir::Assembly::default();
        let cls_name = asm.alloc_string("Foo");
        let cls_idx = asm
            .class_def(ClassDef::new(
                cls_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap();
        let missing_name = asm.alloc_string("Clone");
        let missing = asm.alloc_class_ref(ClassRef::new(missing_name, None, false, [].into()));
        asm.class_mut(cls_idx).add_interface(missing);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_iface_missing".to_string(),
            public_module_full_name: None,
            module_name: "pe_iface_missing.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let _ = super::super::export::export_pe(&mut asm, &options);
    }

    /// Pass 1.5's fail-loudly validation, second arm: an `implements` target that resolves to a
    /// registered class def which is NOT `is_interface` would be a CLR load-time
    /// `TypeLoadException` — turn it into an export-time panic instead.
    #[test]
    #[should_panic(expected = "is a class, not an interface")]
    fn same_assembly_implements_of_a_non_interface_class_panics() {
        use crate::ir::ClassDef;

        let mut asm = crate::ir::Assembly::default();
        let base_name = asm.alloc_string("NotAnIface");
        let base_idx = asm
            .class_def(ClassDef::new(
                base_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap();
        let cls_name = asm.alloc_string("Foo");
        let cls_idx = asm
            .class_def(ClassDef::new(
                cls_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap();
        asm.class_mut(cls_idx).add_interface(*base_idx);

        let options = super::super::export::ExportOptions {
            runtime: DotnetRuntime::Net8,
            is_dll: true,
            assembly_name: "pe_iface_notiface".to_string(),
            public_module_full_name: None,
            module_name: "pe_iface_notiface.dll".to_string(),
            pdb_file_name: String::new(),
        };
        let _ = super::super::export::export_pe(&mut asm, &options);
    }

    // ---------------------------------------------------------------------------------------
    // (a) A tiny module (one TypeDef, one MethodDef, one MemberRef, one string) round-trips
    // through a hand-rolled reader that parses the metadata root back apart.
    // ---------------------------------------------------------------------------------------

    /// Minimal test-only BSJB reader: enough to assert stream offsets/sizes, `#~` header fields,
    /// row widths, and decode specific rows — NOT a general-purpose metadata parser.
    struct MetadataReader<'a> {
        bytes: &'a [u8],
        streams: HashMap<String, (usize, usize)>,
    }

    impl<'a> MetadataReader<'a> {
        fn parse(bytes: &'a [u8]) -> Self {
            assert_eq!(&bytes[0..4], b"BSJB", "magic");
            let major = u16::from_le_bytes([bytes[4], bytes[5]]);
            let minor = u16::from_le_bytes([bytes[6], bytes[7]]);
            assert_eq!((major, minor), (1, 1));
            // Layout (§II.24.2.1): Signature(4) MajorVersion(2) MinorVersion(2) Reserved(4)
            // Length(4) Version(Length bytes) — the version string starts at offset 16, not 20;
            // `Reserved` occupies 8..12 and `Length` occupies 12..16.
            let version_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            let version_bytes = &bytes[16..16 + version_len];
            let nul = version_bytes.iter().position(|&b| b == 0).unwrap();
            assert_eq!(&version_bytes[..nul], b"v4.0.30319");

            let mut cursor = 16 + version_len;
            cursor += 2; // Flags
            let n_streams = u16::from_le_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
            cursor += 2;

            let mut streams = HashMap::new();
            for _ in 0..n_streams {
                let offset =
                    u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
                let size =
                    u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
                cursor += 8;
                let name_start = cursor;
                let name_end =
                    bytes[name_start..].iter().position(|&b| b == 0).unwrap() + name_start;
                let name = std::str::from_utf8(&bytes[name_start..name_end])
                    .unwrap()
                    .to_string();
                let mut name_len = name_end - name_start + 1;
                while name_len % 4 != 0 {
                    name_len += 1;
                }
                cursor = name_start + name_len;
                streams.insert(name, (offset, size));
            }
            Self { bytes, streams }
        }

        fn stream(&self, name: &str) -> &'a [u8] {
            let (off, size) = self.streams[name];
            &self.bytes[off..off + size]
        }

        /// Byte width of one row of `table_id`, given whether each of its columns is a wide
        /// (4-byte) or narrow (2-byte) index — mirrors this module's own `write_*_rows` column
        /// shapes exactly (kept in one place so tests don't hand-duplicate per-table byte math
        /// that silently drifts from the writer).
        fn row_width(table_id: u32, header: &TablesHeader) -> usize {
            let w = |wide: bool| if wide { 4 } else { 2 };
            let hs = header.heap_sizes;
            let str_w = w(hs & 0x1 != 0);
            let blob_w = w(hs & 0x4 != 0);
            let guid_w = w(hs & 0x2 != 0);
            let count = |id: u32| {
                header
                    .counts
                    .iter()
                    .find(|&&(t, _)| t == id)
                    .map_or(0, |&(_, c)| c)
            };
            let simple_w = |rows: usize| w(rows > 0xFFFF);
            // Mirrors this module's own `coded_wide` exactly (§II.24.2.6: `>=`, not `>` — see its
            // doc comment for why the threshold row count itself already needs a wide column).
            let coded_w =
                |tag_bits: u32, max_rows: usize| w(max_rows >= (1usize << (16 - tag_bits)));

            match table_id {
                Token::TABLE_MODULE => 2 + str_w + 3 * guid_w,
                Token::TABLE_TYPE_REF => {
                    let scope_max = count(Token::TABLE_MODULE)
                        .max(count(Token::TABLE_MODULE_REF))
                        .max(count(Token::TABLE_ASSEMBLY_REF))
                        .max(count(Token::TABLE_TYPE_REF));
                    coded_w(2, scope_max) + 2 * str_w
                }
                Token::TABLE_TYPE_DEF => {
                    let tdor_max = count(Token::TABLE_TYPE_DEF)
                        .max(count(Token::TABLE_TYPE_REF))
                        .max(count(Token::TABLE_TYPE_SPEC));
                    4 + 2 * str_w
                        + coded_w(2, tdor_max)
                        + simple_w(count(Token::TABLE_FIELD))
                        + simple_w(count(Token::TABLE_METHOD_DEF))
                }
                Token::TABLE_FIELD => 2 + str_w + blob_w,
                Token::TABLE_METHOD_DEF => {
                    4 + 2 + 2 + str_w + blob_w + simple_w(count(Token::TABLE_PARAM))
                }
                Token::TABLE_PARAM => 2 + 2 + str_w,
                Token::TABLE_INTERFACE_IMPL => {
                    let tdor_max = count(Token::TABLE_TYPE_DEF)
                        .max(count(Token::TABLE_TYPE_REF))
                        .max(count(Token::TABLE_TYPE_SPEC));
                    simple_w(count(Token::TABLE_TYPE_DEF)) + coded_w(2, tdor_max)
                }
                Token::TABLE_MEMBER_REF => {
                    let mrp_max = count(Token::TABLE_TYPE_DEF)
                        .max(count(Token::TABLE_TYPE_REF))
                        .max(count(Token::TABLE_MODULE_REF))
                        .max(count(Token::TABLE_METHOD_DEF))
                        .max(count(Token::TABLE_TYPE_SPEC));
                    coded_w(3, mrp_max) + str_w + blob_w
                }
                Token::TABLE_CONSTANT => {
                    let hc_max = count(Token::TABLE_FIELD)
                        .max(count(Token::TABLE_PARAM))
                        .max(count(Token::TABLE_PROPERTY));
                    2 + coded_w(2, hc_max) + blob_w
                }
                Token::TABLE_CUSTOM_ATTRIBUTE => {
                    // Must mirror `Widths::new`'s `has_custom_attribute_max` universe — every
                    // `HasCustomAttribute` target table this backend can populate (§II.24.2.6).
                    let hca_max = count(Token::TABLE_METHOD_DEF)
                        .max(count(Token::TABLE_FIELD))
                        .max(count(Token::TABLE_TYPE_REF))
                        .max(count(Token::TABLE_TYPE_DEF))
                        .max(count(Token::TABLE_PARAM))
                        .max(count(Token::TABLE_INTERFACE_IMPL))
                        .max(count(Token::TABLE_MEMBER_REF))
                        .max(count(Token::TABLE_MODULE))
                        .max(count(Token::TABLE_ASSEMBLY))
                        .max(count(Token::TABLE_ASSEMBLY_REF))
                        .max(count(Token::TABLE_TYPE_SPEC))
                        .max(count(Token::TABLE_GENERIC_PARAM))
                        .max(count(Token::TABLE_EVENT))
                        .max(count(Token::TABLE_PROPERTY))
                        .max(count(Token::TABLE_STAND_ALONE_SIG))
                        .max(count(Token::TABLE_MODULE_REF))
                        .max(count(Token::TABLE_METHOD_SPEC));
                    let cat_max =
                        count(Token::TABLE_METHOD_DEF).max(count(Token::TABLE_MEMBER_REF));
                    coded_w(5, hca_max) + coded_w(3, cat_max) + blob_w
                }
                Token::TABLE_CLASS_LAYOUT => 2 + 4 + simple_w(count(Token::TABLE_TYPE_DEF)),
                Token::TABLE_FIELD_LAYOUT => 4 + simple_w(count(Token::TABLE_FIELD)),
                Token::TABLE_STAND_ALONE_SIG => blob_w,
                Token::TABLE_EVENT_MAP => {
                    simple_w(count(Token::TABLE_TYPE_DEF)) + simple_w(count(Token::TABLE_EVENT))
                }
                Token::TABLE_EVENT => {
                    let tdor_max = count(Token::TABLE_TYPE_DEF)
                        .max(count(Token::TABLE_TYPE_REF))
                        .max(count(Token::TABLE_TYPE_SPEC));
                    2 + str_w + coded_w(2, tdor_max)
                }
                Token::TABLE_PROPERTY_MAP => {
                    simple_w(count(Token::TABLE_TYPE_DEF)) + simple_w(count(Token::TABLE_PROPERTY))
                }
                Token::TABLE_PROPERTY => 2 + str_w + blob_w,
                Token::TABLE_METHOD_SEMANTICS => {
                    // Association is a `HasSemantics` coded index (Event | Property, 1 tag bit);
                    // the larger of the two target tables bounds the width.
                    2 + simple_w(count(Token::TABLE_METHOD_DEF))
                        + coded_w(
                            1,
                            count(Token::TABLE_EVENT).max(count(Token::TABLE_PROPERTY)),
                        )
                }
                Token::TABLE_METHOD_IMPL => {
                    let mdor_max =
                        count(Token::TABLE_METHOD_DEF).max(count(Token::TABLE_MEMBER_REF));
                    simple_w(count(Token::TABLE_TYPE_DEF)) + 2 * coded_w(1, mdor_max)
                }
                Token::TABLE_MODULE_REF => str_w,
                Token::TABLE_TYPE_SPEC => blob_w,
                Token::TABLE_IMPL_MAP => {
                    let mf_max = count(Token::TABLE_FIELD).max(count(Token::TABLE_METHOD_DEF));
                    2 + coded_w(1, mf_max) + str_w + simple_w(count(Token::TABLE_MODULE_REF))
                }
                Token::TABLE_FIELD_RVA => 4 + simple_w(count(Token::TABLE_FIELD)),
                // HashAlgId(4) + 4×Version(2) + Flags(4) + PublicKey(blob) + Name/Culture(str).
                // The Flags u32 was MISSING here (a latent reader-only bug: no prior test ever
                // read a table sorted AFTER Assembly in an image carrying an Assembly row —
                // GenericParam, 0x2A, is the first).
                Token::TABLE_ASSEMBLY => 4 + 4 * 2 + 4 + blob_w + 2 * str_w,
                Token::TABLE_ASSEMBLY_REF => 2 * 4 + 4 + blob_w + 2 * str_w + blob_w,
                Token::TABLE_METHOD_SPEC => {
                    let mdor_max =
                        count(Token::TABLE_METHOD_DEF).max(count(Token::TABLE_MEMBER_REF));
                    coded_w(1, mdor_max) + blob_w
                }
                Token::TABLE_GENERIC_PARAM => {
                    // Number (u16) + Flags (u16) + Owner (TypeOrMethodDef, 1 tag bit) + Name.
                    let tomd_max = count(Token::TABLE_TYPE_DEF).max(count(Token::TABLE_METHOD_DEF));
                    2 + 2 + coded_w(1, tomd_max) + str_w
                }
                other => panic!("row_width: unhandled table {other:#x}"),
            }
        }

        /// Byte offset (relative to `row_data_offset`) of `table_id`'s row data — the sum of
        /// every earlier (lower table-id) valid table's total row bytes.
        fn table_offset(&self, table_id: u32, header: &TablesHeader) -> usize {
            let mut offset = 0;
            for &(id, count) in &header.counts {
                if id == table_id {
                    return offset;
                }
                offset += count * Self::row_width(id, header);
            }
            panic!("table {table_id:#x} has no rows (not in Valid)");
        }

        fn strings_at(&self, off: u32) -> &'a str {
            let bytes = self.stream("#Strings");
            let start = off as usize;
            let end = bytes[start..].iter().position(|&b| b == 0).unwrap() + start;
            std::str::from_utf8(&bytes[start..end]).unwrap()
        }

        /// Parses the `#~` stream header, returning (heap_sizes, valid, sorted, row_counts in
        /// table-id order for every table with `valid` bit set) plus the byte offset where row
        /// data begins.
        fn tables_header(&self) -> TablesHeader {
            let bytes = self.stream("#~");
            let heap_sizes = bytes[6];
            let valid = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
            let sorted = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
            let mut cursor = 24;
            let mut counts = Vec::new();
            for id in 0..64u32 {
                if valid & (1u64 << id) != 0 {
                    let count = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
                    counts.push((id, count as usize));
                    cursor += 4;
                }
            }
            TablesHeader {
                heap_sizes,
                valid,
                sorted,
                counts,
                row_data_offset: cursor,
            }
        }
    }

    struct TablesHeader {
        heap_sizes: u8,
        valid: u64,
        sorted: u64,
        counts: Vec<(u32, usize)>,
        row_data_offset: usize,
    }

    #[test]
    fn tiny_module_roundtrips_through_the_reader() {
        let mut mb = MetadataBuilder::new();
        mb.finish_module("test_module");

        // One string interned via a Field so #Strings is nonempty and independently checkable.
        let type_tok = mb.add_type_def("", "MyType", false, None, None, None, &[]);
        assert_eq!(type_tok.table(), Token::TABLE_TYPE_DEF);
        // Row 1 is always the mandatory `<Module>` pseudo-type (§II.22.37) `MetadataBuilder::new`
        // seeds — the first REAL class def lands on row 2.
        assert_eq!(type_tok.rid(), 2);

        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01); // void
            mb.blobs.intern(&out)
        };
        let method_tok = mb.add_method(
            "DoIt",
            sig_blob,
            &[],
            &[],
            true,
            false,
            false,
            None,
            false,
            None,
        );
        assert_eq!(method_tok.table(), Token::TABLE_METHOD_DEF);
        assert_eq!(method_tok.rid(), 1);

        let ext_scope = mb.assembly_ref(
            "System.Runtime",
            AssemblyRefTarget::Bcl {
                version: (8, 0, 0, 0),
                token: ECMA_PUBLIC_KEY_TOKEN,
            },
        );
        let console_ref = mb.type_ref(Some(ext_scope), "System", "Console");
        let write_line_sig = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01); // void
            mb.blobs.intern(&out)
        };
        let member_ref_tok = mb.member_ref(console_ref, "WriteLine", write_line_sig);
        assert_eq!(member_ref_tok.table(), Token::TABLE_MEMBER_REF);

        let bytes = mb.serialize();

        let reader = MetadataReader::parse(&bytes);
        let header = reader.tables_header();
        assert_eq!(
            header.heap_sizes, 0,
            "small tables: no wide heap indices needed"
        );

        let expected_valid_tables = [
            Token::TABLE_MODULE,
            Token::TABLE_TYPE_REF,
            Token::TABLE_TYPE_DEF,
            Token::TABLE_METHOD_DEF,
            Token::TABLE_MEMBER_REF,
            Token::TABLE_ASSEMBLY_REF,
        ];
        for id in expected_valid_tables {
            assert!(
                header.valid & (1u64 << id) != 0,
                "table {id:#x} should be Valid"
            );
        }
        assert_eq!(
            header.sorted, 0,
            "no sorted tables populated in this tiny module"
        );

        let counts: HashMap<u32, usize> = header.counts.into_iter().collect();
        assert_eq!(counts[&Token::TABLE_MODULE], 1);
        // 2, not 1: row 1 is the mandatory `<Module>` pseudo-type `MetadataBuilder::new` seeds
        // (§II.22.37), row 2 is the real `MyType` added above.
        assert_eq!(counts[&Token::TABLE_TYPE_DEF], 2);
        assert_eq!(counts[&Token::TABLE_METHOD_DEF], 1);
        assert_eq!(counts[&Token::TABLE_MEMBER_REF], 1);
        assert_eq!(counts[&Token::TABLE_TYPE_REF], 1);
        assert_eq!(counts[&Token::TABLE_ASSEMBLY_REF], 1);

        // Decode the TypeDef rows' Name columns and check "MyType" round-trips via #Strings.
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        // Module row: Generation(2) + Name(2) + Mvid(2) + EncId(2) + EncBaseId(2) = 10 bytes (all
        // narrow since nothing here exceeds 0xFFFF).
        let module_name_off = u16::from_le_bytes(row_bytes[2..4].try_into().unwrap());
        assert_eq!(reader.strings_at(u32::from(module_name_off)), "test_module");

        // TypeRef rows come right after Module (1 row * 10 bytes).
        let type_ref_start = 10;
        // TypeRef: ResolutionScope(2, coded) + Name(2) + Namespace(2) = 6 bytes.
        let type_ref_name_off = u16::from_le_bytes(
            row_bytes[type_ref_start + 2..type_ref_start + 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(reader.strings_at(u32::from(type_ref_name_off)), "Console");

        // TypeDef rows come after TypeRef (1 row * 6 bytes). Row 1 (the `<Module>` pseudo-type)
        // comes first; "MyType" is row 2.
        let type_def_start = type_ref_start + 6;
        // TypeDef: Flags(4) + Name(2) + Namespace(2) + Extends(2, coded) + FieldList(2) +
        // MethodList(2) = 14 bytes.
        let module_pseudo_type_name_off = u16::from_le_bytes(
            row_bytes[type_def_start + 4..type_def_start + 6]
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            reader.strings_at(u32::from(module_pseudo_type_name_off)),
            "<Module>"
        );
        let my_type_start = type_def_start + 14;
        let type_def_name_off = u16::from_le_bytes(
            row_bytes[my_type_start + 4..my_type_start + 6]
                .try_into()
                .unwrap(),
        );
        assert_eq!(reader.strings_at(u32::from(type_def_name_off)), "MyType");
        let method_list = u16::from_le_bytes(
            row_bytes[my_type_start + 10..my_type_start + 12]
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            method_list, 1,
            "MyType owns MethodDef row 1 (1-based FieldList/MethodList)"
        );

        // MethodDef rows: RVA(4) + ImplFlags(2) + Flags(2) + Name(2) + Signature(2) +
        // ParamList(2) = 14 bytes.
        let method_def_start = my_type_start + 14;
        let method_name_off = u16::from_le_bytes(
            row_bytes[method_def_start + 8..method_def_start + 10]
                .try_into()
                .unwrap(),
        );
        assert_eq!(reader.strings_at(u32::from(method_name_off)), "DoIt");
        let method_flags = u16::from_le_bytes(
            row_bytes[method_def_start + 6..method_def_start + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(method_flags & 0x10, 0x10, "static bit must be set");
    }

    /// `Module.Name` (§II.22.30) and `Assembly.Name` (§II.22.2) are DISTINCT columns on DISTINCT
    /// tables — `finish_module` and `set_assembly` must never be called with the same string
    /// under the assumption they're "the same name". A real regression (found via the
    /// `cd_json`/`cd_async`/`pal_threads` A/B differential): the linker's `DIRECT_PE` call site
    /// passed the SAME string (the executable's `"_"` assembly-identity placeholder, mirroring
    /// `il_exporter`'s `.assembly _{}`) to both `MetadataBuilder::set_assembly` AND
    /// `export_pe`'s `finish_module` call — but `ilasm`, given no explicit `.module` directive
    /// (`il_exporter` never emits one), defaults `Module.Name` to its `-output:` file's own
    /// basename, NOT the assembly identity. Stamping `Module.Name = "_"` made
    /// `AssemblyLoadContext.InternalLoad`'s native path reject the image with
    /// `System.IO.FileLoadException: Could not load file or assembly '_, ...'` (`0x8007000C`) —
    /// thrown from native code before the CLI-aware managed loader (or even
    /// `System.Reflection.Metadata`'s own reader, which accepted the same bytes with zero errors)
    /// ever inspected the metadata.
    #[test]
    fn module_name_and_assembly_name_are_independent_columns() {
        let mut mb = MetadataBuilder::new();
        // The exact real-world shape: an executable's assembly identity is the "_" placeholder,
        // but its Module.Name must be the real output filename.
        mb.set_assembly("_", (0, 0, 0, 0));
        mb.finish_module("cd_json-7dec5593b2da6ade.exe");

        let bytes = mb.serialize();
        let reader = MetadataReader::parse(&bytes);
        let header = reader.tables_header();
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];

        // Module row is always first: Generation(2) + Name(2) + Mvid(2) + EncId(2) + EncBaseId(2).
        let module_name_off = u16::from_le_bytes(row_bytes[2..4].try_into().unwrap());
        assert_eq!(
            reader.strings_at(u32::from(module_name_off)),
            "cd_json-7dec5593b2da6ade.exe",
            "Module.Name must be the output filename, not the assembly identity"
        );

        // Assembly row comes after Module + TypeDef(<Module> pseudo-type, since no add_type_def
        // was called here it's the only TypeDef row). Locate it via the table header's row
        // ordering instead of a hardcoded offset, to stay robust to unrelated table layout.
        let counts: HashMap<u32, usize> = header.counts.iter().copied().collect();
        assert_eq!(counts.get(&Token::TABLE_ASSEMBLY).copied(), Some(1));
        // Module(10) + TypeDef(1 row * 14 bytes, the mandatory <Module> pseudo-type).
        let assembly_start = 10 + 14;
        // Assembly: HashAlgId(4) + MajorVersion(2) + MinorVersion(2) + BuildNumber(2) +
        // RevisionNumber(2) + Flags(4) + PublicKey(2, blob) + Name(2) = offset 18 for Name.
        let assembly_name_off = u16::from_le_bytes(
            row_bytes[assembly_start + 18..assembly_start + 20]
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            reader.strings_at(u32::from(assembly_name_off)),
            "_",
            "Assembly.Name keeps the executable's placeholder identity"
        );

        assert_ne!(
            module_name_off, assembly_name_off,
            "Module.Name and Assembly.Name must intern to different #Strings offsets here"
        );
    }

    /// `add_method`'s `param_names` slice drives real `Param` rows (§II.22.33), one per entry —
    /// including entries with no name. Verified against real CoreCLR `ilasm`/`monodis`: an
    /// unnamed parameter still gets a `Param` row (empty `Name`, real 1-based `Sequence`), it is
    /// not omitted — so this backend's "always push a row" behavior in `add_method` (never
    /// skipping `None` entries) is the semantically correct mirror of what the real assembler
    /// does, not an over-approximation. Phase 1b's `export_pe` wiring (Cluster C item 1) depends
    /// on this: it must be safe to pass `method.arg_names()` straight through without special-
    /// casing the `None` slots.
    #[test]
    fn named_and_unnamed_params_both_get_param_rows_with_correct_sequence() {
        let mut mb = MetadataBuilder::new();
        mb.finish_module("param_test");
        mb.add_type_def("", "MyType", false, None, None, None, &[]);

        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 2);
            out.push(0x01); // void return
            out.push(0x08); // int32
            out.push(0x08); // int32
            mb.blobs.intern(&out)
        };
        // Mirrors `il_exporter`'s per-arg `Option<name>` shape: first param named, second not.
        let method_tok = mb.add_method(
            "Mixed",
            sig_blob,
            &[Some("x"), None],
            &[],
            true,
            false,
            false,
            None,
            false,
            None,
        );
        assert_eq!(method_tok.rid(), 1);

        let bytes = mb.serialize();
        let reader = MetadataReader::parse(&bytes);
        let header = reader.tables_header();
        let counts: HashMap<u32, usize> = header.counts.iter().copied().collect();
        assert_eq!(
            counts[&Token::TABLE_PARAM],
            2,
            "one Param row per arg_names entry, named or not"
        );

        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let param_start = reader.table_offset(Token::TABLE_PARAM, &header);
        // Param row: Flags(2) + Sequence(2) + Name(2, narrow — small tables here) = 6 bytes.
        let row0 = &row_bytes[param_start..param_start + 6];
        let row1 = &row_bytes[param_start + 6..param_start + 12];

        let seq0 = u16::from_le_bytes(row0[2..4].try_into().unwrap());
        let name0 = u16::from_le_bytes(row0[4..6].try_into().unwrap());
        assert_eq!(
            seq0, 1,
            "first real arg is Sequence 1 (this is a static method, no implicit this)"
        );
        assert_eq!(reader.strings_at(u32::from(name0)), "x");

        let seq1 = u16::from_le_bytes(row1[2..4].try_into().unwrap());
        let name1 = u16::from_le_bytes(row1[4..6].try_into().unwrap());
        assert_eq!(seq1, 2, "second arg is Sequence 2");
        assert_eq!(
            name1, 0,
            "unnamed param has a null #Strings offset, not a dangling/garbage one"
        );
    }

    // ---------------------------------------------------------------------------------------
    // (b) Coded-index width flips at the documented row-count thresholds.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn simple_index_width_flips_at_2_16() {
        assert!(!simple_wide(0xFFFF));
        assert!(simple_wide(0x1_0000));
    }

    #[test]
    fn coded_index_width_flips_at_documented_thresholds() {
        // TypeDefOrRef: 2 tag bits -> threshold 2^14. The threshold row count itself is already
        // wide (§II.24.2.6 uses `>=`); one row below it must still be narrow.
        assert!(!coded_wide(2, (1 << 14) - 1));
        assert!(coded_wide(2, 1 << 14));
        // MethodDefOrRef: 1 tag bit -> threshold 2^15.
        assert!(!coded_wide(1, (1 << 15) - 1));
        assert!(coded_wide(1, 1 << 15));
        // HasCustomAttribute: 5 tag bits -> threshold 2^11.
        assert!(!coded_wide(5, (1 << 11) - 1));
        assert!(coded_wide(5, 1 << 11));
    }

    /// Regression test for the coded-index width off-by-one at the EXACT threshold row count.
    ///
    /// §II.24.2.6: a coded index with `tag_bits` tag bits must use a 4-byte column "if the number
    /// of rows in the largest target table is equal to or greater than 2^(16-tag_bits)". That is
    /// a `>=` predicate, not `>`. At `max_rows == 2^(16-tag_bits)` exactly, the largest encodable
    /// coded value is `(max_rows << tag_bits) | tag = (2^(16-tag_bits) << tag_bits) = 0x1_0000`,
    /// which does not fit in a `u16` — so the column MUST already be wide at that row count, one
    /// row earlier than a strict `>` comparison would switch it.
    ///
    /// Ground-truthed against `System.Reflection.Metadata.Ecma335.MetadataBuilder` (.NET 8): for
    /// the `TypeDefOrRef` coded index (`tag_bits = 2`, threshold `2^14 = 16384`), SRM emits a
    /// wide `Extends` column once the largest target table (`TypeRef` here) reaches exactly 16384
    /// rows, not 16385.
    #[test]
    fn coded_index_width_flips_at_the_threshold_row_count_itself_not_one_past_it() {
        // TypeDefOrRef: 2 tag bits -> threshold 2^14 = 16384. At exactly 16384 rows the column
        // must ALREADY be wide (previously: `coded_wide(2, 1 << 14)` incorrectly returned false).
        assert!(
            coded_wide(2, 1 << 14),
            "coded index must go wide AT the 2^(16-tag_bits) row count, not only past it"
        );
        // One row below the threshold must still be narrow.
        assert!(!coded_wide(2, (1 << 14) - 1));
    }

    /// Reproduces the concrete corruption `coded_wide`'s off-by-one causes: at exactly the
    /// threshold row count, a coded value that should require 4 bytes gets truncated to 2,
    /// silently dropping the high bits and pointing the reader at the wrong row/table entirely.
    #[test]
    fn coded_index_at_threshold_row_count_does_not_truncate_high_bits() {
        // TypeDefOrRef, tag_bits = 2, threshold = 2^14 = 16384 target rows.
        let tag_bits = 2u32;
        let max_rows = 1usize << (16 - tag_bits); // 16384
        assert!(
            coded_wide(tag_bits, max_rows),
            "must be wide at the threshold row count"
        );

        // The coded value for the highest-numbered row (rid = max_rows, tag = 0):
        // (rid << tag_bits) | tag == (16384 << 2) | 0 == 0x1_0000 — doesn't fit in u16.
        let value: u32 = (u32::try_from(max_rows).unwrap() << tag_bits) | 0;
        assert_eq!(value, 0x1_0000);

        let mut out = Vec::new();
        write_coded_idx(&mut out, value, coded_wide(tag_bits, max_rows));
        assert_eq!(out.len(), 4, "wide column must write 4 bytes");
        let round_tripped = u32::from_le_bytes(out.try_into().unwrap());
        assert_eq!(
            round_tripped, value,
            "coded index must round-trip losslessly at the threshold row count, not truncate to 0"
        );
    }

    #[test]
    fn widths_compute_flips_type_def_or_ref_and_produces_wide_rows() {
        let sizes_small = RowCounts {
            type_def: 1,
            type_ref: 1,
            ..Default::default()
        };
        let strings = StringsHeap::default();
        let blobs = BlobHeap::default();
        let guids = GuidHeap::default();
        let us = UserStringHeap::default();
        let w_small = Widths::compute(&sizes_small, &strings, &blobs, &guids, &us);
        assert!(!w_small.type_def_or_ref_wide);

        let sizes_big = RowCounts {
            type_def: (1 << 14) + 1,
            ..Default::default()
        };
        let w_big = Widths::compute(&sizes_big, &strings, &blobs, &guids, &us);
        assert!(w_big.type_def_or_ref_wide);
    }

    #[test]
    fn heap_sizes_bits_flip_when_a_heap_exceeds_0xffff_bytes() {
        let sizes = RowCounts::default();
        let mut strings = StringsHeap::default();
        // Force the #Strings heap past 0xFFFF bytes.
        let big = "x".repeat(0x1_0000);
        strings.intern(&big);
        let blobs = BlobHeap::default();
        let guids = GuidHeap::default();
        let us = UserStringHeap::default();
        let w = Widths::compute(&sizes, &strings, &blobs, &guids, &us);
        assert!(w.str_wide);
        assert_eq!(w.heap_sizes & 0x1, 0x1);
        assert_eq!(
            w.heap_sizes & 0x2,
            0,
            "GUID heap untouched, must stay narrow"
        );
        assert_eq!(
            w.heap_sizes & 0x4,
            0,
            "Blob heap untouched, must stay narrow"
        );
    }

    // ---------------------------------------------------------------------------------------
    // (c) Sorted-table enforcement.
    // ---------------------------------------------------------------------------------------

    #[test]
    fn interface_impl_rows_are_emitted_sorted_by_class() {
        let mut mb = MetadataBuilder::new();
        // Two TypeDefs, each implementing the same (single) interface TypeRef, added in an order
        // that would violate Class-sort if rows were emitted in insertion order. `MetadataBuilder::
        // new` already seeds the mandatory `<Module>` pseudo-type at TypeDef rid 1 (§II.22.37), so
        // the first REAL class def below (`BType`) lands on rid 2, not rid 1.
        let ext = mb.assembly_ref("SomeLib", AssemblyRefTarget::NameOnly);
        let iface = mb.type_ref(Some(ext), "", "ISomeInterface");

        // Add type B first (becomes TypeDef rid 2, after the seeded `<Module>` at rid 1) — its
        // `implements: &[iface]` already pushes one InterfaceImpl row for class=2 via
        // `add_type_def` itself…
        let _b = mb.add_type_def("", "BType", false, None, None, None, &[iface]);
        // …then push a second class=2 row plus a class=1 row (referencing `<Module>` itself, a
        // synthetic lower-rid "class" value — §II.22.23 doesn't forbid a pseudo-type row
        // appearing in InterfaceImpl; this test only cares about sort-order mechanics, not
        // semantic validity) directly via the standalone API, so insertion order (2, 2, 1)
        // disagrees with the required ascending-by-Class sort order.
        mb.interface_impl(Token::new(Token::TABLE_TYPE_DEF, 2), iface);
        mb.interface_impl(Token::new(Token::TABLE_TYPE_DEF, 1), iface);

        let bytes = mb.serialize();
        let reader = MetadataReader::parse(&bytes);
        let header = reader.tables_header();
        assert!(
            header.sorted & (1u64 << Token::TABLE_INTERFACE_IMPL) != 0,
            "InterfaceImpl must have its Sorted bit set"
        );

        // Decode: find InterfaceImpl's row offset generically (row_width/table_offset mirror the
        // writer's exact column shapes, so this never needs hand-duplicated byte math).
        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let interface_impl_start = reader.table_offset(Token::TABLE_INTERFACE_IMPL, &header);
        let row_width = MetadataReader::row_width(Token::TABLE_INTERFACE_IMPL, &header);
        let classes: Vec<u16> = (0..3)
            .map(|i| {
                let start = interface_impl_start + i * row_width;
                u16::from_le_bytes(row_bytes[start..start + 2].try_into().unwrap())
            })
            .collect();
        assert!(
            classes.windows(2).all(|w| w[0] <= w[1]),
            "InterfaceImpl rows must be sorted ascending by Class: {classes:?}"
        );
        assert_eq!(
            classes,
            vec![1, 2, 2],
            "the three rows added (class=2 via add_type_def's own implements + class=2, class=1 standalone)"
        );
    }

    #[test]
    fn field_layout_rows_are_emitted_sorted_by_field() {
        let mut mb = MetadataBuilder::new();
        let _t = mb.add_type_def("", "Layout", true, None, Some(1), Some(8), &[]);
        let field_sig = {
            let mut out = Vec::new();
            out.push(sig::SIG_FIELD);
            out.push(0x08); // i4
            mb.blobs.intern(&out)
        };
        // Add fields with descending offsets so insertion order disagrees with Field-rid sort
        // only if rid order and offset order diverge; here rid IS insertion order (1, 2), so
        // exercise divergence by adding a THIRD field then re-checking sortedness is by rid
        // (Field), not by offset value — the point of this test is the *rid* ordering survives.
        let f1 = mb.add_field("a", field_sig, Some(4));
        let f2 = mb.add_field("b", field_sig, Some(0));
        assert!(f1.rid() < f2.rid());

        let bytes = mb.serialize();
        let reader = MetadataReader::parse(&bytes);
        let header = reader.tables_header();
        assert!(header.sorted & (1u64 << Token::TABLE_FIELD_LAYOUT) != 0);

        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let field_layout_start = reader.table_offset(Token::TABLE_FIELD_LAYOUT, &header);
        let row_width = MetadataReader::row_width(Token::TABLE_FIELD_LAYOUT, &header);
        // FieldLayout row: Offset(4) + Field(simple index) — the Field column follows Offset.
        let offset_col_width = 4;
        let field_0 = u16::from_le_bytes(
            row_bytes
                [field_layout_start + offset_col_width..field_layout_start + offset_col_width + 2]
                .try_into()
                .unwrap(),
        );
        let field_1 = u16::from_le_bytes(
            row_bytes[field_layout_start + row_width + offset_col_width
                ..field_layout_start + row_width + offset_col_width + 2]
                .try_into()
                .unwrap(),
        );
        assert!(
            field_0 <= field_1,
            "FieldLayout must be sorted ascending by Field"
        );
        assert_eq!((field_0, field_1), (1, 2));
    }

    #[test]
    fn dotnet_class_name_shortens_over_long_names_deterministically() {
        let long = "x".repeat(2000);
        let short = dotnet_class_name(&long);
        assert!(short.len() < long.len());
        assert!(short.contains("__h"));
        let short2 = dotnet_class_name(&long);
        assert_eq!(short, short2, "must be a pure function of the input");
        let within_limit = "x".repeat(100);
        assert_eq!(dotnet_class_name(&within_limit), within_limit.as_str());
    }

    #[test]
    fn assembly_ref_bcl_vs_name_only() {
        let mut mb = MetadataBuilder::new();
        let bcl = mb.assembly_ref(
            "System.Runtime",
            AssemblyRefTarget::Bcl {
                version: (8, 0, 0, 0),
                token: ECMA_PUBLIC_KEY_TOKEN,
            },
        );
        let name_only = mb.assembly_ref("MyLib", AssemblyRefTarget::NameOnly);
        assert_ne!(bcl, name_only);
        assert_eq!(bcl.table(), Token::TABLE_ASSEMBLY_REF);
        assert_eq!(mb.assembly_ref.len(), 2);
        assert_ne!(
            mb.assembly_ref[0].public_key_or_token, 0,
            "BCL ref carries the ECMA token blob"
        );
        assert_eq!(
            mb.assembly_ref[1].public_key_or_token, 0,
            "name-only ref carries no token"
        );
    }

    /// Both BCL-reference paths must consume the builder's explicit runtime rather than a process
    /// global or a hardcoded .NET 8 literal.
    #[test]
    fn bcl_assembly_refs_are_stamped_from_the_explicit_runtime() {
        let expected = DotnetRuntime::Net9.assembly_ver_tuple();

        let mut mb = MetadataBuilder::new();
        mb.set_runtime(DotnetRuntime::Net9);
        mb.set_is_lib(true);
        let sys_runtime_tok = mb.find_or_create_assembly_ref("System.Runtime");
        let row = &mb.assembly_ref[(sys_runtime_tok.rid() - 1) as usize];
        assert_eq!((row.major, row.minor, row.build, row.revision), expected);

        // A second, distinct BCL name via the same helper must agree too (not a fluke of caching
        // the first lookup).
        let mut mb2 = MetadataBuilder::new();
        mb2.set_runtime(DotnetRuntime::Net9);
        mb2.set_is_lib(true);
        let intrinsics_tok = mb2.find_or_create_assembly_ref("System.Runtime.Intrinsics");
        let row2 = &mb2.assembly_ref[(intrinsics_tok.rid() - 1) as usize];
        assert_eq!(
            (row2.major, row2.minor, row2.build, row2.revision),
            expected
        );

        // `system_runtime_assembly_ref` (used by the ThreadStaticAttribute bootstrap path) is a
        // separate call site from `find_or_create_assembly_ref` — exercise it directly via a TLS
        // static field, which routes through `thread_static_attribute` -> `thread_static_ctor_ref`
        // -> `system_runtime_assembly_ref`.
        let mut mb3 = MetadataBuilder::new();
        mb3.set_runtime(DotnetRuntime::Net9);
        mb3.set_is_lib(true);
        let field = mb3.add_static_field("TLS", 0, None, true, false);
        let _ = field;
        assert_eq!(
            mb3.assembly_ref.len(),
            1,
            "the TLS path must have created exactly one AssemblyRef"
        );
        let row3 = &mb3.assembly_ref[0];
        assert_eq!(
            (row3.major, row3.minor, row3.build, row3.revision),
            expected
        );
    }

    /// The `is_lib` gate itself (not just that the version, when stamped, comes from
    /// runtime): an executable-shaped builder (`is_lib` left at its `false` default)
    /// must produce NAME-ONLY/`0.0.0.0` `AssemblyRef` rows for BCL assemblies too — mirrors
    /// `il_exporter`'s executable path, which emits no `.assembly extern` headers at all and lets
    /// `ilasm` infer unversioned externs. This is the regression test for the concrete
    /// `FileLoadException 0x8007000C` bug: a version-stamped `AssemblyRef` on an executable's own
    /// dependency graph makes the CLR's native binder try to resolve each BCL reference at that
    /// exact version via the app's rollForward machinery and fail before the managed loader (or
    /// even a lenient `System.Reflection.Metadata` reader) ever runs.
    #[test]
    fn executable_shaped_builder_leaves_bcl_assembly_refs_unversioned() {
        let mut mb = MetadataBuilder::new();
        // `is_lib` NOT set — defaults to `false`, matching `export_pe`'s call before Pass 0.
        let sys_runtime_tok = mb.find_or_create_assembly_ref("System.Runtime");
        let row = &mb.assembly_ref[(sys_runtime_tok.rid() - 1) as usize];
        assert_eq!(
            (row.major, row.minor, row.build, row.revision),
            (0, 0, 0, 0)
        );
        assert_eq!(
            row.public_key_or_token, 0,
            "an unversioned exe ref carries no public-key token either"
        );

        let mut mb2 = MetadataBuilder::new();
        let field = mb2.add_static_field("TLS", 0, None, true, false);
        let _ = field;
        let row2 = &mb2.assembly_ref[0];
        assert_eq!(
            (row2.major, row2.minor, row2.build, row2.revision),
            (0, 0, 0, 0),
            "system_runtime_assembly_ref must respect is_lib too"
        );
    }

    #[test]
    fn thread_static_attribute_uses_fixed_blob() {
        let mut mb = MetadataBuilder::new();
        let field = mb.add_static_field("TLS", 0, None, false, false);
        let attr = mb.thread_static_attribute(field);
        assert_eq!(attr.table(), Token::TABLE_CUSTOM_ATTRIBUTE);
        assert_eq!(mb.custom_attribute.len(), 1);
        let value_off = mb.custom_attribute[0].value;
        let bytes = mb.blobs.as_bytes();
        // length-prefix(1) + the 4 fixed bytes.
        assert_eq!(
            &bytes[value_off as usize..value_off as usize + 5],
            &[4, 0x01, 0x00, 0x00, 0x00]
        );
    }

    /// The general `add_custom_attribute` emitter, attached to a `TypeDef` — the exact shape
    /// `#[dotnet_class(attr(...))]` uses. Regression-guards a real bug this test would have
    /// caught: `encode_has_custom_attribute`'s tag table was off-by-one for `TypeRef`/`TypeDef`/
    /// `Param` (only `Field`, tag 1, had ever been exercised before this general emitter existed),
    /// so a `TypeDef`-parented `CustomAttribute` row decoded as a bogus `Param`-parented one —
    /// caught empirically via `monodis --customattr` showing `Param: <token>` instead of
    /// `TypeDef: <token>` for a class-level attribute.
    #[test]
    fn add_custom_attribute_on_a_typedef_parent_decodes_as_typedef() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let class_tok = mb.add_type_def("", "Widget", false, None, None, None, &[]);

        let attr_name = asm.alloc_string("FooAttribute".to_string());
        let attr_asm = asm.alloc_string("SomeAssembly".to_string());
        let attr_type =
            asm.alloc_class_ref(ClassRef::new(attr_name, Some(attr_asm), false, [].into()));
        let prop_name = asm.alloc_string("Bar".to_string());
        let field_name = asm.alloc_string("Baz".to_string());
        let ctor_arg = asm.alloc_string("hello".to_string());
        let attr_def = crate::ir::class::CustomAttrDef::new_with_named_args(
            attr_type,
            vec![crate::ir::class::CustomAttrArg::Str(ctor_arg)],
            vec![
                crate::ir::class::CustomAttrNamedArg::property(
                    prop_name,
                    crate::ir::class::CustomAttrArg::I32(7),
                ),
                crate::ir::class::CustomAttrNamedArg::field(
                    field_name,
                    crate::ir::class::CustomAttrArg::Bool(true),
                ),
            ],
        );

        let attr_tok = mb.add_custom_attribute(&mut asm, class_tok, &attr_def);
        assert_eq!(attr_tok.table(), Token::TABLE_CUSTOM_ATTRIBUTE);
        assert_eq!(mb.custom_attribute.len(), 1);

        // The `parent` coded index must decode back to the ORIGINAL `TypeDef` token — this is
        // exactly the bug: it used to decode as `Param` instead.
        let row = &mb.custom_attribute[0];
        let parent_tag = row.parent & 0x1F; // 5 tag bits (§II.24.2.6 HasCustomAttribute)
        assert_eq!(
            parent_tag, 3,
            "TypeDef must be HasCustomAttribute tag 3 per §II.24.2.6"
        );
        assert_eq!(row.parent >> 5, class_tok.rid());

        // One MemberRef `.ctor` row, with a HASTHIS/1-param/VOID-return/STRING-param signature.
        assert_eq!(mb.member_ref.len(), 1);

        // Blob: prolog(2) + FixedArg string "hello" (1-byte len + 5 bytes) + NumNamed(2) +
        // one PROPERTY/I4 and one FIELD/BOOLEAN NamedArg.
        let bytes = mb.blobs.as_bytes();
        // `+1`: the blob heap itself prefixes every entry with its own compressed length
        // (§II.24.2.4) — `row.value` points at THAT prefix, not the raw `CustomAttrib` bytes
        // (mirrors `thread_static_attribute_uses_fixed_blob`'s identical `+1`-shaped skip, there
        // spelled as a literal leading `4` in its expected slice).
        let off = row.value as usize + 1;
        assert_eq!(&bytes[off..off + 2], &[0x01, 0x00], "prolog");
        assert_eq!(bytes[off + 2], 5, "ctor string arg length prefix");
        assert_eq!(&bytes[off + 3..off + 8], b"hello");
        assert_eq!(&bytes[off + 8..off + 10], &[0x02, 0x00], "NumNamed = 2");
        assert_eq!(bytes[off + 10], 0x54, "NamedArg kind = PROPERTY");
        assert_eq!(bytes[off + 11], 0x08, "NamedArg type = ELEMENT_TYPE_I4");
        assert_eq!(bytes[off + 12], 3, "property name length prefix");
        assert_eq!(&bytes[off + 13..off + 16], b"Bar");
        assert_eq!(&bytes[off + 16..off + 20], &7i32.to_le_bytes());
        assert_eq!(bytes[off + 20], 0x53, "NamedArg kind = FIELD");
        assert_eq!(
            bytes[off + 21],
            0x02,
            "NamedArg type = ELEMENT_TYPE_BOOLEAN"
        );
        assert_eq!(bytes[off + 22], 3, "field name length prefix");
        assert_eq!(&bytes[off + 23..off + 26], b"Baz");
        assert_eq!(bytes[off + 26], 1, "field bool value");
    }

    #[test]
    fn custom_attribute_u8_constructor_argument_uses_u1_and_one_byte_payload() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let class_tok = mb.add_type_def("", "Widget", false, None, None, None, &[]);
        let attr_name = asm.alloc_string("NullableContextAttribute".to_string());
        let attr_asm = asm.alloc_string("System.Runtime".to_string());
        let attr_type =
            asm.alloc_class_ref(ClassRef::new(attr_name, Some(attr_asm), false, [].into()));
        let attr_def = crate::ir::class::CustomAttrDef::new(
            attr_type,
            vec![crate::ir::class::CustomAttrArg::U8(1)],
            vec![],
        );

        mb.add_custom_attribute(&mut asm, class_tok, &attr_def);
        let row = &mb.custom_attribute[0];
        let bytes = mb.blobs.as_bytes();
        let off = row.value as usize + 1;
        assert_eq!(
            &bytes[off..off + 5],
            &[0x01, 0x00, 0x01, 0x00, 0x00],
            "prolog, U1 fixed arg, and zero named arguments"
        );

        let signature_offset = mb.member_ref[0].signature as usize;
        let signature_len = usize::from(bytes[signature_offset]);
        assert_eq!(
            &bytes[signature_offset + 1..signature_offset + 1 + signature_len],
            &[0x20, 0x01, 0x01, ATTR_ELEM_U1],
            "HASTHIS .ctor(byte) returning void"
        );
    }

    #[test]
    fn method_nullability_emits_context_return_and_parameter_attributes() {
        let mut mb = MetadataBuilder::new();
        mb.add_type_def("", "Api", false, None, None, None, &[]);
        let signature = mb.blobs.intern(&[
            sig::SIG_DEFAULT,
            0x02,
            ATTR_ELEM_STRING,
            ATTR_ELEM_STRING,
            ATTR_ELEM_STRING,
        ]);
        let parameter_flags = [None, Some(2)];
        let method = mb.add_method(
            "Choose",
            signature,
            &[Some("required"), Some("optional")],
            &[],
            true,
            false,
            false,
            None,
            false,
            Some(MethodNullability {
                context: 1,
                return_flag: Some(2),
                parameter_flags: &parameter_flags,
            }),
        );

        assert_eq!(
            mb.param.iter().map(|row| row.sequence).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "an attributed return uses Sequence 0 before ordinary parameters"
        );
        assert_eq!(mb.custom_attribute.len(), 3);
        assert_eq!(
            mb.custom_attribute[0].parent,
            encode_has_custom_attribute(method),
            "NullableContextAttribute belongs to the MethodDef"
        );
        let return_param = Token::new(Token::TABLE_PARAM, 1);
        let optional_param = Token::new(Token::TABLE_PARAM, 3);
        assert_eq!(
            mb.custom_attribute[1].parent,
            encode_has_custom_attribute(return_param)
        );
        assert_eq!(
            mb.custom_attribute[2].parent,
            encode_has_custom_attribute(optional_param)
        );
    }

    #[test]
    fn general_method_return_and_parameter_attributes_target_the_correct_rows() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        mb.add_type_def("", "Api", false, None, None, None, &[]);
        let signature = mb.blobs.intern(&[
            sig::SIG_DEFAULT,
            0x02,
            ATTR_ELEM_I4,
            ATTR_ELEM_I4,
            ATTR_ELEM_I4,
        ]);
        let method = mb.add_method(
            "Compute",
            signature,
            &[Some("left"), Some("right")],
            &[],
            true,
            false,
            false,
            None,
            false,
            None,
        );
        let attr_name = asm.alloc_string("MarkerAttribute".to_string());
        let attr_asm = asm.alloc_string("Tests".to_string());
        let attr_type =
            asm.alloc_class_ref(ClassRef::new(attr_name, Some(attr_asm), false, [].into()));
        let attribute = crate::ir::class::CustomAttrDef::new(attr_type, vec![], vec![]);
        mb.add_method_custom_attributes(
            &mut asm,
            method,
            std::slice::from_ref(&attribute),
            std::slice::from_ref(&attribute),
            &[vec![], vec![attribute.clone()]],
        );

        assert_eq!(
            mb.param.iter().map(|row| row.sequence).collect::<Vec<_>>(),
            vec![1, 2, 0],
            "return Param can be appended to the current method's run after ordinary parameters"
        );
        assert_eq!(mb.custom_attribute.len(), 3);
        assert_eq!(
            mb.custom_attribute[0].parent,
            encode_has_custom_attribute(method)
        );
        assert_eq!(
            mb.custom_attribute[1].parent,
            encode_has_custom_attribute(Token::new(Token::TABLE_PARAM, 3)),
            "return attribute targets Sequence 0"
        );
        assert_eq!(
            mb.custom_attribute[2].parent,
            encode_has_custom_attribute(Token::new(Token::TABLE_PARAM, 2)),
            "second argument attribute targets Sequence 2"
        );
    }

    /// `StaticFieldDef::is_const` sets `FieldAttributes::InitOnly`; metadata literals are a
    /// separate enum-only API and table path.
    #[test]
    fn add_static_field_is_const_sets_initonly_flag_only() {
        let mut mb = MetadataBuilder::new();
        let plain = mb.add_static_field("Plain", 0, None, false, false);
        let konst = mb.add_static_field("Konst", 0, None, false, true);

        let plain_flags = mb.field[(plain.rid() - 1) as usize].flags;
        let konst_flags = mb.field[(konst.rid() - 1) as usize].flags;

        assert_eq!(
            plain_flags & 0x20,
            0,
            "non-const static must NOT have InitOnly set"
        );
        assert_eq!(
            konst_flags & 0x20,
            0x20,
            "const static must have InitOnly set"
        );
        // Both still carry the ordinary Public|Static bits — `is_const` only adds InitOnly, it
        // doesn't replace the base flag set. `FieldAttributes::Public` is 0x6 (FieldAccessMask,
        // §II.23.1.5), not 0x1 (that's `Private` — see `add_field`'s doc for the bug this fixes).
        assert_eq!(plain_flags & (0x6 | 0x10), 0x6 | 0x10);
        assert_eq!(konst_flags & (0x6 | 0x10), 0x6 | 0x10);
        assert_eq!(
            mb.custom_attribute.len(),
            0,
            "is_const must not add any CustomAttribute row"
        );
    }

    #[test]
    fn enum_fields_carry_literal_flags_and_constant_rows() {
        let mut mb = MetadataBuilder::new();
        let value = mb.add_enum_value_field(7);
        let ready = mb.add_enum_literal_field("Ready", 11, Const::U32(u32::MAX));

        assert_eq!(
            mb.field[(value.rid() - 1) as usize].flags,
            0x6 | 0x0200 | 0x0400
        );
        assert_eq!(
            mb.field[(ready.rid() - 1) as usize].flags,
            0x6 | 0x0010 | 0x0040 | 0x8000
        );
        assert_eq!(mb.constant.len(), 1);
        assert_eq!(mb.constant[0].type_code, 0x09, "ELEMENT_TYPE_U4");
        assert_eq!(mb.constant[0].parent, ready.rid() << 2);

        let bytes = mb.serialize();
        let reader = MetadataReader::parse(&bytes);
        let header = reader.tables_header();
        assert_eq!(
            header
                .counts
                .iter()
                .find(|&&(id, _)| id == Token::TABLE_CONSTANT),
            Some(&(Token::TABLE_CONSTANT, 1))
        );
        assert_ne!(header.sorted & (1u64 << Token::TABLE_CONSTANT), 0);
        let start = reader.table_offset(Token::TABLE_CONSTANT, &header);
        let rows = &reader.stream("#~")[header.row_data_offset..];
        assert_eq!(rows[start], 0x09);
        assert_eq!(rows[start + 1], 0);
    }

    #[test]
    fn set_field_rva_materializes_pending_row() {
        let mut mb = MetadataBuilder::new();
        let field = mb.add_static_field("DATA", 0, Some(vec![1, 2, 3]), false, false);
        assert_eq!(mb.field_rva.len(), 1);
        assert_eq!(mb.field_rva[0].rva, 0);
        mb.set_field_rva(field, 0x2000);
        assert_eq!(mb.field_rva[0].rva, 0x2000);
    }

    #[test]
    #[should_panic(expected = "no pending FieldRVA")]
    fn set_field_rva_panics_for_unknown_field() {
        let mut mb = MetadataBuilder::new();
        mb.set_field_rva(Token::new(Token::TABLE_FIELD, 99), 0);
    }

    #[test]
    fn method_def_and_member_ref_tokens_are_distinct_tables() {
        let mut mb = MetadataBuilder::new();
        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01);
            mb.blobs.intern(&out)
        };
        let _t = mb.add_type_def("", "Owner", false, None, None, None, &[]);
        let m = mb.add_method(
            "Foo",
            sig_blob,
            &[],
            &[],
            true,
            false,
            false,
            None,
            false,
            None,
        );
        assert_eq!(m.table(), Token::TABLE_METHOD_DEF);

        let ext = mb.assembly_ref("Other", AssemblyRefTarget::NameOnly);
        let tref = mb.type_ref(Some(ext), "", "Bar");
        let mr = mb.member_ref(tref, "Baz", sig_blob);
        assert_eq!(mr.table(), Token::TABLE_MEMBER_REF);
        assert_ne!(m, mr);
    }

    /// Regression for `MissingMethodException: Method not found: 'Void
    /// Dictionary\`2..ctor(Dictionary\`2<Int32,IntPtr>)'` caught wiring `DIRECT_PE=1` into the
    /// linker: a `MethodRef`'s stored `FnSig` carries the IMPLICIT receiver (`this`) at
    /// `inputs()[0]` for every non-static kind (mirrors `MethodDef`'s identical convention,
    /// `il_exporter`'s oracle skip at mod.rs:436/796/1068/1337). `TokenSink::method_token`
    /// resolving an out-of-assembly instance/virtual/ctor `MethodRef` as a `MemberRef` must strip
    /// that receiver before encoding the `MemberRefSig` blob — a `HASTHIS` signature (§II.23.2.1)
    /// already encodes the receiver implicitly via the calling-convention byte; writing it out
    /// AGAIN as parameter #0 doubles it, corrupting the argument list for every real argument
    /// after it (here: a parameterless generic `.ctor()` came out looking like it took ONE
    /// argument typed as the class itself).
    #[test]
    fn instance_member_ref_signature_excludes_the_implicit_receiver() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();

        // An out-of-assembly instance method `Foo::Bar(int32) -> bool`: `FnSig::inputs()` is
        // `[Foo (the receiver), int32]` per the "receiver at index 0" convention, matching how
        // the codegen backend actually builds a `MethodRef`'s stored signature.
        let owner_name = asm.alloc_string("Foo");
        let owner = asm.alloc_class_ref(ClassRef::new(owner_name, None, false, [].into()));
        let method_name = asm.alloc_string("Bar");
        let fn_sig = asm.sig(
            [Type::ClassRef(owner), Type::Int(crate::ir::Int::I32)],
            Type::Bool,
        );
        let mref = asm.alloc_methodref(crate::ir::MethodRef::new(
            owner,
            method_name,
            fn_sig,
            crate::ir::cilnode::MethodKind::Instance,
            vec![].into(),
        ));

        let tok = TokenSink::method_token(&mut mb, &mut asm, MethodDefIdx::from_raw(mref), &[]);
        assert_eq!(tok.table(), Token::TABLE_MEMBER_REF);
        let sig_off = mb.member_ref[(tok.rid() - 1) as usize].signature;
        let blob = &mb.blobs.as_bytes()[sig_off as usize..];
        // `SIG_HASTHIS`, param-count-prefix (1, NOT 2 — the receiver must not be counted),
        // return type ET_BOOLEAN, then the ONE real parameter ET_I4. `blob[0]` (`SIG_HASTHIS`)
        // has a length prefix from blob interning ahead of it in `#Blob` — read via the marker
        // bytes directly instead of assuming a fixed offset.
        assert_eq!(blob[1], sig::SIG_HASTHIS, "calling convention byte");
        assert_eq!(
            blob[2], 1,
            "param count must be 1 (the receiver is NOT counted)"
        );
        const ET_BOOLEAN: u8 = 0x02;
        const ET_I4: u8 = 0x08;
        assert_eq!(blob[3], ET_BOOLEAN, "return type");
        assert_eq!(
            blob[4], ET_I4,
            "the ONE real parameter, not the receiver's own ClassRef type"
        );
    }

    #[test]
    fn type_ref_interning_dedupes_identical_requests() {
        let mut mb = MetadataBuilder::new();
        let ext = mb.assembly_ref("SomeLib", AssemblyRefTarget::NameOnly);
        let a = mb.type_ref(Some(ext), "Some.Ns", "Type");
        let b = mb.type_ref(Some(ext), "Some.Ns", "Type");
        assert_eq!(a, b);
        assert_eq!(mb.type_ref.len(), 1);
        let c = mb.type_ref(Some(ext), "Some.Ns", "Other");
        assert_ne!(a, c);
        assert_eq!(mb.type_ref.len(), 2);
    }

    #[test]
    fn pinvoke_method_populates_impl_map_and_module_ref() {
        let mut mb = MetadataBuilder::new();
        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01);
            mb.blobs.intern(&out)
        };
        let _t = mb.add_type_def("", "Extern", false, None, None, None, &[]);
        let _m = mb.add_method(
            "libc_call",
            sig_blob,
            &[],
            &[],
            true,
            false,
            false,
            Some(("libc", Some("native_call"), PInvokeCallConv::Stdcall, true)),
            false,
            None,
        );
        assert_eq!(mb.impl_map.len(), 1);
        assert_eq!(mb.module_ref.len(), 1);
        assert_eq!(
            reader_strings_at_offset(&mb, mb.impl_map[0].import_name),
            "native_call"
        );
        assert_eq!(mb.impl_map[0].mapping_flags & 0x700, 0x300, "stdcall");
        assert_eq!(
            mb.impl_map[0].mapping_flags & 0x40,
            0x40,
            "SupportsLastError set"
        );
    }

    #[test]
    fn class_layout_row_added_for_explicit_layout_type() {
        let mut mb = MetadataBuilder::new();
        let t = mb.add_type_def("", "Packed", true, None, Some(4), Some(16), &[]);
        assert_eq!(mb.class_layout.len(), 1);
        assert_eq!(mb.class_layout[0].parent, t.rid());
        assert_eq!(mb.class_layout[0].packing_size, 4);
        assert_eq!(mb.class_layout[0].class_size, 16);
    }

    /// Regression for a `dotnet` load-time misattribution caught wiring `DIRECT_PE=1` into the
    /// linker: a caller that creates every `TypeDef` row UP FRONT (needed so a field's type can
    /// forward-reference a class def appearing later in iteration order — `export_pe`'s Pass 1)
    /// leaves every row's `FieldList`/`MethodList` (§II.22.37 run-start columns) stamped `1`,
    /// since `add_type_def` captures "one past the current end of field/method rows" AT ITS OWN
    /// CALL, before any field/method exists. Left unpatched, EVERY class's fields/methods read
    /// back as belonging to the FIRST class in the table (`dotnet` doesn't error — it silently
    /// resolves cross-class field/method references to whichever TypeDef the stale run actually
    /// covers, surfacing downstream as `TypeLoadException`/`FieldAccessException` far from the
    /// real cause). `set_type_def_field_list`/`set_type_def_method_list` re-stamp the correct
    /// run-start once a class's own rows are about to be appended; this test builds two classes
    /// with disjoint field/method sets and checks each `TypeDef`'s run genuinely covers only ITS
    /// OWN rows, not a `1..1` no-op or the other class's range.
    #[test]
    fn set_type_def_field_and_method_list_patches_the_run_start_not_left_at_the_placeholder() {
        let mut mb = MetadataBuilder::new();

        // Two `TypeDef`s created up front (mirrors `export_pe`'s Pass 1) — at this point BOTH
        // rows have the placeholder `field_list == method_list == 1`.
        let a = mb.add_type_def("", "A", false, None, None, None, &[]);
        let b = mb.add_type_def("", "B", false, None, None, None, &[]);
        assert_eq!(mb.type_def[(a.rid() - 1) as usize].field_list, 1);
        assert_eq!(mb.type_def[(b.rid() - 1) as usize].field_list, 1);
        assert_eq!(mb.type_def[(a.rid() - 1) as usize].method_list, 1);
        assert_eq!(mb.type_def[(b.rid() - 1) as usize].method_list, 1);

        // Populate `A`'s fields (2) then `B`'s fields (1), re-stamping immediately before each,
        // exactly as `export_pe`'s Pass 2 does.
        let field_sig = mb.field_sig_for_valuetype_token(a); // any resolvable signature blob.
        mb.set_type_def_field_list(a);
        mb.add_field("a0", field_sig, None);
        mb.add_field("a1", field_sig, None);
        mb.set_type_def_field_list(b);
        mb.add_field("b0", field_sig, None);

        let a_row = &mb.type_def[(a.rid() - 1) as usize];
        let b_row = &mb.type_def[(b.rid() - 1) as usize];
        assert_eq!(a_row.field_list, 1, "A's fields start at row 1 (a0)");
        assert_eq!(
            b_row.field_list, 3,
            "B's fields start at row 3 (b0), AFTER A's 2 fields"
        );
        assert_eq!(mb.field.len(), 3, "3 field rows total: a0, a1, b0");
        assert_eq!(
            &mb.strings.as_bytes()[mb.field[0].name as usize..][..2],
            b"a0"
        );
        assert_eq!(
            &mb.strings.as_bytes()[mb.field[2].name as usize..][..2],
            b"b0"
        );

        // Same shape for methods: `B` gets 2 methods, `A` gets 0 — checks a ZERO-method class
        // still gets a correct (empty) run for its neighbor's sake.
        let method_sig = {
            let mut blob = Vec::new();
            blob.push(sig::SIG_DEFAULT);
            blob.push(0); // 0 params
            blob.push(0x01); // ET_VOID return
            mb.blobs.intern(&blob)
        };
        mb.set_type_def_method_list(a);
        mb.set_type_def_method_list(b);
        mb.add_method(
            "b_m0",
            method_sig,
            &[],
            &[],
            true,
            false,
            false,
            None,
            false,
            None,
        );
        mb.add_method(
            "b_m1",
            method_sig,
            &[],
            &[],
            true,
            false,
            false,
            None,
            false,
            None,
        );

        let a_row = &mb.type_def[(a.rid() - 1) as usize];
        let b_row = &mb.type_def[(b.rid() - 1) as usize];
        assert_eq!(
            a_row.method_list, 1,
            "A owns zero methods: its run starts where B's begins"
        );
        assert_eq!(b_row.method_list, 1, "B's methods start at row 1 (b_m0)");
        assert_eq!(mb.method_def.len(), 2);
    }

    /// `add_blob_sized_valuetype` — the `__rcl_const_blob_{n}` carrier type a const-data
    /// `FieldRVA` static needs (Phase 1b Cluster C item 4, lesson 1's blob-sizing rule). Checks
    /// every column `il_exporter`'s equivalent text (`.class private explicit ansi sealed
    /// '__rcl_const_blob_{n}' extends [System.Runtime]System.ValueType {{ .pack 1 .size {n} }}`,
    /// `il_exporter/mod.rs:120`) implies: NotPublic, Sealed, ExplicitLayout flags; a `ClassLayout`
    /// row with `.pack 1` and `.size` set to the EXACT blob length (not rounded/truncated — the
    /// whole point of the lesson-1 fix); and `Extends` resolving to whatever `System.ValueType`
    /// token the caller passes in (this method doesn't create that TypeRef itself — the caller,
    /// e.g. `export_pe`'s existing `system_runtime_type_ref` helper, is expected to own that, same
    /// as every other `extends` resolution in this module).
    #[test]
    fn add_blob_sized_valuetype_is_private_sealed_explicit_and_exactly_sized() {
        let mut mb = MetadataBuilder::new();
        let value_type_ref = mb.type_ref(None, "System", "ValueType");

        let tok = mb.add_blob_sized_valuetype("__rcl_const_blob_37", value_type_ref, 37);
        assert_eq!(tok.table(), Token::TABLE_TYPE_DEF);

        let row = &mb.type_def[(tok.rid() - 1) as usize];
        assert_eq!(
            row.flags & 0x1,
            0,
            "must be NotPublic (0x0), not Public (0x1)"
        );
        assert_eq!(row.flags & 0x100, 0x100, "Sealed bit must be set");
        assert_eq!(row.flags & 0x10, 0x10, "ExplicitLayout bit must be set");
        assert_eq!(
            row.extends,
            encode_type_def_or_ref_token(value_type_ref),
            "Extends must resolve to the caller-supplied System.ValueType TypeRef"
        );

        assert_eq!(mb.class_layout.len(), 1);
        assert_eq!(mb.class_layout[0].parent, tok.rid());
        assert_eq!(
            mb.class_layout[0].packing_size, 1,
            ".pack 1, matching il_exporter's literal text"
        );
        assert_eq!(
            mb.class_layout[0].class_size, 37,
            "class size must be the blob's EXACT byte length, per the FieldRVA-sizing lesson \
             (commit 4b487f7) — a mismatch here is exactly the bug that lesson fixed"
        );

        let name = reader_strings_at_offset(&mb, row.name);
        assert_eq!(name, "__rcl_const_blob_37");
    }

    /// Two distinct blob sizes must land as two distinct `TypeDef`/`ClassLayout` row pairs (each
    /// `n` needs its own carrier type — `il_exporter` dedups by exact size via a sorted+deduped
    /// `Vec<usize>` over `asm.const_data.values()`, mod.rs:116-121). This module doesn't dedupe
    /// internally (the caller is expected to, same as `il_exporter`'s explicit dedup step) — this
    /// test documents that contract so a future caller doesn't assume dedup happens for free.
    #[test]
    fn add_blob_sized_valuetype_does_not_dedupe_distinct_sizes() {
        let mut mb = MetadataBuilder::new();
        let value_type_ref = mb.type_ref(None, "System", "ValueType");
        let a = mb.add_blob_sized_valuetype("__rcl_const_blob_4", value_type_ref, 4);
        let b = mb.add_blob_sized_valuetype("__rcl_const_blob_8", value_type_ref, 8);
        assert_ne!(a, b);
        assert_eq!(mb.class_layout.len(), 2);
        assert_eq!(mb.class_layout[0].class_size, 4);
        assert_eq!(mb.class_layout[1].class_size, 8);
    }

    /// Small helper shared by the two tests above: reads a `#Strings` offset back out via the
    /// same private `strings_eq`-adjacent access other tests in this module use directly (kept as
    /// a free fn since it's only needed here).
    fn reader_strings_at_offset(mb: &MetadataBuilder, off: u32) -> String {
        let bytes = mb.strings.as_bytes();
        let start = off as usize;
        let end = bytes[start..].iter().position(|&b| b == 0).unwrap() + start;
        std::str::from_utf8(&bytes[start..end]).unwrap().to_string()
    }

    #[test]
    fn resolver_reuses_type_def_row_for_a_class_defined_in_this_assembly() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let name = asm.alloc_string("MyType");
        asm.class_def(crate::ir::ClassDef::new(
            name,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        ))
        .unwrap();
        let cref = asm.alloc_class_ref(ClassRef::new(name, None, false, [].into()));

        let tok = mb.add_type_def("", "MyType", false, None, None, None, &[]);
        // Rid 2, not 1: `MetadataBuilder::new` seeds the mandatory `<Module>` pseudo-type at
        // TypeDef rid 1 (§II.22.37) — the first real class def lands on rid 2.
        assert_eq!(tok.rid(), 2);

        let coded = TypeDefOrRefResolver::type_def_or_ref(&mut mb, cref, &mut asm);
        assert_eq!(
            decode_type_def_or_ref(coded),
            tok,
            "must resolve to the TypeDef, not create a TypeRef"
        );
        assert_eq!(
            mb.type_ref.len(),
            0,
            "no TypeRef should be created for an in-assembly type"
        );
    }

    /// Regression test for a real bug caught wiring the `cd_interop` C#-consumer A/B battery
    /// target: a `TypeDef` for a DOTTED (namespaced) Rust-exported name (e.g. `cd_interop.Point`,
    /// the backend's real shape for a `#[repr(C)]` struct crossing the Rust/.NET boundary) must
    /// split its `Namespace`/`Name` columns on the last `.`, exactly like `ilasm` does (confirmed
    /// byte-for-byte against a real ilasm-built `cd_interop.dll`: `TypeDef.Namespace="cd_interop"`,
    /// `TypeDef.Name="Point"`) — NOT dump the whole dotted string into `Name` with an empty
    /// `Namespace`. Both shapes load and run identically under `dotnet` (CLR method/field
    /// resolution is token-based, never looks at `Namespace`), which is exactly why this shipped
    /// undetected through every earlier E2E/A-B round: only Roslyn's COMPILE-TIME reference
    /// resolution (`csc`/`dotnet build` against a `<Reference>`) looks a type up by
    /// `Namespace`+`Name`, and no prior round built a C# PROJECT against a pe_exporter-produced
    /// library (only `dotnet run`/`Assembly.Load` on the raw bytes) until this one did — the
    /// unsplit shape surfaced as `CS0246: The type or namespace name 'cd_interop' could not be
    /// found`. Exercises both halves together: `add_type_def` (population) and
    /// `TypeDefOrRefResolver::type_def_or_ref` -> `find_type_def` (lookup) must agree on the SAME
    /// split, or a self-referencing signature (e.g. a method returning this very type) would fail
    /// to resolve even though the row itself is correctly split.
    #[test]
    fn namespaced_type_def_splits_into_namespace_and_name_columns_matching_ilasm() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let name = asm.alloc_string("cd_interop.Point");
        asm.class_def(crate::ir::ClassDef::new(
            name,
            true,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        ))
        .unwrap();
        let cref = asm.alloc_class_ref(ClassRef::new(name, None, true, [].into()));

        let (namespace, short_name) = split_namespace("cd_interop.Point");
        let tok = mb.add_type_def(namespace, short_name, true, None, None, None, &[]);

        // The row itself must carry the SPLIT columns, not the whole dotted string in `Name`.
        let row = &mb.type_def[(tok.rid() - 1) as usize];
        assert!(
            mb.strings_eq(row.namespace, "cd_interop"),
            "TypeDef.Namespace must be \"cd_interop\""
        );
        assert!(
            mb.strings_eq(row.name, "Point"),
            "TypeDef.Name must be \"Point\", not the full dotted string"
        );

        // A self-referencing `ClassRef` (e.g. a method returning `Point`) must still resolve to
        // this SAME TypeDef token via `find_type_def`'s lookup, proving population and lookup
        // agree on the split (not just population alone).
        let coded = TypeDefOrRefResolver::type_def_or_ref(&mut mb, cref, &mut asm);
        assert_eq!(
            decode_type_def_or_ref(coded),
            tok,
            "a self-reference to a namespaced TypeDef must resolve via find_type_def's matching split, not fail to find it"
        );
        assert_eq!(
            mb.type_ref.len(),
            0,
            "no TypeRef should be created for an in-assembly type"
        );
    }

    #[test]
    fn projected_main_module_keeps_sentinel_self_references_on_the_public_type_def() {
        let mut mb = MetadataBuilder::new();
        mb.set_public_module_full_name(Some("Monark.PositionParser.NativeExports"));
        let mut asm = Assembly::default();
        let main = asm.main_module();
        let main_ref = asm[main].ref_to();
        let main_ref = asm.alloc_class_ref(main_ref);

        let tok = mb.add_type_def(
            "Monark.PositionParser",
            "NativeExports",
            false,
            None,
            None,
            None,
            &[],
        );
        let coded = TypeDefOrRefResolver::type_def_or_ref(&mut mb, main_ref, &mut asm);

        assert_eq!(decode_type_def_or_ref(coded), tok);
        assert!(
            mb.type_ref.is_empty(),
            "the sentinel self reference must not become a TypeRef"
        );
        let row = &mb.type_def[(tok.rid() - 1) as usize];
        assert!(mb.strings_eq(row.namespace, "Monark.PositionParser"));
        assert!(mb.strings_eq(row.name, "NativeExports"));
    }

    #[test]
    fn resolver_creates_type_ref_for_an_external_class() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let cref = ClassRef::console(&mut asm);

        let coded = TypeDefOrRefResolver::type_def_or_ref(&mut mb, cref, &mut asm);
        let tok = decode_type_def_or_ref(coded);
        assert_eq!(tok.table(), Token::TABLE_TYPE_REF);
        assert_eq!(mb.type_ref.len(), 1);
        assert_eq!(
            mb.assembly_ref.len(),
            1,
            "System.Console's assembly must be registered"
        );
    }

    #[test]
    fn resolver_caches_repeat_lookups_of_the_same_class_ref() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let cref = ClassRef::console(&mut asm);

        let a = TypeDefOrRefResolver::type_def_or_ref(&mut mb, cref, &mut asm);
        let b = TypeDefOrRefResolver::type_def_or_ref(&mut mb, cref, &mut asm);
        assert_eq!(a, b);
        assert_eq!(
            mb.type_ref.len(),
            1,
            "second lookup must reuse the cached row"
        );
    }

    #[test]
    fn generic_class_ref_resolves_to_the_open_shape() {
        // Per the task spec: a ClassRef with non-empty generics still resolves to the open
        // TypeRef/TypeDef row (sig.rs wraps GENERICINST itself).
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let open = ClassRef::span(&mut asm, Type::Int(Int::U8));
        let coded = TypeDefOrRefResolver::type_def_or_ref(&mut mb, open, &mut asm);
        let tok = decode_type_def_or_ref(coded);
        assert_eq!(tok.table(), Token::TABLE_TYPE_REF);
        assert_eq!(mb.type_ref.len(), 1);
    }

    /// Regression for `TypeLoadException: Could not load type '…\`2' from assembly '_'` caught
    /// wiring `DIRECT_PE=1` into the linker: `TokenSink::type_token`/`method_token`/`field_token`
    /// must NOT use `TypeDefOrRefResolver::type_def_or_ref`'s bare open-shape token for a generic
    /// declaring type — that's only valid INSIDE a signature blob (see that impl's doc). A
    /// `newobj`/`call`/`ldfld`/`castclass` OPERAND naming a closed generic instantiation (e.g.
    /// `Dictionary<int32,object>::.ctor()`) needs a `TypeSpec` (§II.22.39) carrying the full
    /// `GENERICINST` blob — `class_ref_token` (the shared helper both paths route through) is
    /// what makes that split. A non-generic `ClassRef` must still resolve to the plain
    /// `TypeDef`/`TypeRef` (no spurious `TypeSpec` row for the common case).
    #[test]
    fn generic_declaring_type_resolves_to_a_type_spec_not_the_bare_open_type_ref() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();

        // `Span<u8>` (a generic external BCL type, arity 1) as an instruction operand's type —
        // e.g. what `box`/`castclass`/`newobj` on it would need.
        let generic = ClassRef::span(&mut asm, Type::Int(Int::U8));
        let tok = TokenSink::type_token(&mut mb, &mut asm, Type::ClassRef(generic));
        assert_eq!(
            tok.table(),
            Token::TABLE_TYPE_SPEC,
            "a generic declaring type used as an operand must resolve to a TypeSpec"
        );
        assert_eq!(mb.type_spec.len(), 1);
        // The open TypeRef (`Span\`1`) still gets created too — the TypeSpec's blob references it
        // via the ordinary `type_def_or_ref` coded index inside `GENERICINST`.
        assert_eq!(mb.type_ref.len(), 1);

        // A non-generic ClassRef must NOT get the TypeSpec treatment — plain TypeRef, no new row.
        let plain = ClassRef::console(&mut asm);
        let plain_tok = TokenSink::type_token(&mut mb, &mut asm, Type::ClassRef(plain));
        assert_eq!(plain_tok.table(), Token::TABLE_TYPE_REF);
        assert_eq!(
            mb.type_spec.len(),
            1,
            "a non-generic ClassRef must not add a TypeSpec row"
        );
    }

    #[test]
    fn generic_external_type_ref_name_carries_the_arity_postfix() {
        // Real BCL metadata names a generic external type with a `` `N `` arity postfix baked
        // into the `TypeRef`/`TypeDef` `Name` column itself — confirmed against a real CoreCLR
        // `System.Runtime.Intrinsics.dll` (`.class extern forwarder
        // System.Runtime.Intrinsics.Vector128`1`, via `monodis`). `il_exporter` gets this for
        // free because ilasm parses the postfix back out of the quoted `'Name`1'` text it emits;
        // this writer has no assembler, so `type_def_or_ref` must append it by hand before
        // interning/splitting the name, or the TypeRef row resolves to a type that doesn't exist.
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let open = ClassRef::span(&mut asm, Type::Int(Int::U8));

        let _ = TypeDefOrRefResolver::type_def_or_ref(&mut mb, open, &mut asm);

        assert_eq!(mb.type_ref.len(), 1);
        let row = &mb.type_ref[0];
        assert!(mb.strings_eq(row.namespace, "System"));
        assert!(
            mb.strings_eq(row.name, "Span`1"),
            "generic external TypeRef name must carry the `N postfix, matching real BCL metadata"
        );
    }

    /// Regression for `MissingMethodException: Method not found: 'Int32
    /// System.Linq.Queryable.Count(System.Linq.IQueryable\`1<!!0>)'` caught wiring `DIRECT_PE=1`
    /// into cd_linq_expr's `IntQuery::count` (a call through `gmethod1`'s WF-9 generic-method
    /// bridge, e.g. `Queryable.Count<int32>(IQueryable<int32>)`). `TokenSink::method_token`'s
    /// out-of-assembly MemberRef-building arm hardcoded `generic_params = 0` and never OR'd
    /// `SIG_GENERIC` into the calling convention, even when `generic_args` (the call site's own
    /// instantiation, e.g. `[Int32]`) is non-empty — so the base MemberRef a `MethodSpec` points
    /// at (§II.22.29) was encoded as an ordinary non-generic signature. §II.23.2.2 requires that
    /// base MemberRef to carry `GENERIC` (0x10) + the method's own generic-parameter COUNT (not
    /// the instantiation's concrete types — those live only in the MethodSpec's own blob); its
    /// parameter positions reference that arity via `ET_MVAR`/`!!0`
    /// (`Type::PlatformGeneric(_, CallGeneric)`, independently confirmed correct elsewhere).
    /// Ground-truthed against the actual `.il` ilasm consumed for cd_linq_expr's `Count` call:
    /// `call int32 …Queryable::'Count'<int32>(class …IQueryable\`1<!!0>)` — the `<int32>`
    /// instantiation syntax is exactly what forces ilasm to stamp `GENERIC` on the underlying
    /// MemberRef; a hand-rolled writer gets no such auto-detection from the assembler.
    #[test]
    fn generic_method_call_stamps_sig_generic_on_the_base_member_ref() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();

        // `Queryable::Count<T>(this IQueryable<T> source) -> int32`, called as `Count<int32>`.
        // `Type::PlatformGeneric(0, CallGeneric)` inside the receiver's ClassRef generics is
        // exactly what `call_gmethod` (src/terminator/call.rs) builds for the method's own `!!0`
        // marker — mirrored here rather than re-deriving it.
        let owner_name = asm.alloc_string("Queryable");
        let owner = asm.alloc_class_ref(ClassRef::new(owner_name, None, false, [].into()));
        let iqueryable_name = asm.alloc_string("IQueryable");
        let mvar0 = Type::PlatformGeneric(0, crate::ir::tpe::GenericKind::CallGeneric);
        let iqueryable_of_mvar0 =
            asm.alloc_class_ref(ClassRef::new(iqueryable_name, None, false, [mvar0].into()));
        let method_name = asm.alloc_string("Count");
        let fn_sig = asm.sig(
            [Type::ClassRef(iqueryable_of_mvar0)],
            Type::Int(crate::ir::Int::I32),
        );
        let mref = asm.alloc_methodref(crate::ir::MethodRef::new(
            owner,
            method_name,
            fn_sig,
            crate::ir::cilnode::MethodKind::Static,
            vec![].into(),
        ));

        // The call site's instantiation: `Count<int32>`.
        let generic_args = [Type::Int(crate::ir::Int::I32)];
        let tok = TokenSink::method_token(
            &mut mb,
            &mut asm,
            MethodDefIdx::from_raw(mref),
            &generic_args,
        );
        assert_eq!(
            tok.table(),
            Token::TABLE_METHOD_SPEC,
            "call site wraps the base in a MethodSpec"
        );

        // Unwrap the MethodSpec to inspect the base MemberRef's OWN signature (not the
        // instantiation blob, which was already correct before this fix). `MethodSpecRow.method`
        // is a MethodDefOrRef-coded u32 (`(rid << 1) | tag`, tag=1 for MemberRef, mirroring
        // `encode_method_def_or_ref`) — not a plain `Token`.
        let spec_row = &mb.method_spec[(tok.rid() - 1) as usize];
        assert_eq!(
            spec_row.method & 1,
            1,
            "base must be a MemberRef, not a MethodDef"
        );
        let base_rid = spec_row.method >> 1;
        let sig_off = mb.member_ref[(base_rid - 1) as usize].signature;
        let blob = &mb.blobs.as_bytes()[sig_off as usize..];
        // Blob has a length-prefix byte from interning ahead of the actual signature bytes.
        assert_eq!(
            blob[1] & sig::SIG_GENERIC,
            sig::SIG_GENERIC,
            "the base MemberRef for a generic-method call site must carry SIG_GENERIC, or CoreCLR \
             resolves it as a non-generic overload and MissingMethodExceptions"
        );
        assert_eq!(
            blob[2], 1,
            "generic parameter COUNT (the method has one type parameter)"
        );
        assert_eq!(
            blob[3], 1,
            "value parameter count (the receiver is not counted)"
        );
    }

    #[test]
    fn method_spec_wraps_generic_call_tokens() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let ext = mb.assembly_ref("SomeLib", AssemblyRefTarget::NameOnly);
        let tref = mb.type_ref(Some(ext), "", "Generic");
        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_GENERIC | sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 1);
            write_compressed_u32(&mut out, 0);
            out.push(0x01);
            mb.blobs.intern(&out)
        };
        let base = mb.member_ref(tref, "DoIt", sig_blob);
        let inst_blob = {
            let mut out = Vec::new();
            sig::encode_method_spec_sig(&[Type::Int(Int::I32)], &mut asm, &mut mb, &mut out);
            mb.blobs.intern(&out)
        };
        let spec = mb.method_spec(base, inst_blob);
        assert_eq!(spec.table(), Token::TABLE_METHOD_SPEC);
        assert_eq!(mb.method_spec.len(), 1);
    }

    #[test]
    fn f128_type_token_is_not_yet_supported() {
        // sig::encode_type still todo!()s on f128 (a pre-existing, out-of-scope gap noted in
        // sig.rs); confirm type_token surfaces the same gap rather than silently mishandling it.
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            TokenSink::type_token(&mut mb, &mut asm, Type::Float(Float::F128))
        }));
        assert!(
            result.is_err(),
            "f128 signature encoding is a known todo!() in sig.rs"
        );
    }

    #[test]
    fn method_kind_smoke() {
        // Exercises the MethodKind import so clippy doesn't flag an unused import if every other
        // test above is trimmed during future edits.
        let _ = MethodKind::Static;
    }

    /// Regression for the SIGSEGV-on-first-virtual-dispatch bug found bisecting cd_collections
    /// under `DIRECT_PE=1`: `add_method`'s `is_ctor` flag is the ONLY thing that sets
    /// `SpecialName (0x0800) | RTSpecialName (0x1000)` (§II.23.1.10) TOGETHER on a `MethodDef` row
    /// (`add_event` stamps bare `SpecialName` on event accessors, never `RTSpecialName`). The
    /// assembly-wide static initializer built by `Assembly::cctor()` (asm.rs) is a
    /// `MethodKind::Static` method literally named `.cctor` — NOT `MethodKind::Constructor` — so
    /// a caller that derives `is_ctor` purely from `MethodKind::Constructor` (as `export.rs`'s
    /// driver originally did) never sets these flags for it. Without `RTSpecialName` the CLR
    /// loader does not recognize the method as a type initializer and never auto-invokes it
    /// before first access to the type's static fields — every static/const-data/vtable
    /// initializer inside `.cctor` (including `dyn Trait` vtable slots, populated by `ldftn`
    /// writes INSIDE `.cctor`, not `FieldRVA` data) silently never runs, leaving those fields
    /// zeroed. `il_exporter`'s emitted `.il` text for `.cctor` has no `specialname`/
    /// `rtspecialname` keywords either (ground-truthed against the actual `.il` ilasm consumes
    /// for cd_collections) — ilasm auto-recognizes the reserved name `.cctor` and stamps the
    /// flags in regardless, so a hand-rolled writer must special-case the name explicitly
    /// (`export.rs`'s `is_ctor` now also checks `name == asm::CCTOR`).
    #[test]
    fn add_method_sets_specialname_rtspecialname_for_is_ctor_true() {
        let mut mb = MetadataBuilder::new();
        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01); // void
            mb.blobs.intern(&out)
        };
        let _t = mb.add_type_def("", "MainModule", false, None, None, None, &[]);
        // Mirrors `export.rs`'s call for `.cctor`: MethodKind is Static, but the reserved-name
        // check must still route `is_ctor = true` into `add_method`.
        let tok = mb.add_method(
            ".cctor",
            sig_blob,
            &[],
            &[],
            true,
            false,
            true,
            None,
            false,
            None,
        );
        let row = &mb.method_def[(tok.rid() - 1) as usize];
        assert_eq!(
            row.flags & (0x1000 | 0x0800),
            0x1000 | 0x0800,
            ".cctor's MethodDef row must carry SpecialName|RTSpecialName or the CLR will never \
             auto-invoke it as a type initializer"
        );
    }

    #[test]
    fn add_method_does_not_set_specialname_for_an_ordinary_static_method() {
        // Negative case: a plain static helper (e.g. `.tcctor`, which this project explicitly
        // `call`s rather than relying on CLR auto-invocation) must NOT get SpecialName/
        // RTSpecialName — those flags are reserved for `.cctor`/`.ctor` and setting them on an
        // arbitrary method would be its own (different) metadata bug.
        let mut mb = MetadataBuilder::new();
        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01);
            mb.blobs.intern(&out)
        };
        let _t = mb.add_type_def("", "MainModule", false, None, None, None, &[]);
        let tok = mb.add_method(
            ".tcctor",
            sig_blob,
            &[],
            &[],
            true,
            false,
            false,
            None,
            false,
            None,
        );
        let row = &mb.method_def[(tok.rid() - 1) as usize];
        assert_eq!(row.flags & (0x1000 | 0x0800), 0);
    }

    #[test]
    fn add_method_preserves_assembly_accessibility() {
        let mut mb = MetadataBuilder::new();
        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01); // void
            mb.blobs.intern(&out)
        };
        let _t = mb.add_type_def("", "FactoryOwned", false, None, None, None, &[]);
        let tok = mb.add_method_with_access(
            ".ctor",
            crate::Access::Assembly,
            sig_blob,
            &[],
            &[],
            false,
            false,
            true,
            None,
            false,
            None,
        );
        let row = &mb.method_def[(tok.rid() - 1) as usize];
        assert_eq!(
            row.flags & 0x7,
            0x3,
            "MethodAttributes.MemberAccessMask must encode Assembly, not Public"
        );
    }

    /// **Regression test for the `pal_threads` `TypeLoadException: Abstract method with non-zero
    /// RVA` load bug** (root-caused via `UnmanagedThreadStart::Start`, `cilly/src/ir/builtins/
    /// thread.rs` — a real virtual method with a real body). `add_method`'s `is_virtual` branch
    /// previously OR'd in `0x0400`, believing it to be `NewSlot` (§II.23.1.10) — it is actually
    /// `Abstract`. `NewSlot` is `0x0100`. This silently marked EVERY virtual method this exporter
    /// ever emitted as `Abstract`, regardless of whether it had a real body/RVA; CoreCLR's native
    /// type loader enforces "abstract methods must have RVA == 0" (§II.22.26) at TYPE-LOAD time
    /// (not method-call time), which is why the resulting `TypeLoadException` surfaced arbitrarily
    /// far from the actual defect in a stack trace, and why a purely-managed reader
    /// (`System.Reflection.Metadata`, which never enforces this invariant) found nothing wrong.
    #[test]
    fn add_method_virtual_sets_newslot_not_abstract() {
        let mut mb = MetadataBuilder::new();
        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01); // void
            mb.blobs.intern(&out)
        };
        let _t = mb.add_type_def("", "Start", false, None, None, None, &[]);
        let tok = mb.add_method(
            "Start",
            sig_blob,
            &[],
            &[],
            false,
            true,
            false,
            None,
            false,
            None,
        );
        let row = &mb.method_def[(tok.rid() - 1) as usize];
        const NEW_SLOT: u16 = 0x0100;
        const ABSTRACT: u16 = 0x0400;
        assert_eq!(
            row.flags & NEW_SLOT,
            NEW_SLOT,
            "a virtual method must carry NewSlot (0x0100)"
        );
        assert_eq!(
            row.flags & ABSTRACT,
            0,
            "a virtual method with a real body must NOT carry Abstract (0x0400) — CoreCLR's \
             native type loader rejects Abstract+nonzero-RVA as malformed at type-load time"
        );
    }

    /// Regression test for the `pe_exporter` `AggressiveInlining` parity gap documented in
    /// `pdb.rs`'s module doc (Phase-0 probe, gap (a)): `il_exporter` hints RyuJIT to inline small,
    /// single-block, handler-free leaf bodies via `MethodImplOptions.AggressiveInlining` in its
    /// emitted `.il` text, but under `DIRECT_PE=1` the `MethodDefRow.impl_flags` never carried the
    /// `0x100` `AggressiveInlining` bit (§II.23.1.11) — confirmed on the real fractal-rs demo's
    /// hot `render_mandelbrot` kernel, whose 3 saturating-float-to-int `cast_f64_*` helper calls
    /// per pixel (`cilly::ir::builtins::casts::insert_casts`) went through non-inlined calls only
    /// under the direct-PE path. `add_method`'s new `aggressive_inline: bool` parameter is the
    /// fix's plumbing; `export.rs` computes it from the same shape `il_exporter` checks.
    #[test]
    fn add_method_aggressive_inline_true_sets_the_impl_attributes_bit() {
        let mut mb = MetadataBuilder::new();
        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01); // void
            mb.blobs.intern(&out)
        };
        let _t = mb.add_type_def("", "MainModule", false, None, None, None, &[]);
        let tok = mb.add_method(
            "Leaf",
            sig_blob,
            &[],
            &[],
            true,
            false,
            false,
            None,
            true,
            None,
        );
        let row = &mb.method_def[(tok.rid() - 1) as usize];
        const AGGRESSIVE_INLINING: u16 = 0x0100;
        assert_eq!(
            row.impl_flags & AGGRESSIVE_INLINING,
            AGGRESSIVE_INLINING,
            "aggressive_inline=true must set MethodImplAttributes.AggressiveInlining (0x100) in \
             impl_flags"
        );
    }

    #[test]
    fn add_method_aggressive_inline_false_leaves_impl_attributes_at_managed_default() {
        let mut mb = MetadataBuilder::new();
        let sig_blob = {
            let mut out = Vec::new();
            out.push(sig::SIG_DEFAULT);
            write_compressed_u32(&mut out, 0);
            out.push(0x01);
            mb.blobs.intern(&out)
        };
        let _t = mb.add_type_def("", "MainModule", false, None, None, None, &[]);
        let tok = mb.add_method(
            "NotInlined",
            sig_blob,
            &[],
            &[],
            true,
            false,
            false,
            None,
            false,
            None,
        );
        let row = &mb.method_def[(tok.rid() - 1) as usize];
        assert_eq!(
            row.impl_flags, 0,
            "aggressive_inline=false (the default for large/multi-block/handler-bearing bodies) \
             must leave impl_flags at Managed/IL (0x0), matching il_exporter's non-hinted methods"
        );
    }
}
