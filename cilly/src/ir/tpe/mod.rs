use std::num::NonZeroU8;

use serde::{Deserialize, Serialize};
use simd::SIMDVector;

use super::{bimap::Interned, Assembly, ClassRef, Float, FnSig, Int};

pub mod float;
pub mod int;
pub mod simd;

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Type {
    Ptr(Interned<Type>),
    Ref(Interned<Type>),
    Int(Int),
    ClassRef(Interned<ClassRef>),
    Float(Float),
    PlatformString,
    PlatformChar,
    PlatformGeneric(u32, GenericKind),
    PlatformObject,
    Bool,
    Void,
    PlatformArray {
        elem: Interned<Type>,
        dims: NonZeroU8,
    },
    FnPtr(Interned<FnSig>),
    SIMDVector(SIMDVector),
}
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum GenericKind {
    MethodGeneric,
    CallGeneric,
    TypeGeneric,
}
impl Type {
    /// Checks if this type is a GC reference. This function may raise false positive.
    pub fn is_gcref(&self, asm: &Assembly) -> bool {
        match self {
            Type::ClassRef(c) => !asm[*c].is_valuetype(),
            // Conservatively assume all C# generic. *could* be GC refs.
            Type::PlatformGeneric(_, _) => true,
            Type::PlatformArray { .. } | Type::PlatformObject | Type::PlatformString => true,
            Type::Int(_)
            | Type::Float(_)
            | Type::Bool
            | Type::PlatformChar
            | Type::Void
            | Type::Ptr(_)
            | Type::Ref(_)
            | Type::FnPtr(_)
            | Type::SIMDVector(_) => false,
        }
    }
    /// Like [`Type::is_gcref`], but recurses into value-type `ClassRef` fields to catch a
    /// managed reference *nested* inside an outer struct/newtype — e.g.
    /// `mycorrhiza::task::TaskFuture<T>` is itself a plain (non-overlapping) struct wrapping a
    /// raw `Task<T>` object handle (`RustcCLRInteropManagedGeneric`, a genuine gcref) in its
    /// `task` field. `is_gcref` only looks at the outer type's own valuetype-ness, so it reports
    /// `false` for `TaskFuture<T>` even though it transitively carries a real GC reference —
    /// [`super::class::ClassDef::layout_check`] needs this deeper check to reason about whether a
    /// field placed in a coroutine's overlapping variant storage is safe. Bounded recursion depth
    /// guards against a malformed/cyclic type graph; an unresolved valuetype `ClassRef` (no
    /// `ClassDef` registered — e.g. `System.Runtime.InteropServices.GCHandle`, an external BCL
    /// struct cilly never defines fields for) is treated as gcref-free, same as `is_gcref`.
    pub fn contains_gcref(&self, asm: &Assembly) -> bool {
        self.contains_gcref_impl(asm, 0)
    }
    fn contains_gcref_impl(&self, asm: &Assembly, depth: u32) -> bool {
        if depth > 64 {
            // Pathological nesting — conservatively assume it could hide a gcref rather than
            // risk unbounded recursion on a cyclic/malformed type graph.
            return true;
        }
        match self {
            Type::ClassRef(c) => {
                let cref = &asm[*c];
                if !cref.is_valuetype() {
                    return true;
                }
                match asm
                    .class_ref_to_def(*c)
                    .and_then(|idx| asm.class_defs().get(&idx))
                {
                    Some(def) => def
                        .fields()
                        .iter()
                        .any(|(t, _, _)| t.contains_gcref_impl(asm, depth + 1)),
                    None => false,
                }
            }
            _ => self.is_gcref(asm),
        }
    }
    pub fn iter_class_refs<'a, 'asm: 'a>(
        &'a self,
        asm: &'asm Assembly,
    ) -> impl Iterator<Item = Interned<ClassRef>> + 'a {
        let tmp: Box<dyn Iterator<Item = Interned<ClassRef>>> = match self {
            Type::PlatformArray { elem: inner, .. } | Type::Ptr(inner) | Type::Ref(inner) => {
                asm[*inner].iter_class_refs::<'a, 'asm>(asm)
            }
            Type::Int(_)
            | Type::Float(_)
            | Type::PlatformString
            | Type::PlatformChar
            | Type::PlatformGeneric(_, _)
            | Type::PlatformObject
            | Type::Bool
            | Type::Void
            | Type::SIMDVector(_) => Box::new(std::iter::empty()),
            Type::FnPtr(sig) => Box::new(
                asm[*sig]
                    .iter_types()
                    .flat_map(|tpe| tpe.iter_class_refs(asm).collect::<Box<_>>()),
            ),
            Type::ClassRef(cref) => Box::new(std::iter::once(*cref)),
        };
        tmp
    }
    #[must_use]
    pub fn deref<'a, 'b: 'a>(&'a self, asm: &'b Assembly) -> &'a Self {
        self.try_deref(asm).unwrap()
    }
    #[must_use]
    pub fn try_deref<'a, 'b: 'a>(&'a self, asm: &'b Assembly) -> Option<&'a Self> {
        match self {
            Type::Ptr(inner) | Type::Ref(inner) => Some(&asm[*inner]),
            _ => None,
        }
    }

    /// Returns a mangled ASCI representation of this type.
    /// ```
    /// # use cilly::*;
    /// # use cilly::Int;
    /// # let asm = cilly::Assembly::default();
    /// assert_eq!(Type::PlatformString.mangle(&asm),"st");
    /// assert_eq!(Type::Int(Int::I128).mangle(&asm),"i128");
    /// ```
    #[must_use]
    pub fn mangle(&self, asm: &Assembly) -> String {
        match self {
            Type::SIMDVector(val) => val.name(),
            Type::Ptr(inner) => format!("p{}", asm[*inner].mangle(asm)),
            Type::Ref(inner) => format!("r{}", asm[*inner].mangle(asm)),
            Type::Int(int) => int.name().to_owned(),
            Type::ClassRef(cref) => {
                let cref = asm.class_ref(*cref);
                let asm_name = match cref.asm() {
                    Some(asm_name) => format!(
                        "{len}{asm_name}",
                        len = asm[asm_name].len(),
                        asm_name = &asm[asm_name]
                    ),
                    None => "n".into(),
                };
                let name = &asm[cref.name()];
                format!("{asm_name}{len}{name}", len = name.len())
            }
            Type::Float(float) => float.name().to_owned(),
            Type::PlatformString => "st".into(),
            Type::PlatformChar => "c".into(),
            // A generic parameter `!N` (class) / `!!N` (method). Mangled so methodrefs whose
            // signatures carry generic-definition-shape types (WF-9 generic interop bridge) get a
            // stable, collision-free name; the kind tag keeps `!0` and `!!0` distinct.
            Type::PlatformGeneric(arg, kind) => {
                let kind = match kind {
                    GenericKind::TypeGeneric => "t",
                    GenericKind::MethodGeneric => "m",
                    GenericKind::CallGeneric => "c",
                };
                format!("g{kind}{arg}")
            }
            Type::PlatformObject => "o".into(),
            Type::Bool => "b".into(),
            Type::Void => "v".into(),
            Type::PlatformArray { elem, dims } => format!(
                "a{dims}{elem}",
                elem = asm[*elem].mangle(asm),
                dims = dims.get()
            ),
            Type::FnPtr(sig) => {
                let sig = &asm[*sig];
                let argc = sig.inputs().len();
                let output = sig.output().mangle(asm);
                let inputs = sig
                    .inputs()
                    .iter()
                    .map(|input| input.mangle(asm))
                    .collect::<String>();
                format!("{argc}{inputs}{output}")
            }
        }
    }
    #[must_use]
    /// If this type is a class reference, returns that class reference.
    /// ```
    /// # use cilly::*;
    /// # use cilly::{ClassRef};
    /// # let mut asm = cilly::Assembly::default();
    /// let uint_128 = ClassRef::uint_128(&mut asm);
    /// assert_eq!(Type::ClassRef(uint_128).as_class_ref(),Some(uint_128));
    /// assert_eq!(Type::Int(Int::U8).as_class_ref(),None);
    /// ```
    pub fn as_class_ref(&self) -> Option<Interned<ClassRef>> {
        if let Self::ClassRef(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// If this type is a pointer(*T) or a reference(&T), returns the pointed type(T).
    /// ```
    /// # use cilly::*;
    /// # let mut asm = cilly::Assembly::default();
    /// # let u8_tpe = asm.alloc_type(Type::Int(Int::U8));
    /// assert_eq!(asm.nptr(u8_tpe).pointed_to(),Some(u8_tpe));
    /// assert_eq!(asm.nref(u8_tpe).pointed_to(),Some(u8_tpe));
    /// assert_eq!(Type::Int(Int::U8).pointed_to(),None);
    /// ```
    pub fn pointed_to(&self) -> Option<Interned<Type>> {
        match self {
            Type::Ptr(type_idx) | Type::Ref(type_idx) => Some(*type_idx),
            _ => None,
        }
    }
    /// Checks if this can be assigned to another type.
    /// ```
    /// # use cilly::*;
    /// # use cilly::{ClassRef,Int};
    /// # let mut asm = cilly::Assembly::default();
    /// // You can assign a string to an object.
    /// let ps = Type::PlatformString;
    /// let obj = Type::PlatformObject;
    /// // All non-valuetype classes can be assigned to an object.
    /// assert!(ps.is_assignable_to(obj,&asm));
    /// // Valuetype, so can't be directly assigned to an object(it needs to be boxed first).
    /// assert!(!Type::ClassRef(ClassRef::int_128(&mut asm)).is_assignable_to(obj,&asm));
    /// // But you can't assign an object to a string.
    /// assert!(!obj.is_assignable_to(ps,&asm));
    /// // Types are always assignable to themselves.
    /// assert!(Type::Bool.is_assignable_to(Type::Bool,&asm));
    /// // A class ref to int_128 is assignable to Int::I128
    /// assert!(Type::Int(Int::I128).is_assignable_to(Type::ClassRef(ClassRef::int_128(&mut asm)),&asm));
    /// // A class ref to uint_128 is assignable to Int::U128
    /// assert!(Type::Int(Int::U128).is_assignable_to(Type::ClassRef(ClassRef::uint_128(&mut asm)),&asm));
    /// // You can assign a *T to a &T, but not the other way round.
    /// # let refu8 = asm.nref(Int::U8);
    /// # let ptru8 = asm.nptr(Int::U8);
    /// assert!(ptru8.is_assignable_to(refu8,&asm));
    /// assert!(!refu8.is_assignable_to(ptru8,&asm));
    /// //     Ignores partial matches.
    /// # let u128_name = asm.alloc_string("System.UInt128");
    /// # let i128_name = asm.alloc_string("System.Int128");
    /// # let system_runtime = Some(asm.alloc_string("System.Runtime"));
    /// # let string_name = asm.alloc_string("System.String");
    /// // Has the right name and is in the right assembly, but the valuetype is not right.
    /// assert!(!Type::Int(Int::U128).is_assignable_to(Type::ClassRef(asm.alloc_class_ref(ClassRef::new(u128_name, system_runtime, false, [].into()))),&asm));
    /// assert!(!Type::Int(Int::I128).is_assignable_to(Type::ClassRef(asm.alloc_class_ref(ClassRef::new(i128_name, system_runtime, false, [].into()))),&asm));
    /// assert!(!Type::PlatformString.is_assignable_to(Type::ClassRef(asm.alloc_class_ref(ClassRef::new(string_name, system_runtime, true, [].into()))),&asm));
    /// // Has the right assembly, valuetype, but the wrong name
    /// assert!(!Type::Int(Int::I128).is_assignable_to(Type::ClassRef(asm.alloc_class_ref(ClassRef::new(string_name, system_runtime, true, [].into()))),&asm));
    /// assert!(!Type::Int(Int::U128).is_assignable_to(Type::ClassRef(asm.alloc_class_ref(ClassRef::new(string_name, system_runtime, true, [].into()))),&asm));
    /// assert!(!Type::PlatformString.is_assignable_to(Type::ClassRef(asm.alloc_class_ref(ClassRef::new(u128_name, system_runtime, false, [].into()))),&asm));
    /// ```
    pub fn is_assignable_to(&self, to: Type, asm: &Assembly) -> bool {
        if *self == to {
            return true;
        }
        match (*self, to) {
            (Type::PlatformString, Type::PlatformObject) => true,
            (Type::ClassRef(cref), Type::PlatformObject) => {
                let cref = asm.class_ref(cref);
                !cref.is_valuetype()
            }
            // The reverse of the arm above, restricted to `System.Object`: the intrinsic `object`
            // (`PlatformObject`, e.g. the result of a `box`) IS `System.Object`, so it is assignable to
            // a reference-typed `System.Object` ClassRef local (the shape mycorrhiza uses everywhere:
            // `RustcCLRInteropManagedClass<.., "System.Object">`). Restricted to the name `System.Object`
            // so this can never permit an unchecked downcast to some *other* class.
            (Type::PlatformObject, Type::ClassRef(cref)) => {
                let cref = asm.class_ref(cref);
                !cref.is_valuetype() && &asm[cref.name()] == "System.Object"
            }
            (Type::ClassRef(cref), Type::PlatformString)
            | (Type::PlatformString, Type::ClassRef(cref)) => {
                let cref = asm.class_ref(cref);
                !cref.is_valuetype()
                    && cref.asm().map(|s| asm[s].as_ref()) == Some("System.Runtime")
                    && &asm[cref.name()] == "System.String"
            }
            (Type::ClassRef(cref), Type::Int(Int::I128))
            | (Type::Int(Int::I128), Type::ClassRef(cref)) => {
                let cref = asm.class_ref(cref);
                cref.is_valuetype()
                    && cref.asm().map(|s| asm[s].as_ref()) == Some("System.Runtime")
                    && &asm[cref.name()] == "System.Int128"
            }
            (Type::ClassRef(cref), Type::Int(Int::U128))
            | (Type::Int(Int::U128), Type::ClassRef(cref)) => {
                let cref = asm.class_ref(cref);
                cref.is_valuetype()
                    && cref.asm().map(|s| asm[s].as_ref()) == Some("System.Runtime")
                    && &asm[cref.name()] == "System.UInt128"
            }
            (Type::Int(Int::U16 | Int::I16), Type::PlatformChar) => true,
            // A pointer/byref whose pointee is a generic marker `!N` is mutually assignable with a
            // pointer/byref of the concrete type `!N` binds to — e.g. `Span<T>.get_Item(int)` returns
            // `!0&`, produced into a Rust `*mut T` local. ONLY a *marker* pointee is loosened here (a
            // concrete-vs-concrete pointee still requires a sound leaf arm below), and every such site
            // is proven consistent at codegen by `check_generic_marker` (which recurses into Ptr/Ref
            // pointees). Placed before the exact `(Ptr, Ref)` arm so a marker pointee is caught for all
            // four pointer/byref combinations.
            (Type::Ptr(a) | Type::Ref(a), Type::Ptr(b) | Type::Ref(b))
                if matches!(asm[a], Type::PlatformGeneric(_, _))
                    || matches!(asm[b], Type::PlatformGeneric(_, _)) =>
            {
                asm[a].is_assignable_to(asm[b], asm)
            }
            (Type::Ptr(ptr), Type::Ref(rf)) => ptr == rf,
            // Fat-pointer (DST) layout equivalence — a PROVEN false-positive fix (Phase P1 / WF-TC).
            //
            // Every `FatPtr<T>` class emitted by the backend (see `fat_ptr_to` in
            // `src/type/mod.rs`) has the *identical* on-stack layout regardless of
            // the pointee `T`: field 0 is a type-erased `void*` data pointer at offset 0, field 1 is
            // a `usize` metadata word at offset 8, with explicit size 16 / align 8. The data pointer
            // is erased to `void*` in *all* of them. Therefore `FatPtr<u8>` and
            // `FatPtr<FatPtr<u8>>` (etc.) are byte-for-byte interchangeable, but the checker's
            // name-based comparison flags them as distinct. We accept them as mutually assignable
            // *only* when both are FatPtr-named valuetypes AND their concrete field layouts match —
            // so this can never over-permit a genuinely different type (guarded by the layout check).
            (Type::ClassRef(lhs), Type::ClassRef(rhs)) if Self::fat_ptr_layout_eq(lhs, rhs, asm) => {
                true
            }
            // A generic parameter `!N` (class) / `!!N` (method) is mutually assignable with any
            // concrete type, at the StLoc and call-arg boundaries. `!N` appears ONLY in the
            // *signature* of a generic-instantiation methodref — never as a bare value of ordinary
            // (non-generic-call) codegen — emitted by the WF-9 bridge (`call_generic`/`ctor_generic`)
            // or a builtin like `ThreadLocal<T>`. The IL methodref MUST use the textual `!N` for the
            // CLR to bind it, so the checker has to accept `!N` against the concrete type the call
            // pushes/returns. This is NOT a "trust the CLR to re-verify" relaxation — CoreCLR runs
            // UNVERIFIED, so a wrong binding would silently narrow/widen, not abort. Soundness instead
            // comes from `src/terminator/call.rs::check_generic_marker`, which fails the build at
            // codegen unless every `!N` provably resolves (via the concrete class generics) to exactly
            // the runtime type it is paired with. So a `!N` value here is *guaranteed* to equal its
            // concrete binding; accepting the pair cannot mask a real mismatch.
            (_, Type::PlatformGeneric(_, _)) | (Type::PlatformGeneric(_, _), _) => true,
            // Two instantiations of the SAME open generic type are mutually assignable when their
            // type arguments are pairwise assignable. This is what lets a *definition-shape*
            // nested-generic methodref signature — `Dictionary<K,V>.KeyCollection<!0,!1>`,
            // `Comparison<!0>`, `Task<!0>` — bind against the concrete instantiation the WF-9 bridge
            // produces/consumes (`KeyCollection<int32,int32>`, `Comparison<int32>`, `Task<int32>`).
            // The only *loose* element-pairing is `!N`-vs-concrete, handled by the `PlatformGeneric`
            // arm above; every such site is proven consistent at codegen by
            // `src/terminator/call.rs::check_generic_marker`, which recurses into nested generics —
            // so a `!N` element here provably equals its concrete binding and this cannot mask a real
            // mismatch. Concrete-vs-concrete arguments must themselves be soundly assignable (the
            // recursion bottoms out in the sound leaf arms), and the open types must match exactly
            // (same name/assembly/valuetype and arity), so unrelated generics stay unassignable.
            (Type::ClassRef(lhs), Type::ClassRef(rhs)) => {
                let lref = asm.class_ref(lhs);
                let rref = asm.class_ref(rhs);
                let same_open = lref.name() == rref.name()
                    && lref.asm() == rref.asm()
                    && lref.is_valuetype() == rref.is_valuetype()
                    && !lref.generics().is_empty()
                    && lref.generics().len() == rref.generics().len();
                if same_open {
                    let lg: Vec<Type> = lref.generics().to_vec();
                    let rg: Vec<Type> = rref.generics().to_vec();
                    lg.iter().zip(rg.iter()).all(|(a, b)| a.is_assignable_to(*b, asm))
                } else {
                    false
                }
            }
            _ => false,
        }
    }
    /// Returns `true` iff `lhs` and `rhs` are both `FatPtr<…>` value-type classes with identical
    /// concrete layout (field types/names/offsets, explicit size, align). See the call site in
    /// [`Type::is_assignable_to`] for the soundness argument. Narrowly scoped to the `FatPtr` family
    /// so it cannot mask a real type mismatch between unrelated structs that merely share a size.
    fn fat_ptr_layout_eq(
        lhs: Interned<ClassRef>,
        rhs: Interned<ClassRef>,
        asm: &Assembly,
    ) -> bool {
        let lref = asm.class_ref(lhs);
        let rref = asm.class_ref(rhs);
        // Both must be value-type, generic-free, locally-defined `FatPtr…` classes.
        if !(lref.is_valuetype() && rref.is_valuetype()) {
            return false;
        }
        if !lref.generics().is_empty() || !rref.generics().is_empty() {
            return false;
        }
        let lname = &asm[lref.name()];
        let rname = &asm[rref.name()];
        if !(lname.starts_with("FatPtr") && rname.starts_with("FatPtr")) {
            return false;
        }
        // The layout guarantee comes from the *producer invariant*: `fat_ptr_to` is the only thing
        // that mints a `FatPtr…` value-type class, and it always builds the identical 16/8 layout
        // (`void*` data @0, `usize` meta @8). So a value-type, generic-free `FatPtr…` name uniquely
        // identifies that layout. When *both* class definitions happen to be present in this
        // assembly (single-CGU / post-link), we additionally assert their concrete layouts match as
        // a defensive cross-check; when a def is absent (the common per-CGU case, where only the
        // `ClassRef` is interned), we rely on the producer invariant alone.
        match (asm.class_ref_to_def(lhs), asm.class_ref_to_def(rhs)) {
            (Some(ldef), Some(rdef)) => {
                let ldef = &asm[ldef];
                let rdef = &asm[rdef];
                ldef.explict_size() == rdef.explict_size()
                    && ldef.align() == rdef.align()
                    && ldef.fields() == rdef.fields()
            }
            // At least one definition is not in this assembly — accept on the producer invariant.
            _ => true,
        }
    }
    /// If this type is an int, return that int.
    /// ```
    /// # use cilly::Int;
    /// # use cilly::*;
    /// let tpe = Type::PlatformString;
    /// // Not an int, so this returns none.
    /// assert_eq!(tpe.as_int(),None);
    /// let tpe = Type::Int(Int::ISize);
    /// // An int, so this returns Some.
    /// assert_eq!(tpe.as_int(),Some(Int::ISize));
    /// ```
    pub fn as_int(&self) -> Option<Int> {
        if let Self::Int(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    /// If this type is an [`SIMDVector`], return that SIMDVector.
    /// ```
    /// # use cilly::tpe::simd::{SIMDElem,SIMDVector};
    /// # use cilly::*;
    /// let vec = SIMDVector::new(Int::U64.into(),4);
    /// assert_eq!(Type::SIMDVector(vec).as_simdvector(),Some(&vec));
    /// assert_eq!(Type::Int(Int::U64).as_simdvector(),None);
    /// ```
    pub fn as_simdvector(&self) -> Option<&SIMDVector> {
        if let Self::SIMDVector(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Returns `true` if the type is [`Ptr`].
    ///
    /// [`Ptr`]: Type::Ptr
    #[must_use]
    pub fn is_ptr(&self) -> bool {
        matches!(self, Self::Ptr(..))
    }
}

#[cfg(test)]
mod fat_ptr_assignability_tests {
    //! Soundness tests for the `FatPtr` layout-equivalence arm of [`Type::is_assignable_to`]
    //! (Phase P1 of the absolute-correctness plan). Proves the relaxation accepts the proven
    //! false-positive (fat-ptr nesting) while *not* over-permitting unrelated same-size structs.
    use super::*;
    use crate::ir::{Access, ClassDef};
    use std::num::NonZeroU32;

    /// Mirror the backend's `type::fat_ptr_to`: a value-type class named `FatPtr<elem>`
    /// with a type-erased `void*` data pointer @0 and a `usize` metadata word @8, size 16 / align 8.
    fn make_fat_ptr(asm: &mut Assembly, elem_mangled: &str) -> Interned<ClassRef> {
        let name = asm.alloc_string(format!("FatPtr{elem_mangled}"));
        let cref = asm.alloc_class_ref(ClassRef::new(name, None, true, [].into()));
        if asm.class_ref_to_def(cref).is_none() {
            let void_ptr = asm.nptr(Type::Void);
            let data = asm.alloc_string(crate::DATA_PTR);
            let meta = asm.alloc_string(crate::METADATA);
            let def = ClassDef::new(
                name,
                true,
                0,
                None,
                vec![
                    (void_ptr, data, Some(0)),
                    (Type::Int(Int::USize), meta, Some(8)),
                ],
                vec![],
                Access::Public,
                Some(NonZeroU32::new(16).unwrap()),
                Some(NonZeroU32::new(8).unwrap()),
                true,
            );
            asm.class_def(def).unwrap();
        }
        cref
    }

    #[test]
    fn fat_ptr_nesting_is_assignable() {
        let mut asm = Assembly::default();
        let fp_u8 = make_fat_ptr(&mut asm, "u8");
        let fp_fp_u8 = make_fat_ptr(&mut asm, "FatPtru8");
        assert!(
            Type::ClassRef(fp_u8).is_assignable_to(Type::ClassRef(fp_fp_u8), &asm),
            "FatPtr<u8> and FatPtr<FatPtr<u8>> have identical layout and must be assignable"
        );
        assert!(
            Type::ClassRef(fp_fp_u8).is_assignable_to(Type::ClassRef(fp_u8), &asm),
            "the relation is symmetric for identical layouts"
        );
    }

    /// GUARD against over-permitting: a non-`FatPtr` value-type with the *same* 16-byte/align-8
    /// layout must NOT be considered assignable to a `FatPtr` — the name-prefix scoping holds.
    #[test]
    fn same_layout_non_fatptr_is_not_assignable() {
        let mut asm = Assembly::default();
        let fp_u8 = make_fat_ptr(&mut asm, "u8");
        // An unrelated 16-byte struct with the same field shape but a different name.
        let name = asm.alloc_string("SomeOtherWideStruct");
        let other = asm.alloc_class_ref(ClassRef::new(name, None, true, [].into()));
        let void_ptr = asm.nptr(Type::Void);
        let data = asm.alloc_string(crate::DATA_PTR);
        let meta = asm.alloc_string(crate::METADATA);
        let def = ClassDef::new(
            name, true, 0, None,
            vec![(void_ptr, data, Some(0)), (Type::Int(Int::USize), meta, Some(8))],
            vec![], Access::Public,
            Some(NonZeroU32::new(16).unwrap()), Some(NonZeroU32::new(8).unwrap()), true,
        );
        asm.class_def(def).unwrap();
        assert!(
            !Type::ClassRef(fp_u8).is_assignable_to(Type::ClassRef(other), &asm),
            "an identically-laid-out but non-FatPtr struct must NOT be silently assignable"
        );
    }
}
