use crate::{
    asm::MissingMethodPatcher, tpe::simd::SIMDElem, Assembly, BasicBlock, BinOp, CILNode, CILRoot,
    Const, Int, Interned, MethodImpl, MethodRef, Type,
};
macro_rules! binop {
    ($op_name:ident,$op_dotnet:literal,$binop:expr) => {
        pub fn $op_name(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
            let name = asm.alloc_string(stringify!($op_name));
            let generator = move |mref: $crate::ir::Interned<$crate::ir::MethodRef>,
                                  asm: &mut Assembly| {
                let sig = asm[asm[mref].sig()].clone();

                let Some(comparands) = sig.inputs()[0].as_simdvector() else {
                    // Array fallback: an unsupported vector size (sub-64-bit / >512 / non-power-of-2)
                    // has no managed `Vector{bits}` class, so lower the op per lane instead.
                    return lane_binop_body(mref, asm, &|asm, l, r, _, _| asm.biop(l, r, $binop));
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
binop!(simd_or, "BitwiseOr", BinOp::Or);
binop!(simd_add, "Add", BinOp::Add);
binop!(simd_and, "BitwiseAnd", BinOp::And);
binop!(simd_sub, "Subtract", BinOp::Sub);
binop!(simd_mul, "Multiply", BinOp::Mul);
// NOTE: `simd_div` is NOT a `binop!`: `System.Runtime.Intrinsics.Vector{bits}` exposes no generic
// static `Divides`/`Divide<T>` for every element type (the old `"Divides"` mapping would
// `MissingMethodException`). It is lowered per-lane instead — see `register_value_lane_ops`.
/// Recover `(element, lane count)` for a SIMD operand whose lowered type is EITHER a
/// `Type::SIMDVector` (the managed-vector case for 64/128/256/512-bit widths) OR the fixed-array
/// fallback used for unsupported sizes — sub-64-bit (`Simd<i8,4>`), >512-bit, or non-power-of-two
/// (see `rustc_codegen_clr_type::r#type::get_type`). The array fallback is a class with a single
/// field `f0` of the element type, sized to hold `count` contiguous elements, so the lane count is
/// `byte_size / sizeof(element)`. The two reps share an identical contiguous memory layout, which
/// is exactly why the spill-and-index per-lane body below works unchanged on either.
pub(super) fn simd_lane_info(tpe: Type, asm: &Assembly) -> Option<(SIMDElem, u64)> {
    if let Some(v) = tpe.as_simdvector() {
        return Some((v.elem(), u64::from(v.count())));
    }
    let Type::ClassRef(cref) = tpe else {
        return None;
    };
    let def = asm.class_ref_to_def(cref)?;
    let def = &asm[def];
    let (elem_tpe, _, _) = *def.fields().first()?;
    let elem: SIMDElem = elem_tpe.try_into().ok()?;
    let total = u64::from(def.explict_size()?.get());
    let elem_size = u64::from(asm.sizeof_type(elem_tpe));
    if elem_size == 0 {
        return None;
    }
    Some((elem, total / elem_size))
}

/// Per-lane spill-and-index body for an element-wise binary op: read lane `i` of each input
/// (reinterpreting the operand address as `*elem`), apply `op`, store to result lane `i`. Works for
/// BOTH `SIMDVector` operands and the array fallback (same memory layout) via `simd_lane_info`, so
/// the .NET fast-path ops can delegate here for unsupported vector sizes.
pub(super) fn lane_binop_body(
    mref: Interned<MethodRef>,
    asm: &mut Assembly,
    op: &dyn Fn(&mut Assembly, Interned<CILNode>, Interned<CILNode>, SIMDElem, Type) -> Interned<CILNode>,
) -> MethodImpl {
    let sig = asm[asm[mref].sig()].clone();
    let res = *sig.output();
    let (res_elem_s, _) = simd_lane_info(res, asm).expect("simd binop result is not a vector");
    let res_elem: Type = res_elem_s.into();
    let (elem, count) =
        simd_lane_info(sig.inputs()[0], asm).expect("simd binop input is not a vector");
    let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
    let tpe: Type = elem.into();
    let tpe_idx = asm.alloc_type(tpe);
    let lhs = asm.alloc_node(CILNode::LdArgA(0));
    let rhs = asm.alloc_node(CILNode::LdArgA(1));
    let lhs = asm.cast_ptr(lhs, tpe);
    let rhs = asm.cast_ptr(rhs, tpe);
    let mut roots = vec![];
    for idx in 0..count {
        let lhs = asm.offset(lhs, Const::USize(idx), tpe);
        let rhs = asm.offset(rhs, Const::USize(idx), tpe);
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
        let res_ptr = asm.offset(res_ptr, Const::USize(idx), res_elem);
        let res = op(asm, lhs, rhs, elem, res_elem);
        roots.push(asm.alloc_root(CILRoot::StInd(Box::new((res_ptr, res, res_elem, false)))));
    }
    let ret = asm.alloc_node(CILNode::LdLoc(0));
    roots.push(asm.alloc_root(CILRoot::Ret(ret)));
    MethodImpl::MethodBody {
        blocks: vec![BasicBlock::new(roots, 0, None)],
        locals: vec![(None, asm.alloc_type(res))],
    }
}

/// Per-lane spill-and-index body for a unary op (`(vec) -> vec`): read lane `i`, apply `op`, store
/// to result lane `i`. The array-fallback counterpart of the BCL-static unops (`simd_neg`, …).
pub(super) fn lane_unop_body(
    mref: Interned<MethodRef>,
    asm: &mut Assembly,
    op: &dyn Fn(&mut Assembly, Interned<CILNode>, SIMDElem, Type) -> Interned<CILNode>,
) -> MethodImpl {
    let sig = asm[asm[mref].sig()].clone();
    let res = *sig.output();
    let (res_elem_s, _) = simd_lane_info(res, asm).expect("simd unop result is not a vector");
    let res_elem: Type = res_elem_s.into();
    let (elem, count) =
        simd_lane_info(sig.inputs()[0], asm).expect("simd unop input is not a vector");
    let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
    let tpe: Type = elem.into();
    let tpe_idx = asm.alloc_type(tpe);
    let src = asm.alloc_node(CILNode::LdArgA(0));
    let src = asm.cast_ptr(src, tpe);
    let mut roots = vec![];
    for idx in 0..count {
        let slot = asm.offset(src, Const::USize(idx), tpe);
        let lane = asm.alloc_node(CILNode::LdInd {
            addr: slot,
            tpe: tpe_idx,
            volatile: false,
        });
        let res_ptr = asm.cast_ptr(res_ptr, res_elem);
        let res_ptr = asm.offset(res_ptr, Const::USize(idx), res_elem);
        let res = op(asm, lane, elem, res_elem);
        roots.push(asm.alloc_root(CILRoot::StInd(Box::new((res_ptr, res, res_elem, false)))));
    }
    let ret = asm.alloc_node(CILNode::LdLoc(0));
    roots.push(asm.alloc_root(CILRoot::Ret(ret)));
    MethodImpl::MethodBody {
        blocks: vec![BasicBlock::new(roots, 0, None)],
        locals: vec![(None, asm.alloc_type(res))],
    }
}

/// Per-lane body for `simd_vec_from_val` (splat): store the scalar `LdArg(0)` into every lane of the
/// result. The array-fallback counterpart of the BCL `Vector{bits}.Create(scalar)`.
pub(super) fn lane_splat_body(mref: Interned<MethodRef>, asm: &mut Assembly) -> MethodImpl {
    let sig = asm[asm[mref].sig()].clone();
    let res = *sig.output();
    let (res_elem_s, count) = simd_lane_info(res, asm).expect("simd splat result is not a vector");
    let res_elem: Type = res_elem_s.into();
    let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
    let mut roots = vec![];
    for idx in 0..count {
        let val = asm.alloc_node(CILNode::LdArg(0));
        let res_ptr = asm.cast_ptr(res_ptr, res_elem);
        let slot = asm.offset(res_ptr, Const::USize(idx), res_elem);
        roots.push(asm.alloc_root(CILRoot::StInd(Box::new((slot, val, res_elem, false)))));
    }
    let ret = asm.alloc_node(CILNode::LdLoc(0));
    roots.push(asm.alloc_root(CILRoot::Ret(ret)));
    MethodImpl::MethodBody {
        blocks: vec![BasicBlock::new(roots, 0, None)],
        locals: vec![(None, asm.alloc_type(res))],
    }
}

#[derive(Clone, Copy)]
pub(super) enum CmpKind {
    Eq,
    Lt,
    Gt,
    Ge,
    Le,
}

/// Per-lane comparison producing an all-ones/zero mask lane — matching both the BCL `Vector`
/// comparisons and Rust's `Mask` representation — for the array fallback of
/// `simd_eq`/`lt`/`gt`/`ge`/`le`. Reuses `lane_binop_body`; signedness of `<`/`>` is taken from the
/// (input) lane type, and `>=`/`<=` are `!(<)`/`!(>)`. The result lane type is the mask element.
pub(super) fn lane_cmp_body(
    mref: Interned<MethodRef>,
    asm: &mut Assembly,
    kind: CmpKind,
) -> MethodImpl {
    lane_binop_body(mref, asm, &move |asm, l, r, elem, res_elem| {
        let unsigned = matches!(elem, SIMDElem::Int(i) if !i.is_signed());
        let lt = if unsigned { BinOp::LtUn } else { BinOp::Lt };
        let gt = if unsigned { BinOp::GtUn } else { BinOp::Gt };
        let cmp01 = match kind {
            CmpKind::Eq => asm.biop(l, r, BinOp::Eq),
            CmpKind::Lt => asm.biop(l, r, lt),
            CmpKind::Gt => asm.biop(l, r, gt),
            CmpKind::Ge => {
                let lt = asm.biop(l, r, lt);
                let zero = asm.alloc_node(Const::I32(0));
                asm.biop(lt, zero, BinOp::Eq)
            }
            CmpKind::Le => {
                let gt = asm.biop(l, r, gt);
                let zero = asm.alloc_node(Const::I32(0));
                asm.biop(gt, zero, BinOp::Eq)
            }
        };
        // `cmp01` is 0/1 (i32). Widen to the mask lane width, then negate: 0 -> 0, 1 -> all-ones.
        let widened = asm.int_cast(
            cmp01,
            res_elem.as_int().expect("simd mask element must be an integer"),
            crate::cilnode::ExtendKind::ZeroExtend,
        );
        asm.neg(widened)
    })
}

/// Per-lane body for `simd_eq_all`/`simd_eq_any` (`(vec, vec) -> bool`): fold `a[i] == b[i]` across
/// lanes with `&&` (all) or `||` (any). Array fallback for the BCL `EqualsAll`/`EqualsAny`.
pub(super) fn lane_all_any_body(
    mref: Interned<MethodRef>,
    asm: &mut Assembly,
    all: bool,
) -> MethodImpl {
    let sig = asm[asm[mref].sig()].clone();
    let (elem, count) =
        simd_lane_info(sig.inputs()[0], asm).expect("simd_eq_all/any input is not a vector");
    let tpe: Type = elem.into();
    let tpe_idx = asm.alloc_type(tpe);
    let lhs = asm.alloc_node(CILNode::LdArgA(0));
    let lhs = asm.cast_ptr(lhs, tpe);
    let rhs = asm.alloc_node(CILNode::LdArgA(1));
    let rhs = asm.cast_ptr(rhs, tpe);
    let acc_addr = asm.alloc_node(CILNode::LdLocA(0));
    // `all` seeds true and ANDs; `any` seeds false and ORs.
    let seed = asm.alloc_node(Const::Bool(all));
    let mut roots = vec![asm.alloc_root(CILRoot::StInd(Box::new((
        acc_addr,
        seed,
        Type::Bool,
        false,
    ))))];
    for idx in 0..count {
        let a = asm.offset(lhs, Const::USize(idx), tpe);
        let a = asm.alloc_node(CILNode::LdInd {
            addr: a,
            tpe: tpe_idx,
            volatile: false,
        });
        let b = asm.offset(rhs, Const::USize(idx), tpe);
        let b = asm.alloc_node(CILNode::LdInd {
            addr: b,
            tpe: tpe_idx,
            volatile: false,
        });
        let eq = asm.biop(a, b, BinOp::Eq);
        let acc = asm.alloc_node(CILNode::LdLoc(0));
        let new_acc = asm.biop(acc, eq, if all { BinOp::And } else { BinOp::Or });
        roots.push(asm.alloc_root(CILRoot::StInd(Box::new((
            acc_addr,
            new_acc,
            Type::Bool,
            false,
        )))));
    }
    let ret = asm.alloc_node(CILNode::LdLoc(0));
    roots.push(asm.alloc_root(CILRoot::Ret(ret)));
    MethodImpl::MethodBody {
        blocks: vec![BasicBlock::new(roots, 0, None)],
        locals: vec![(None, asm.alloc_type(Type::Bool))],
    }
}

fn simd_binop(
    op: impl Fn(&mut Assembly, Interned<CILNode>, Interned<CILNode>, SIMDElem, Type) -> Interned<CILNode>
        + 'static,
    name: &str,
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
) {
    let name = asm.alloc_string(name);
    let generator =
        move |mref: Interned<MethodRef>, asm: &mut Assembly| lane_binop_body(mref, asm, &op);
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
    // Multiply is missing from the .NET BCL set on the C path; provide a per-lane version too.
    simd_binop(
        |asm, lhs, rhs, _, _| asm.biop(lhs, rhs, BinOp::Mul),
        "simd_mul",
        asm,
        patcher,
    );
    // The remaining per-lane "value" ops (xor/shl/shr/div/cast) are shared with the .NET path.
    register_value_lane_ops(asm, patcher);
}

/// Per-lane "value" SIMD ops shared by BOTH the .NET (`simd`) and C (`fallback_simd`) builtin sets:
/// plain element-wise transforms with no mask-convention subtlety, so the target-agnostic
/// spill-and-index loop is correct on either target. (Comparisons and basic arithmetic are
/// registered per-path: the .NET path uses the BCL `Vector` statics for all-ones masks / hardware
/// SIMD; the C path uses the scalar loops in `fallback_simd`.)
pub(super) fn register_value_lane_ops(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
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
    // `simd_div` — element-wise division. Pick signed/unsigned/float division from the lane type:
    // `BinOp` distinguishes `Div` (signed/float) from `DivUn` (unsigned), so unsigned lanes must use
    // `DivUn` to avoid a signed-division miscompile.
    simd_binop(
        |asm, lhs, rhs, elem, _| {
            let op = match elem {
                SIMDElem::Int(int) if !int.is_signed() => BinOp::DivUn,
                _ => BinOp::Div,
            };
            asm.biop(lhs, rhs, op)
        },
        "simd_div",
        asm,
        patcher,
    );
    // `simd_cast<T,U>` — per-lane numeric conversion. Not a binop (single input vector), so it
    // has its own generator that walks lanes, converting each `src_elem` to `dst_elem`.
    simd_cast(asm, patcher);
    // `simd_select` (mask-driven blend) and the `simd_reduce_*` horizontal reductions are per-lane
    // and target-agnostic, so they live on both the .NET and C builtin sets.
    simd_select(asm, patcher);
    simd_reduce("simd_reduce_add_ordered", ReduceKind::Add, true, asm, patcher);
    simd_reduce("simd_reduce_add_unordered", ReduceKind::Add, false, asm, patcher);
    simd_reduce("simd_reduce_mul_ordered", ReduceKind::Mul, true, asm, patcher);
    simd_reduce("simd_reduce_mul_unordered", ReduceKind::Mul, false, asm, patcher);
    simd_reduce("simd_reduce_and", ReduceKind::And, false, asm, patcher);
    simd_reduce("simd_reduce_or", ReduceKind::Or, false, asm, patcher);
    simd_reduce("simd_reduce_xor", ReduceKind::Xor, false, asm, patcher);
    simd_reduce("simd_reduce_min", ReduceKind::Min, false, asm, patcher);
    simd_reduce("simd_reduce_max", ReduceKind::Max, false, asm, patcher);
    // The SIMD "tail": `simd_shuffle`, per-lane integer bit ops (ctlz/cttz/ctpop/bswap/bitreverse),
    // float rounders (sqrt/floor/ceil/trunc/round/round_ties_even), and fma. All target-agnostic
    // per-lane bodies, so they serve both the .NET and C builtin sets.
    super::tail::register_tail_ops(asm, patcher);
}

/// `simd_select<M, T>(mask: M, if_true: T, if_false: T) -> T`: per-lane
/// `mask[i] != 0 ? if_true[i] : if_false[i]`. The element type may be a float, which the IR's *value*
/// `select` does not support — so we select the source *address* per lane (`select` supports pointer
/// operands) and load through it. Rust masks are all-ones/zero; `!= 0` is expressed as
/// `(lane == 0) ? if_false : if_true` because `BinOp` has no `Ne`.
fn simd_select(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_select");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        let (elem_s, count) = simd_lane_info(res, asm).expect("simd_select result is not a vector");
        let elem: Type = elem_s.into();
        let (mask_elem_s, _) =
            simd_lane_info(sig.inputs()[0], asm).expect("simd_select mask is not a vector");
        let mask_elem: Type = mask_elem_s.into();
        let elem_idx = asm.alloc_type(elem);
        let mask_idx = asm.alloc_type(mask_elem);
        let elem_ptr_ty = asm.nptr(elem_idx);

        let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
        let res_ptr = asm.cast_ptr(res_ptr, elem);
        let mask = asm.alloc_node(CILNode::LdArgA(0));
        let mask = asm.cast_ptr(mask, mask_elem);
        let a = asm.alloc_node(CILNode::LdArgA(1));
        let a = asm.cast_ptr(a, elem);
        let b = asm.alloc_node(CILNode::LdArgA(2));
        let b = asm.cast_ptr(b, elem);

        let mut roots = vec![];
        for idx in 0..count {
            let m_slot = asm.offset(mask, Const::USize(idx), mask_elem);
            let m_val = asm.load(m_slot, mask_idx);
            let m_i32 = asm.int_cast(m_val, Int::I32, crate::cilnode::ExtendKind::SignExtend);
            let zero = asm.alloc_node(Const::I32(0));
            // predicate true  => mask lane is zero => pick `if_false`.
            let is_false = asm.biop(m_i32, zero, BinOp::Eq);
            let a_slot = asm.offset(a, Const::USize(idx), elem);
            let b_slot = asm.offset(b, Const::USize(idx), elem);
            let chosen = asm.select(elem_ptr_ty, b_slot, a_slot, is_false);
            let val = asm.load(chosen, elem_idx);
            let r_slot = asm.offset(res_ptr, Const::USize(idx), elem);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((r_slot, val, elem, false)))));
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

#[derive(Clone, Copy)]
enum ReduceKind {
    Add,
    Mul,
    And,
    Or,
    Xor,
    Min,
    Max,
}

/// Horizontal reduction `simd_reduce_*<T, U>(x: T[, acc: U]) -> U`: fold all lanes of `x` with a
/// scalar operation. `ordered` reductions (`*_ordered`) seed the accumulator with the second
/// argument and fold left-to-right (for float bit-exactness); the rest seed with lane 0. `Min`/`Max`
/// fold via a per-lane compare + pointer-`select`, so they work for float lanes too.
fn simd_reduce(
    name: &'static str,
    kind: ReduceKind,
    ordered: bool,
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
) {
    let nm = asm.alloc_string(name);
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let (elem_s, count) =
            simd_lane_info(sig.inputs()[0], asm).expect("simd_reduce input is not a vector");
        let elem: Type = elem_s.into();
        let signed = match elem_s {
            SIMDElem::Int(int) => int.is_signed(),
            SIMDElem::Float(_) => true,
        };
        let elem_idx = asm.alloc_type(elem);

        let x = asm.alloc_node(CILNode::LdArgA(0));
        let x = asm.cast_ptr(x, elem);
        let acc_addr = asm.alloc_node(CILNode::LdLocA(0));

        let mut roots = vec![];
        // Seed the accumulator (local 0).
        let (init, start) = if ordered {
            (asm.alloc_node(CILNode::LdArg(1)), 0usize)
        } else {
            let slot0 = asm.offset(x, Const::USize(0u64), elem);
            (asm.load(slot0, elem_idx), 1usize)
        };
        roots.push(asm.alloc_root(CILRoot::StInd(Box::new((acc_addr, init, elem, false)))));
        for idx in start..(count as usize) {
            let slot = asm.offset(x, Const::USize(idx as u64), elem);
            let lane = asm.load(slot, elem_idx);
            let acc = asm.alloc_node(CILNode::LdLoc(0));
            let new_acc = match kind {
                ReduceKind::Add => asm.biop(acc, lane, BinOp::Add),
                ReduceKind::Mul => asm.biop(acc, lane, BinOp::Mul),
                ReduceKind::And => asm.biop(acc, lane, BinOp::And),
                ReduceKind::Or => asm.biop(acc, lane, BinOp::Or),
                ReduceKind::Xor => asm.biop(acc, lane, BinOp::XOr),
                ReduceKind::Min | ReduceKind::Max => {
                    // Spill the lane so it has an address, then pick &lane or &acc by comparison.
                    let lane_addr = asm.alloc_node(CILNode::LdLocA(1));
                    roots.push(
                        asm.alloc_root(CILRoot::StInd(Box::new((lane_addr, lane, elem, false)))),
                    );
                    let lane_v = asm.load(lane_addr, elem_idx);
                    let acc_v = asm.alloc_node(CILNode::LdLoc(0));
                    let cmp = match (kind, signed) {
                        (ReduceKind::Min, true) => BinOp::Lt,
                        (ReduceKind::Min, false) => BinOp::LtUn,
                        (_, true) => BinOp::Gt,
                        (_, false) => BinOp::GtUn,
                    };
                    let take_lane = asm.biop(lane_v, acc_v, cmp);
                    let ptr_ty = asm.nptr(elem_idx);
                    let chosen = asm.select(ptr_ty, lane_addr, acc_addr, take_lane);
                    asm.load(chosen, elem_idx)
                }
            };
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((acc_addr, new_acc, elem, false)))));
        }
        let ret = asm.alloc_node(CILNode::LdLoc(0));
        roots.push(asm.alloc_root(CILRoot::Ret(ret)));
        let locals = match kind {
            ReduceKind::Min | ReduceKind::Max => vec![(None, elem_idx), (None, elem_idx)],
            _ => vec![(None, elem_idx)],
        };
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals,
        }
    };
    patcher.insert(nm, Box::new(generator));
}

