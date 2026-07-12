//! Signature-blob encoding (§II.23.2): `Type` → `ELEMENT_TYPE_*` byte sequences for field,
//! method, local-variable, `MethodSpec`, and `calli` stand-alone signatures.
//!
//! Class references need a metadata *row* to point at, which only the table builder knows, so the
//! encoder is parameterized over a [`TypeDefOrRefResolver`]. Everything else — including wrapping
//! a generic instantiation in `GENERICINST` and lowering `i128`/`u128`/`f16` to their BCL
//! valuetype classes — happens here, mirroring how `il_exporter`'s `type_il` renders the same
//! `Type` values (that rendering is the semantic spec; see `docs/PE_EMISSION_PLAN.md`).

use super::heaps::write_compressed_u32;
use crate::ir::tpe::GenericKind;
use crate::ir::{Assembly, ClassRef, FnSig, Interned, Type};

// ELEMENT_TYPE_* constants (§II.23.1.16) — only the ones this backend emits.
const ET_VOID: u8 = 0x01;
const ET_BOOLEAN: u8 = 0x02;
const ET_CHAR: u8 = 0x03;
const ET_I1: u8 = 0x04;
const ET_U1: u8 = 0x05;
const ET_I2: u8 = 0x06;
const ET_U2: u8 = 0x07;
const ET_I4: u8 = 0x08;
const ET_U4: u8 = 0x09;
const ET_I8: u8 = 0x0A;
const ET_U8: u8 = 0x0B;
const ET_R4: u8 = 0x0C;
const ET_R8: u8 = 0x0D;
const ET_STRING: u8 = 0x0E;
const ET_PTR: u8 = 0x0F;
const ET_BYREF: u8 = 0x10;
const ET_VALUETYPE: u8 = 0x11;
const ET_CLASS: u8 = 0x12;
const ET_VAR: u8 = 0x13;
const ET_ARRAY: u8 = 0x14;
const ET_GENERICINST: u8 = 0x15;
const ET_I: u8 = 0x18;
const ET_U: u8 = 0x19;
const ET_FNPTR: u8 = 0x1B;
const ET_OBJECT: u8 = 0x1C;
const ET_SZARRAY: u8 = 0x1D;
const ET_MVAR: u8 = 0x1E;

// Method-signature calling-convention byte (§II.23.2.1–II.23.2.3).
pub const SIG_HASTHIS: u8 = 0x20;
pub const SIG_GENERIC: u8 = 0x10;
pub const SIG_DEFAULT: u8 = 0x00;
/// Field-signature marker (§II.23.2.4).
pub const SIG_FIELD: u8 = 0x06;
/// Local-variable-signature marker (§II.23.2.6).
pub const SIG_LOCALS: u8 = 0x07;
/// Property-signature marker (§II.23.2.5) — ORed with [`SIG_HASTHIS`] for an instance property.
pub const SIG_PROPERTY: u8 = 0x08;
/// MethodSpec instantiation marker (§II.23.2.15).
pub const SIG_GENERICINST_METHOD: u8 = 0x0A;

/// Supplies the §II.23.2.8 `TypeDefOrRef` *coded index* for the **open** shape of a class
/// reference — `(row << 2) | tag` with tag 0 = TypeDef, 1 = TypeRef, 2 = TypeSpec. The encoder
/// wraps generic arguments in `GENERICINST` itself, so implementations key rows on
/// (assembly, name, arity) and ignore the `generics` list.
pub trait TypeDefOrRefResolver {
    fn type_def_or_ref(&mut self, cref: Interned<ClassRef>, asm: &mut Assembly) -> u32;
}

