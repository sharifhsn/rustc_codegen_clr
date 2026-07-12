pub mod adt;
pub mod utilis;

use rustc_middle::ty::{PseudoCanonicalInput, TyCtxt};
pub trait GetTypeExt<'tcx> {
    fn type_from_cache(&mut self, ty: Ty<'tcx>) -> Type;
}
impl<'tcx> GetTypeExt<'tcx> for MethodCompileCtx<'tcx, '_> {
    fn type_from_cache(&mut self, ty: Ty<'tcx>) -> Type {
        get_type(ty, self)
    }
}
pub fn align_of<'tcx>(ty: rustc_middle::ty::Ty<'tcx>, tcx: TyCtxt<'tcx>) -> u64 {
    let layout = tcx
        .layout_of(PseudoCanonicalInput {
            typing_env: rustc_middle::ty::TypingEnv::fully_monomorphized(),
            value: ty,
        })
        .expect("Can't get layout of a type.")
        .layout;

    let align = layout.align.abi;
    align.bytes()
}

use crate::fn_ctx::MethodCompileCtx;
use crate::r#type::adt::FieldOffsetIterator;
use crate::r#type::utilis::{
    INTEROP_ARR_TPE_NAME, INTEROP_BYREF_TPE_NAME, INTEROP_CHR_TPE_NAME, INTEROP_CLASS_TPE_NAME,
    INTEROP_GENERIC_STRUCT_TPE_NAME, INTEROP_GENERIC_TPE_NAME, INTEROP_METHOD_GENERIC_TPE_NAME,
    INTEROP_STRUCT_TPE_NAME, INTEROP_TYPE_GENERIC_TPE_NAME, is_zst, resolve_const_size,
};
use crate::r#type::utilis::{adt_name, stable_adt_name};
use crate::r#type::utilis::{garg_to_string, garg_to_usize, ptr_is_fat, tuple_name};
use cilly::IString;
use cilly::bimap::Interned;
use cilly::class::{ClassDefIdx, FixedArrayLayout};
use cilly::{
    Assembly, IntoAsmIndex, add, ld_arg, ptr_cast,
    tpe::GenericKind,
    tpe::simd::SIMDVector,
    {
        Access, BasicBlock, CILNode, CILRoot, ClassDef, ClassRef, FieldDesc, Float, Int, MethodDef,
        MethodImpl, Type, cilnode::MethodKind,
    },
};
/// A representation of a primitve type or a reference.
use std::{
    collections::{HashMap, HashSet},
    num::{NonZero, NonZeroU32},
};

use rustc_abi::{Layout, VariantIdx};
use rustc_middle::ty::{
    AdtDef, AdtKind, CoroutineArgsExt, FloatTy, IntTy, List, Ty, TyKind, UintTy,
};

#[must_use]
pub fn from_int(int_tpe: &IntTy) -> cilly::Type {
    use cilly::Type;
    match int_tpe {
        IntTy::I8 => Type::Int(Int::I8),
        IntTy::I16 => Type::Int(Int::I16),
        IntTy::I32 => Type::Int(Int::I32),
        IntTy::I64 => Type::Int(Int::I64),
        IntTy::I128 => Type::Int(Int::I128),
        IntTy::Isize => Type::Int(Int::ISize),
    }
}

#[must_use]
pub fn from_uint(uint_tpe: &UintTy) -> cilly::Type {
    use cilly::Type;
    match uint_tpe {
        UintTy::U8 => Type::Int(Int::U8),
        UintTy::U16 => Type::Int(Int::U16),
        UintTy::U32 => Type::Int(Int::U32),
        UintTy::U64 => Type::Int(Int::U64),
        UintTy::U128 => Type::Int(Int::U128),
        UintTy::Usize => Type::Int(Int::USize),
    }
}

