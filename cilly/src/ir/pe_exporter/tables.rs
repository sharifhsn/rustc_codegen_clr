//! The ECMA-335 metadata tables (§II.22) this backend needs, plus the `#~` stream container
//! that holds them (§II.24.2.6).
//!
//! Scope: exactly the tables `il_exporter` drives, per the inventory in
//! `docs/PE_EMISSION_PLAN.md` — Module, TypeRef, TypeDef, Field, MethodDef, Param,
//! InterfaceImpl, MemberRef, CustomAttribute, ClassLayout, FieldLayout, StandAloneSig, TypeSpec,
//! ModuleRef, ImplMap, FieldRVA, Assembly, AssemblyRef, MethodSpec. No `Constant` table: default
//! field values in this backend are always `FieldRVA` blobs (see `il_exporter`'s `.data cil`
//! static-field rendering), never metadata-`Constant` literals.
//!
//! Pipeline: **populate** (the `add_*`/`*_ref` methods, called while walking the `Assembly`) →
//! **size** (row counts fix each table's row-index width; heap final sizes fix `HeapSizes`) →
//! **serialize** (`#~` stream bytes, `Valid`/`Sorted` bitmasks, then the four heap streams).
//! Row order within a table is insertion order except where §II.22 requires a *sorted* table
//! (`InterfaceImpl`, `ClassLayout`, `FieldLayout`, `FieldRVA`, `ImplMap`, `MethodSpec` is NOT
//! sorted) — sorting is a `serialize()`-time concern, not a population-time one, so implementers
//! can append rows in whatever order is convenient while walking the `Assembly`.

use super::heaps::{write_compressed_u32, BlobHeap, GuidHeap, StringsHeap, UserStringHeap};
use super::sig::{self, TypeDefOrRefResolver};
use crate::ir::{Assembly, ClassRef, FieldDesc, Interned, MethodDefIdx, StaticFieldDesc, Type};
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
    pub const TABLE_CUSTOM_ATTRIBUTE: u32 = 0x0C;
    pub const TABLE_CLASS_LAYOUT: u32 = 0x0F;
    pub const TABLE_FIELD_LAYOUT: u32 = 0x10;
    pub const TABLE_STAND_ALONE_SIG: u32 = 0x11;
    pub const TABLE_MODULE_REF: u32 = 0x1A;
    pub const TABLE_TYPE_SPEC: u32 = 0x1B;
    pub const TABLE_IMPL_MAP: u32 = 0x1C;
    pub const TABLE_FIELD_RVA: u32 = 0x1D;
    pub const TABLE_ASSEMBLY: u32 = 0x20;
    pub const TABLE_ASSEMBLY_REF: u32 = 0x23;
    pub const TABLE_METHOD_SPEC: u32 = 0x2B;
    /// The `#US` (User String) heap's "table id" (§II.22.2) — not a real metadata table, but
    /// `ldstr` tokens are shaped identically (`0x70 << 24 | offset`), so [`Token`] represents
    /// them the same way.
    const TABLE_USER_STRING: u32 = 0x70;

    /// Builds a token from a table id (§II.22 table-id byte) and a 1-based row index.
    #[must_use]
    pub fn new(table: u32, rid: u32) -> Self {
        debug_assert!(table <= 0xFF, "table id {table:#x} doesn't fit a token byte");
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
/// versioned BCL assembly (mirrors `il_exporter`'s `.assembly extern '<name>' { .ver …
/// .publickeytoken = (…) }` for `is_bcl_assembly` names, lines ~87-105) or a bare name-only
/// reference (mirrors the `else` arm, lines ~96-104, used for a consumer's own non-BCL library).
pub enum AssemblyRefTarget<'a> {
    /// A BCL assembly: `.ver` triplet (from `dotnet_version().assembly_ver()`) + the shared ECMA
    /// public-key token (`B0 3F 5F 7F 11 D5 0A 3A`).
    Bcl { version: (u16, u16, u16, u16) },
    /// A consumer-supplied assembly, referenced by simple name only — no version, no token.
    NameOnly,
    #[doc(hidden)]
    _Marker(std::marker::PhantomData<&'a ()>),
}

/// The fixed ECMA public-key token every BCL assembly reference carries (§II.22.5), matching
/// `il_exporter`'s `B0 3F 5F 7F 11 D5 0A 3A` literal.
const ECMA_PUBLIC_KEY_TOKEN: [u8; 8] = [0xB0, 0x3F, 0x5F, 0x7F, 0x11, 0xD5, 0x0A, 0x3A];

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
}

/// The 64-bit `Valid`/`Sorted` bitmask position for each table id (§II.24.2.6: bit `N` set iff
/// table `N` has at least one row / must be emitted sorted). Table ids double as bit indices
/// since every id in this backend's inventory is `< 64`.
const SORTED_TABLES: &[u32] = &[
    Token::TABLE_INTERFACE_IMPL,
    Token::TABLE_CLASS_LAYOUT,
    Token::TABLE_FIELD_LAYOUT,
    Token::TABLE_FIELD_RVA,
    Token::TABLE_IMPL_MAP,
];