/// Encodes one `Type` (§II.23.2.12) into `out`, substituting the real `RustVoid` valuetype
/// (§II sentinel already materialized as a module-local `ClassDef` by `Assembly::prepared`,
/// called before every exporter runs — see `src/lib.rs`'s `.prepared()` call) for `Type::Void`.
///
/// `ELEMENT_TYPE_VOID` (§II.23.1.16) is only legal as a method's *return* type (or under a
/// `PTR`, §II.23.2.10) — a field signature, a parameter, a local variable, or a `MethodSpec`
/// generic argument typed `void` is malformed metadata (CoreCLR: "Illegal 'void' in
/// signature."). `il_exporter`'s `non_void_type_il` (the semantic oracle for this exact
/// substitution) renders `Type::Void` as `valuetype RustVoid` everywhere except a bare return
/// type / `FnPtr` output — mirror that split here: [`encode_type`] stays the raw encoder (used
/// for return types and `PTR`/`BYREF` targets, matching `il_exporter::type_il`), and every
/// value-carrying position (field sig, method params, locals, `MethodSpec` args) must call
/// this wrapper instead.
fn encode_non_void_type(
    tpe: Type,
    asm: &mut Assembly,
    resolver: &mut impl TypeDefOrRefResolver,
    out: &mut Vec<u8>,
) {
    if tpe == Type::Void {
        // No `bcl_class!` row: `RustVoid` is a module-local `ClassDef` (no assembly
        // qualifier), not a BCL type — construct the reference the same way
        // `Assembly::eliminate_dead_types` does (`asm.rs`) when it needs to name the same
        // sentinel class: `ClassRef::new(name, /*asm*/ None, /*is_valuetype*/ true, [])`.
        let name = asm.alloc_string("RustVoid");
        let cref = asm.alloc_class_ref(ClassRef::new(name, None, true, vec![].into()));
        encode_class(cref, /*is_valuetype*/ true, asm, resolver, out);
    } else {
        encode_type(tpe, asm, resolver, out);
    }
}