#[must_use]
pub fn from_float(float: &FloatTy) -> cilly::Type {
    use cilly::Type;
    match float {
        FloatTy::F16 => Type::Float(Float::F16),
        FloatTy::F32 => Type::Float(Float::F32),
        FloatTy::F64 => Type::Float(Float::F64),
        FloatTy::F128 => Type::Float(Float::F128),
    }
}
fn get_adt<'tcx>(
    adt_ty: Ty<'tcx>,
    def: AdtDef<'tcx>,
    subst: &'tcx List<rustc_middle::ty::GenericArg<'tcx>>,
    name: Interned<IString>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<ClassRef> {
    let cref = ClassRef::new(name, None, true, [].into());
    let semantic_size = ctx.layout_of(adt_ty).layout.size().bytes();
    if ctx.contains_ref(&cref) {
        let cref = ctx.alloc_class_ref(cref);
        ctx.set_rust_semantic_size(cref, semantic_size);
        cref
    } else {
        let cref = ctx.alloc_class_ref(cref);
        // Register before recursively lowering fields. Self-referential fields may encounter this
        // pre-interned ClassRef while its definition is still under construction, and any semantic
        // size operation in that path must still observe Rust's layout rather than CLR storage.
        ctx.set_rust_semantic_size(cref, semantic_size);
        let adt_kind = def.adt_kind();
        // A library's exported structs get public accessors synthesized below (see `add_record_accessors`).
        let is_exported_struct =
            adt_kind == AdtKind::Struct && stable_adt_name(def, ctx.tcx(), subst).is_some();
        let class_def = match adt_kind {
            AdtKind::Struct => struct_(name, def, adt_ty, subst, ctx),
            AdtKind::Enum => enum_(name, def, adt_ty, subst, ctx),
            AdtKind::Union => union_(name, def, adt_ty, subst, ctx),
        };
        // Capture the field list before the def is moved into `class_def` (registration).
        let accessor_fields = is_exported_struct.then(|| class_def.fields().to_vec());
        ctx.class_def(class_def).unwrap();
        // Synthesize accessors only *after* the class is registered — `new_method` requires the owning
        // ClassDef to already exist in the assembly.
        if let Some(fields) = accessor_fields {
            add_record_accessors(cref, &fields, ctx);
        }
        cref
    }
}
/// Lowers a generic argument that must be a *tuple type* into the lowered .NET types of its elements.
/// Used by the WF-9 generic interop bridge to pass a generic .NET type's argument list (e.g. the
/// `(i32,)` of `List<i32>`, or the `(K, V)` of `Dictionary<K, V>`) as a single type parameter.
pub fn tuple_garg_types<'tcx>(
    garg: rustc_middle::ty::GenericArg<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Vec<Type> {
    let ty = ctx.monomorphize(
        garg.as_type()
            .expect("a generic-argument tuple must be passed as a type"),
    );
    match ty.kind() {
        TyKind::Tuple(elems) => elems
            .iter()
            .map(|elem| get_type(ctx.monomorphize(elem), ctx))
            .collect(),
        _ => panic!("expected a tuple of generic arguments, got {ty:?}"),
    }
}
/// Converts a Rust MIR type to an optimized .NET type representation.
pub fn get_type<'tcx>(ty: Ty<'tcx>, ctx: &mut MethodCompileCtx<'tcx, '_>) -> Type {
    let ty = ctx.monomorphize(ty);
    // The WF-9 generic-parameter markers (`RustcCLRInteropTypeGeneric`/`…MethodGeneric`) are
    // zero-sized but must lower to `!N`/`!!N` — they only ever appear at the type level (inside a
    // method's definition-shape signature), never as a runtime value, so the usual ZST→Void
    // collapse would erase them. Exempt them before the ZST early-return.
    let is_generic_marker = if let TyKind::Adt(def, _) = ty.kind() {
        let item_name = ctx.tcx().item_name(def.did());
        matches!(
            item_name.as_str(),
            INTEROP_TYPE_GENERIC_TPE_NAME
                | INTEROP_METHOD_GENERIC_TPE_NAME
                | INTEROP_BYREF_TPE_NAME
        )
    } else {
        false
    };
    // If this is a ZST, return a void type.
    if is_zst(ty, ctx.tcx()) && !is_generic_marker {
        return Type::Void;
    }

    match ty.kind() {
        TyKind::Bound(_, _inner) => Type::Void,
        TyKind::Bool => Type::Bool,
        TyKind::Char => Type::Int(Int::U32),
        TyKind::Closure(_def, args) => {
            // Get the closure fields.
            let closure = args.as_closure();
            let fields: Box<[_]> = closure
                .upvar_tys()
                .iter()
                .map(|ty| get_type(ty, ctx))
                .collect();
            // Get a closure name.
            let name = closure_name(ty, ctx);
            let name = ctx.alloc_string(name);
            // Get the layout of the closure
            let layout = ctx.layout_of(ty);
            // Allocate a class reference to the closure
            let cref = ctx.alloc_class_ref(ClassRef::new(name, None, true, [].into()));
            ctx.set_rust_semantic_size(cref, layout.layout.size().bytes());
            // If there is no defition of this closure present, create the closure.
            if ctx.class_ref_to_def(cref).is_none() {
                let type_def = closure_typedef(&fields, layout.layout, ctx, name);
                ctx.class_def(type_def).unwrap();
            }
            Type::ClassRef(cref)
        }
        TyKind::Dynamic(_list, _) => {
            let name = ctx.alloc_string("Dyn");
            let cref = ctx.alloc_class_ref(ClassRef::new(name, None, true, [].into()));
            if ctx.class_ref_to_def(cref).is_none() {
                ctx.class_def(ClassDef::new(
                    name,
                    true,
                    0,
                    None,
                    vec![],
                    vec![],
                    cilly::Access::Public,
                    None,
                    None,
                    false, // Two separate pointers.
                ))
                .unwrap();
            }
            Type::ClassRef(cref)
        }
        TyKind::Float(float) => from_float(float),
        TyKind::Foreign(_foregin) => Type::Void,
        TyKind::FnDef(_did, _subst) => Type::Void,
        TyKind::FnPtr(sig, _) => {
            let sig = ctx.tcx().normalize_erasing_late_bound_regions(
                rustc_middle::ty::TypingEnv::fully_monomorphized(),
                *sig,
            );
            //let sig = crate::function_sig::from_poly_sig(method, tcx, self, sig);
            let output = get_type(ctx.monomorphize(sig.output()), ctx);
            let inputs: Box<[Type]> = sig
                .inputs()
                .iter()
                .map(|input| get_type(ctx.monomorphize(*input), ctx))
                .collect();
            let sig = ctx.sig(inputs, output);
            Type::FnPtr(sig)
        }
        TyKind::Int(int) => from_int(int),
        TyKind::Uint(int) => from_uint(int),
        TyKind::Never => Type::Void,
        TyKind::RawPtr(inner, _) | TyKind::Ref(_, inner, _) => {
            if ptr_is_fat(*inner, ctx.tcx(), ctx.instance()) {
                let inner = match inner.kind() {
                    TyKind::Slice(inner) => ctx.monomorphize(*inner),
                    TyKind::Str => Ty::new_uint(ctx.tcx(), UintTy::U8),
                    _ => ctx.monomorphize(*inner),
                };
                Type::ClassRef(fat_ptr_to(inner, ctx))
            } else {
                let inner = get_type(*inner, ctx);
                ctx.nptr(inner)
            }
        }
        // Slice type is almost never refered to directly, and should pop up here ONLY in the case of
        // a DST.
        TyKind::Str => Type::Int(Int::U8),
        TyKind::Slice(inner) => {
            let inner = ctx.monomorphize(*inner);
            get_type(inner, ctx)
        }
        TyKind::Tuple(types) => {
            let types: Vec<_> = types.iter().map(|ty| get_type(ty, ctx)).collect();
            if types.is_empty() {
                Type::Void
            } else {
                let name = tuple_name(&types, ctx);
                let name = ctx.alloc_string(name);
                let cref = ClassRef::new(name, None, true, [].into());
                // This only checks if a refernce to this class has already been allocated. In theory, allocating a class reference beforhand could break this, and make it not add the type definition
                if !ctx.contains_ref(&cref) {
                    let layout = ctx.layout_of(ty);
                    let _ = tuple_typedef(&types, layout.layout, ctx, name);
                }
                Type::ClassRef(ctx.alloc_class_ref(cref))
            }
        }
        TyKind::Adt(def, subst) => {
            // Prefer a stable, de-mangled public name for a library's exported types; otherwise fall
            // back to the mangled name. See `stable_adt_name` for the (coherence-preserving) criteria.
            let name = stable_adt_name(*def, ctx.tcx(), subst)
                .unwrap_or_else(|| adt_name(*def, ctx.tcx(), subst));
            if def.repr().simd() {
                let (count, elem) = ty.simd_size_and_type(ctx.tcx());
                let elem = ctx.type_from_cache(elem);
                // if count == 1, then this is just a single type.
                if count == 1 {
                    return elem;
                }
                // .NET has a managed intrinsic vector class ONLY for the four widths
                // Vector64/128/256/512. Any other size has no managed class: too wide
                // (`Simd<u32, 32>` = 1024 bits), too narrow (`Simd<i8, 4>` = 32 bits — produced
                // e.g. when a 4-lane mask is materialized to `[bool; 4]`), or a non-power-of-two
                // width; the element may also not be a valid SIMD element. Rather than ICE in
                // `SIMDVector::new`, represent any such vector as a plain fixed-size array — the
                // only SIZE-CORRECT representation (padding to a wider managed vector would change
                // the type's byte size and corrupt struct/array/transmute layout). The SIMD
                // intrinsic builtins detect this array fallback via `simd_lane_info` and lower ops
                // over it element-wise (the per-lane spill-and-index path already works on it).
                let layout = ctx.layout_of(ty);
                let vec_bits = layout.layout.size().bytes().saturating_mul(8);
                let elem_simd: Result<cilly::tpe::simd::SIMDElem, _> = elem.try_into();
                if elem_simd.is_err() || !matches!(vec_bits, 64 | 128 | 256 | 512) {
                    let arr_size = layout.layout.size().bytes();
                    let arr_align = layout.layout.align().abi.bytes();
                    // I3 totality: a SIMD vector lowered to a fixed array can't exceed 2^32 bytes on
                    // .NET. Unreachable (max SIMD is kilobytes), but fail loud rather than return a
                    // silent ZST `Void`. `span_fatal` returns `!`, so the fixed_array call below runs
                    // only for representable sizes.
                    if std::convert::TryInto::<u32>::try_into(arr_size).is_err() {
                        ctx.tcx().dcx().span_fatal(
                            ctx.span(),
                            format!(
                                "SIMD vector {ty:?} lowered to a fixed array of {arr_size} bytes, which exceeds the .NET maximum type size of 2^32 bytes."
                            ),
                        );
                    }
                    let cref = fixed_array(ctx, elem, count, arr_size, arr_size, arr_align);
                    return Type::ClassRef(cref);
                }
                return Type::SIMDVector(SIMDVector::new(
                    elem_simd.unwrap(),
                    count.try_into().unwrap(),
                ));
            }
            // Gate the interop lowering on the OUTER ADT's own item name, not on a substring of
            // the fully monomorphized `name` — the monomorphized name embeds nested generics, so
            // a wrapper like `Option<RustcCLRInteropManagedClass<..>>` (item_name `Option`) or
            // `RustcCLRInteropManagedArray<RustcCLRInteropManagedClass<..>, 1>` *contains* the
            // interop substring and `is_name_magic` would (mis)route it here, where it has no
            // matching arm and would `todo!`. Only the four interop ADTs themselves qualify; a
            // generic type that merely HOLDS a managed value falls through to the normal ADT path.
            let item_name = ctx.tcx().item_name(def.did());
            let item_name = item_name.as_str();
            let is_interop_adt = matches!(
                item_name,
                INTEROP_CLASS_TPE_NAME
                    | INTEROP_STRUCT_TPE_NAME
                    | INTEROP_ARR_TPE_NAME
                    | INTEROP_CHR_TPE_NAME
                    | INTEROP_GENERIC_TPE_NAME
                    | INTEROP_GENERIC_STRUCT_TPE_NAME
                    | INTEROP_TYPE_GENERIC_TPE_NAME
                    | INTEROP_METHOD_GENERIC_TPE_NAME
                    | INTEROP_BYREF_TPE_NAME
            );
            if is_interop_adt {
                if item_name == INTEROP_CLASS_TPE_NAME {
                    assert!(
                        subst.len() == 2,
                        "Managed object reference must have exactly 2 generic arguments!"
                    );
                    let assembly = garg_to_string(subst[0], ctx.tcx());
                    let assembly = Some(assembly)
                        .filter(|assembly| !assembly.is_empty())
                        .map(|asm| ctx.alloc_string(asm));
                    let name = garg_to_string(subst[1], ctx.tcx());
                    let name = ctx.alloc_string(name);
                    Type::ClassRef(ctx.alloc_class_ref(ClassRef::new(
                        name,
                        assembly,
                        false,
                        [].into(),
                    )))
                } else if item_name == INTEROP_STRUCT_TPE_NAME {
                    // A managed value type carries 3 generics: <ASSEMBLY, CLASS_PATH, SIZE>.
                    // (The size hint is only used Rust-side for layout; the CLR knows the real size.)
                    assert!(
                        subst.len() == 3,
                        "Managed struct reference must have exactly 3 generic arguments (assembly, class, size)!"
                    );
                    let assembly = garg_to_string(subst[0], ctx.tcx());
                    let assembly = Some(assembly)
                        .filter(|assembly| !assembly.is_empty())
                        .map(|asm| ctx.alloc_string(asm));
                    let name = garg_to_string(subst[1], ctx.tcx());
                    let name = ctx.alloc_string(name);
                    Type::ClassRef(ctx.alloc_class_ref(ClassRef::new(
                        name,
                        assembly,
                        true,
                        [].into(),
                    )))
                } else if item_name == INTEROP_ARR_TPE_NAME {
                    assert!(
                        subst.len() == 2,
                        "Managed array reference must have exactly 2 generic arguments: type and dimension count!"
                    );
                    let element = &subst[0].as_type().expect("Array type must be specified!");
                    let element = get_type(ctx.monomorphize(*element), ctx);
                    let dimensions = garg_to_usize(subst[1], ctx.tcx());
                    Type::PlatformArray {
                        elem: ctx.alloc_type(element),
                        dims: std::num::NonZeroU8::new(dimensions.try_into().unwrap()).unwrap(),
                    }
                } else if item_name == INTEROP_CHR_TPE_NAME {
                    Type::PlatformChar
                } else if item_name == INTEROP_GENERIC_TPE_NAME {
                    // `RustcCLRInteropManagedGeneric<ASSEMBLY, CLASS_PATH, ClassGenerics>` — a handle
                    // to a managed object of a generic instantiation (e.g. `List<i32>`). The third
                    // generic is a *tuple* of the concrete .NET type arguments; lower it to a
                    // `ClassRef` carrying those generics (the exporter renders the `` `arity<args> ``).
                    assert!(
                        subst.len() == 3,
                        "RustcCLRInteropManagedGeneric must have exactly 3 generic arguments (assembly, class, class-generics-tuple)!"
                    );
                    let assembly = garg_to_string(subst[0], ctx.tcx());
                    let assembly = Some(assembly)
                        .filter(|assembly| !assembly.is_empty())
                        .map(|asm| ctx.alloc_string(asm));
                    let name = garg_to_string(subst[1], ctx.tcx());
                    let name = ctx.alloc_string(name);
                    let class_generics: Vec<Type> = tuple_garg_types(subst[2], ctx);
                    Type::ClassRef(ctx.alloc_class_ref(ClassRef::new(
                        name,
                        assembly,
                        false,
                        class_generics.into(),
                    )))
                } else if item_name == INTEROP_GENERIC_STRUCT_TPE_NAME {
                    // `RustcCLRInteropManagedGenericStruct<ASSEMBLY, CLASS_PATH, SIZE, ClassGenerics>`
                    // — a *value type* of a generic instantiation (e.g. `Nullable<JsonNodeOptions>`).
                    // Like the reference-type `INTEROP_GENERIC_TPE_NAME` arm, the trailing generic is
                    // a *tuple* of the concrete .NET type arguments; lower to a `ClassRef` that is
                    // BOTH a value type (`true`) AND carries those generics. `SIZE` (subst[2]) is only
                    // used Rust-side for layout — the CLR knows the real size — so it is ignored here.
                    assert!(
                        subst.len() == 4,
                        "RustcCLRInteropManagedGenericStruct must have exactly 4 generic arguments (assembly, class, size, class-generics-tuple)!"
                    );
                    let assembly = garg_to_string(subst[0], ctx.tcx());
                    let assembly = Some(assembly)
                        .filter(|assembly| !assembly.is_empty())
                        .map(|asm| ctx.alloc_string(asm));
                    let name = garg_to_string(subst[1], ctx.tcx());
                    let name = ctx.alloc_string(name);
                    let class_generics: Vec<Type> = tuple_garg_types(subst[3], ctx);
                    Type::ClassRef(ctx.alloc_class_ref(ClassRef::new(
                        name,
                        assembly,
                        true,
                        class_generics.into(),
                    )))
                } else if item_name == INTEROP_TYPE_GENERIC_TPE_NAME {
                    // Lowers to the .NET *class* generic parameter `!N` (a method-definition-shape
                    // marker used when calling a method on a generic instantiation).
                    let n = garg_to_usize(subst[0], ctx.tcx());
                    Type::PlatformGeneric(
                        u32::try_from(n).expect("class generic index over 2^32"),
                        GenericKind::TypeGeneric,
                    )
                } else if item_name == INTEROP_METHOD_GENERIC_TPE_NAME {
                    // Lowers to the .NET *method* generic parameter `!!N`.
                    let n = garg_to_usize(subst[0], ctx.tcx());
                    Type::PlatformGeneric(
                        u32::try_from(n).expect("method generic index over 2^32"),
                        GenericKind::CallGeneric,
                    )
                } else if item_name == INTEROP_BYREF_TPE_NAME {
                    // Lowers to a managed byref `Inner&` (`Type::Ref`) — the return shape of a
                    // byref-returning member, e.g. `Span<T>.get_Item(int) -> ref T` written as
                    // `RustcCLRInteropByRef<gen!(0)>` => `!0&`. `Inner` is a type argument (often a
                    // `!N` marker), lowered recursively then wrapped in a managed reference.
                    let inner = subst[0]
                        .as_type()
                        .expect("RustcCLRInteropByRef expects a type argument");
                    let inner = ctx.monomorphize(inner);
                    let inner = get_type(inner, ctx);
                    ctx.nref(inner)
                } else {
                    todo!("Interop type {name:?} is not yet supported!")
                }
            } else {
                let name = ctx.alloc_string(name);
                Type::ClassRef(get_adt(ty, *def, subst, name, ctx))
            }
        }
        TyKind::Array(element, length) => {
            // Get the lenght of thid array
            let length = ctx.monomorphize(*length);
            let length: usize = resolve_const_size(length).unwrap();
            // Get the element of the array
            let element = ctx.monomorphize(*element);
            let element = get_type(element, ctx);
            // Get the layout and size of this array
            let layout = ctx.layout_of(ty);
            let arr_size = layout.layout.size().bytes();
            let arr_align = layout.layout.align().abi.bytes();
            // An array > 2^32 bytes can't be a .NET fixed-size value type — but it is also
            // *uninstantiable*: a 4 GiB+ value can never be materialized (on the stack, as a struct
            // field — the enclosing struct is then also over-size and hits this same path — or on
            // the heap, where a real ~128 TB allocation OOMs exactly as it would natively). So the
            // type only ever appears at the type level: `size_of`/`align_of` read rustc's
            // `layout_of` (the true size, below), and `&[T; N]`/`Box<[T; N]>` lower to pointers
            // (size-agnostic, and array indexing is pointer arithmetic in element strides — it
            // never reads the array's declared .NET size). We therefore lower it to a placeholder
            // fixed array whose .NET `.size` attribute is capped to a single element stride: the
            // class identity is still keyed on the real `length` (so it never aliases a small
            // array), and the capped size is never read because the value is never materialized.
            // Faithful for every reachable use. (Previously `span_fatal` under the I3-totality
            // assumption that this was unreachable — the rust-lang/rust coretests suite, e.g.
            // `size_of::<[u8; isize::MAX as usize]>()`, disproves that, so it must not be fatal.
            // This is NOT the silent-`Void`-ZST miscompile the fatal replaced: that aliased the
            // array to a 0-byte slot that place ops then read/wrote; here the type is uninstantiable
            // so no place op ever touches it, and `size_of` stays exact via rustc's layout.)
            let n_arr_size = if std::convert::TryInto::<u32>::try_into(arr_size).is_err() {
                // one element stride: >= element size (so the f0 field fits) and < 2^32 for any
                // real element (an element that were itself over-size would recurse into this arm).
                (arr_size / (length as u64).max(1))
                    .max(arr_align)
                    .min(u64::from(u32::MAX - 1))
            } else {
                arr_size
            };
            let cref = fixed_array(ctx, element, length as u64, n_arr_size, arr_size, arr_align);
            Type::ClassRef(cref)
        }
        TyKind::Alias(_) => panic!("Attempted to get the .NET type of an unmorphized type"),
        TyKind::Coroutine(_defid, coroutine_args) => {
            let coroutine_args = coroutine_args.as_coroutine();

            // Extract the closure fields
            let fields: Box<[_]> = coroutine_args
                .upvar_tys()
                .iter()
                .map(|ty| get_type(ty, ctx))
                .collect();
            // Get a coroutine name.
            let name = coroutine_name(ty, ctx);
            let name = ctx.alloc_string(name);
            // Get the layout of the coroutine
            let layout = ctx.layout_of(ty);
            // Allocate a class reference to the coroutine
            let cref = ctx.alloc_class_ref(ClassRef::new(name, None, true, [].into()));
            ctx.set_rust_semantic_size(cref, layout.layout.size().bytes());
            // If there is no defition of this coroutine present, create the coroutine.
            if ctx.class_ref_to_def(cref).is_none() {
                let type_def = coroutine_typedef(&fields, ty, layout.layout, ctx, name);
                ctx.class_def(type_def).unwrap();
            }

            Type::ClassRef(cref)
        }
        // A pattern type (e.g. `pattern_type!(*const u8 is !null)`) has the exact same
        // representation as its base type: rustc's layout for `ty::Pat` clones the base
        // layout and only tightens the scalar valid-range / niche, which is applied when
        // computing *enclosing* layouts (e.g. the niche that lets `Option<NonNull<T>>` be
        // pointer-sized). The pattern type itself is laid out identically to its base, so
        // the .NET type is just the base's. (Matches how rustc_codegen_ssa looks through
        // `ty::Pat`.) This is pattern-agnostic — it holds for `NotNull`, `Range`, and `Or`.
        TyKind::Pat(base, _) => get_type(*base, ctx),
        _ => todo!("Can't yet get type {ty:?} from type cache."),
    }
}
//
pub fn fixed_array(
    asm: &mut Assembly,
    element: Type,
    length: u64,
    requested_size: u64,
    semantic_size: u64,
    align: u64,
) -> Interned<ClassRef> {
    assert_ne!(requested_size, 0);
    // Key the synthetic type by the caller-known Rust storage request, not by opportunistic local
    // ClassDef availability. Two shards must derive the same identity even if only one currently
    // carries the element's authoritative managed-sidecar definition; post-link merging will then
    // expose any contradictory physical normalization instead of silently creating two CLR types
    // for one Rust type. Element + length alone is insufficient because ordinary and over-aligned
    // arrays can share both while requiring distinct storage definitions.
    let cref = ClassRef::fixed_array_with_layout(element, length, requested_size, align, asm);
    asm.set_rust_semantic_size(cref, semantic_size);

    // If the array definition not already present, add it.
    if asm.class_ref_to_def(cref).is_none() {
        let fields = vec![(element, asm.alloc_string("f0"), Some(0))];
        let class_ref = asm.class_ref(cref).clone();
        let Ok(size) = std::convert::TryInto::<u32>::try_into(requested_size) else {
            panic!(
                "Array of {element:?} with requested CLR storage size {requested_size} >= 2^32. Unsupported.",
                element = element.mangle(asm)
            )
        };
        let arr = asm
            .class_def(
                ClassDef::new(
                    class_ref.name(),
                    true,
                    0,
                    None,
                    fields,
                    vec![],
                    Access::Public,
                    Some(NonZeroU32::new(size).unwrap()),
                    NonZeroU32::new(align.try_into().unwrap()),
                    true,
                )
                .with_fixed_array_layout(FixedArrayLayout::new(
                    element,
                    length,
                    requested_size,
                    semantic_size,
                    align,
                )),
            )
            .expect("Layout error in array!");

        // Common nodes
        let ldarg_2 = ld_arg!(2).into_idx(asm);
        let elem_tpe_idx = asm.alloc_type(element);
        let elem_addr = add!(
            ptr_cast!(ld_arg!(0), *elem_tpe_idx),
            cilly::mul!(ld_arg!(1), cilly::size_of!(elem_tpe_idx))
        )
        .into_idx(asm);
        // Defintion of the set_Item method.
        let set_item = asm.alloc_string("set_Item");
        let this_ref = asm.nref(Type::ClassRef(cref));
        let set_sig = asm.sig([this_ref, Type::Int(Int::USize), element], Type::Void);
        let arg_names = vec![
            Some(asm.alloc_string("this")),
            Some(asm.alloc_string("idx")),
            Some(asm.alloc_string("elem")),
        ];
        let set_root = asm.alloc_root(CILRoot::StInd(Box::new((
            elem_addr, ldarg_2, element, false,
        ))));
        let void_ret = asm.alloc_root(CILRoot::VoidRet);
        asm.new_method(MethodDef::new(
            Access::Public,
            arr,
            set_item,
            set_sig,
            MethodKind::Instance,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![set_root, void_ret], 0, None)],
                locals: vec![],
            },
            arg_names,
        ));
        // Implementation of the get_Item method
        let get_item = asm.alloc_string("get_Item");
        let get_sig = asm.sig([this_ref, Type::Int(Int::USize)], element);
        let arg_names = vec![
            Some(asm.alloc_string("this")),
            Some(asm.alloc_string("idx")),
        ];
        let elem_val = asm.alloc_node(CILNode::LdInd {
            addr: elem_addr,
            tpe: elem_tpe_idx,
            volatile: false,
        });
        let elem_ret = asm.alloc_root(CILRoot::Ret(elem_val));
        asm.new_method(MethodDef::new(
            Access::Public,
            arr,
            get_item,
            get_sig,
            MethodKind::Instance,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![elem_ret], 0, None)],
                locals: vec![],
            },
            arg_names,
        ));
        // Implementation of the get_Address method
        let get_address = asm.alloc_string("get_Address");
        let elem_ref_tpe = asm.nptr(element);
        let addr_sig = asm.sig([this_ref, Type::Int(Int::USize)], elem_ref_tpe);
        let arg_names = vec![
            Some(asm.alloc_string("this")),
            Some(asm.alloc_string("idx")),
        ];

        let elem_ret = asm.alloc_root(CILRoot::Ret(elem_addr));
        asm.new_method(MethodDef::new(
            Access::Public,
            arr,
            get_address,
            addr_sig,
            MethodKind::Instance,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![elem_ret], 0, None)],
                locals: vec![],
            },
            arg_names,
        ));
    }
    cref
}

