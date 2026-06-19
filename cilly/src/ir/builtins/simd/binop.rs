use crate::{
    asm::MissingMethodPatcher, tpe::simd::SIMDElem, Assembly, BasicBlock, BinOp, CILNode, CILRoot,
    Const, Interned, MethodImpl, MethodRef, Type,
};
macro_rules! binop {
    ($op_name:ident,$op_dotnet:literal) => {
        pub fn $op_name(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
            let name = asm.alloc_string(stringify!($op_name));
            let generator = move |mref: $crate::ir::Interned<$crate::ir::MethodRef>,
                                  asm: &mut Assembly| {
                let sig = asm[asm[mref].sig()].clone();

                let Some(comparands) = sig.inputs()[0].as_simdvector() else {
                    let name = stringify!($op_name);
                    todo!("Can't {name} {comparands:?} ", comparands = sig.inputs()[0])
                };
                let elem: Type = comparands.elem().into();

                let extension_class = comparands.extension_class(asm);
                let extension_class = asm[extension_class].clone();
                let equals = asm.alloc_string($op_dotnet);
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
                let res = asm.alloc_node(CILNode::call(equals, [lhs, rhs]));

                let ret = asm.alloc_root(CILRoot::Ret(res));
                MethodImpl::MethodBody {
                    blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                    locals: vec![],
                }
            };
            patcher.insert(name, Box::new(generator));
        }
    };
}
binop!(simd_or, "BitwiseOr");
binop!(simd_add, "Add");
binop!(simd_and, "BitwiseAnd");
binop!(simd_sub, "Subtract");
binop!(simd_mul, "Multiply");
binop!(simd_div, "Divides");
fn simd_binop(
    op: impl Fn(&mut Assembly, Interned<CILNode>, Interned<CILNode>, SIMDElem, Type) -> Interned<CILNode>
        + 'static,
    name: &str,
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
) {
    let name = asm.alloc_string(name);
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        // Extrac types from signature
        let sig = &asm[asm[mref].sig()];
        let res = *sig.output();
        let res_elem = res.as_simdvector().unwrap().elem().into();
        let vec = sig.inputs()[0].as_simdvector().unwrap().clone();
        let elem = vec.elem();
        let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
        let tpe: Type = elem.into();
        let tpe_idx = asm.alloc_type(tpe);
        // Get args
        let lhs = asm.alloc_node(CILNode::LdArgA(0));
        let rhs = asm.alloc_node(CILNode::LdArgA(1));
        let lhs = asm.cast_ptr(lhs, tpe);
        let rhs = asm.cast_ptr(rhs, tpe);
        // Iter trough all elements
        let mut roots = vec![];
        for idx in 0..vec.count() {
            let lhs = asm.offset(lhs, Const::USize(idx as u64), tpe);
            let rhs = asm.offset(rhs, Const::USize(idx as u64), tpe);
            let lhs = asm.alloc_node(CILNode::LdInd {
                addr: lhs,
                tpe: tpe_idx,
                volatile: false,
            });
            let rhs = asm.alloc_node(CILNode::LdInd {
                addr: rhs,
                tpe: tpe_idx,
                volatile: false,
            });
            let res_ptr = asm.cast_ptr(res_ptr, res_elem);
            let res_ptr = asm.offset(res_ptr, Const::USize(idx as u64), res_elem);
            let res = op(asm, lhs, rhs, elem, res_elem);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((res_ptr, res, res_elem, false)))));
        }
        let ret = asm.alloc_node(CILNode::LdLoc(0));
        roots.push(asm.alloc_root(CILRoot::Ret(ret)));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals: vec![(None, asm.alloc_type(res))],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn fallback_simd(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    simd_binop(
        |asm, lhs, rhs, _, res_tpe| {
            let res = asm.biop(lhs, rhs, BinOp::Lt);
            asm.int_cast(
                res,
                res_tpe.as_int().unwrap(),
                crate::cilnode::ExtendKind::ZeroExtend,
            )
        },
        "simd_lt",
        asm,
        patcher,
    );
    simd_binop(
        |asm, lhs, rhs, _, res_tpe| {
            let res = asm.biop(lhs, rhs, BinOp::Eq);
            asm.int_cast(
                res,
                res_tpe.as_int().unwrap(),
                crate::cilnode::ExtendKind::ZeroExtend,
            )
        },
        "simd_eq",
        asm,
        patcher,
    );
    simd_binop(
        |asm, lhs, rhs, _, _| asm.biop(lhs, rhs, BinOp::Add),
        "simd_add",
        asm,
        patcher,
    );
    simd_binop(
        |asm, lhs, rhs, _, _| asm.biop(lhs, rhs, BinOp::Sub),
        "simd_sub",
        asm,
        patcher,
    );
    simd_binop(
        |asm, lhs, rhs, _, res_tpe| {
            let res = asm.biop(lhs, rhs, BinOp::Gt);
            asm.int_cast(
                res,
                res_tpe.as_int().unwrap(),
                crate::cilnode::ExtendKind::ZeroExtend,
            )
        },
        "simd_gt",
        asm,
        patcher,
    );
    // No `>=`/`<=` BinOp exists, so express them via the available comparisons:
    //   x >= y  <=>  !(x < y)  -> compute `Lt` (0/1) then `== 0`.
    //   x <= y  <=>  !(x > y)  -> compute `Gt` (0/1) then `== 0`.
    // Result follows the same 0/1 lane convention as `simd_lt`/`simd_eq` above.
    simd_binop(
        |asm, lhs, rhs, _, res_tpe| {
            let lt = asm.biop(lhs, rhs, BinOp::Lt);
            let zero = asm.alloc_node(Const::I32(0));
            let ge = asm.biop(lt, zero, BinOp::Eq);
            asm.int_cast(
                ge,
                res_tpe.as_int().unwrap(),
                crate::cilnode::ExtendKind::ZeroExtend,
            )
        },
        "simd_ge",
        asm,
        patcher,
    );
    simd_binop(
        |asm, lhs, rhs, _, res_tpe| {
            let gt = asm.biop(lhs, rhs, BinOp::Gt);
            let zero = asm.alloc_node(Const::I32(0));
            let le = asm.biop(gt, zero, BinOp::Eq);
            asm.int_cast(
                le,
                res_tpe.as_int().unwrap(),
                crate::cilnode::ExtendKind::ZeroExtend,
            )
        },
        "simd_le",
        asm,
        patcher,
    );
    // Bitwise / shift element-wise binops (`(vec, vec) -> vec`).
    simd_binop(
        |asm, lhs, rhs, _, _| asm.biop(lhs, rhs, BinOp::XOr),
        "simd_xor",
        asm,
        patcher,
    );
    simd_binop(
        |asm, lhs, rhs, _, _| asm.biop(lhs, rhs, BinOp::Shl),
        "simd_shl",
        asm,
        patcher,
    );
    // `simd_shr` is an arithmetic shift for signed lanes and a logical shift for unsigned
    // lanes; pick the CIL opcode from the (per-lane) element type's signedness. Float lanes
    // can't be shifted, so fall back to `Shr` (unreachable for well-typed MIR).
    simd_binop(
        |asm, lhs, rhs, elem, _| {
            let signed = match elem {
                SIMDElem::Int(int) => int.is_signed(),
                SIMDElem::Float(_) => true,
            };
            let op = if signed { BinOp::Shr } else { BinOp::ShrUn };
            asm.biop(lhs, rhs, op)
        },
        "simd_shr",
        asm,
        patcher,
    );
    // `simd_cast<T,U>` — per-lane numeric conversion. Not a binop (single input vector), so it
    // has its own generator that walks lanes, converting each `src_elem` to `dst_elem`.
    simd_cast(asm, patcher);
}