/// Encodes one `Type` (§II.23.2.12) into `out`.
pub fn encode_type(
    tpe: Type,
    asm: &mut Assembly,
    resolver: &mut impl TypeDefOrRefResolver,
    out: &mut Vec<u8>,
) {
    use crate::ir::{Float, Int};
    match tpe {
        Type::Void => out.push(ET_VOID),
        Type::Bool => out.push(ET_BOOLEAN),
        Type::PlatformChar => out.push(ET_CHAR),
        Type::PlatformString => out.push(ET_STRING),
        Type::PlatformObject => out.push(ET_OBJECT),
        Type::Int(int) => match int {
            Int::I8 => out.push(ET_I1),
            Int::U8 => out.push(ET_U1),
            Int::I16 => out.push(ET_I2),
            Int::U16 => out.push(ET_U2),
            Int::I32 => out.push(ET_I4),
            Int::U32 => out.push(ET_U4),
            Int::I64 => out.push(ET_I8),
            Int::U64 => out.push(ET_U8),
            Int::ISize => out.push(ET_I),
            Int::USize => out.push(ET_U),
            // 128-bit ints are BCL valuetypes, exactly as type_il renders them.
            Int::I128 => {
                let cref = ClassRef::int_128(asm);
                encode_class(cref, /*is_valuetype*/ true, asm, resolver, out);
            }
            Int::U128 => {
                let cref = ClassRef::uint_128(asm);
                encode_class(cref, true, asm, resolver, out);
            }
        },
        Type::Float(float) => match float {
            Float::F32 => out.push(ET_R4),
            Float::F64 => out.push(ET_R8),
            Float::F16 => {
                let cref = ClassRef::half(asm);
                encode_class(cref, true, asm, resolver, out);
            }
            // il_exporter renders `valuetype f128` (a synthetic module-local struct); wiring that
            // TypeDef up is deferred with the rest of f128 (a .NET-mode wall — C-mode only).
            Float::F128 => todo!("f128 in a PE signature"),
        },
        Type::Ptr(inner) => {
            out.push(ET_PTR);
            encode_type(asm[inner], asm, resolver, out);
        }
        Type::Ref(inner) => {
            out.push(ET_BYREF);
            encode_type(asm[inner], asm, resolver, out);
        }
        Type::ClassRef(cref) => {
            // `System.Object`/`System.String` have DEDICATED CLI element types
            // (ELEMENT_TYPE_OBJECT 0x1C / ELEMENT_TYPE_STRING 0x0E, §II.23.1.16) — mirrors
            // `il_exporter::type_il`'s identical `Type::ClassRef` special-case (that fn's own
            // doc comment: "BCL method signatures are encoded with those [element types], so a
            // plain `class […]System.Object` typeref does NOT match `object` during runtime
            // method resolution -> MissingMethodException"). Some codegen paths still produce a
            // `Type::ClassRef` naming `System.String`/`System.Object` instead of the intrinsic
            // `Type::PlatformString`/`Type::PlatformObject` (a "residual of the pre-P2-S1
            // default" per that same comment) — e.g. `StringBuilder::ToString()`'s return type
            // arrived this way, and without this fallback the encoded `ET_CLASS +
            // TypeRef(System.String)` blob doesn't match the real BCL signature's `ET_STRING`
            // byte, so `dotnet` rejects the `MemberRef` with `MissingMethodException: Method not
            // found` even though the class/name/param-count are all correct (regression caught
            // wiring `DIRECT_PE=1`).
            let cr = asm.class_ref(cref);
            if !cr.is_valuetype() && cr.generics().is_empty() {
                match &asm[cr.name()] {
                    "System.Object" => {
                        out.push(ET_OBJECT);
                        return;
                    }
                    "System.String" => {
                        out.push(ET_STRING);
                        return;
                    }
                    _ => {}
                }
            }
            let is_valuetype = asm[cref].is_valuetype();
            encode_class(cref, is_valuetype, asm, resolver, out);
        }
        // NB the naming crossover (mirrors type_il): MethodGeneric/TypeGeneric render as `!N`
        // (a *type's* generic parameter, VAR); CallGeneric renders as `!!N` (a *method's*
        // generic parameter, MVAR).
        Type::PlatformGeneric(idx, GenericKind::MethodGeneric | GenericKind::TypeGeneric) => {
            out.push(ET_VAR);
            write_compressed_u32(out, idx);
        }
        Type::PlatformGeneric(idx, GenericKind::CallGeneric) => {
            out.push(ET_MVAR);
            write_compressed_u32(out, idx);
        }
        Type::PlatformArray { elem, dims } => {
            if dims.get() == 1 {
                out.push(ET_SZARRAY);
                encode_type(asm[elem], asm, resolver, out);
            } else {
                // General array shape (§II.23.2.13): rank, no explicit sizes, no lower bounds —
                // matching the bare `T[,]` il_exporter writes.
                out.push(ET_ARRAY);
                encode_type(asm[elem], asm, resolver, out);
                write_compressed_u32(out, u32::from(dims.get()));
                write_compressed_u32(out, 0);
                write_compressed_u32(out, 0);
            }
        }
        Type::FnPtr(sig) => {
            // il_exporter writes `method ret *(args)` — the managed DEFAULT convention.
            out.push(ET_FNPTR);
            let sig = asm[sig].clone();
            encode_method_sig(SIG_DEFAULT, 0, &sig, asm, resolver, out);
        }
        Type::SIMDVector(simdvec) => {
            // `SIMDVector::class` already builds exactly the `ClassRef` `il_exporter::type_il`'s
            // `Type::SIMDVector` arm renders textually: a `valuetype` in
            // `System.Runtime.Intrinsics` named `System.Runtime.Intrinsics.Vector{bits}` with the
            // scalar element type as its sole generic argument — e.g. `Vector128<int32>`. Routing
            // it through `encode_class` (the same path `i128`/`u128`/`f16` use) gets the
            // `GENERICINST`+`VALUETYPE` wrapping and the `TypeDefOrRefResolver` call for free.
            let cref = simdvec.class(asm);
            encode_class(cref, /*is_valuetype*/ true, asm, resolver, out);
        }
    }
}

/// `CLASS`/`VALUETYPE` (+ `GENERICINST` when instantiated) for a class reference.
fn encode_class(
    cref: Interned<ClassRef>,
    is_valuetype: bool,
    asm: &mut Assembly,
    resolver: &mut impl TypeDefOrRefResolver,
    out: &mut Vec<u8>,
) {
    let kind = if is_valuetype { ET_VALUETYPE } else { ET_CLASS };
    let generics: Vec<Type> = asm[cref].generics().to_vec();
    if generics.is_empty() {
        out.push(kind);
        write_compressed_u32(out, resolver.type_def_or_ref(cref, asm));
    } else {
        out.push(ET_GENERICINST);
        out.push(kind);
        write_compressed_u32(out, resolver.type_def_or_ref(cref, asm));
        write_compressed_u32(out, u32::try_from(generics.len()).unwrap());
        for g in generics {
            encode_type(g, asm, resolver, out);
        }
    }
}