/// Returns a fat pointer to an inner type.
pub fn fat_ptr_to<'tcx>(
    mut inner: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<ClassRef> {
    inner = ctx.monomorphize(inner);
    let inner_tpe = get_type(inner, ctx);
    let name = format!("FatPtr{elem}", elem = inner_tpe.mangle(ctx));
    let name = ctx.alloc_string(name);
    let cref = ctx.alloc_class_ref(ClassRef::new(name, None, true, [].into()));
    if ctx.class_ref_to_def(cref).is_none() {
        let def = ClassDef::new(
            name,
            true,
            0,
            None,
            vec![
                (
                    ctx.nptr(Type::Void),
                    ctx.alloc_string(cilly::DATA_PTR),
                    Some(0),
                ),
                (
                    Type::Int(Int::USize),
                    ctx.alloc_string(cilly::METADATA),
                    Some(8),
                ),
            ],
            vec![],
            Access::Public,
            Some(NonZeroU32::new(16).unwrap()),
            Some(NonZeroU32::new(8).unwrap()),
            true,
        );
        ctx.class_def(def).unwrap();
    }
    cref
}
fn generated_type_name(kind: &str, identity: &str) -> String {
    format!("{kind}.tid_{identity}")
}

#[cfg(test)]
mod generated_type_identity_tests {
    use super::generated_type_name;

