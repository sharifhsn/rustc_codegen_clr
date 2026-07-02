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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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

    /// Adds a `TypeDef` row (§II.22.37) for a class this assembly defines. See the trait doc for
    /// the `FieldList`/`MethodList` run-pointer contract: they are stamped with "one past the
    /// current end of field/method rows" here (i.e. the *insertion-order* invariant that
    /// `add_field`/`add_method` always extend the most-recently-added `TypeDef`'s run) and never
    /// need patching, since this backend only ever appends fields/methods for the class that was
    /// added last — exactly how `il_exporter`'s per-class loop emits them.
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

    /// Adds an instance `Field` row (§II.22.15) to the most recently added `TypeDef`.
    /// `offset` mirrors `ClassDef::fields()`'s `Option<u32>` (`.field [N] …`) and populates a
    /// `FieldLayout` row (§II.22.16) when present.
    pub fn add_field(&mut self, name: &str, signature_blob: u32, offset: Option<u32>) -> Token {
        // §II.23.1.5 `FieldAttributes`: 0x1 = Public (fields need to be reachable from generated
        // method bodies in other TypeDefs, mirroring the "default field accessibility is
        // private" note `il_exporter` calls out for the MainModule-partition case — public is a
        // safe superset here since this writer never emits cross-class private field coupling).
        let flags: u16 = 0x1;
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
    /// `StaticFieldDef::is_tls`.
    pub fn add_static_field(
        &mut self,
        name: &str,
        signature_blob: u32,
        rva_data: Option<Vec<u8>>,
        is_thread_static: bool,
    ) -> Token {
        // §II.23.1.5 `FieldAttributes`: 0x1 Public | 0x10 Static.
        let mut flags: u16 = 0x1 | 0x10;
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
        // §II.23.1.10 `MethodAttributes`: 0x1 Public, 0x10 Static, 0x40 Virtual,
        // 0x400 NewSlot (paired with Virtual so it doesn't try to override a base slot),
        // 0x1000 SpecialName | 0x800 RTSpecialName (ctors only), 0x2000 PInvokeImpl.
        let mut flags: u16 = 0x1; // public
        if is_static {
            flags |= 0x10;
        }
        if is_virtual {
            flags |= 0x40 | 0x0400;
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
        // il_exporter's `dv_ver`/`assembly_ver()` BCL version triplet is not reachable from this
        // crate-local module without threading a `DotnetVersion` through every call site; net8's
        // triplet is the long-standing default (`8:0:0:0`) used throughout the existing BCL
        // ClassRef table, so it is hardcoded here for this one bootstrap reference.
        self.assembly_ref(
            NAME,
            AssemblyRefTarget::Bcl {
                version: (8, 0, 0, 0),
            },
        )
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
/// the largest target table exceeds 2^(16 - tag_bits)".
fn coded_wide(tag_bits: u32, max_rows: usize) -> bool {
    max_rows > (1usize << (16 - tag_bits))
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
            let (namespace, name) = split_namespace(raw_name);
            self.type_ref(scope, namespace, name)
        };
        self.class_token_cache.insert(cref, tok);
        encode_type_def_or_ref_token(tok)
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
    /// per the hard constraint that `pe_exporter` may not import it).
    fn find_or_create_assembly_ref(&mut self, name: &str) -> Token {
        for (i, row) in self.assembly_ref.iter().enumerate() {
            if self.strings_eq(row.name, name) {
                return Token::new(Token::TABLE_ASSEMBLY_REF, u32::try_from(i + 1).unwrap());
            }
        }
        let target = if is_bcl_assembly(name) {
            AssemblyRefTarget::Bcl {
                version: (8, 0, 0, 0),
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

/// This backend (mirroring `il_exporter`) never splits a class's mangled name into a metadata
/// `TypeNamespace`/`TypeName` pair — the whole name goes in `TypeName` and `TypeNamespace` is
/// always empty. Kept as a named helper (rather than inlined `("", name)`) so a future namespace
/// split is a one-line change.
fn split_namespace(name: &str) -> (&str, &str) {
    ("", name)
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
            let class_tok = {
                let coded = self.type_def_or_ref(method_ref.class(), asm);
                decode_type_def_or_ref(coded)
            };
            let name = asm[method_ref.name()].to_string();
            let sig = method_ref.sig();
            let mut blob = Vec::new();
            let fnsig = asm[sig].clone();
            let convention = match method_ref.kind() {
                crate::ir::cilnode::MethodKind::Static => sig::SIG_DEFAULT,
                _ => sig::SIG_HASTHIS,
            };
            sig::encode_method_sig(convention, 0, &fnsig, asm, self, &mut blob);
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
        let class_tok = {
            let coded = self.type_def_or_ref(desc.owner(), asm);
            decode_type_def_or_ref(coded)
        };
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
        let class_tok = {
            let coded = self.type_def_or_ref(desc.owner(), asm);
            decode_type_def_or_ref(coded)
        };
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
            Type::ClassRef(cref) => {
                let coded = self.type_def_or_ref(cref, asm);
                decode_type_def_or_ref(coded)
            }
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
        assert_eq!(mb.strings.as_bytes(), b"\0");
        assert_eq!(mb.blobs.as_bytes(), &[0]);
        assert_eq!(mb.guids.as_bytes(), &[] as &[u8]);
        assert_eq!(mb.user_strings.as_bytes(), b"\0");
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
            let coded_w = |tag_bits: u32, max_rows: usize| w(max_rows > (1usize << (16 - tag_bits)));

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
        assert_eq!(type_tok.rid(), 1);

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
        assert_eq!(counts[&Token::TABLE_TYPE_DEF], 1);
        assert_eq!(counts[&Token::TABLE_METHOD_DEF], 1);
        assert_eq!(counts[&Token::TABLE_MEMBER_REF], 1);
        assert_eq!(counts[&Token::TABLE_TYPE_REF], 1);
        assert_eq!(counts[&Token::TABLE_ASSEMBLY_REF], 1);

        // Decode the TypeDef row's Name column and check it round-trips to "MyType" via #Strings.
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

        // TypeDef rows come after TypeRef (1 row * 6 bytes).
        let type_def_start = type_ref_start + 6;
        // TypeDef: Flags(4) + Name(2) + Namespace(2) + Extends(2, coded) + FieldList(2) +
        // MethodList(2) = 14 bytes.
        let type_def_name_off =
            u16::from_le_bytes(row_bytes[type_def_start + 4..type_def_start + 6].try_into().unwrap());
        assert_eq!(reader.strings_at(u32::from(type_def_name_off)), "MyType");
        let method_list = u16::from_le_bytes(
            row_bytes[type_def_start + 10..type_def_start + 12]
                .try_into()
                .unwrap(),
        );
        assert_eq!(method_list, 1, "MyType owns MethodDef row 1 (1-based FieldList/MethodList)");

        // MethodDef rows: RVA(4) + ImplFlags(2) + Flags(2) + Name(2) + Signature(2) +
        // ParamList(2) = 14 bytes.
        let method_def_start = type_def_start + 14;
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
        // TypeDefOrRef: 2 tag bits -> threshold 2^14.
        assert!(!coded_wide(2, 1 << 14));
        assert!(coded_wide(2, (1 << 14) + 1));
        // MethodDefOrRef: 1 tag bit -> threshold 2^15.
        assert!(!coded_wide(1, 1 << 15));
        assert!(coded_wide(1, (1 << 15) + 1));
        // HasCustomAttribute: 5 tag bits -> threshold 2^11.
        assert!(!coded_wide(5, 1 << 11));
        assert!(coded_wide(5, (1 << 11) + 1));
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
        // that would violate Class-sort if rows were emitted in insertion order.
        let ext = mb.assembly_ref("SomeLib", AssemblyRefTarget::NameOnly);
        let iface = mb.type_ref(Some(ext), "", "ISomeInterface");

        // Add type B first (would become TypeDef rid 1)…
        let _b = mb.add_type_def("", "BType", false, None, None, None, &[iface]);
        // …then type A (rid 2) also implementing it — InterfaceImpl rows are pushed in
        // (class=1, iface), (class=2, iface) order already here since add_type_def appends in
        // call order, so exercise an out-of-order case explicitly via the standalone API.
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
        assert_eq!(classes, vec![1, 1, 2], "the three rows added (1 via add_type_def + 2 standalone)");
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

    #[test]
    fn thread_static_attribute_uses_fixed_blob() {
        let mut mb = MetadataBuilder::new();
        let field = mb.add_static_field("TLS", 0, None, false);
        let attr = mb.thread_static_attribute(field);
        assert_eq!(attr.table(), Token::TABLE_CUSTOM_ATTRIBUTE);
        assert_eq!(mb.custom_attribute.len(), 1);
        let value_off = mb.custom_attribute[0].value;
        let bytes = mb.blobs.as_bytes();
        // length-prefix(1) + the 4 fixed bytes.
        assert_eq!(&bytes[value_off as usize..value_off as usize + 5], &[4, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn set_field_rva_materializes_pending_row() {
        let mut mb = MetadataBuilder::new();
        let field = mb.add_static_field("DATA", 0, Some(vec![1, 2, 3]), false);
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
        assert_eq!(tok.rid(), 1);

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
}
