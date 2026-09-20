use crate::{
    Assembly, BasicBlock, BinOp, CILNode, CILRoot, Const, Float, Int, Interned, MethodImpl,
    MethodRef, Type, asm::MissingMethodPatcher, tpe::simd::SIMDElem,
};

/// Build a scalar operator call for a `System.Half` lane.
///
/// `System.Half` is a value type rather than one of the ECMA-335 native floating-point stack
/// types.  Consequently a raw CIL `add`/`div`/`ceq` on an f16 lane is not a valid fallback: the
/// operation must go through the operator method that the BCL exposes on `Half`.  The vector
/// intrinsics currently reject `VectorN<Half>` on the supported CoreCLR runtime, so all f16 SIMD
/// fallbacks use this helper while f32/f64 continue to use native CIL operators.
pub(super) fn half_binop(
    asm: &mut Assembly,
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    op: BinOp,
    output: Type,
) -> Interned<CILNode> {
    let half = Float::F16.class(asm);
    let half = asm[half].clone();
    let sig = [Type::Float(Float::F16), Type::Float(Float::F16)];
    let method = half.static_mref(&sig, output, asm.alloc_string(op.dotnet_name()), asm);
    asm.alloc_node(CILNode::call(method, [lhs, rhs]))
}

/// Build a scalar unary operator call for a `System.Half` lane.  The ECMA-335 `neg` instruction
/// only accepts the native `float32`/`float64` stack types, so f16 negation must use Half's
/// overloaded `op_UnaryNegation` method as well.
pub(super) fn half_unop(
    asm: &mut Assembly,
    value: Interned<CILNode>,
    name: &str,
    output: Type,
) -> Interned<CILNode> {
    let half = Float::F16.class(asm);
    let half = asm[half].clone();
    let method = half.static_mref(
        &[Type::Float(Float::F16)],
        output,
        asm.alloc_string(name),
        asm,
    );
    asm.alloc_node(CILNode::call(method, [value]))
}