    #[test]
    fn identical_type_identity_is_deterministic_across_shards() {
        let identity = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            generated_type_name("Coroutine", identity),
            generated_type_name("Coroutine", identity)
        );
    }

    #[test]
    fn distinct_type_identities_do_not_share_a_generated_class() {
        assert_ne!(
            generated_type_name("Closure", "0123456789abcdef0123456789abcdef"),
            generated_type_name("Closure", "fedcba9876543210fedcba9876543210")
        );
    }
}

/// Return a deterministic identity name for a closure. Presentation text, field handles, and
/// session-local DefId debug values are deliberately excluded; rustc's TypeId hash covers the
/// closure definition plus its fully instantiated captured/generic types across crate metadata.
pub fn closure_name<'tcx, 'asm>(ty: Ty<'tcx>, ctx: &mut MethodCompileCtx<'tcx, 'asm>) -> String {
    let identity = format!("{:032x}", ctx.tcx().type_id_hash(ty));
    generated_type_name("Closure", &identity)
}

/// Return the corresponding stable identity name for a coroutine/generator.
pub fn coroutine_name<'tcx, 'asm>(ty: Ty<'tcx>, ctx: &mut MethodCompileCtx<'tcx, 'asm>) -> String {
    let identity = format!("{:032x}", ctx.tcx().type_id_hash(ty));
    generated_type_name("Coroutine", &identity)
}
/// Creates a [`ClassDef`] representing a closure with certain layout and fields.
#[must_use]
pub fn closure_typedef(
    fields: &[Type],
    layout: Layout,
    ctx: &mut MethodCompileCtx<'_, '_>,
    closure_name: Interned<IString>,
) -> ClassDef {
    // Collects all field types, offsets, and names
    let field_iter = fields
        .iter()
        .enumerate()
        .map(|(idx, ty)| (format!("f_{idx}"), *ty));
    let offset_iter = FieldOffsetIterator::fields((*layout.0).clone());
    let mut fields = Vec::new();
    let mut unique_checks = HashSet::new();
    for ((name, field), offset) in (field_iter).zip(offset_iter) {
        if field == Type::Void {
            continue;
        }
        fields.push((field, ctx.alloc_string(name), Some(offset)));
        unique_checks.insert(offset);
    }
    let has_nonverlaping_layout = unique_checks.len() == fields.len();
    // Create a class definition representing this closure.
    ClassDef::new(
        closure_name,
        true,
        0,
        None,
        fields,
        vec![],
        Access::Public,
        Some(
            NonZeroU32::new(
                layout
                    .size()
                    .bytes()
                    .try_into()
                    .expect("Closure size exceeds 2^32"),
            )
            .unwrap(),
        ),
        Some(
            NonZeroU32::new(
                layout
                    .align()
                    .abi
                    .bytes()
                    .try_into()
                    .expect("Closure alignment exceeds 2^32"),
            )
            .unwrap(),
        ),
        has_nonverlaping_layout,
    )
}

