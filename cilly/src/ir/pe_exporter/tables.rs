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

use super::heaps::{BlobHeap, GuidHeap, StringsHeap, UserStringHeap};
use super::sig::TypeDefOrRefResolver;
use crate::ir::{Assembly, ClassRef, FieldDesc, Interned, MethodDefIdx, StaticFieldDesc, Type};

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

/// Owns the four metadata heaps and every table row this backend populates. One instance per
/// emitted assembly (mirrors one `ILExporter::export_to_write` call).
///
/// Row storage is deliberately left as an implementation detail (`todo!()` bodies for now) —
/// implementers choose whatever per-table row struct is convenient, as long as `serialize()`
/// produces a spec-conformant `#~` stream. `mod.rs`'s existing heap/sig tests are the only
/// contract fixed in stone at this stage.
#[derive(Default)]
pub struct MetadataBuilder {
    pub strings: StringsHeap,
    pub blobs: BlobHeap,
    pub guids: GuidHeap,
    pub user_strings: UserStringHeap,
}

impl MetadataBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns an `AssemblyRef` row (§II.22.5), returning its token. `name` is the `.NET`
    /// assembly identity (e.g. `"System.Runtime"`); `target` selects the BCL-versioned vs.
    /// name-only shape per `il_exporter`'s `is_bcl_assembly` split (lines ~87-105).
    pub fn assembly_ref(&mut self, name: &str, target: AssemblyRefTarget<'_>) -> Token {
        let _ = (name, target);
        todo!("AssemblyRef row (§II.22.5)")
    }

    /// Interns a `TypeRef` row (§II.22.38): a reference to a type defined in another module or
    /// assembly (`resolution_scope` is the token of the owning `AssemblyRef`/`ModuleRef`, or
    /// `None` for a nested/self-module reference). Mirrors `simple_class_ref`/`class_ref` in
    /// `il_exporter` (the `[assembly]'Name'` rendering).
    pub fn type_ref(&mut self, resolution_scope: Option<Token>, namespace: &str, name: &str) -> Token {
        let _ = (resolution_scope, namespace, name);
        todo!("TypeRef row (§II.22.38)")
    }

    /// Adds a `TypeDef` row (§II.22.37) for a class this assembly defines. `extends` is the
    /// `TypeDefOrRef` coded-index token of the base type (`None` only for `System.Object`
    /// itself, which this backend never defines — every `ClassDef` has an explicit base per
    /// `il_exporter`'s `extends`/`is_valuetype` fallback logic). `pack`/`size` mirror
    /// `ClassDef::align`/`explict_size` (`.pack`/`.size` directives, explicit-layout classes
    /// only). `implements` mirrors `ClassDef::implements()` and populates matching
    /// `InterfaceImpl` rows as a side effect. Returns the new row's token; callers patch in the
    /// field/method run start indices once those tables are fully populated (§II.22.37's
    /// `FieldList`/`MethodList` are "first row owned" pointers, computed at `serialize()` time
    /// from table population order, not passed in here).
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
        let _ = (namespace, name, is_valuetype, extends, pack, size, implements);
        todo!("TypeDef row (§II.22.37) + ClassLayout (§II.22.8) + InterfaceImpl (§II.22.23)")
    }

    /// Adds an instance `Field` row (§II.22.15) to the most recently added `TypeDef`.
    /// `offset` mirrors `ClassDef::fields()`'s `Option<u32>` (`.field [N] …`) and populates a
    /// `FieldLayout` row (§II.22.16) when present.
    pub fn add_field(&mut self, name: &str, signature_blob: u32, offset: Option<u32>) -> Token {
        let _ = (name, signature_blob, offset);
        todo!("Field row (§II.22.15) + optional FieldLayout (§II.22.16)")
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
        let _ = (name, signature_blob, rva_data, is_thread_static);
        todo!("static Field row (§II.22.15), queues FieldRVA data for the pe layout pass")
    }

    /// Adds a `MethodDef` row (§II.22.26) to the most recently added `TypeDef`, plus its `Param`
    /// rows (§II.22.33, one per named argument — mirrors `MethodDef::arg_names()`). The body RVA
    /// is unknown until `body.rs` assembles bytes and `pe.rs` lays them out, so it starts at 0
    /// and must be patched via [`MetadataBuilder::set_method_body_rva`] before `serialize()`.
    /// `impl_flags`/`flags` follow `il_exporter`'s per-`MethodKind` mapping (static/instance/
    /// virtual/`specialname rtspecialname` ctor) and the `pinvokeimpl`/`preservesig` case (which
    /// additionally populates an `ImplMap` row, §II.22.22, and a `ModuleRef`, §II.22.31, for the
    /// `lib` name).
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
        let _ = (
            name,
            signature_blob,
            param_names,
            is_static,
            is_virtual,
            is_ctor,
            pinvoke,
        );
        todo!("MethodDef row (§II.22.26) + Param rows (§II.22.33) + optional ImplMap/ModuleRef")
    }

    /// Patches the body RVA of a previously-added `MethodDef` row (§II.22.26 `RVA` column) once
    /// `pe.rs`'s layout pass has placed the assembled body bytes in `.text`.
    pub fn set_method_body_rva(&mut self, method: Token, rva: u32) {
        let _ = (method, rva);
        todo!("patch MethodDef.RVA")
    }

    /// Interns a `MemberRef` row (§II.22.25): a reference to a field or method defined in
    /// another `TypeRef`/`TypeSpec`/`MethodDef` (`class` is that owner's coded index token).
    /// Used for every BCL call (`Console.WriteLine`, …) and cross-assembly field access.
    pub fn member_ref(&mut self, class: Token, name: &str, signature_blob: u32) -> Token {
        let _ = (class, name, signature_blob);
        todo!("MemberRef row (§II.22.25)")
    }

    /// Interns a `TypeSpec` row (§II.22.39): a signature-encoded type too complex for a
    /// `TypeDef`/`TypeRef` token alone (generic instantiations, arrays, pointers used as a
    /// standalone type operand — e.g. `ldelem`/`newarr` on `List<T>`). `blob` is a pre-encoded
    /// `sig::encode_type` signature (NOT a field/method/locals-wrapped one, per §II.23.2.14).
    pub fn type_spec(&mut self, blob: u32) -> Token {
        let _ = blob;
        todo!("TypeSpec row (§II.22.39)")
    }

    /// Interns a `MethodSpec` row (§II.22.29): a generic-method instantiation (`method<T,…>` in
    /// `il_exporter`'s rendering). `method` is the generic `MethodDef`/`MemberRef` token;
    /// `instantiation_blob` is a `sig::encode_method_spec_sig` blob.
    pub fn method_spec(&mut self, method: Token, instantiation_blob: u32) -> Token {
        let _ = (method, instantiation_blob);
        todo!("MethodSpec row (§II.22.29)")
    }

    /// Interns a `StandAloneSig` row (§II.22.36): either a `calli` call-site signature or a
    /// method body's `.locals` signature (both are bare signature blobs with no owning row).
    pub fn standalone_sig(&mut self, signature_blob: u32) -> Token {
        let _ = signature_blob;
        todo!("StandAloneSig row (§II.22.36)")
    }

    /// Adds an `InterfaceImpl` row (§II.22.23) directly. Normally populated as a side effect of
    /// [`MetadataBuilder::add_type_def`]'s `implements` argument; exposed separately for the rare
    /// case a caller needs to add one after the fact.
    pub fn interface_impl(&mut self, class: Token, interface: Token) -> Token {
        let _ = (class, interface);
        todo!("InterfaceImpl row (§II.22.23)")
    }

    /// Emits the **only** custom attribute this backend produces: a bare
    /// `[ThreadStaticAttribute]` (`System.ThreadStaticAttribute::.ctor()`) on a static field,
    /// mirroring `il_exporter`'s `.custom instance void
    /// [System.Runtime]System.ThreadStaticAttribute::.ctor() = (01 00 00 00)` (the fixed 4-byte
    /// `01 00 00 00` prolog+zero-named-args blob, §II.23.3). `field` is the owning `Field` row's
    /// token (`HasCustomAttribute` coded index, §II.24.2.6).
    pub fn thread_static_attribute(&mut self, field: Token) -> Token {
        let _ = field;
        todo!("CustomAttribute row (§II.22.10) — ThreadStaticAttribute only")
    }

    /// Records the `MethodDef` token `serialize()` must stamp into the CLI header's
    /// `EntryPointToken` (§II.25.3.3), mirroring `il_exporter`'s "method literally named
    /// `entrypoint`" convention (`ENTRYPOINT` in `asm.rs`).
    pub fn set_entry_point(&mut self, method: Token) {
        let _ = method;
        todo!("remember EntryPointToken for the pe writer")
    }

    /// Records the RVA of a `FieldRVA` blob once `pe.rs`'s layout pass has placed it in
    /// `.sdata`, and materializes the deferred `FieldRVA` row (§II.22.18) for `field` (added via
    /// [`MetadataBuilder::add_static_field`]'s `rva_data`).
    pub fn set_field_rva(&mut self, field: Token, rva: u32) {
        let _ = (field, rva);
        todo!("materialize FieldRVA row (§II.22.18) now that the blob has a real RVA")
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
        todo!("assemble the BSJB metadata root: #~ tables stream + four heap streams, 4-aligned")
    }
}