/// Builtin generator for `simd_cast` / `simd_as`: a single-input per-lane numeric convert.
/// Mirrors `simd_binop`'s spill-and-index memory idiom, but reads from one source vector and
/// converts each lane to the destination element type (int<->int via `IntCast`, anything
/// touching floats via `FloatCast`).
fn simd_cast(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_cast");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        // Both the src and dst may be the array fallback (e.g. `mask.to_array()` casts an
        // `i32x4` mask to an `i8x4` `[bool; 4]`), so recover lane info via `simd_lane_info`.
        let (res_elem, _) = simd_lane_info(res, asm).expect("simd_cast result is not a vector");
        let res_elem_tpe: Type = res_elem.into();
        let (src_elem, count) =
            simd_lane_info(sig.inputs()[0], asm).expect("simd_cast input is not a vector");
        let src_elem_tpe: Type = src_elem.into();

        let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
        let src_tpe_idx = asm.alloc_type(src_elem_tpe);
        let src = asm.alloc_node(CILNode::LdArgA(0));
        let src = asm.cast_ptr(src, src_elem_tpe);
        let mut roots = vec![];
        for idx in 0..count {
            let slot = asm.offset(src, Const::USize(idx), src_elem_tpe);
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
            let res_ptr = asm.offset(res_ptr, Const::USize(idx), res_elem_tpe);
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