/// One field participating in a Rust overlapping layout before CLR normalization.
#[derive(Clone, Copy)]
struct OverlapField<'tcx> {
    tpe: Type,
    name: Interned<IString>,
    natural_offset: u32,
    /// The source Rust type supplies the sidecar's size/alignment when relocation is required.
    /// Synthetic discriminants have no Rust field type and can never themselves contain a gcref.
    rust_ty: Option<Ty<'tcx>>,
}

/// Determines whether a Rust field lowers to a GC-tracked managed value without consulting the
/// partially-built CIL type graph.
///
/// Codegen shards discover and register `ClassDef`s in different orders. Basing this decision on
/// whether a nested definition happens to be present therefore makes physical field offsets vary
/// between shards. Walking the fully monomorphized Rust type is deterministic and sees the interop
/// marker ADT at the leaf regardless of registration order.
fn rust_ty_contains_managed_value<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
    depth: u32,
) -> bool {
    if depth > 64 {
        return true;
    }
    let ty = ctx.monomorphize(ty);
    if is_zst(ty, ctx.tcx()) {
        return false;
    }
    match ty.kind() {
        TyKind::Adt(def, args) => {
            let item = ctx.tcx().item_name(def.did());
            if matches!(
                item.as_str(),
                INTEROP_CLASS_TPE_NAME
                    | INTEROP_GENERIC_TPE_NAME
                    | INTEROP_ARR_TPE_NAME
                    | INTEROP_STRUCT_TPE_NAME
                    | INTEROP_GENERIC_STRUCT_TPE_NAME
                    | INTEROP_TYPE_GENERIC_TPE_NAME
                    | INTEROP_METHOD_GENERIC_TPE_NAME
                    | INTEROP_BYREF_TPE_NAME
            ) {
                return true;
            }
            def.all_fields().any(|field| {
                let field_ty = ctx.monomorphize(field.ty(ctx.tcx(), args).skip_normalization());
                rust_ty_contains_managed_value(field_ty, ctx, depth + 1)
            })
        }
        TyKind::Closure(_, args) => args
            .as_closure()
            .upvar_tys()
            .iter()
            .any(|field| rust_ty_contains_managed_value(field, ctx, depth + 1)),
        TyKind::Coroutine(def_id, args) => {
            let args = args.as_coroutine();
            args.upvar_tys()
                .iter()
                .any(|field| rust_ty_contains_managed_value(field, ctx, depth + 1))
                || args.state_tys(*def_id, ctx.tcx()).any(|variant| {
                    variant
                        .into_iter()
                        .any(|field| rust_ty_contains_managed_value(field, ctx, depth + 1))
                })
        }
        TyKind::Tuple(fields) => fields
            .iter()
            .any(|field| rust_ty_contains_managed_value(field, ctx, depth + 1)),
        TyKind::Array(field, _) => rust_ty_contains_managed_value(*field, ctx, depth + 1),
        TyKind::Pat(base, _) => rust_ty_contains_managed_value(*base, ctx, depth + 1),
        // Rust references/raw pointers and fat pointers are native pointer values. Their pointee may
        // describe a managed marker, but the pointer itself is not a GC-tracked object reference.
        TyKind::Ref(..)
        | TyKind::RawPtr(..)
        | TyKind::FnPtr(..)
        | TyKind::Dynamic(..)
        | TyKind::Bool
        | TyKind::Char
        | TyKind::Int(..)
        | TyKind::Uint(..)
        | TyKind::Float(..)
        | TyKind::Never
        | TyKind::Foreign(..)
        | TyKind::FnDef(..)
        | TyKind::Bound(..) => false,
        TyKind::Slice(field) => rust_ty_contains_managed_value(*field, ctx, depth + 1),
        TyKind::Alias(..) => true,
        _ => false,
    }
}

/// Converts Rust's union-style field placement into a CoreCLR-GC-safe physical layout.
///
/// Rust may assign the same byte offset to differently typed fields in mutually-exclusive enum or
/// coroutine states. CoreCLR permits that explicit layout only while the offset's GC interpretation
/// remains unambiguous. For each conflicting offset group, this routine keeps the ordinary Rust bytes
/// at their natural offset and hoists every GC-bearing field into an aligned sidecar after the Rust
/// extent. Fields with the same lowered type and original offset share one sidecar slot, preserving
/// the useful overlap between mutually-exclusive variants without confusing the GC map.
///
/// The returned size is the CLR *storage* size. Rust's semantic size is registered separately on the
/// assembly and `MethodCompileCtx::size_of` lowers it to a constant, so sidecars cannot change Rust
/// `size_of` or pointer stride.
fn normalize_overlapping_layout<'tcx>(
    fields: Vec<OverlapField<'tcx>>,
    rust_size: u64,
    rust_align: u64,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> (Vec<(Type, Interned<IString>, Option<u32>)>, u64, u64) {
    let mut by_offset: HashMap<u32, Vec<(Type, bool)>> = HashMap::new();
    for field in &fields {
        let managed = field
            .rust_ty
            .is_some_and(|ty| rust_ty_contains_managed_value(ty, ctx, 0));
        by_offset
            .entry(field.natural_offset)
            .or_default()
            .push((field.tpe, managed));
    }
    let unsafe_offsets: HashSet<u32> = by_offset
        .into_iter()
        .filter_map(|(offset, fields)| {
            let contains_gcref = fields.iter().any(|(_, managed)| *managed);
            let disagree = fields
                .first()
                .is_some_and(|(first, _)| fields.iter().any(|(tpe, _)| tpe != first));
            (contains_gcref && disagree).then_some(offset)
        })
        .collect();

    let mut storage_end = rust_size;
    let mut storage_align = rust_align.max(1);
    let mut sidecars: HashMap<(u32, Type), u32> = HashMap::new();
    let mut normalized = Vec::with_capacity(fields.len());
    for field in fields {
        let must_hoist = unsafe_offsets.contains(&field.natural_offset)
            && field
                .rust_ty
                .is_some_and(|ty| rust_ty_contains_managed_value(ty, ctx, 0));
        let offset = if must_hoist {
            if let Some(existing) = sidecars.get(&(field.natural_offset, field.tpe)) {
                *existing
            } else {
                let rust_ty = field
                    .rust_ty
                    .expect("a GC-bearing overlapping field must have a source Rust type");
                let layout = ctx.layout_of(rust_ty).layout;
                let mut field_size = layout.size().bytes();
                let mut field_align = layout.align().abi.bytes().max(1);
                if let Type::ClassRef(class) = field.tpe {
                    if let Some(def) = ctx
                        .class_ref_to_def(class)
                        .and_then(|idx| ctx.class_defs().get(&idx))
                    {
                        field_size = field_size
                            .max(def.explict_size().map_or(0, |size| u64::from(size.get())));
                        field_align =
                            field_align.max(def.align().map_or(1, |align| u64::from(align.get())));
                    }
                }
                let start = storage_end.next_multiple_of(field_align);
                storage_end = start
                    .checked_add(field_size)
                    .expect("managed sidecar layout size overflow");
                storage_align = storage_align.max(field_align);
                let start = u32::try_from(start)
                    .expect("managed sidecar field offset exceeds the CLR 32-bit limit");
                sidecars.insert((field.natural_offset, field.tpe), start);
                start
            }
        } else {
            field.natural_offset
        };
        normalized.push((field.tpe, field.name, Some(offset)));
    }

    let storage_size = storage_end.next_multiple_of(storage_align);
    let mut normalized_by_offset: HashMap<u32, Vec<(Type, Interned<IString>)>> = HashMap::new();
    for (tpe, name, offset) in &normalized {
        normalized_by_offset
            .entry(offset.expect("normalized fields always have offsets"))
            .or_default()
            .push((*tpe, *name));
    }
    for (offset, group) in normalized_by_offset {
        if group.iter().any(|(tpe, _)| tpe.contains_gcref(&*ctx))
            && group
                .first()
                .is_some_and(|(first, _)| group.iter().any(|(tpe, _)| tpe != first))
        {
            panic!(
                "overlapping-layout normalization left an unsafe GC slot at offset {offset}: {:?}",
                group
                    .iter()
                    .map(|(tpe, name)| (
                        tpe.mangle(&*ctx),
                        ctx[*name].to_string(),
                        tpe.contains_gcref(&*ctx)
                    ))
                    .collect::<Vec<_>>()
            );
        }
    }
    (normalized, storage_size, storage_align)
}

