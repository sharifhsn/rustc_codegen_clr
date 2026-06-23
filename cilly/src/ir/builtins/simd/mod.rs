use crate::{
    asm::MissingMethodPatcher, bimap::Interned, tpe::simd::SIMDVector, Assembly, BasicBlock,
    CILNode, CILRoot, MethodImpl, MethodRef, Type,
};
mod eq;
use eq::*;
mod binop;
use binop::*;
mod tail;
fn dotnet_vec_cast(
    src: Interned<CILNode>,
    src_type: SIMDVector,
    target_type: SIMDVector,
    asm: &mut Assembly,
) -> Interned<CILNode> {
    if src_type == target_type {
        return src;
    }
    eprintln!("Can't cast {src_type:?} -> {target_type:?}");
    let _ = asm;
    src
}

fn simd_ones_compliment(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_ones_compliment");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();

        let Some(vec_type) = sig.inputs()[0].as_simdvector() else {
            todo!(
                "Can't calc the ones compliment of {vec_type:?}",
                vec_type = sig.inputs()[0]
            )
        };
        let elem: Type = vec_type.elem().into();
        let extension_class = vec_type.extension_class(asm);
        let extension_class = asm[extension_class].clone();
        let ones_compliment = asm.alloc_string("OnesComplement");
        // Generic vec
        let generic_class = vec_type.class(asm);
        let mut generic_class = asm[generic_class].clone();
        generic_class.set_generics(vec![Type::PlatformGeneric(
            0,
            crate::tpe::GenericKind::CallGeneric,
        )]);
        let generic_class = asm.alloc_class_ref(generic_class);
        let ones_compliment = extension_class.static_mref_generic(
            &[Type::ClassRef(generic_class)],
            Type::ClassRef(generic_class),
            ones_compliment,
            asm,
            [elem].into(),
        );
        let val = asm.alloc_node(CILNode::LdArg(0));
        let res = asm.alloc_node(CILNode::call(ones_compliment, [val]));
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

fn simd_neg(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_neg");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();

        let Some(vec_type) = sig.inputs()[0].as_simdvector() else {
            todo!(
                "Can't calc the ones compliment of {vec_type:?}",
                vec_type = sig.inputs()[0]
            )
        };
        let elem: Type = vec_type.elem().into();
        let extension_class = vec_type.extension_class(asm);
        let extension_class = asm[extension_class].clone();
        let ones_compliment = asm.alloc_string("Negate");
        // Generic vec
        let generic_class = vec_type.class(asm);
        let mut generic_class = asm[generic_class].clone();
        generic_class.set_generics(vec![Type::PlatformGeneric(
            0,
            crate::tpe::GenericKind::CallGeneric,
        )]);
        let generic_class = asm.alloc_class_ref(generic_class);
        let ones_compliment = extension_class.static_mref_generic(
            &[Type::ClassRef(generic_class)],
            Type::ClassRef(generic_class),
            ones_compliment,
            asm,
            [elem].into(),
        );
        let val = asm.alloc_node(CILNode::LdArg(0));
        let res = asm.alloc_node(CILNode::call(ones_compliment, [val]));
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
fn simd_abs(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_abs");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();

        let Some(vec_type) = sig.inputs()[0].as_simdvector() else {
            todo!(
                "Can't calc simd_abs of {vec_type:?}",
                vec_type = sig.inputs()[0]
            )
        };
        let elem: Type = vec_type.elem().into();
        let extension_class = vec_type.extension_class(asm);
        let extension_class = asm[extension_class].clone();
        let ones_compliment = asm.alloc_string("Abs");
        // Generic vec
        let generic_class = vec_type.class(asm);
        let mut generic_class = asm[generic_class].clone();
        generic_class.set_generics(vec![Type::PlatformGeneric(
            0,
            crate::tpe::GenericKind::CallGeneric,
        )]);
        let generic_class = asm.alloc_class_ref(generic_class);
        let ones_compliment = extension_class.static_mref_generic(
            &[Type::ClassRef(generic_class)],
            Type::ClassRef(generic_class),
            ones_compliment,
            asm,
            [elem].into(),
        );
        let val = asm.alloc_node(CILNode::LdArg(0));
        let res = asm.alloc_node(CILNode::call(ones_compliment, [val]));
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
// `simd_shuffle` and the rest of the SIMD tail (per-lane ctlz/cttz/ctpop/bswap/bitreverse, the float
// rounders, and fma) live in `tail.rs` and are registered through `register_value_lane_ops`.
fn simd_vec_from_val(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_vec_from_val");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let Some(vec_type) = sig.output().as_simdvector() else {
            todo!(
                "Can't simd_vec_from_val  {vec_type:?}",
                vec_type = sig.output()
            )
        };
        let extension_class = vec_type.extension_class(asm);
        let extension_class = asm[extension_class].clone();
        let create = asm.alloc_string("Create");
        let create = extension_class.static_mref(&[sig.inputs()[0]], *sig.output(), create, asm);
        let val = asm.alloc_node(CILNode::LdArg(0));
        let res = asm.alloc_node(CILNode::call(create, [val]));
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
fn simd_allset(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_allset");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let Some(vec_type) = sig.output().as_simdvector() else {
            todo!("Can't simd_allset {vec_type:?}", vec_type = sig.output())
        };
        let class = vec_type.class(asm);
        let class = asm[class].clone();
        let generic_class = vec_type.class(asm);
        let mut generic_class = asm[generic_class].clone();
        generic_class.set_generics(vec![Type::PlatformGeneric(
            0,
            crate::tpe::GenericKind::TypeGeneric,
        )]);
        let generic_class = asm.alloc_class_ref(generic_class);
        let create = asm.alloc_string("get_AllBitsSet");
        let create = class.static_mref(&[], Type::ClassRef(generic_class), create, asm);
        let res = asm.alloc_node(CILNode::call(create, []));
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

pub fn simd(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    // Comparisons via the BCL `Vector` statics (all-ones masks, hardware SIMD).
    simd_eq(asm, patcher);
    simd_lt(asm, patcher);
    simd_gt(asm, patcher);
    simd_ge(asm, patcher);
    simd_le(asm, patcher);
    simd_ones_compliment(asm, patcher);
    simd_neg(asm, patcher);
    simd_abs(asm, patcher);
    simd_vec_from_val(asm, patcher);
    // Element-wise arithmetic / bitwise via the BCL `Vector` statics.
    simd_or(asm, patcher);
    simd_add(asm, patcher);
    simd_and(asm, patcher);
    simd_sub(asm, patcher);
    simd_allset(asm, patcher);
    simd_eq_all(asm, patcher);
    simd_eq_any(asm, patcher);
    simd_mul(asm, patcher);
    // Per-lane value ops with no BCL-static equivalent here (xor/shl/shr/div/cast).
    binop::register_value_lane_ops(asm, patcher);
}
pub use binop::fallback_simd;