/// A `MethodDefSig`/`MethodRefSig` (§II.23.2.1–II.23.2.2): convention byte, optional generic
/// parameter count, parameter count, return type, parameter types. `convention` is a bitwise OR
/// of [`SIG_DEFAULT`]/[`SIG_HASTHIS`]/[`SIG_GENERIC`]; `generic_params` must be non-zero iff
/// `SIG_GENERIC` is set.
pub fn encode_method_sig(
    convention: u8,
    generic_params: u32,
    sig: &FnSig,
    asm: &mut Assembly,
    resolver: &mut impl TypeDefOrRefResolver,
    out: &mut Vec<u8>,
) {
    debug_assert_eq!(convention & SIG_GENERIC != 0, generic_params != 0);
    out.push(convention);
    if generic_params != 0 {
        write_compressed_u32(out, generic_params);
    }
    // For instance methods the `this` pointer is implicit — it is NOT in the param count.
    write_compressed_u32(out, u32::try_from(sig.inputs().len()).unwrap());
    // Return position: bare `void` is legal (§II.23.2.11) — matches `il_exporter::type_il`.
    encode_type(*sig.output(), asm, resolver, out);
    // Parameter position: `void` is illegal — matches `il_exporter::non_void_type_il`.
    for input in sig.inputs().to_vec() {
        encode_non_void_type(input, asm, resolver, out);
    }
}

/// A field signature (§II.23.2.4). A field typed `void` is illegal metadata — substitutes
/// `RustVoid`, matching `il_exporter`'s `non_void_type_il` at every `.field` call site.
pub fn encode_field_sig(
    tpe: Type,
    asm: &mut Assembly,
    resolver: &mut impl TypeDefOrRefResolver,
    out: &mut Vec<u8>,
) {
    out.push(SIG_FIELD);
    encode_non_void_type(tpe, asm, resolver, out);
}

/// A `PropertySig` (§II.23.2.5): `PROPERTY (0x08) [| HASTHIS (0x20)]`, compressed `ParamCount`,
/// the property's value `Type`, then the indexer parameter types. This backend only emits
/// NON-INDEXER properties (`#[dotnet_property]` scope), so `ParamCount` is always 0 and no
/// parameter types follow. The value type is encoded with the RAW encoder ([`encode_type`]),
/// byte-matching the getter's return-type encoding — the two blobs must agree for csc's
/// `PEPropertySymbol` accessor-shape validation; a `void`-typed property is rejected upstream
/// (macro + comptime), so no `RustVoid` substitution question arises here.
pub fn encode_property_sig(
    has_this: bool,
    tpe: Type,
    asm: &mut Assembly,
    resolver: &mut impl TypeDefOrRefResolver,
    out: &mut Vec<u8>,
) {
    let convention = if has_this {
        SIG_PROPERTY | SIG_HASTHIS
    } else {
        SIG_PROPERTY
    };
    out.push(convention);
    write_compressed_u32(out, 0); // ParamCount — non-indexer only at shipped scope.
    encode_type(tpe, asm, resolver, out);
}

/// A local-variable signature (§II.23.2.6), stored via `StandAloneSig` and referenced by fat
/// method-body headers. A `void`-typed local is illegal — substitutes `RustVoid`, matching
/// `il_exporter`'s `.locals` rendering (`non_void_type_il`).
pub fn encode_locals_sig(
    locals: &[Type],
    asm: &mut Assembly,
    resolver: &mut impl TypeDefOrRefResolver,
    out: &mut Vec<u8>,
) {
    out.push(SIG_LOCALS);
    write_compressed_u32(out, u32::try_from(locals.len()).unwrap());
    for local in locals {
        encode_non_void_type(*local, asm, resolver, out);
    }
}