/// Creates a [`ClassDef`] representing a coroutine (the state machine `async fn`/`gen` blocks
/// lower to). A coroutine is enum-like: it has upvar fields (the captured environment, shared
/// across all states — laid out like a closure's `f_N`), an `ENUM_TAG` discriminant, and a set
/// of per-variant *saved-local* fields (the locals live across each suspend point), one group
/// per coroutine variant. The variants overlap in memory (only one is live at a time), exactly
/// like enum variants.
///
/// The saved-local field names MUST match the scheme used by
/// [`crate::r#type::adt::coroutine_field_descriptor`] — `"{variant_name}_{field_idx}"` with
/// `variant_name` from [`crate::r#type::adt::coroutine_variant_name`] — so place projections resolve.
#[must_use]
fn coroutine_typedef<'tcx>(
    upvars: &[Type],
    ty: Ty<'tcx>,
    layout: Layout,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    coroutine_name: Interned<IString>,
) -> ClassDef {
    let (def_id, coroutine_args) = match ty.kind() {
        TyKind::Coroutine(def_id, args) => (*def_id, args.as_coroutine()),
        _ => unreachable!("coroutine_typedef on non-coroutine {ty:?}"),
    };
    let mut fields: Vec<OverlapField<'tcx>> = Vec::new();
    // Upvar fields (the captured environment), laid out like a closure's `f_N`.
    {
        let offset_iter = FieldOffsetIterator::fields((*layout.0).clone());
        for (((idx, field), rust_ty), offset) in upvars
            .iter()
            .enumerate()
            .zip(coroutine_args.upvar_tys().iter())
            .zip(offset_iter)
        {
            if *field == Type::Void {
                continue;
            }
            let name = ctx.alloc_string(format!("f_{idx}"));
            fields.push(OverlapField {
                tpe: *field,
                name,
                natural_offset: offset,
                rust_ty: Some(ctx.monomorphize(rust_ty)),
            });
        }
    }
    // The discriminant (which coroutine state we are in).
    let mut tag = Vec::new();
    handle_tag(&layout, ctx, ty, &mut tag);
    fields.extend(tag.into_iter().map(|(tpe, name, offset)| OverlapField {
        tpe,
        name,
        natural_offset: offset.expect("coroutine discriminant must have an explicit offset"),
        rust_ty: None,
    }));
    // Per-variant saved-local fields. `state_tys` yields one inner iterator per coroutine
    // variant (outer index = `VariantIdx`); the reserved Unresumed/Returned/Panicked variants
    // have no saved locals, so their inner iterators are empty and are naturally skipped.
    //
    // rustc's own coroutine layout is free to reuse the same byte offset across DIFFERENT
    // variants with DIFFERENT saved-local types (only one variant is ever live at a time, so
    // native code has no problem with the reuse). CoreCLR's class loader is pickier: reusing a
    // byte offset for a gcref-shaped field (e.g. `mycorrhiza::task::TaskFuture<T>`, which nests a
    // real managed `Task<T>` reference) in one variant and something else (a raw primitive, or a
    // differently-shaped field) in another produces "object field at offset N is incorrectly
    // aligned or overlapped by a non-object field" at type-LOAD time — `cilly`'s own
    // `ClassDef::layout_check` now catches this pattern at compile time (see its doc comment for
    // exactly what it allows: identical-typed reuse is fine, this is not). Rather than merely
    // rejecting the compile, give any field that would create such a collision its own private,
    // non-overlapping slot appended after the coroutine's natural extent — cheap (this is
    // strictly additional memory that only exists on the CoreCLR side, never observed by Rust
    // code, which never `size_of`s a coroutine) and keeps the fast, fully-overlapping layout for
    // every other (safe) field.
    let variant_state_tys: Vec<Vec<Ty<'tcx>>> = coroutine_args
        .state_tys(def_id, ctx.tcx())
        .map(|variant| variant.collect())
        .collect();
    for (vidx, variant_field_tys) in variant_state_tys.into_iter().enumerate() {
        let var = VariantIdx::from_u32(vidx as u32);
        let offset_iter = crate::r#type::adt::FieldOffsetIterator::fields(
            crate::r#type::adt::get_variant_at_index(var, (*layout.0).clone()),
        );
        for (field_idx, (sty, offset)) in variant_field_tys.into_iter().zip(offset_iter).enumerate()
        {
            let mono_sty = ctx.monomorphize(sty);
            let fty = get_type(mono_sty, ctx);
            // Parity with closure/enum field handling: ZST-typed fields have no .NET slot.
            if fty == Type::Void {
                continue;
            }
            let fname = ctx.alloc_string(format!(
                "{vname}_{field_idx}",
                vname = crate::r#type::adt::coroutine_variant_name(var)
            ));
            fields.push(OverlapField {
                tpe: fty,
                name: fname,
                natural_offset: offset,
                rust_ty: Some(mono_sty),
            });
        }
    }
    let (fields, total_size, total_align) = normalize_overlapping_layout(
        fields,
        layout.size().bytes(),
        layout.align().abi.bytes(),
        ctx,
    );
    // Coroutine variants overlap in memory (like enum variants), so the layout is NOT
    // non-overlapping — `closure_typedef`'s upvar-only uniqueness check would wrongly report
    // `true` once overlapping variant fields are present, so force `false` here.
    ClassDef::new(
        coroutine_name,
        true,
        0,
        None,
        fields,
        vec![],
        Access::Public,
        Some(NonZeroU32::new(total_size.try_into().expect("Coroutine size exceeds 2^32")).unwrap()),
        Some(
            NonZeroU32::new(
                total_align
                    .try_into()
                    .expect("Coroutine alignment exceeds 2^32"),
            )
            .unwrap(),
        ),
        false,
    )
}
/// Turns an adt struct defintion into a [`ClassDef`]
fn struct_<'tcx>(
    name: Interned<IString>,
    adt: AdtDef<'tcx>,
    adt_ty: Ty<'tcx>,
    subst: &'tcx List<rustc_middle::ty::GenericArg<'tcx>>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> ClassDef {
    // Double-check is not a ZST.

    // Get the layout of this struct
    let layout = ctx.layout_of(adt_ty);

    // Go trough fields, collectiing them and their offsets
    let mut fields = Vec::new();
    let explicit_offset_iter = FieldOffsetIterator::fields((*layout.layout.0).clone());
    let mut unique_checks = HashSet::new();
    for (field, offset) in adt
        .variant(rustc_abi::VariantIdx::from_u32(0))
        .fields
        .iter()
        .zip(explicit_offset_iter)
    {
        let name = escape_field_name(&field.name.to_string());
        let field_type = get_type(
            ctx.monomorphize(field.ty(ctx.tcx(), subst).skip_normalization()),
            ctx,
        );
        if field_type == Type::Void {
            continue;
        }
        unique_checks.insert(offset);
        fields.push((field_type, ctx.alloc_string(name), Some(offset)));
    }
    let size = layout.layout.size().bytes();
    let size = match std::convert::TryInto::<u32>::try_into(size) {
        Ok(size) => size,
        // A struct > 2^32 bytes — same reasoning as the over-size array arm above: such a value is
        // *uninstantiable* (you can never materialize a 4 GiB+ value type), so it only ever appears
        // at the type level, where `size_of`/`align_of` read rustc's `layout_of` (the true size).
        // We lower it to a placeholder whose .NET `.size` is capped just past the last field rather
        // than `span_fatal`. The field that makes the struct over-size is itself an over-size type
        // and so was already placeholdered (its .NET extent is small), so every field's slot fits
        // under the capped size. (Was `span_fatal` on the assumption this is unreachable — the
        // rust-lang/rust coretests suite disproves it: `ptr::align_offset`'s
        // `HugeSize([u8; isize::MAX - 1])` is exactly this. This is NOT the old silent `u32::MAX`
        // clamp that mis-laid-out an *instantiated* type: the type here is never materialized, so
        // the capped .NET size is never read — `size_of` stays exact via rustc's layout.)
        Err(_) => {
            let max_field_off = fields.iter().filter_map(|(_, _, o)| *o).max().unwrap_or(0) as u64;
            let align_b = layout.layout.align().abi.bytes().max(1);
            (max_field_off + align_b).min(u64::from(u32::MAX - 1)) as u32
        }
    };
    let has_nonverlaping_layout = unique_checks.len() == fields.len();
    ClassDef::new(
        name,
        true,
        0,
        None,
        fields,
        vec![],
        Access::Public,
        NonZeroU32::new(size),
        Some(
            NonZeroU32::new(
                layout
                    .layout
                    .align()
                    .abi
                    .bytes()
                    .try_into()
                    .expect("Struct alignement exceeds 2^32"),
            )
            .unwrap(),
        ),
        has_nonverlaping_layout,
    )
}