pub(super) fn float_lane_binop(
    asm: &mut Assembly,
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    elem: SIMDElem,
    op: BinOp,
    output: Type,
) -> Interned<CILNode> {
    match elem {
        SIMDElem::Float(Float::F16) => half_binop(asm, lhs, rhs, op, output),
        _ => asm.biop(lhs, rhs, op),
    }
}
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
                    return lane_binop_body(mref, asm, &|asm, l, r, elem, res_elem| {
                        float_lane_binop(asm, l, r, elem, $binop, res_elem)
                    });
                };
                // CoreCLR currently rejects the generic `VectorN<Half>` surface at runtime
                // (`NotSupportedException` from `VectorN<Half>.Count`/the operation itself).
                // Keep the managed-vector representation, but lower f16 values per lane through
                // `System.Half` operators just like the non-vector fallback.
                if matches!(comparands.elem(), SIMDElem::Float(Float::F16)) {
                    return lane_binop_body(mref, asm, &|asm, l, r, elem, res_elem| {
                        float_lane_binop(asm, l, r, elem, $binop, res_elem)
                    });
                }
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
/// (see the backend's `type::get_type`). The array fallback is a class with a single
/// field `f0` of the element type, sized to hold `count` contiguous elements, so the lane count is
/// `byte_size / sizeof(element)`. The two reps share an identical contiguous memory layout, which
/// is exactly why the spill-and-index per-lane body below works unchanged on either.
pub(super) fn simd_lane_info(tpe: Type, asm: &Assembly) -> Option<(SIMDElem, u64)> {
    if let Some(v) = tpe.as_simdvector() {
        return Some((v.elem(), u64::from(v.count())));
    }
    // A 1-lane vector (`Simd<T, 1>`) is lowered straight to its scalar element (see `get_type`'s
    // `count == 1` early return), so a SIMD op on it arrives with a plain `Int`/`Float` operand —
    // treat that as a single-lane vector.
    match tpe {
        Type::Int(int) => return Some((SIMDElem::Int(int), 1)),
        Type::Float(float) => return Some((SIMDElem::Float(float), 1)),
        _ => {}
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

fn scalar_return_body(asm: &mut Assembly, value: Interned<CILNode>) -> MethodImpl {
    let ret = asm.alloc_root(CILRoot::Ret(value));
    MethodImpl::MethodBody {
        blocks: vec![BasicBlock::new(vec![ret], 0, None)],
        locals: vec![],
    }
}

/// Per-lane spill-and-index body for an element-wise binary op: read lane `i` of each input
/// (reinterpreting the operand address as `*elem`), apply `op`, store to result lane `i`. Works for
/// BOTH `SIMDVector` operands and the array fallback (same memory layout) via `simd_lane_info`, so
/// the .NET fast-path ops can delegate here for unsupported vector sizes.
pub(super) fn lane_binop_body(
    mref: Interned<MethodRef>,
    asm: &mut Assembly,
    op: &dyn Fn(
        &mut Assembly,
        Interned<CILNode>,
        Interned<CILNode>,
        SIMDElem,
        Type,
    ) -> Interned<CILNode>,
) -> MethodImpl {
    let sig = asm[asm[mref].sig()].clone();
    let res = *sig.output();
    let (res_elem_s, _) = simd_lane_info(res, asm).expect("simd binop result is not a vector");
    let res_elem: Type = res_elem_s.into();
    let (elem, count) =
        simd_lane_info(sig.inputs()[0], asm).expect("simd binop input is not a vector");
    // `Simd<T, 1>` is represented as the scalar `T` by the Rust type lowering.  Taking an address
    // of a scalar `Half` argument and reinterpreting it as an element pointer is not a valid ABI
    // operation on all CoreCLR call paths (and can become an access violation), so use a direct
    // scalar call for this shape.  Multi-lane managed vectors and fixed-array fallbacks retain the
    // spill-and-index implementation below.
    if count == 1
        && matches!(sig.inputs()[0], Type::Int(_) | Type::Float(_))
        && matches!(res, Type::Int(_) | Type::Float(_))
    {
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let value = op(asm, lhs, rhs, elem, res_elem);
        return scalar_return_body(asm, value);
    }
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
    if count == 1
        && matches!(sig.inputs()[0], Type::Int(_) | Type::Float(_))
        && matches!(res, Type::Int(_) | Type::Float(_))
    {
        let value = asm.alloc_node(CILNode::LdArg(0));
        let value = op(asm, value, elem, res_elem);
        return scalar_return_body(asm, value);
    }
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
    // `Simd<T, 1>` is lowered to the scalar `T`.  Pointer vectors use this path for their
    // one-lane instantiations; there is no aggregate local to spill, so the splat is simply the
    // scalar value itself.
    if matches!(
        res,
        Type::Ptr(_) | Type::Ref(_) | Type::Int(_) | Type::Float(_)
    ) {
        let value = asm.alloc_node(CILNode::LdArg(0));
        return scalar_return_body(asm, value);
    }
    // Pointer vectors are represented by the fixed-array fallback because raw pointers are not
    // valid CLR `Vector<T>` element types.  Recover their field type directly instead of limiting
    // splat to `SIMDElem` (ints/floats); this is needed by portable-simd's pointer helpers, which
    // construct `Simd<*const T, N>` from a scalar pointer before exercising pointer intrinsics.
    let (res_elem, count) = if let Some((elem, count)) = simd_lane_info(res, asm) {
        (Type::from(elem), count)
    } else {
        let Type::ClassRef(cref) = res else {
            panic!("simd splat result is not a vector")
        };
        let def = asm
            .class_ref_to_def(cref)
            .expect("simd splat array fallback has no class definition");
        let def = &asm[def];
        let (elem, _, _) = *def
            .fields()
            .first()
            .expect("simd splat array fallback has no element field");
        let total = u64::from(
            def.explict_size()
                .expect("simd splat array fallback has no explicit size")
                .get(),
        );
        let elem_size = u64::from(asm.sizeof_type(elem));
        assert!(
            elem_size != 0,
            "simd splat array fallback has a zero-sized element"
        );
        (elem, total / elem_size)
    };
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
        let scalar_cmp = |asm: &mut Assembly,
                          lhs: Interned<CILNode>,
                          rhs: Interned<CILNode>,
                          op: BinOp|
         -> Interned<CILNode> {
            float_lane_binop(asm, lhs, rhs, elem, op, Type::Bool)
        };
        let cmp01 = match kind {
            CmpKind::Eq => scalar_cmp(asm, l, r, BinOp::Eq),
            CmpKind::Lt => scalar_cmp(asm, l, r, lt),
            CmpKind::Gt => scalar_cmp(asm, l, r, gt),
            // Scalar comparison nodes produce `bool`, not an integer 0/1.
            // Comparing that bool to an i32 zero is rejected by the verifier
            // for f16 lanes. Compare it with a bool false instead; this is the
            // ordered complement required for `>=`/`<=` (and keeps NaN
            // unordered as false) without relying on integer-only `Not`.
            CmpKind::Ge => {
                if matches!(elem, SIMDElem::Float(Float::F16)) {
                    // Half's ordered operators return false for NaN, so `>=` must be expressed
                    // as `>` OR `==`; complementing `<` would incorrectly accept unordered lanes.
                    let gt = scalar_cmp(asm, l, r, gt);
                    let eq = scalar_cmp(asm, l, r, BinOp::Eq);
                    asm.biop(gt, eq, BinOp::Or)
                } else {
                    let lt = scalar_cmp(asm, l, r, lt);
                    let false_value = asm.alloc_node(Const::Bool(false));
                    asm.biop(lt, false_value, BinOp::Eq)
                }
            }
            CmpKind::Le => {
                if matches!(elem, SIMDElem::Float(Float::F16)) {
                    let lt = scalar_cmp(asm, l, r, lt);
                    let eq = scalar_cmp(asm, l, r, BinOp::Eq);
                    asm.biop(lt, eq, BinOp::Or)
                } else {
                    let gt = scalar_cmp(asm, l, r, gt);
                    let false_value = asm.alloc_node(Const::Bool(false));
                    asm.biop(gt, false_value, BinOp::Eq)
                }
            }
        };
        // `cmp01` is 0/1 (i32). Widen to the mask lane width, then negate: 0 -> 0, 1 -> all-ones.
        let widened = asm.int_cast(
            cmp01,
            res_elem
                .as_int()
                .expect("simd mask element must be an integer"),
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
        let eq = float_lane_binop(asm, a, b, elem, BinOp::Eq, Type::Bool);
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
    op: impl Fn(
        &mut Assembly,
        Interned<CILNode>,
        Interned<CILNode>,
        SIMDElem,
        Type,
    ) -> Interned<CILNode>
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
/// Per-lane value SIMD operations used by the .NET builtin set.
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
        |asm, lhs, rhs, elem, res_elem| {
            let op = match elem {
                SIMDElem::Int(int) if !int.is_signed() => BinOp::DivUn,
                _ => BinOp::Div,
            };
            float_lane_binop(asm, lhs, rhs, elem, op, res_elem)
        },
        "simd_div",
        asm,
        patcher,
    );
    // `simd_rem` — element-wise remainder. Like `simd_div`, pick signed/unsigned/float remainder
    // from the lane type (`Rem` for signed/float, `RemUn` for unsigned).
    simd_binop(
        |asm, lhs, rhs, elem, res_elem| {
            let op = match elem {
                SIMDElem::Int(int) if !int.is_signed() => BinOp::RemUn,
                _ => BinOp::Rem,
            };
            float_lane_binop(asm, lhs, rhs, elem, op, res_elem)
        },
        "simd_rem",
        asm,
        patcher,
    );
    // `simd_maximum_number_nsz` / `simd_minimum_number_nsz` — element-wise IEEE maximumNumber/
    // minimumNumber (NaN-ignoring) on float lanes; the `_nsz` (no-signed-zero) is an optimization
    // hint we can safely ignore. Per-lane `System.{Single,Double,Half}.MaxNumber`/`MinNumber`.
    simd_binop(
        |asm, l, r, elem, _| match elem {
            SIMDElem::Float(f) => f.math2(l, r, asm, "MaxNumber"),
            SIMDElem::Int(_) => asm.biop(l, r, BinOp::Add), // unreachable: float-only op
        },
        "simd_maximum_number_nsz",
        asm,
        patcher,
    );
    simd_binop(
        |asm, l, r, elem, _| match elem {
            SIMDElem::Float(f) => f.math2(l, r, asm, "MinNumber"),
            SIMDElem::Int(_) => asm.biop(l, r, BinOp::Add), // unreachable: float-only op
        },
        "simd_minimum_number_nsz",
        asm,
        patcher,
    );
    // `simd_cast<T,U>` — per-lane numeric conversion. Not a binop (single input vector), so it
    // has its own generator that walks lanes, converting each `src_elem` to `dst_elem`.
    simd_cast(asm, patcher);
    // `simd_select` (mask-driven blend) and the `simd_reduce_*` horizontal reductions are per-lane
    // and target-agnostic, so they live on both the .NET and C builtin sets.
    simd_select(asm, patcher);
    simd_reduce(
        "simd_reduce_add_ordered",
        ReduceKind::Add,
        true,
        asm,
        patcher,
    );
    simd_reduce(
        "simd_reduce_add_unordered",
        ReduceKind::Add,
        false,
        asm,
        patcher,
    );
    simd_reduce(
        "simd_reduce_mul_ordered",
        ReduceKind::Mul,
        true,
        asm,
        patcher,
    );
    simd_reduce(
        "simd_reduce_mul_unordered",
        ReduceKind::Mul,
        false,
        asm,
        patcher,
    );
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
                ReduceKind::Add => float_lane_binop(asm, acc, lane, elem_s, BinOp::Add, elem),
                ReduceKind::Mul => float_lane_binop(asm, acc, lane, elem_s, BinOp::Mul, elem),
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
                    let take_lane = if matches!(elem_s, SIMDElem::Float(Float::F16)) {
                        // `Half` is a value type and cannot be compared with raw CIL `clt`/`cgt`.
                        // Use the BCL operator, preserving the ordered NaN behavior expected by
                        // the scalar `f16::min`/`max` implementations.
                        half_binop(asm, lane_v, acc_v, cmp, Type::Bool)
                    } else {
                        asm.biop(lane_v, acc_v, cmp)
                    };
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
                (SIMDElem::Float(src_float), SIMDElem::Int(dst_int)) => {
                    // Rust `as` float->int SATURATES (NaN->0, clamp to [MIN, MAX]); a bare `conv.i*`
                    // truncates and is undefined on overflow (e.g. a large-negative `f32 -> i8`
                    // wraps to 0 instead of -128). Reuse the per-`(float, int)` saturating
                    // `cast_<f>_<i>` builtins that the SCALAR cast (`src/casts.rs::float_to_int`)
                    // uses, applied per lane. f16/f128 sources and i128/u128 targets have no such
                    // builtin — keep the plain conv there (rare in SIMD).
                    let has_builtin = matches!(src_float, crate::Float::F32 | crate::Float::F64)
                        && !matches!(dst_int, Int::I128 | Int::U128);
                    if has_builtin {
                        let name = asm.alloc_string(format!(
                            "cast_{}_{}",
                            src_float.name(),
                            dst_int.name()
                        ));
                        let main_module = *asm.main_module();
                        let cast_mref = asm.class_ref(main_module).clone().static_mref(
                            &[src_elem_tpe],
                            res_elem_tpe,
                            name,
                            asm,
                        );
                        asm.alloc_node(CILNode::call(cast_mref, [lane]))
                    } else {
                        let extend = if dst_int.is_signed() {
                            crate::cilnode::ExtendKind::SignExtend
                        } else {
                            crate::cilnode::ExtendKind::ZeroExtend
                        };
                        asm.int_cast(lane, dst_int, extend)
                    }
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
