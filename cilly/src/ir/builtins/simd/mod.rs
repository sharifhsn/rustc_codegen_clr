use crate::{
    Assembly, BasicBlock, CILNode, CILRoot, MethodImpl, MethodRef, Type, asm::MissingMethodPatcher,
    bimap::Interned, tpe::simd::SIMDVector,
};
mod eq;
use eq::*;
mod binop;
use binop::*;
mod bitmask;
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
    assert_eq!(
        src_type.bits(),
        target_type.bits(),
        "SIMD mask reinterpret must preserve the total vector width"
    );

    // BCL comparisons return `VectorN<TInput>`, while Rust's SIMD comparison result may use a
    // signedness-different mask element (for example `u8x16 -> i8x16`). These are identical bits,
    // not a numeric per-lane conversion. Route the reinterpret through the canonical transmute
    // helper so the IR and final verifier both see the destination vector type explicitly.
    asm.transmute_on_stack(
        Type::SIMDVector(src_type),
        Type::SIMDVector(target_type),
        src,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Int;

    #[test]
    fn signedness_only_vector_cast_is_an_explicit_reinterpret() {
        let mut asm = Assembly::default();
        let src_ty = SIMDVector::new(Int::U8.into(), 16);
        let dst_ty = SIMDVector::new(Int::I8.into(), 16);
        let src = asm.alloc_node(CILNode::LdArg(0));

        let cast = dotnet_vec_cast(src, src_ty, dst_ty, &mut asm);
        let CILNode::Call(call) = &asm[cast] else {
            panic!("SIMD signedness reinterpret did not use the typed transmute helper");
        };
        let sig = asm[asm[call.0].sig()].clone();
        assert_eq!(*sig.output(), Type::SIMDVector(dst_ty));
        assert_eq!(sig.inputs(), &[Type::SIMDVector(src_ty)]);
    }
}

fn simd_ones_compliment(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_ones_compliment");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();

        let Some(vec_type) = sig.inputs()[0].as_simdvector() else {
            // Array fallback (unsupported vector size): per-lane bitwise NOT.
            return binop::lane_unop_body(mref, asm, &|asm, x, _, _| asm.not(x));
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
            // Array fallback (unsupported vector size): negate per lane.
            return binop::lane_unop_body(mref, asm, &|asm, x, _, _| asm.neg(x));
        };
        // IL `neg` (sign-flip) preserves the sign of zero/NaN, matching Rust `-x`; the BCL
        // `Vector{bits}.Negate` computes `0 - x`, which turns `+0.0` into `+0.0` instead of `-0.0`.
        // Negate FLOAT lanes per-lane (correct signed zero); ints keep the hardware `Negate`.
        if matches!(vec_type.elem(), crate::tpe::simd::SIMDElem::Float(_)) {
            return binop::lane_unop_body(mref, asm, &|asm, x, _, _| asm.neg(x));
        }
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
            // Array fallback (unsupported vector size): per-lane abs. Signed ints use the branchless
            // `(x ^ s) - s` with `s = x >> (bits-1)` (arithmetic shift); unsigned is the identity.
            return binop::lane_unop_body(mref, asm, &|asm, x, elem, _| match elem {
                crate::tpe::simd::SIMDElem::Int(int) if !int.is_signed() => x,
                crate::tpe::simd::SIMDElem::Int(int) => {
                    let bits = int.bits().unwrap_or(64);
                    let shift = asm.alloc_node(crate::Const::I32(i32::from(bits) - 1));
                    let s = asm.biop(x, shift, crate::BinOp::Shr);
                    let xs = asm.biop(x, s, crate::BinOp::XOr);
                    asm.biop(xs, s, crate::BinOp::Sub)
                }
                crate::tpe::simd::SIMDElem::Float(f) => f.math1(x, asm, "Abs"),
            });
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
            // Array fallback (unsupported vector size): splat the scalar into every lane.
            return binop::lane_splat_body(mref, asm);
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
            // Array fallback (unsupported vector size): store all-ones into every lane.
            let res = *sig.output();
            let (elem_s, count) =
                binop::simd_lane_info(res, asm).expect("simd_allset result is not a vector");
            let elem: Type = elem_s.into();
            let allones = match elem_s {
                crate::tpe::simd::SIMDElem::Int(int) => {
                    let neg1 = asm.alloc_node(crate::Const::I64(-1));
                    asm.int_cast(neg1, int, crate::cilnode::ExtendKind::SignExtend)
                }
                crate::tpe::simd::SIMDElem::Float(_) => {
                    todo!("simd_allset float array fallback for an unsupported vector size")
                }
            };
            let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
            let mut roots = vec![];
            for idx in 0..count {
                let rp = asm.cast_ptr(res_ptr, elem);
                let slot = asm.offset(rp, crate::Const::USize(idx), elem);
                roots.push(asm.alloc_root(CILRoot::StInd(Box::new((slot, allones, elem, false)))));
            }
            let ret = asm.alloc_node(CILNode::LdLoc(0));
            roots.push(asm.alloc_root(CILRoot::Ret(ret)));
            return MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(roots, 0, None)],
                locals: vec![(None, asm.alloc_type(res))],
            };
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
    bitmask::register_most_significant_bits(asm, patcher);
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