/// Synthesize a public all-fields constructor and per-field getters for an exported value-type struct.
///
/// The struct's fields are emitted without a CIL visibility modifier (so they are private to the
/// assembly); these additive public methods are what let a .NET consumer construct the value
/// (`new Point(x, y)`) and read its fields (`p.get_x()`). The struct's layout/codegen is unchanged —
/// only methods are added — so this is a no-op for the `::stable` gate (whose programs are executables
/// and therefore never `stable_adt_name`-eligible). Follows the `fixed_array` method-synthesis pattern.
fn add_record_accessors(
    cref: Interned<ClassRef>,
    fields: &[(Type, Interned<IString>, Option<u32>)],
    ctx: &mut MethodCompileCtx<'_, '_>,
) {
    // The owning class handle for the synthesized methods (already registered by `get_adt`).
    let class_idx = ClassDefIdx(cref);
    // `this` is a managed reference to the value type (as for any value-type instance method).
    let this_ref = ctx.nref(Type::ClassRef(cref));

    // ---- all-fields constructor: `.ctor(this, f0, f1, ...)`, storing each arg into its field ----
    let mut ctor_inputs = Vec::with_capacity(fields.len() + 1);
    ctor_inputs.push(this_ref);
    ctor_inputs.extend(fields.iter().map(|(tpe, _, _)| *tpe));
    let ctor_sig = ctx.sig(ctor_inputs, Type::Void);
    let mut ctor_roots = Vec::with_capacity(fields.len() + 1);
    let mut ctor_arg_names = Vec::with_capacity(fields.len() + 1);
    ctor_arg_names.push(Some(ctx.alloc_string("this")));
    for (i, (tpe, fname, _)) in fields.iter().enumerate() {
        let field = ctx.alloc_field(FieldDesc::new(cref, *fname, *tpe));
        let this_addr = ctx.alloc_node(CILNode::LdArg(0));
        let val = ctx.alloc_node(CILNode::LdArg((i + 1) as u32));
        ctor_roots.push(ctx.alloc_root(CILRoot::SetField(Box::new((field, this_addr, val)))));
        ctor_arg_names.push(Some(*fname));
    }
    ctor_roots.push(ctx.alloc_root(CILRoot::VoidRet));
    let ctor_name = ctx.alloc_string(".ctor");
    // `Access::Extern` (not `Public`): these accessors exist solely for .NET consumers, so nothing in
    // the Rust crate calls them. `Extern` both emits them as `public` CIL *and* marks them dead-code
    // roots (like the `#[unsafe(no_mangle)]` exports), so the optimizer's `eliminate_dead_fns` keeps them.
    ctx.new_method(MethodDef::new(
        Access::Extern,
        class_idx,
        ctor_name,
        ctor_sig,
        MethodKind::Constructor,
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(ctor_roots, 0, None)],
            locals: vec![],
        },
        ctor_arg_names,
    ));

    // ---- per-field getter: `get_<field>(this) -> field_type` ----
    for (tpe, fname, _) in fields {
        let getter_name = {
            let raw = ctx[*fname].to_string();
            ctx.alloc_string(format!("get_{raw}"))
        };
        let getter_sig = ctx.sig([this_ref], *tpe);
        let field = ctx.alloc_field(FieldDesc::new(cref, *fname, *tpe));
        let this_addr = ctx.alloc_node(CILNode::LdArg(0));
        let ld = ctx.alloc_node(CILNode::LdField {
            addr: this_addr,
            field,
        });
        let ret = ctx.alloc_root(CILRoot::Ret(ld));
        let this_name = ctx.alloc_string("this");
        ctx.new_method(MethodDef::new(
            Access::Extern,
            class_idx,
            getter_name,
            getter_sig,
            MethodKind::Instance,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![Some(this_name)],
        ));
    }
}

/// `DUMP_LAYOUT=<substr>`: append the backend's computed enum layout — tag encoding, tag type + byte
/// offset, per-variant field offsets, and (for niche enums) rustc's `untagged_variant` /
/// `niche_variants` / `niche_start` — to a file (`DUMP_LAYOUT_OUT`, default `/tmp/dump_layout.txt`), for
/// any enum whose type name contains `<substr>`. This is the introspection that root-caused the regex
/// `get_discr` 128-bit-niche miscompile (`niche_start=2^128-2` revealed the index-vs-value compare bug):
/// it lets you diff what the backend laid out / decodes against rustc's intent. The whole discriminant
/// class (Direct vs Niche tags, multi-byte/128-bit tags, shifted/nested niches, tag offsets) is exactly
/// the kind of bug that passes the type-checker and fails silently — keep this around. Off unless set.
/// Pairs with runtime `TRACE_VAL` / `feasibility/rcc-debug` and `cargo_tests/probe_enum_discr`.
fn dump_enum_layout<'tcx>(adt_ty: Ty<'tcx>, ctx: &mut MethodCompileCtx<'tcx, '_>) {
    let Ok(filter) = std::env::var("DUMP_LAYOUT") else {
        return;
    };
    if filter.is_empty() || !format!("{adt_ty:?}").contains(filter.as_str()) {
        return;
    }
    use std::io::Write as _;
    let layout = ctx.layout_of(adt_ty);
    let (tag_type, tag_offset) = crate::r#type::adt::enum_tag_info(layout.layout, ctx);
    let mut out = format!(
        "LAYOUT {adt_ty:?}  size={} align={}\n",
        layout.layout.size().bytes(),
        layout.layout.align().abi.bytes()
    );
    match &layout.layout.variants {
        rustc_abi::Variants::Multiple {
            tag_encoding,
            tag_field,
            ..
        } => {
            out += &match tag_encoding {
                rustc_abi::TagEncoding::Direct => format!(
                    "  encoding=Direct tag_type={tag_type:?} tag_offset={tag_offset} tag_field={tag_field:?}\n"
                ),
                rustc_abi::TagEncoding::Niche {
                    untagged_variant,
                    niche_variants,
                    niche_start,
                } => format!(
                    "  encoding=Niche tag_type={tag_type:?} tag_offset={tag_offset} tag_field={tag_field:?} untagged={untagged_variant:?} niche_variants={niche_variants:?} niche_start={niche_start}\n"
                ),
            };
            if let Some(adt) = adt_ty.ty_adt_def() {
                for (vidx, _v) in adt.variants().iter_enumerated() {
                    let voff: Vec<u32> =
                        crate::r#type::adt::variant_offsets(adt, layout.layout, vidx).collect();
                    out += &format!("  variant {vidx:?} field_offsets={voff:?}\n");
                }
            }
        }
        other => {
            out += &format!("  variants={other:?} tag_type={tag_type:?} tag_offset={tag_offset}\n");
        }
    }
    let path =
        std::env::var("DUMP_LAYOUT_OUT").unwrap_or_else(|_| "/tmp/dump_layout.txt".to_string());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(out.as_bytes());
    }
}

