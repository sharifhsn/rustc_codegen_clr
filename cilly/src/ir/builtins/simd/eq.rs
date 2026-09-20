use crate::{
    Assembly, BasicBlock, CILNode, CILRoot, Float, MethodImpl, MethodRef, Type,
    asm::MissingMethodPatcher, bimap::Interned, tpe::simd::SIMDElem,
};

use super::binop::{CmpKind, lane_all_any_body, lane_cmp_body};
use super::dotnet_vec_cast;
macro_rules! simd_cmp {
    ($fn_name:ident, $dotnet:literal, $kind:expr) => {
        pub(super) fn $fn_name(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
            let name = asm.alloc_string(stringify!($fn_name));
            let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
                let sig = asm[asm[mref].sig()].clone();
                let result = sig.output();
                let Some(comparands) = sig.inputs()[0].as_simdvector() else {
                    // Array fallback (unsupported vector size): compare per lane into an all-ones mask.
                    return lane_cmp_body(mref, asm, $kind);
                };
                if matches!(comparands.elem(), SIMDElem::Float(Float::F16)) {
                    return lane_cmp_body(mref, asm, $kind);
                }
                let elem: Type = comparands.elem().into();
                let Some(result) = result.as_simdvector() else {
                    todo!("Can't simd compare {comparands:?} and get {result:?}",)
                };
                let extension_class = comparands.extension_class(asm);
                let extension_class = asm[extension_class].clone();
                let method = asm.alloc_string($dotnet);
                let generic_class = comparands.class(asm);
                let mut generic_class = asm[generic_class].clone();
                generic_class.set_generics(vec![Type::PlatformGeneric(
                    0,
                    crate::tpe::GenericKind::CallGeneric,
                )]);
                let generic_class = asm.alloc_class_ref(generic_class);
                let method = extension_class.static_mref_generic(
                    &[Type::ClassRef(generic_class), Type::ClassRef(generic_class)],
                    Type::ClassRef(generic_class),
                    method,
                    asm,
                    [elem].into(),
                );
                let lhs = asm.alloc_node(CILNode::LdArg(0));
                let rhs = asm.alloc_node(CILNode::LdArg(1));
                let call = asm.alloc_node(CILNode::call(method, [lhs, rhs]));
                let cast = dotnet_vec_cast(call, *comparands, *result, asm);
                let ret = asm.alloc_root(CILRoot::Ret(cast));
                MethodImpl::MethodBody {
                    blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                    locals: vec![],
                }
            };
            patcher.insert(name, Box::new(generator));
        }
    };
}
// All vector comparisons share the same BCL dispatch and lane fallback.  The result cast keeps
// signed and unsigned mask element types bit-identical to Rust's SIMD comparison contract.
simd_cmp!(simd_eq, "Equals", CmpKind::Eq);
simd_cmp!(simd_lt, "LessThan", CmpKind::Lt);
simd_cmp!(simd_gt, "GreaterThan", CmpKind::Gt);
simd_cmp!(simd_ge, "GreaterThanOrEqual", CmpKind::Ge);
simd_cmp!(simd_le, "LessThanOrEqual", CmpKind::Le);
macro_rules! simd_all_any {
    ($fn_name:ident, $dotnet:literal, $all:expr) => {
        pub(super) fn $fn_name(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
            let name = asm.alloc_string(stringify!($fn_name));
            let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
                let sig = asm[asm[mref].sig()].clone();
                let Some(comparands) = sig.inputs()[0].as_simdvector() else {
                    return lane_all_any_body(mref, asm, $all);
                };
                if matches!(comparands.elem(), SIMDElem::Float(Float::F16)) {
                    return lane_all_any_body(mref, asm, $all);
                }
                let elem: Type = comparands.elem().into();
                let extension_class = comparands.extension_class(asm);
                let extension_class = asm[extension_class].clone();
                let generic_class = comparands.class(asm);
                let mut generic_class = asm[generic_class].clone();
                generic_class.set_generics(vec![Type::PlatformGeneric(
                    0,
                    crate::tpe::GenericKind::CallGeneric,
                )]);
                let generic_class = asm.alloc_class_ref(generic_class);
                let method = extension_class.static_mref_generic(
                    &[Type::ClassRef(generic_class), Type::ClassRef(generic_class)],
                    Type::Bool,
                    asm.alloc_string($dotnet),
                    asm,
                    [elem].into(),
                );
                let lhs = asm.alloc_node(CILNode::LdArg(0));
                let rhs = asm.alloc_node(CILNode::LdArg(1));
                let call = asm.alloc_node(CILNode::call(method, [lhs, rhs]));
                let ret = asm.alloc_root(CILRoot::Ret(call));
                MethodImpl::MethodBody {
                    blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                    locals: vec![],
                }
            };
            patcher.insert(name, Box::new(generator));
        }
    };
}
simd_all_any!(simd_eq_all, "EqualsAll", true);
simd_all_any!(simd_eq_any, "EqualsAny", false);