/// A `MethodSpec` instantiation blob (§II.23.2.15) — the `<int32, …>` of a generic-method call.
/// Generic arguments render with the RAW encoder, matching `il_exporter`'s call-site generic
/// list (mod.rs `generic_list`, built with `type_il`, not `non_void_type_il`) — a `void`
/// generic argument does not occur in practice (ZSTs never reach codegen as `Type::Void`
/// generic args), so this mirrors the oracle exactly rather than guessing a substitution it
/// doesn't make.
pub fn encode_method_spec_sig(
    args: &[Type],
    asm: &mut Assembly,
    resolver: &mut impl TypeDefOrRefResolver,
    out: &mut Vec<u8>,
) {
    out.push(SIG_GENERICINST_METHOD);
    write_compressed_u32(out, u32::try_from(args.len()).unwrap());
    for arg in args {
        encode_type(*arg, asm, resolver, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Float, Int};
    use std::num::NonZeroU8;

    /// Hands out fixed coded indices so blobs are predictable without a table builder.
    struct StubResolver;
    impl TypeDefOrRefResolver for StubResolver {
        fn type_def_or_ref(&mut self, _: Interned<ClassRef>, _: &mut Assembly) -> u32 {
            // TypeRef row 1 → (1 << 2) | 1 = 5.
            5
        }
    }

    fn encode(tpe: Type, asm: &mut Assembly) -> Vec<u8> {
        let mut out = Vec::new();
        encode_type(tpe, asm, &mut StubResolver, &mut out);
        out
    }

    #[test]
    fn primitives() {
        let mut asm = Assembly::default();
        assert_eq!(encode(Type::Bool, &mut asm), [ET_BOOLEAN]);
        assert_eq!(encode(Type::Int(Int::I32), &mut asm), [ET_I4]);
        assert_eq!(encode(Type::Int(Int::USize), &mut asm), [ET_U]);
        assert_eq!(encode(Type::Float(Float::F64), &mut asm), [ET_R8]);
        assert_eq!(encode(Type::PlatformString, &mut asm), [ET_STRING]);
        assert_eq!(encode(Type::PlatformObject, &mut asm), [ET_OBJECT]);
    }

    /// Regression for `MissingMethodException: Method not found:
    /// 'System.String System.Text.StringBuilder.ToString()'` caught wiring `DIRECT_PE=1` into
    /// the linker: some codegen paths hand `encode_type` a `Type::ClassRef` naming
    /// `System.String`/`System.Object` instead of the intrinsic `Type::PlatformString`/
    /// `Type::PlatformObject` (`StringBuilder::ToString()`'s RETURN type arrived this way). A
    /// real BCL method signature is encoded with the DEDICATED `ET_STRING`/`ET_OBJECT` element
    /// types (§II.23.1.16), not `ET_CLASS + TypeRef(System.String)` — the two are NOT
    /// interchangeable for CoreCLR's method-lookup, so an un-collapsed `ClassRef` blob doesn't
    /// match the real signature and `dotnet` rejects the whole `MemberRef` with
    /// `MissingMethodException`, even though the class/name/param-count all match. Mirrors
    /// `il_exporter::type_il`'s identical `Type::ClassRef` fallback (that fn's own doc explains
    /// the exact same `GCHandle.Alloc(object)` failure mode this fixes for the PE path).
    #[test]
    fn classref_naming_system_string_or_object_collapses_to_the_dedicated_element_type() {
        let mut asm = Assembly::default();

        let string_name = asm.alloc_string("System.String");
        let string_asm = asm.alloc_string("System.Runtime");
        let string_cref = asm.alloc_class_ref(ClassRef::new(
            string_name,
            Some(string_asm),
            false,
            [].into(),
        ));
        assert_eq!(
            encode(Type::ClassRef(string_cref), &mut asm),
            [ET_STRING],
            "a ClassRef literally naming System.String must collapse to ET_STRING, not ET_CLASS+TypeRef"
        );

        let object_name = asm.alloc_string("System.Object");
        let object_cref = asm.alloc_class_ref(ClassRef::new(
            object_name,
            Some(string_asm),
            false,
            [].into(),
        ));
        assert_eq!(
            encode(Type::ClassRef(object_cref), &mut asm),
            [ET_OBJECT],
            "a ClassRef literally naming System.Object must collapse to ET_OBJECT, not ET_CLASS+TypeRef"
        );

        // A DIFFERENT class named "System.String" nowhere near the real BCL type (e.g. a
        // VALUETYPE, or one with generics) must NOT collapse — the fallback is scoped to
        // `!is_valuetype && generics().is_empty()`, matching `il_exporter`'s own guard exactly.
        let valuetype_string_cref = asm.alloc_class_ref(ClassRef::new(
            string_name,
            Some(string_asm),
            true,
            [].into(),
        ));
        let encoded = encode(Type::ClassRef(valuetype_string_cref), &mut asm);
        assert_ne!(
            encoded,
            vec![ET_STRING],
            "a VALUETYPE must not collapse to ET_STRING"
        );
        assert_eq!(encoded[0], ET_VALUETYPE);
    }

    #[test]
    fn pointers_nest() {
        let mut asm = Assembly::default();
        let inner = asm.alloc_type(Type::Int(Int::U8));
        assert_eq!(encode(Type::Ptr(inner), &mut asm), [ET_PTR, ET_U1]);
        let void = asm.alloc_type(Type::Void);
        assert_eq!(encode(Type::Ptr(void), &mut asm), [ET_PTR, ET_VOID]);
        let ptr = asm.alloc_type(Type::Ptr(inner));
        assert_eq!(encode(Type::Ref(ptr), &mut asm), [ET_BYREF, ET_PTR, ET_U1]);
    }

    #[test]
    fn int128_lowers_to_bcl_valuetype() {
        let mut asm = Assembly::default();
        assert_eq!(
            encode(Type::Int(Int::I128), &mut asm),
            [ET_VALUETYPE, 5],
            "i128 must encode as VALUETYPE System.Int128 via the resolver"
        );
    }

    #[test]
    fn simd_vector_lowers_to_intrinsics_generic_valuetype() {
        // Mirrors il_exporter::type_il's `Type::SIMDVector` arm: `valuetype
        // [System.Runtime.Intrinsics]System.Runtime.Intrinsics.Vector128`1<int32>` becomes
        // GENERICINST + VALUETYPE + (coded TypeDefOrRef index) + argc(1) + the element type.
        use crate::ir::tpe::simd::SIMDVector;
        let mut asm = Assembly::default();
        let vec4xi32 = SIMDVector::new(Int::I32.into(), 4); // 4 * 32 = 128 bits
        assert_eq!(
            encode(Type::SIMDVector(vec4xi32), &mut asm),
            [ET_GENERICINST, ET_VALUETYPE, 5, 1, ET_I4],
            "SIMD vector must encode as GENERICINST VALUETYPE Vector128<int32> via the resolver"
        );

        // A different element width and lane count still round through the same shape (256-bit,
        // f64 elements) — this also exercises the arity-postfix fix in
        // `MetadataBuilder::type_def_or_ref` end-to-end for a *second* distinct generic external
        // ClassRef (name differs per `bits()`, so it must not collide with the 128-bit case above
        // in the resolver's cache).
        let vec4xf64 = SIMDVector::new(Float::F64.into(), 4); // 4 * 64 = 256 bits
        assert_eq!(
            encode(Type::SIMDVector(vec4xf64), &mut asm),
            [ET_GENERICINST, ET_VALUETYPE, 5, 1, ET_R8]
        );
    }

    #[test]
    fn simd_vector_resolver_uses_the_real_metadata_builder() {
        // End-to-end through the real MetadataBuilder resolver (not the StubResolver): confirms
        // encode_type actually drives `type_def_or_ref` (a live TypeRef row, not a stub), and
        // that resolving the same (open) ClassRef a second time reuses the cached row instead of
        // minting a fresh one — the two encodings must be byte-for-byte identical. The emitted
        // row's exact `Vector128`1`/`System.Runtime.Intrinsics` name+namespace (and the
        // arity-postfix fix that makes that possible) is covered by
        // `tables::tests::generic_external_type_ref_name_carries_the_arity_postfix`, which has
        // visibility into MetadataBuilder's private row storage.
        use super::super::tables::MetadataBuilder;
        use crate::ir::tpe::simd::SIMDVector;
        let mut asm = Assembly::default();
        let mut mb = MetadataBuilder::new();
        let vec2xi64 = SIMDVector::new(Int::I64.into(), 2); // 2 * 64 = 128 bits

        let mut first = Vec::new();
        encode_type(Type::SIMDVector(vec2xi64), &mut asm, &mut mb, &mut first);
        assert_eq!(first[..2], [ET_GENERICINST, ET_VALUETYPE]);

        let mut second = Vec::new();
        encode_type(Type::SIMDVector(vec2xi64), &mut asm, &mut mb, &mut second);
        assert_eq!(
            first, second,
            "the resolver's cache must dedupe the repeat lookup"
        );
    }

    #[test]
    fn generic_params_map_to_var_mvar() {
        let mut asm = Assembly::default();
        assert_eq!(
            encode(Type::PlatformGeneric(0, GenericKind::TypeGeneric), &mut asm),
            [ET_VAR, 0]
        );
        assert_eq!(
            encode(
                Type::PlatformGeneric(1, GenericKind::MethodGeneric),
                &mut asm
            ),
            [ET_VAR, 1],
            "MethodGeneric renders as !N (VAR) — the type_il naming crossover"
        );
        assert_eq!(
            encode(Type::PlatformGeneric(2, GenericKind::CallGeneric), &mut asm),
            [ET_MVAR, 2]
        );
    }

    #[test]
    fn arrays() {
        let mut asm = Assembly::default();
        let elem = asm.alloc_type(Type::Int(Int::I32));
        assert_eq!(
            encode(
                Type::PlatformArray {
                    elem,
                    dims: NonZeroU8::new(1).unwrap()
                },
                &mut asm
            ),
            [ET_SZARRAY, ET_I4]
        );
        assert_eq!(
            encode(
                Type::PlatformArray {
                    elem,
                    dims: NonZeroU8::new(2).unwrap()
                },
                &mut asm
            ),
            [ET_ARRAY, ET_I4, 2, 0, 0]
        );
    }

    #[test]
    fn method_and_field_and_locals_sigs() {
        let mut asm = Assembly::default();
        let sig = FnSig::new([Type::Int(Int::I32), Type::Bool], Type::Void);
        let mut out = Vec::new();
        encode_method_sig(SIG_DEFAULT, 0, &sig, &mut asm, &mut StubResolver, &mut out);
        assert_eq!(out, [SIG_DEFAULT, 2, ET_VOID, ET_I4, ET_BOOLEAN]);

        let mut out = Vec::new();
        encode_field_sig(Type::Int(Int::U64), &mut asm, &mut StubResolver, &mut out);
        assert_eq!(out, [SIG_FIELD, ET_U8]);

        let mut out = Vec::new();
        encode_locals_sig(
            &[Type::Int(Int::I32), Type::PlatformString],
            &mut asm,
            &mut StubResolver,
            &mut out,
        );
        assert_eq!(out, [SIG_LOCALS, 2, ET_I4, ET_STRING]);

        let mut out = Vec::new();
        encode_method_spec_sig(
            &[Type::Int(Int::I32)],
            &mut asm,
            &mut StubResolver,
            &mut out,
        );
        assert_eq!(out, [SIG_GENERICINST_METHOD, 1, ET_I4]);
    }

    /// §II.23.2.5: an instance `int Volume` property's signature blob is exactly
    /// `[PROPERTY|HASTHIS, 0 params, ET_I4]` = `[0x28, 0x00, 0x08]` — the byte triple a real
    /// Roslyn-compiled `int Volume { get; set; }` interface property carries.
    #[test]
    fn property_sig_encodes_property_hasthis_paramcount_and_type() {
        let mut asm = Assembly::default();
        let mut out = Vec::new();
        encode_property_sig(
            true,
            Type::Int(Int::I32),
            &mut asm,
            &mut StubResolver,
            &mut out,
        );
        assert_eq!(out, [SIG_PROPERTY | SIG_HASTHIS, 0x00, ET_I4]);

        // A managed-typed (System.String) property uses the dedicated ET_STRING element type.
        let mut out = Vec::new();
        encode_property_sig(
            true,
            Type::PlatformString,
            &mut asm,
            &mut StubResolver,
            &mut out,
        );
        assert_eq!(out, [SIG_PROPERTY | SIG_HASTHIS, 0x00, ET_STRING]);

        // The (currently-unreached) static shape omits HASTHIS — pinned so a future static
        // property doesn't silently ship the instance convention byte.
        let mut out = Vec::new();
        encode_property_sig(false, Type::Bool, &mut asm, &mut StubResolver, &mut out);
        assert_eq!(out, [SIG_PROPERTY, 0x00, ET_BOOLEAN]);
    }

    #[test]
    fn fn_ptr_embeds_a_method_sig() {
        let mut asm = Assembly::default();
        let sig = asm.sig([Type::Int(Int::I32)], Type::Bool);
        assert_eq!(
            encode(Type::FnPtr(sig), &mut asm),
            [ET_FNPTR, SIG_DEFAULT, 1, ET_BOOLEAN, ET_I4]
        );
    }

    /// Regression for a `dotnet` load-time `FileLoadException: Illegal 'void' in signature.`
    /// hit smoke-testing `DIRECT_PE=1` on `cargo_tests/hello_world`: `Type::Void` is only legal
    /// as a method's *return* type (§II.23.1.16 / CoreCLR's own signature validator) — a field,
    /// a parameter, or a local variable typed bare `void` is malformed metadata. `il_exporter`'s
    /// `non_void_type_il` substitutes `valuetype RustVoid` in exactly these positions; these
    /// four assertions pin the same substitution in the PE writer's blob encoder so this class
    /// of bug cannot regress silently (a hand-built `Assembly` — not a real Rust program — can
    /// still exercise the same code path deterministically here).
    #[test]
    fn void_is_illegal_outside_return_position_and_gets_rust_void_substituted() {
        let mut asm = Assembly::default();

        // A field typed `void` (the `global_void` static's actual shape, `asm.rs::global_void`)
        // must encode as `valuetype RustVoid`, not the bare ET_VOID byte.
        let mut out = Vec::new();
        encode_field_sig(Type::Void, &mut asm, &mut StubResolver, &mut out);
        assert_eq!(
            out,
            [SIG_FIELD, ET_VALUETYPE, 5],
            "a void-typed field must substitute RustVoid (ET_VALUETYPE), not ET_VOID"
        );

        // A `void`-typed method PARAMETER (e.g. a `PassMode::Ignore`/ZST receiver slot) must
        // substitute RustVoid; the RETURN type in the same signature stays bare `void`
        // (ET_VOID) — this is the same split `il_exporter::type_il`/`non_void_type_il` draw.
        let sig = FnSig::new([Type::Void, Type::Int(Int::I32)], Type::Void);
        let mut out = Vec::new();
        encode_method_sig(SIG_DEFAULT, 0, &sig, &mut asm, &mut StubResolver, &mut out);
        assert_eq!(
            out,
            [SIG_DEFAULT, 2, ET_VOID, ET_VALUETYPE, 5, ET_I4],
            "return-position void stays ET_VOID; parameter-position void substitutes RustVoid"
        );

        // A `void`-typed local must substitute RustVoid.
        let mut out = Vec::new();
        encode_locals_sig(&[Type::Void], &mut asm, &mut StubResolver, &mut out);
        assert_eq!(
            out,
            [SIG_LOCALS, 1, ET_VALUETYPE, 5],
            "a void-typed local must substitute RustVoid, not ET_VOID"
        );

        // `Type::FnPtr`'s own return position stays bare `void`; its param position substitutes
        // RustVoid — exercises the same split through the `encode_type` -> `encode_method_sig`
        // nesting `Type::FnPtr` uses.
        let fnptr_sig = asm.sig([Type::Void], Type::Void);
        assert_eq!(
            encode(Type::FnPtr(fnptr_sig), &mut asm),
            [ET_FNPTR, SIG_DEFAULT, 1, ET_VOID, ET_VALUETYPE, 5],
            "an fn-ptr's own return type stays ET_VOID; its parameter substitutes RustVoid"
        );
    }
}