fn handle_tag<'tcx>(
    layout: &Layout,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    adt_ty: Ty<'tcx>,
    fields: &mut Vec<(Type, Interned<IString>, Option<u32>)>,
) {
    dump_enum_layout(adt_ty, ctx);
    match &layout.variants {
        rustc_abi::Variants::Single { index: _ } => {
            let (tag_type, offset) = crate::r#type::adt::enum_tag_info(*layout, ctx);

            if tag_type != Type::Void {
                fields.push((tag_type, ctx.alloc_string(cilly::ENUM_TAG), Some(offset)));
            }
        }
        rustc_abi::Variants::Empty => (),
        rustc_abi::Variants::Multiple {
            tag: _,
            tag_encoding,
            tag_field: _,
            variants: _,
        } => {
            let layout = ctx.layout_of(adt_ty);

            match tag_encoding {
                rustc_abi::TagEncoding::Direct => {
                    let (tag_type, offset) = crate::r#type::adt::enum_tag_info(layout.layout, ctx);

                    if tag_type != Type::Void {
                        fields.push((tag_type, ctx.alloc_string(cilly::ENUM_TAG), Some(offset)));
                    }
                }
                rustc_abi::TagEncoding::Niche {
                    untagged_variant: _,
                    niche_variants: _,
                    ..
                } => {
                    let (tag_type, offset) = crate::r#type::adt::enum_tag_info(layout.layout, ctx);
                    let offsets = FieldOffsetIterator::fields((*layout.layout.0).clone());

                    assert!(offsets.count() > 0, "layout.fields:{:?}", layout.fields);
                    if tag_type != Type::Void {
                        fields.push((tag_type, ctx.alloc_string(cilly::ENUM_TAG), Some(offset)));
                    }
                }
            }
        }
    }
}
/// Turns an adt enum defintion into a [`ClassDef`]
fn enum_<'tcx>(
    enum_name: Interned<IString>,
    adt: AdtDef<'tcx>,
    adt_ty: Ty<'tcx>,
    subst: &'tcx List<rustc_middle::ty::GenericArg<'tcx>>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> ClassDef {
    let layout = ctx.layout_of(adt_ty);
    let mut fields: Vec<OverlapField<'tcx>> = vec![];
    // Handle the enum tag.
    let mut tag = Vec::new();
    handle_tag(&layout.layout, ctx, adt_ty, &mut tag);
    fields.extend(tag.into_iter().map(|(tpe, name, offset)| OverlapField {
        tpe,
        name,
        natural_offset: offset.expect("enum discriminant must have an explicit offset"),
        rust_ty: None,
    }));
    // Handle enum variants
    for (vidx, variant) in adt.variants().iter_enumerated() {
        let variant_name = variant.name.to_string();
        let field_offset_iter = crate::r#type::adt::variant_offsets(adt, layout.layout, vidx);

        for (field, offset) in variant.fields.iter().zip(field_offset_iter) {
            let name = format!(
                "{variant_name}_{fname}",
                fname = escape_field_name(&field.name.to_string())
            );
            let rust_ty = ctx.monomorphize(field.ty(ctx.tcx(), subst).skip_normalization());
            let field_ty = get_type(rust_ty, ctx);
            if field_ty == Type::Void {
                continue;
            }

            fields.push(OverlapField {
                tpe: field_ty,
                name: ctx.alloc_string(name),
                natural_offset: offset,
                rust_ty: Some(rust_ty),
            });
        }
    }
    let (fields, storage_size, storage_align) = normalize_overlapping_layout(
        fields,
        layout.layout.size().bytes(),
        layout.layout.align().abi.bytes(),
        ctx,
    );
    ClassDef::new(
        enum_name,
        true,
        0,
        None,
        fields,
        vec![],
        Access::Public,
        Some(NonZeroU32::new(storage_size.try_into().unwrap()).unwrap()),
        Some(
            NonZeroU32::new(
                storage_align
                    .try_into()
                    .expect("Enum alignement exceeds 2^32"),
            )
            .unwrap(),
        ),
        false,
    )
}
/// Turns an adt union defintion into a [`ClassDef`]
fn union_<'tcx>(
    name: Interned<IString>,
    adt: AdtDef<'tcx>,
    adt_ty: Ty<'tcx>,
    subst: &'tcx List<rustc_middle::ty::GenericArg<'tcx>>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> ClassDef {
    // Get union layout
    let layout = ctx.layout_of(adt_ty);
    let mut fields: Vec<OverlapField<'tcx>> = Vec::new();
    // Get union fields
    for (field, offset) in adt
        .all_fields()
        .zip(FieldOffsetIterator::fields((*layout.layout.0).clone()))
    {
        let field_name = escape_field_name(&field.name.to_string());
        let field_ty = ctx.monomorphize(field.ty(ctx.tcx(), subst).skip_normalization());
        let field_type = get_type(field_ty, ctx);
        if field_type == Type::Void {
            continue;
        }
        fields.push(OverlapField {
            tpe: field_type,
            name: ctx.alloc_string(field_name),
            natural_offset: offset,
            rust_ty: Some(field_ty),
        });
    }
    let (fields, storage_size, storage_align) = normalize_overlapping_layout(
        fields,
        layout.layout.size().bytes(),
        layout.layout.align().abi.bytes(),
        ctx,
    );
    // Create a union ClassDef
    ClassDef::new(
        name,
        true,
        0,
        None,
        fields,
        vec![],
        Access::Public,
        Some(NonZeroU32::new(storage_size.try_into().unwrap()).unwrap()),
        Some(
            NonZeroU32::new(
                storage_align
                    .try_into()
                    .expect("Union alignement exceeds 2^32"),
            )
            .unwrap(),
        ),
        false,
    )
}
#[must_use]
pub fn escape_field_name(name: &str) -> String {
    match name.chars().next() {
        None => "fld".into(),
        Some(first) => {
            if !(first.is_alphabetic() || first == '_')
        || name == "value"
        || name == "flags"
        || name == "alignment"
        || name == "init"
        || name == "string"
        || name == "nint"
        || name == "nuint"
        || name == "out"
        || name == "rem"
        || name == "add"
        || name == "div"
        || name == "error"
        || name == "opt"
        || name == "private"
        || name == "public"
        || name == "object"
        || name == "class"
        //FIXME: this is a sign of a bug. ALL fields not starting with a letter should have been caught by the statement above.
        || name == "0"
            {
                format!("m_{name}")
            } else {
                name.into()
            }
        }
    }
}
#[must_use]
pub fn tuple_typedef(
    elements: &[Type],
    layout: Layout,
    ctx: &mut MethodCompileCtx<'_, '_>,
    name: Interned<IString>,
) -> ClassDefIdx {
    let semantic_size = layout.size().bytes();
    let field_iter = elements
        .iter()
        .enumerate()
        .map(|(idx, ele)| (format!("Item{}", idx + 1), *ele));
    let explicit_offset_iter = FieldOffsetIterator::fields((*layout.0).clone());

    let mut fields = Vec::new();
    for ((name, field), offset) in (field_iter).zip(explicit_offset_iter) {
        if field == Type::Void {
            continue;
        }
        fields.push((field, ctx.alloc_string(name), Some(offset)));
    }
    let class = match ctx.class_def(ClassDef::new(
        name,
        true,
        0,
        None,
        fields,
        vec![],
        Access::Public,
        Some(
            NonZero::new(
                layout
                    .size()
                    .bytes()
                    .try_into()
                    .expect("Tuple size >= 2^32. Unsuported"),
            )
            .expect("Zero-sized tuple!"),
        ),
        Some(
            NonZeroU32::new(
                layout
                    .align()
                    .abi
                    .bytes()
                    .try_into()
                    .expect("Tuple alignement exceeds 2^32"),
            )
            .unwrap(),
        ),
        true,
    )) {
        Ok(cdef) => cdef,
        Err(error) => {
            ctx.tcx().dcx().span_err(
                ctx.span(),
                format!("Tuple type with invalid layout. error:{error:?}!"),
            );
            todo!();
        }
    };
    ctx.set_rust_semantic_size(class.0, semantic_size);
    class
}
