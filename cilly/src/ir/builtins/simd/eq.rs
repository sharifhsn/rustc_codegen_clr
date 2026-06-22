use crate::{
    asm::MissingMethodPatcher, bimap::Interned, Assembly, BasicBlock, CILNode, CILRoot, MethodImpl,
    MethodRef, Type,
};

use super::dotnet_vec_cast;
pub(super) fn simd_eq(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_eq");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let result = sig.output();
        let Some(comparands) = sig.inputs()[0].as_simdvector() else {
            todo!(
                "Can't simd compare {comparands:?} and get {result:?}",
                comparands = sig.inputs()[0]
            )
        };
        let elem: Type = comparands.elem().into();
        let Some(result) = result.as_simdvector() else {
            todo!("Can't simd compare {comparands:?} and get {result:?}",)
        };
        let extension_class = comparands.extension_class(asm);
        let extension_class = asm[extension_class].clone();
        let equals = asm.alloc_string("Equals");
        // Generic vec
        let generic_class = comparands.class(asm);
        let mut generic_class = asm[generic_class].clone();
        generic_class.set_generics(vec![Type::PlatformGeneric(
            0,
            crate::tpe::GenericKind::CallGeneric,
        )]);
        let generic_class = asm.alloc_class_ref(generic_class);
        let equals = extension_class.static_mref_generic(
            &[Type::ClassRef(generic_class), Type::ClassRef(generic_class)],
            Type::ClassRef(generic_class),
            equals,
            asm,
            [elem].into(),
        );
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let equals = asm.alloc_node(CILNode::call(equals, [lhs, rhs]));
        let cast = dotnet_vec_cast(equals, *comparands, *result, asm);
        let ret = asm.alloc_root(CILRoot::Ret(cast));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub(super) fn simd_lt(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_lt");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let result = sig.output();
        let Some(comparands) = sig.inputs()[0].as_simdvector() else {
            todo!(
                "Can't simd compare {comparands:?} and get {result:?}",
                comparands = sig.inputs()[0]
            )
        };
        let elem: Type = comparands.elem().into();
        let Some(result) = result.as_simdvector() else {
            todo!("Can't simd compare {comparands:?} and get {result:?}",)
        };
        let extension_class = comparands.extension_class(asm);
        let extension_class = asm[extension_class].clone();
        let equals = asm.alloc_string("LessThan");
        // Generic vec
        let generic_class = comparands.class(asm);
        let mut generic_class = asm[generic_class].clone();
        generic_class.set_generics(vec![Type::PlatformGeneric(
            0,
            crate::tpe::GenericKind::CallGeneric,
        )]);
        let generic_class = asm.alloc_class_ref(generic_class);
        let equals = extension_class.static_mref_generic(
            &[Type::ClassRef(generic_class), Type::ClassRef(generic_class)],
            Type::ClassRef(generic_class),
            equals,
            asm,
            [elem].into(),
        );
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let equals = asm.alloc_node(CILNode::call(equals, [lhs, rhs]));
        let cast = dotnet_vec_cast(equals, *comparands, *result, asm);
        let ret = asm.alloc_root(CILRoot::Ret(cast));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
/// The remaining element-wise ordered comparisons (`simd_gt`/`simd_ge`/`simd_le`), mirroring
/// `simd_lt` but for the other orderings. Each maps to the BCL `Vector{bits}.<Method><T>` generic
/// static, which returns the correct all-ones-per-true-lane mask Rust's SIMD expects — a per-lane
/// `0`/`1` fallback would silently break `simd_bitmask` and any MSB-reading consumer.
macro_rules! simd_cmp {
    ($fn_name:ident, $dotnet:literal) => {
        pub(super) fn $fn_name(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
            let name = asm.alloc_string(stringify!($fn_name));
            let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
                let sig = asm[asm[mref].sig()].clone();
                let result = sig.output();
                let Some(comparands) = sig.inputs()[0].as_simdvector() else {
                    todo!(
                        "Can't simd compare {comparands:?} and get {result:?}",
                        comparands = sig.inputs()[0]
                    )
                };
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
simd_cmp!(simd_gt, "GreaterThan");
simd_cmp!(simd_ge, "GreaterThanOrEqual");
simd_cmp!(simd_le, "LessThanOrEqual");
pub(super) fn simd_eq_all(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_eq_all");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let result = sig.output();
        let Some(comparands) = sig.inputs()[0].as_simdvector() else {
            todo!(
                "Can't simd compare {comparands:?} and get {result:?}",
                comparands = sig.inputs()[0]
            )
        };
        let elem: Type = comparands.elem().into();

        let extension_class = comparands.extension_class(asm);
        let extension_class = asm[extension_class].clone();
        let equals = asm.alloc_string("EqualsAll");
        // Generic vec
        let generic_class = comparands.class(asm);
        let mut generic_class = asm[generic_class].clone();
        generic_class.set_generics(vec![Type::PlatformGeneric(
            0,
            crate::tpe::GenericKind::CallGeneric,
        )]);
        let generic_class = asm.alloc_class_ref(generic_class);
        let equals = extension_class.static_mref_generic(
            &[Type::ClassRef(generic_class), Type::ClassRef(generic_class)],
            Type::Bool,
            equals,
            asm,
            [elem].into(),
        );
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let equals = asm.alloc_node(CILNode::call(equals, [lhs, rhs]));

        let ret = asm.alloc_root(CILRoot::Ret(equals));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub(super) fn simd_eq_any(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_eq_any");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let result = sig.output();
        let Some(comparands) = sig.inputs()[0].as_simdvector() else {
            todo!(
                "Can't simd compare {comparands:?} and get {result:?}",
                comparands = sig.inputs()[0]
            )
        };
        let elem: Type = comparands.elem().into();

        let extension_class = comparands.extension_class(asm);
        let extension_class = asm[extension_class].clone();
        let equals = asm.alloc_string("EqualsAny");
        // Generic vec
        let generic_class = comparands.class(asm);
        let mut generic_class = asm[generic_class].clone();
        generic_class.set_generics(vec![Type::PlatformGeneric(
            0,
            crate::tpe::GenericKind::CallGeneric,
        )]);
        let generic_class = asm.alloc_class_ref(generic_class);
        let equals = extension_class.static_mref_generic(
            &[Type::ClassRef(generic_class), Type::ClassRef(generic_class)],
            Type::Bool,
            equals,
            asm,
            [elem].into(),
        );
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let equals = asm.alloc_node(CILNode::call(equals, [lhs, rhs]));

        let ret = asm.alloc_root(CILRoot::Ret(equals));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