impl MetadataBuilder {
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

    /// Sets [`MetadataBuilder::is_lib`] — see that field's doc. Must be called before any
    /// `AssemblyRef` row is created (i.e. right after [`MetadataBuilder::new`]), since it only
    /// affects rows created from that point on; `export_pe` calls this first, before Pass 0.
    pub fn set_is_lib(&mut self, is_lib: bool) {
        self.is_lib = is_lib;
    }

    /// Interns an `AssemblyRef` row (§II.22.5), returning its token. `name` is the `.NET`
    /// assembly identity (e.g. `"System.Runtime"`); `target` selects the BCL-versioned vs.
    /// name-only shape per `il_exporter`'s `is_bcl_assembly` split (lines ~87-105).
    pub fn assembly_ref(&mut self, name: &str, target: AssemblyRefTarget<'_>) -> Token {
        let name_off = self.strings.intern(name);
        let (major, minor, build, revision, public_key_or_token) = match target {
            AssemblyRefTarget::Bcl {
                version: (maj, min, bui, rev),
            } => (maj, min, bui, rev, self.blobs.intern(&ECMA_PUBLIC_KEY_TOKEN)),
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
    pub fn type_ref(&mut self, resolution_scope: Option<Token>, namespace: &str, name: &str) -> Token {
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
    /// iteration order, resolved via [`MetadataBuilder::find_type_def`]) gets a WRONG
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
        assert_eq!(tok.table(), Token::TABLE_TYPE_DEF, "not a TypeDef token: {tok:?}");
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
        assert_eq!(tok.table(), Token::TABLE_TYPE_DEF, "not a TypeDef token: {tok:?}");
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
            self.field_layout.push(FieldLayoutRow {
                offset,
                field: rid,
            });
        }
        tok
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
    /// rows (§II.22.33, one per named argument — mirrors `MethodDef::arg_names()`). The body RVA
    /// is unknown until `body.rs` assembles bytes and `pe.rs` lays them out, so it starts at 0
    /// and must be patched via [`MetadataBuilder::set_method_body_rva`] before `serialize()`.
    pub fn add_method(
        &mut self,
        name: &str,
        signature_blob: u32,
        param_names: &[Option<&str>],
        is_static: bool,
        is_virtual: bool,
        is_ctor: bool,
        pinvoke: Option<(&str, bool)>,
    ) -> Token {
        // §II.23.1.10 `MethodAttributes`: the low 3 bits are `MemberAccessMask`, numbered
        // identically to `FieldAttributes::FieldAccessMask` (see `add_field`'s doc) —
        // CompilerControlled=0x0, Private=0x1, FamANDAssem=0x2, Assembly=0x3, Family=0x4,
        // FamORAssem=0x5, **Public=0x6**. 0x10 Static, 0x40 Virtual, **0x100 NewSlot** (paired with
        // Virtual so it doesn't try to override a base slot), 0x1000 SpecialName | 0x800
        // RTSpecialName (ctors only), 0x2000 PInvokeImpl.
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
        let mut flags: u16 = 0x6; // public
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
        let impl_flags: u16 = if pinvoke.is_some() { 0x80 } else { 0x0 };
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
        for (i, pname) in param_names.iter().enumerate() {
            let sequence = u16::try_from(i + 1).unwrap();
            let name_off = pname.map_or(0, |n| self.strings.intern(n));
            self.param.push(ParamRow {
                flags: 0,
                sequence,
                name: name_off,
            });
        }
        if let Some((lib, preserve_errno)) = pinvoke {
            let module_ref = self.intern_module_ref(lib);
            // §II.23.1.7 `PInvokeAttributes`: 0x1 NoMangle | 0x4 CharSetAnsi | cdecl (0x200) |
            // (0x40 SupportsLastError when `preserve_errno`) — mirrors `il_exporter`'s
            // `pinvokeimpl("<lib>" cdecl [lasterr])` rendering.
            let mut mapping_flags: u16 = 0x1 | 0x4 | 0x200;
            if preserve_errno {
                mapping_flags |= 0x40;
            }
            let import_name = self.strings.intern(name);
            self.impl_map.push(ImplMapRow {
                mapping_flags,
                member_forwarded: encode_member_forwarded(tok),
                import_name,
                import_scope: module_ref.rid(),
            });
        }
        tok
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
        assert_eq!(method.table(), Token::TABLE_METHOD_DEF, "not a MethodDef token");
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
        assert_eq!(class.table(), Token::TABLE_TYPE_DEF, "InterfaceImpl.Class must be a TypeDef");
        self.interface_impl.push(InterfaceImplRow {
            class: class.rid(),
            interface: encode_type_def_or_ref_token(interface),
        });
        let rid = u32::try_from(self.interface_impl.len()).unwrap();
        Token::new(Token::TABLE_INTERFACE_IMPL, rid)
    }

    /// Emits the **only** custom attribute this backend produces: a bare
    /// `[ThreadStaticAttribute]` (`System.ThreadStaticAttribute::.ctor()`) on a static field,
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
        // `crate::ir::dotnet_version()` is a free function in this same crate (cilly), so no
        // threading is needed — mirrors `il_exporter`'s `dv_ver = dotnet_version().assembly_ver()`
        // (mod.rs:74). Uses the tuple sibling since `AssemblyRefRow`'s columns are raw `u16`s
        // (§II.22.5), not the `"8:0:0:0"` string il_exporter interpolates into IL text.
        //
        // Gated on `self.is_lib` exactly like `il_exporter`'s `if self.is_lib { … }` (mod.rs:70):
        // an executable gets a name-only (`0.0.0.0`) reference — mirrors ilasm's own inferred-extern
        // default for an `.exe`'s implicit `[[assembly]Type` uses — while a `.dll` gets the real BCL
        // version+token so a C# compiler can bind it directly (CS0012 otherwise). See
        // `MetadataBuilder::is_lib`'s doc for the concrete `FileLoadException` this fixes.
        let target = if self.is_lib {
            AssemblyRefTarget::Bcl {
                version: crate::ir::dotnet_version().assembly_ver_tuple(),
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
            .unwrap_or_else(|| panic!("no pending FieldRVA for {field:?} — was add_static_field called with rva_data?"));
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
        let widths = Widths::compute(&sizes, &self.strings, &self.blobs, &self.guids, &self.user_strings);

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
        }
    }

    fn serialize_tables(&self, sizes: &RowCounts, widths: &Widths) -> Vec<u8> {
        // Every table this backend can ever emit, in ascending table-id order (§II.24.2.6
        // requires tables be written in table-id order regardless of population order).
        let table_rowcounts: [(u32, usize); 19] = [
            (Token::TABLE_MODULE, sizes.module),
            (Token::TABLE_TYPE_REF, sizes.type_ref),
            (Token::TABLE_TYPE_DEF, sizes.type_def),
            (Token::TABLE_FIELD, sizes.field),
            (Token::TABLE_METHOD_DEF, sizes.method_def),
            (Token::TABLE_PARAM, sizes.param),
            (Token::TABLE_INTERFACE_IMPL, sizes.interface_impl),
            (Token::TABLE_MEMBER_REF, sizes.member_ref),
            (Token::TABLE_CUSTOM_ATTRIBUTE, sizes.custom_attribute),
            (Token::TABLE_CLASS_LAYOUT, sizes.class_layout),
            (Token::TABLE_FIELD_LAYOUT, sizes.field_layout),
            (Token::TABLE_STAND_ALONE_SIG, sizes.standalone_sig),
            (Token::TABLE_MODULE_REF, sizes.module_ref),
            (Token::TABLE_TYPE_SPEC, sizes.type_spec),
            (Token::TABLE_IMPL_MAP, sizes.impl_map),
            (Token::TABLE_FIELD_RVA, sizes.field_rva),
            (Token::TABLE_ASSEMBLY, sizes.assembly),
            (Token::TABLE_ASSEMBLY_REF, sizes.assembly_ref),
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
        self.write_custom_attribute_rows(&mut out, widths);
        self.write_class_layout_rows(&mut out, widths);
        self.write_field_layout_rows(&mut out, widths);
        self.write_standalone_sig_rows(&mut out, widths);
        self.write_module_ref_rows(&mut out, widths);
        self.write_type_spec_rows(&mut out, widths);
        self.write_impl_map_rows(&mut out, widths);
        self.write_field_rva_rows(&mut out, widths);
        self.write_assembly_rows(&mut out, widths);
        self.write_assembly_ref_rows(&mut out, widths);
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
/// backend only ever attaches a `CustomAttribute` to a `Field` (tag 1), but the encoder accepts
/// any token whose table appears in the spec's ordering so future callers (e.g. attaching one to
/// a `MethodDef`) aren't blocked.
fn encode_has_custom_attribute(token: Token) -> u32 {
    let tag = match token.table() {
        Token::TABLE_METHOD_DEF => 0,
        Token::TABLE_FIELD => 1,
        Token::TABLE_TYPE_REF => 3,
        Token::TABLE_TYPE_DEF => 4,
        Token::TABLE_PARAM => 5,
        Token::TABLE_INTERFACE_IMPL => 6,
        Token::TABLE_MEMBER_REF => 7,
        Token::TABLE_MODULE => 8,
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

    type_def_or_ref_wide: bool,
    resolution_scope_wide: bool,
    member_ref_parent_wide: bool,
    method_def_or_ref_wide: bool,
    has_custom_attribute_wide: bool,
    custom_attribute_type_wide: bool,
    member_forwarded_wide: bool,
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
        let resolution_scope_max = sizes.module.max(sizes.module_ref).max(sizes.assembly_ref).max(sizes.type_ref);
        let member_ref_parent_max = sizes
            .type_def
            .max(sizes.type_ref)
            .max(sizes.module_ref)
            .max(sizes.method_def)
            .max(sizes.type_spec);
        let method_def_or_ref_max = sizes.method_def.max(sizes.member_ref);
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
            .max(sizes.type_spec);
        let custom_attribute_type_max = sizes.method_def.max(sizes.member_ref);
        let member_forwarded_max = sizes.field.max(sizes.method_def);

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
            type_def_or_ref_wide: coded_wide(2, type_def_or_ref_max),
            resolution_scope_wide: coded_wide(2, resolution_scope_max),
            member_ref_parent_wide: coded_wide(3, member_ref_parent_max),
            method_def_or_ref_wide: coded_wide(1, method_def_or_ref_max),
            has_custom_attribute_wide: coded_wide(5, has_custom_attribute_max),
            custom_attribute_type_wide: coded_wide(3, custom_attribute_type_max),
            member_forwarded_wide: coded_wide(1, member_forwarded_max),
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
        let raw_name = &asm[class_ref.name()];
        let tok = if asm.class_ref_to_def(cref).is_some() {
            // Defined in this assembly: the corresponding TypeDef row must already have been
            // added by `add_type_def` (population walks class defs before any signature needs
            // to resolve one) — if not, this is a caller-ordering bug, not a spec question.
            self.find_type_def(raw_name).unwrap_or_else(|| {
                panic!("ClassRef {raw_name:?} resolves to a class def not yet added via add_type_def")
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
    fn class_ref_token(&mut self, asm: &mut Assembly, cref: Interned<ClassRef>) -> Token {
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
    /// Finds a previously-added `TypeDef` row by its (already-shortened) name. Namespace is
    /// always emitted empty by this backend (mirrors `il_exporter`, which never splits a Rust
    /// mangled name into namespace+name — the full name goes in `Name` and `Namespace` stays
    /// `""`), so lookup only needs to compare `Name`.
    fn find_type_def(&self, raw_name: &str) -> Option<Token> {
        let shortened = dotnet_class_name(raw_name);
        for (i, row) in self.type_def.iter().enumerate() {
            if self.strings_eq(row.name, &shortened) {
                return Some(Token::new(Token::TABLE_TYPE_DEF, u32::try_from(i + 1).unwrap()));
            }
        }
        None
    }

    /// Finds-or-creates an `AssemblyRef` row for `name`, applying the same BCL-vs-consumer split
    /// as `il_exporter`'s `is_bcl_assembly` (this local port avoids depending on `il_exporter`,
    /// per the hard constraint that `pe_exporter` may not import it). Public so other `pe_exporter`
    /// modules (e.g. `export::export_pe`, bootstrapping a `System.Object`/`System.ValueType`
    /// `TypeRef`) share the same deduplicated row instead of calling the always-inserts
    /// [`MetadataBuilder::assembly_ref`] directly and creating duplicate rows for repeated
    /// bootstrap references.
    pub fn find_or_create_assembly_ref(&mut self, name: &str) -> Token {
        for (i, row) in self.assembly_ref.iter().enumerate() {
            if self.strings_eq(row.name, name) {
                return Token::new(Token::TABLE_ASSEMBLY_REF, u32::try_from(i + 1).unwrap());
            }
        }
        let target = if is_bcl_assembly(name) && self.is_lib {
            // Same single source of truth as `system_runtime_assembly_ref`: `dotnet_version()` is
            // reachable here with zero threading (free fn, same crate) — mirrors `il_exporter`'s
            // `dv_ver`. Also gated on `self.is_lib` for the same reason `system_runtime_assembly_ref`
            // is — see `MetadataBuilder::is_lib`'s doc.
            AssemblyRefTarget::Bcl {
                version: crate::ir::dotnet_version().assembly_ver_tuple(),
            }
        } else {
            AssemblyRefTarget::NameOnly
        };
        self.assembly_ref(name, target)
    }
}

/// Local port of `il_exporter::is_bcl_assembly` (kept private/duplicated rather than imported,
/// per the hard constraint that `pe_exporter` code must not depend on `il_exporter`).
fn is_bcl_assembly(name: &str) -> bool {
    name.starts_with("System")
        || name.starts_with("Microsoft")
        || matches!(name, "mscorlib" | "netstandard" | "WindowsBase")
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
/// Only used for `TypeRef` (external, §II.22.38 `TypeNamespace`); `TypeDef` (this assembly's own
/// classes, §II.22.37) intentionally stays UNSPLIT — `il_exporter` never gives its own mangled
/// Rust type names a namespace concept either (its `.class 'MangledName' { … }` is one quoted
/// identifier with no `.` interpreted specially, and nothing outside this backend's own generated
/// code ever needs to resolve one of its `TypeDef`s by namespace).
fn split_namespace(name: &str) -> (&str, &str) {
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
    fn static_field_token(&mut self, asm: &mut Assembly, field: Interned<StaticFieldDesc>) -> Token;

    /// Interns `s` in the `#US` heap and returns the `ldstr` token (§II.22.2's *User String*
    /// token: table id `0x70`, not one of the ordinary metadata tables).
    fn user_string_token(&mut self, s: &str) -> Token;

    /// Interns a `StandAloneSig` row for a `calli` call-site signature and returns its token.
    fn calli_sig_token(&mut self, asm: &mut Assembly, sig_blob: &[u8]) -> Token;

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
            let mut convention = if is_static { sig::SIG_DEFAULT } else { sig::SIG_HASTHIS };
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
            sig::encode_method_sig(convention, generic_param_count, &encode_sig, asm, self, &mut blob);
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

    fn static_field_token(&mut self, asm: &mut Assembly, field: Interned<StaticFieldDesc>) -> Token {
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

    fn calli_sig_token(&mut self, _asm: &mut Assembly, sig_blob: &[u8]) -> Token {
        let off = self.blobs.intern(sig_blob);
        self.standalone_sig(off)
    }

    fn locals_sig_token(&mut self, asm: &mut Assembly, locals: &[Type]) -> Token {
        let mut blob = Vec::new();
        sig::encode_locals_sig(locals, asm, self, &mut blob);
        let off = self.blobs.intern(&blob);
        self.standalone_sig(off)
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
    use crate::ir::{cilnode::MethodKind, Access, Float, Int};

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
        assert_eq!(tok.rid(), 2, "the first real class def must be TypeDef row 2, not row 1");
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
                let offset = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
                let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
                cursor += 8;
                let name_start = cursor;
                let name_end = bytes[name_start..].iter().position(|&b| b == 0).unwrap() + name_start;
                let name = std::str::from_utf8(&bytes[name_start..name_end]).unwrap().to_string();
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
            let count = |id: u32| header.counts.iter().find(|&&(t, _)| t == id).map_or(0, |&(_, c)| c);
            let simple_w = |rows: usize| w(rows > 0xFFFF);
            // Mirrors this module's own `coded_wide` exactly (§II.24.2.6: `>=`, not `>` — see its
            // doc comment for why the threshold row count itself already needs a wide column).
            let coded_w = |tag_bits: u32, max_rows: usize| w(max_rows >= (1usize << (16 - tag_bits)));

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
                Token::TABLE_METHOD_DEF => 4 + 2 + 2 + str_w + blob_w + simple_w(count(Token::TABLE_PARAM)),
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
                Token::TABLE_CUSTOM_ATTRIBUTE => {
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
                        .max(count(Token::TABLE_TYPE_SPEC));
                    let cat_max = count(Token::TABLE_METHOD_DEF).max(count(Token::TABLE_MEMBER_REF));
                    coded_w(5, hca_max) + coded_w(3, cat_max) + blob_w
                }
                Token::TABLE_CLASS_LAYOUT => 2 + 4 + simple_w(count(Token::TABLE_TYPE_DEF)),
                Token::TABLE_FIELD_LAYOUT => 4 + simple_w(count(Token::TABLE_FIELD)),
                Token::TABLE_STAND_ALONE_SIG => blob_w,
                Token::TABLE_MODULE_REF => str_w,
                Token::TABLE_TYPE_SPEC => blob_w,
                Token::TABLE_IMPL_MAP => {
                    let mf_max = count(Token::TABLE_FIELD).max(count(Token::TABLE_METHOD_DEF));
                    2 + coded_w(1, mf_max) + str_w + simple_w(count(Token::TABLE_MODULE_REF))
                }
                Token::TABLE_FIELD_RVA => 4 + simple_w(count(Token::TABLE_FIELD)),
                Token::TABLE_ASSEMBLY => 4 + 2 * 4 + blob_w + 2 * str_w,
                Token::TABLE_ASSEMBLY_REF => 2 * 4 + 4 + blob_w + 2 * str_w + blob_w,
                Token::TABLE_METHOD_SPEC => {
                    let mdor_max = count(Token::TABLE_METHOD_DEF).max(count(Token::TABLE_MEMBER_REF));
                    coded_w(1, mdor_max) + blob_w
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
        let method_tok = mb.add_method("DoIt", sig_blob, &[], true, false, false, None);
        assert_eq!(method_tok.table(), Token::TABLE_METHOD_DEF);
        assert_eq!(method_tok.rid(), 1);

        let ext_scope = mb.assembly_ref(
            "System.Runtime",
            AssemblyRefTarget::Bcl {
                version: (8, 0, 0, 0),
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
        assert_eq!(header.heap_sizes, 0, "small tables: no wide heap indices needed");

        let expected_valid_tables = [
            Token::TABLE_MODULE,
            Token::TABLE_TYPE_REF,
            Token::TABLE_TYPE_DEF,
            Token::TABLE_METHOD_DEF,
            Token::TABLE_MEMBER_REF,
            Token::TABLE_ASSEMBLY_REF,
        ];
        for id in expected_valid_tables {
            assert!(header.valid & (1u64 << id) != 0, "table {id:#x} should be Valid");
        }
        assert_eq!(header.sorted, 0, "no sorted tables populated in this tiny module");

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
        let type_ref_name_off =
            u16::from_le_bytes(row_bytes[type_ref_start + 2..type_ref_start + 4].try_into().unwrap());
        assert_eq!(reader.strings_at(u32::from(type_ref_name_off)), "Console");

        // TypeDef rows come after TypeRef (1 row * 6 bytes). Row 1 (the `<Module>` pseudo-type)
        // comes first; "MyType" is row 2.
        let type_def_start = type_ref_start + 6;
        // TypeDef: Flags(4) + Name(2) + Namespace(2) + Extends(2, coded) + FieldList(2) +
        // MethodList(2) = 14 bytes.
        let module_pseudo_type_name_off =
            u16::from_le_bytes(row_bytes[type_def_start + 4..type_def_start + 6].try_into().unwrap());
        assert_eq!(reader.strings_at(u32::from(module_pseudo_type_name_off)), "<Module>");
        let my_type_start = type_def_start + 14;
        let type_def_name_off =
            u16::from_le_bytes(row_bytes[my_type_start + 4..my_type_start + 6].try_into().unwrap());
        assert_eq!(reader.strings_at(u32::from(type_def_name_off)), "MyType");
        let method_list = u16::from_le_bytes(
            row_bytes[my_type_start + 10..my_type_start + 12]
                .try_into()
                .unwrap(),
        );
        assert_eq!(method_list, 1, "MyType owns MethodDef row 1 (1-based FieldList/MethodList)");

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
        let assembly_name_off =
            u16::from_le_bytes(row_bytes[assembly_start + 18..assembly_start + 20].try_into().unwrap());
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
            true,
            false,
            false,
            None,
        );
        assert_eq!(method_tok.rid(), 1);

        let bytes = mb.serialize();
        let reader = MetadataReader::parse(&bytes);
        let header = reader.tables_header();
        let counts: HashMap<u32, usize> = header.counts.iter().copied().collect();
        assert_eq!(counts[&Token::TABLE_PARAM], 2, "one Param row per arg_names entry, named or not");

        let row_bytes = &reader.stream("#~")[header.row_data_offset..];
        let param_start = reader.table_offset(Token::TABLE_PARAM, &header);
        // Param row: Flags(2) + Sequence(2) + Name(2, narrow — small tables here) = 6 bytes.
        let row0 = &row_bytes[param_start..param_start + 6];
        let row1 = &row_bytes[param_start + 6..param_start + 12];

        let seq0 = u16::from_le_bytes(row0[2..4].try_into().unwrap());
        let name0 = u16::from_le_bytes(row0[4..6].try_into().unwrap());
        assert_eq!(seq0, 1, "first real arg is Sequence 1 (this is a static method, no implicit this)");
        assert_eq!(reader.strings_at(u32::from(name0)), "x");

        let seq1 = u16::from_le_bytes(row1[2..4].try_into().unwrap());
        let name1 = u16::from_le_bytes(row1[4..6].try_into().unwrap());
        assert_eq!(seq1, 2, "second arg is Sequence 2");
        assert_eq!(name1, 0, "unnamed param has a null #Strings offset, not a dangling/garbage one");
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
        assert_eq!(w.heap_sizes & 0x2, 0, "GUID heap untouched, must stay narrow");
        assert_eq!(w.heap_sizes & 0x4, 0, "Blob heap untouched, must stay narrow");
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
        assert_eq!(classes, vec![1, 2, 2], "the three rows added (class=2 via add_type_def's own implements + class=2, class=1 standalone)");
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
            row_bytes[field_layout_start + offset_col_width..field_layout_start + offset_col_width + 2]
                .try_into()
                .unwrap(),
        );
        let field_1 = u16::from_le_bytes(
            row_bytes[field_layout_start + row_width + offset_col_width
                ..field_layout_start + row_width + offset_col_width + 2]
                .try_into()
                .unwrap(),
        );
        assert!(field_0 <= field_1, "FieldLayout must be sorted ascending by Field");
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
            },
        );
        let name_only = mb.assembly_ref("MyLib", AssemblyRefTarget::NameOnly);
        assert_ne!(bcl, name_only);
        assert_eq!(bcl.table(), Token::TABLE_ASSEMBLY_REF);
        assert_eq!(mb.assembly_ref.len(), 2);
        assert_ne!(mb.assembly_ref[0].public_key_or_token, 0, "BCL ref carries the ECMA token blob");
        assert_eq!(mb.assembly_ref[1].public_key_or_token, 0, "name-only ref carries no token");
    }

    /// `system_runtime_assembly_ref` (the bootstrap `System.Runtime` ref backing
    /// `thread_static_ctor_ref`/`system_runtime_type_ref`) and `find_or_create_assembly_ref`'s
    /// BCL branch both stamp the `AssemblyRef.MajorVersion..RevisionNumber` columns from
    /// `crate::ir::dotnet_version()` — the single source of truth `il_exporter` also reads
    /// (`dv_ver = dotnet_version().assembly_ver()`, mod.rs:74) — rather than a crate-local
    /// hardcoded `(8, 0, 0, 0)` literal, WHEN `is_lib` is set (mirrors `il_exporter`'s `if
    /// self.is_lib { … }` gate — an executable gets name-only/`0.0.0.0` refs instead, see
    /// `MetadataBuilder::is_lib`'s doc). This asserts the threading, not a specific version
    /// number: the test process has no `DOTNET_VERSION` env var set, so `dotnet_version()`
    /// resolves to the `Net8` default, and both call sites must agree with whatever that
    /// resolves to (proving they consult it, rather than merely happening to match by
    /// coincidence with a stale literal).
    #[test]
    fn bcl_assembly_refs_are_stamped_from_dotnet_version_not_a_hardcoded_literal() {
        let expected = crate::ir::dotnet_version().assembly_ver_tuple();

        let mut mb = MetadataBuilder::new();
        mb.set_is_lib(true);
        let sys_runtime_tok = mb.find_or_create_assembly_ref("System.Runtime");
        let row = &mb.assembly_ref[(sys_runtime_tok.rid() - 1) as usize];
        assert_eq!((row.major, row.minor, row.build, row.revision), expected);

        // A second, distinct BCL name via the same helper must agree too (not a fluke of caching
        // the first lookup).
        let mut mb2 = MetadataBuilder::new();
        mb2.set_is_lib(true);
        let intrinsics_tok = mb2.find_or_create_assembly_ref("System.Runtime.Intrinsics");
        let row2 = &mb2.assembly_ref[(intrinsics_tok.rid() - 1) as usize];
        assert_eq!((row2.major, row2.minor, row2.build, row2.revision), expected);

        // `system_runtime_assembly_ref` (used by the ThreadStaticAttribute bootstrap path) is a
        // separate call site from `find_or_create_assembly_ref` — exercise it directly via a TLS
        // static field, which routes through `thread_static_attribute` -> `thread_static_ctor_ref`
        // -> `system_runtime_assembly_ref`.
        let mut mb3 = MetadataBuilder::new();
        mb3.set_is_lib(true);
        let field = mb3.add_static_field("TLS", 0, None, true, false);
        let _ = field;
        assert_eq!(mb3.assembly_ref.len(), 1, "the TLS path must have created exactly one AssemblyRef");
        let row3 = &mb3.assembly_ref[0];
        assert_eq!((row3.major, row3.minor, row3.build, row3.revision), expected);
    }

    /// The `is_lib` gate itself (not just that the version, when stamped, comes from
    /// `dotnet_version()`): an executable-shaped builder (`is_lib` left at its `false` default)
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
        assert_eq!((row.major, row.minor, row.build, row.revision), (0, 0, 0, 0));
        assert_eq!(row.public_key_or_token, 0, "an unversioned exe ref carries no public-key token either");

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
        assert_eq!(&bytes[value_off as usize..value_off as usize + 5], &[4, 0x01, 0x00, 0x00, 0x00]);
    }

    /// `StaticFieldDef::is_const` (Cluster C item 3's `initonly` case) sets `FieldAttributes`
    /// bit 0x20 (§II.23.1.5 InitOnly) and nothing else — must not be confused with a metadata
    /// `Constant` row (§II.22.9), which `il_exporter` never emits for these fields either
    /// (mod.rs:224/328 only ever renders the `initonly` keyword).
    #[test]
    fn add_static_field_is_const_sets_initonly_flag_only() {
        let mut mb = MetadataBuilder::new();
        let plain = mb.add_static_field("Plain", 0, None, false, false);
        let konst = mb.add_static_field("Konst", 0, None, false, true);

        let plain_flags = mb.field[(plain.rid() - 1) as usize].flags;
        let konst_flags = mb.field[(konst.rid() - 1) as usize].flags;

        assert_eq!(plain_flags & 0x20, 0, "non-const static must NOT have InitOnly set");
        assert_eq!(konst_flags & 0x20, 0x20, "const static must have InitOnly set");
        // Both still carry the ordinary Public|Static bits — `is_const` only adds InitOnly, it
        // doesn't replace the base flag set. `FieldAttributes::Public` is 0x6 (FieldAccessMask,
        // §II.23.1.5), not 0x1 (that's `Private` — see `add_field`'s doc for the bug this fixes).
        assert_eq!(plain_flags & (0x6 | 0x10), 0x6 | 0x10);
        assert_eq!(konst_flags & (0x6 | 0x10), 0x6 | 0x10);
        assert_eq!(mb.custom_attribute.len(), 0, "is_const must not add any CustomAttribute row");
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
        let m = mb.add_method("Foo", sig_blob, &[], true, false, false, None);
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
        let fn_sig = asm.sig([Type::ClassRef(owner), Type::Int(crate::ir::Int::I32)], Type::Bool);
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
        assert_eq!(blob[2], 1, "param count must be 1 (the receiver is NOT counted)");
        const ET_BOOLEAN: u8 = 0x02;
        const ET_I4: u8 = 0x08;
        assert_eq!(blob[3], ET_BOOLEAN, "return type");
        assert_eq!(blob[4], ET_I4, "the ONE real parameter, not the receiver's own ClassRef type");
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
        let _m = mb.add_method("libc_call", sig_blob, &[], true, false, false, Some(("libc", true)));
        assert_eq!(mb.impl_map.len(), 1);
        assert_eq!(mb.module_ref.len(), 1);
        assert_eq!(mb.impl_map[0].mapping_flags & 0x40, 0x40, "SupportsLastError set");
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
        assert_eq!(b_row.field_list, 3, "B's fields start at row 3 (b0), AFTER A's 2 fields");
        assert_eq!(mb.field.len(), 3, "3 field rows total: a0, a1, b0");
        assert_eq!(&mb.strings.as_bytes()[mb.field[0].name as usize..][..2], b"a0");
        assert_eq!(&mb.strings.as_bytes()[mb.field[2].name as usize..][..2], b"b0");

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
        mb.add_method("b_m0", method_sig, &[], true, false, false, None);
        mb.add_method("b_m1", method_sig, &[], true, false, false, None);

        let a_row = &mb.type_def[(a.rid() - 1) as usize];
        let b_row = &mb.type_def[(b.rid() - 1) as usize];
        assert_eq!(a_row.method_list, 1, "A owns zero methods: its run starts where B's begins");
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
        assert_eq!(row.flags & 0x1, 0, "must be NotPublic (0x0), not Public (0x1)");
        assert_eq!(row.flags & 0x100, 0x100, "Sealed bit must be set");
        assert_eq!(row.flags & 0x10, 0x10, "ExplicitLayout bit must be set");
        assert_eq!(
            row.extends,
            encode_type_def_or_ref_token(value_type_ref),
            "Extends must resolve to the caller-supplied System.ValueType TypeRef"
        );

        assert_eq!(mb.class_layout.len(), 1);
        assert_eq!(mb.class_layout[0].parent, tok.rid());
        assert_eq!(mb.class_layout[0].packing_size, 1, ".pack 1, matching il_exporter's literal text");
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
    /// `Vec<usize>` over `asm.const_data.1.keys()`, mod.rs:116-121). This module doesn't dedupe
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
        assert_eq!(decode_type_def_or_ref(coded), tok, "must resolve to the TypeDef, not create a TypeRef");
        assert_eq!(mb.type_ref.len(), 0, "no TypeRef should be created for an in-assembly type");
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
        assert_eq!(mb.assembly_ref.len(), 1, "System.Console's assembly must be registered");
    }

    #[test]
    fn resolver_caches_repeat_lookups_of_the_same_class_ref() {
        let mut mb = MetadataBuilder::new();
        let mut asm = Assembly::default();
        let cref = ClassRef::console(&mut asm);

        let a = TypeDefOrRefResolver::type_def_or_ref(&mut mb, cref, &mut asm);
        let b = TypeDefOrRefResolver::type_def_or_ref(&mut mb, cref, &mut asm);
        assert_eq!(a, b);
        assert_eq!(mb.type_ref.len(), 1, "second lookup must reuse the cached row");
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
        assert_eq!(mb.type_spec.len(), 1, "a non-generic ClassRef must not add a TypeSpec row");
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
        let fn_sig = asm.sig([Type::ClassRef(iqueryable_of_mvar0)], Type::Int(crate::ir::Int::I32));
        let mref = asm.alloc_methodref(crate::ir::MethodRef::new(
            owner,
            method_name,
            fn_sig,
            crate::ir::cilnode::MethodKind::Static,
            vec![].into(),
        ));

        // The call site's instantiation: `Count<int32>`.
        let generic_args = [Type::Int(crate::ir::Int::I32)];
        let tok = TokenSink::method_token(&mut mb, &mut asm, MethodDefIdx::from_raw(mref), &generic_args);
        assert_eq!(tok.table(), Token::TABLE_METHOD_SPEC, "call site wraps the base in a MethodSpec");

        // Unwrap the MethodSpec to inspect the base MemberRef's OWN signature (not the
        // instantiation blob, which was already correct before this fix). `MethodSpecRow.method`
        // is a MethodDefOrRef-coded u32 (`(rid << 1) | tag`, tag=1 for MemberRef, mirroring
        // `encode_method_def_or_ref`) — not a plain `Token`.
        let spec_row = &mb.method_spec[(tok.rid() - 1) as usize];
        assert_eq!(spec_row.method & 1, 1, "base must be a MemberRef, not a MethodDef");
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
        assert_eq!(blob[2], 1, "generic parameter COUNT (the method has one type parameter)");
        assert_eq!(blob[3], 1, "value parameter count (the receiver is not counted)");
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
        assert!(result.is_err(), "f128 signature encoding is a known todo!() in sig.rs");
    }

    #[test]
    fn method_kind_smoke() {
        // Exercises the MethodKind import so clippy doesn't flag an unused import if every other
        // test above is trimmed during future edits.
        let _ = MethodKind::Static;
    }

    /// Regression for the SIGSEGV-on-first-virtual-dispatch bug found bisecting cd_collections
    /// under `DIRECT_PE=1`: `add_method`'s `is_ctor` flag is the ONLY thing that sets
    /// `SpecialName (0x1000) | RTSpecialName (0x0800)` (§II.23.1.10) on a `MethodDef` row. The
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
        let tok = mb.add_method(".cctor", sig_blob, &[], true, false, true, None);
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
        let tok = mb.add_method(".tcctor", sig_blob, &[], true, false, false, None);
        let row = &mb.method_def[(tok.rid() - 1) as usize];
        assert_eq!(row.flags & (0x1000 | 0x0800), 0);
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
        let tok = mb.add_method("Start", sig_blob, &[], false, true, false, None);
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
}