/// Builtin generator for `simd_cast` / `simd_as`: a single-input per-lane numeric convert.
/// Mirrors `simd_binop`'s spill-and-index memory idiom, but reads from one source vector and
/// converts each lane to the destination element type (int<->int via `IntCast`, anything
/// touching floats via `FloatCast`).
fn simd_cast(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_cast");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = &asm[asm[mref].sig()];
        let res = *sig.output();
        let res_vec = res.as_simdvector().unwrap().clone();
        let res_elem = res_vec.elem();
        let res_elem_tpe: Type = res_elem.into();
        let src_vec = sig.inputs()[0].as_simdvector().unwrap().clone();
        let src_elem = src_vec.elem();
        let src_elem_tpe: Type = src_elem.into();
        let count = src_vec.count();

        let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
        let src_tpe_idx = asm.alloc_type(src_elem_tpe);
        let src = asm.alloc_node(CILNode::LdArgA(0));
        let src = asm.cast_ptr(src, src_elem_tpe);
        let mut roots = vec![];
        for idx in 0..count {
            let slot = asm.offset(src, Const::USize(idx as u64), src_elem_tpe);
            let lane = asm.alloc_node(CILNode::LdInd {
                addr: slot,
                tpe: src_tpe_idx,
                volatile: false,
            });
            // Convert the lane from src_elem -> res_elem.
            let converted = match (src_elem, res_elem) {
                (SIMDElem::Int(src_int), SIMDElem::Int(dst_int)) => {
                    let extend = if src_int.is_signed() {
                        crate::cilnode::ExtendKind::SignExtend
                    } else {
                        crate::cilnode::ExtendKind::ZeroExtend
                    };
                    asm.int_cast(lane, dst_int, extend)
                }
                (SIMDElem::Int(src_int), SIMDElem::Float(dst_float)) => {
                    asm.float_cast(lane, dst_float, src_int.is_signed())
                }
                (SIMDElem::Float(_), SIMDElem::Int(dst_int)) => {
                    // float -> int: cilly's IntCast handles a float source. The ExtendKind
                    // selects the conv opcode's signedness (conv.i* vs conv.u*), so it MUST
                    // track the destination lane's signedness — otherwise a signed `f32 -> i32`
                    // lane would emit `conv.u4` and miscompile negative values. Matches the
                    // sign-selection in `src/casts.rs::to_int`.
                    let extend = if dst_int.is_signed() {
                        crate::cilnode::ExtendKind::SignExtend
                    } else {
                        crate::cilnode::ExtendKind::ZeroExtend
                    };
                    asm.int_cast(lane, dst_int, extend)
                }
                (SIMDElem::Float(_), SIMDElem::Float(dst_float)) => {
                    asm.float_cast(lane, dst_float, true)
                }
            };
            let res_ptr = asm.cast_ptr(res_ptr, res_elem_tpe);
            let res_ptr = asm.offset(res_ptr, Const::USize(idx as u64), res_elem_tpe);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((
                res_ptr,
                converted,
                res_elem_tpe,
                false,
            )))));
        }
        let ret = asm.alloc_node(CILNode::LdLoc(0));
        roots.push(asm.alloc_root(CILRoot::Ret(ret)));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals: vec![(None, asm.alloc_type(res))],
        }
    };
    patcher.insert(name, Box::new(generator));
}