/// [`MetadataBuilder`] is the [`TypeDefOrRefResolver`] the `sig` encoder calls into: it resolves
/// a `ClassRef` to a `TypeDefOrRef` coded index by finding-or-creating the matching `TypeDef`
/// (defined in this assembly) or `TypeRef` (external) row.
impl TypeDefOrRefResolver for MetadataBuilder {
    fn type_def_or_ref(&mut self, cref: Interned<ClassRef>, asm: &mut Assembly) -> u32 {
        let _ = (cref, asm);
        todo!("resolve/create the TypeDef or TypeRef row for `cref` and return its coded index")
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
        let _ = (asm, method, generic_args);
        todo!("MethodDef/MemberRef (+ MethodSpec wrap) token for a method reference")
    }

    fn field_token(&mut self, asm: &mut Assembly, field: Interned<FieldDesc>) -> Token {
        let _ = (asm, field);
        todo!("Field/MemberRef token for an instance field reference")
    }

    fn static_field_token(&mut self, asm: &mut Assembly, field: Interned<StaticFieldDesc>) -> Token {
        let _ = (asm, field);
        todo!("Field/MemberRef token for a static field reference")
    }

    fn user_string_token(&mut self, s: &str) -> Token {
        let off = self.user_strings.intern(s);
        // §II.22.2: the User String token's table id is the fixed value 0x70.
        Token::new(0x70, off)
    }

    fn calli_sig_token(&mut self, asm: &mut Assembly, sig_blob: &[u8]) -> Token {
        let _ = (asm, sig_blob);
        todo!("StandAloneSig token for a calli site")
    }

    fn locals_sig_token(&mut self, asm: &mut Assembly, locals: &[Type]) -> Token {
        let _ = (asm, locals);
        todo!("StandAloneSig token for a .locals signature")
    }

    fn type_token(&mut self, asm: &mut Assembly, tpe: Type) -> Token {
        let _ = (asm, tpe);
        todo!("TypeDef/TypeRef/TypeSpec token for a type operand")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
