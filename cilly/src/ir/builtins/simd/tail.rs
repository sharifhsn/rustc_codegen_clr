//! The "SIMD tail" builtins: per-lane scalar ops that have no guaranteed-correct generic-static BCL
//! `Vector{bits}` method, plus `simd_shuffle`. All of these use the target-agnostic spill-and-index
//! idiom (mirror `simd_binop`/`simd_cast`): cast the source/result vector locals to element pointers,
//! walk lanes, apply the scalar op per lane, and store into the result local. Because they touch no
//! mask convention and no BCL vector intrinsic, the same body is correct on both the .NET and C
//! targets, so they are all registered in `register_value_lane_ops`.
use crate::{
    Assembly, BasicBlock, BinOp, BranchCond, CILNode, CILRoot, ClassRef, Const, Float, Int,
    Interned, MethodImpl, MethodRef, Type,
    asm::MissingMethodPatcher,
    cilnode::{ExtendKind, IsPure, MethodKind},
    tpe::simd::SIMDElem,
};

/// Generic per-lane *unary* SIMD generator: `(vec) -> vec` where output lane `i` is
/// `op(asm, lane_i, src_elem)`. Mirrors `simd_cast`'s spill-and-index loop, but the lane transform
/// is supplied by the caller. The output element type equals the source element type for every op
/// wired through here (ctlz/cttz/ctpop/bswap/bitreverse and the float rounders), so we read and
/// write through the same element type.
fn simd_unary(
    op: impl Fn(&mut Assembly, Interned<CILNode>, SIMDElem) -> Interned<CILNode> + 'static,
    name: &str,
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
) {
    let name = asm.alloc_string(name);
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        let (src_elem, count) = super::binop::simd_lane_info(sig.inputs()[0], asm)
            .expect("simd value-lane unop input is not a vector");
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
            let transformed = op(asm, lane, src_elem);
            let res_ptr = asm.cast_ptr(res_ptr, src_elem_tpe);
            let res_ptr = asm.offset(res_ptr, Const::USize(idx), src_elem_tpe);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((
                res_ptr,
                transformed,
                src_elem_tpe,
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

/// Build a call to a static method `class::name(args) -> ret`.
fn static_call(
    asm: &mut Assembly,
    class: Interned<ClassRef>,
    name: &str,
    inputs: &[Type],
    output: Type,
    args: &[Interned<CILNode>],
) -> Interned<CILNode> {
    let name = asm.alloc_string(name);
    let sig = asm.sig(inputs.to_vec(), output);
    let mref = asm.alloc_methodref(MethodRef::new(
        class,
        name,
        sig,
        MethodKind::Static,
        vec![].into(),
    ));
    asm.call(mref, args, IsPure::NOT)
}

/// `simd_ctpop`: per-lane population count. `System.Numerics.BitOperations.PopCount` operates on
/// `u32`/`u64`; smaller lanes are zero-extended (their high bits are zero, so the count is exact).
/// Result lane has the same width as the source lane, so we narrow the count back down.
fn ctpop_lane(asm: &mut Assembly, lane: Interned<CILNode>, elem: SIMDElem) -> Interned<CILNode> {
    let SIMDElem::Int(int) = elem else {
        todo!("simd_ctpop on a float lane {elem:?}")
    };
    let bit_ops = ClassRef::bit_operations(asm);
    // Widen to the natural BitOperations width. `PopCount` returns `int` for both overloads.
    let wide = if int.bits().unwrap_or(64) > 32 {
        Int::U64
    } else {
        Int::U32
    };
    let widened = asm.int_cast(lane, wide, ExtendKind::ZeroExtend);
    let count = static_call(
        asm,
        bit_ops,
        "PopCount",
        &[Type::Int(wide)],
        Type::Int(Int::I32),
        &[widened],
    );
    // Narrow back to the source lane type.
    asm.int_cast(count, int, ExtendKind::ZeroExtend)
}

/// `simd_ctlz`: per-lane leading-zero count. `BitOperations.LeadingZeroCount` counts within a
/// 32/64-bit register; for a sub-word lane we widen, count, then subtract `(wide_bits - lane_bits)`
/// to correct for the extra high zero bits introduced by widening. Mirrors `ints::ctlz`.
fn ctlz_lane(asm: &mut Assembly, lane: Interned<CILNode>, elem: SIMDElem) -> Interned<CILNode> {
    let SIMDElem::Int(int) = elem else {
        todo!("simd_ctlz on a float lane {elem:?}")
    };
    let bit_ops = ClassRef::bit_operations(asm);
    let lane_bits = int.bits().unwrap_or(64) as i32;
    let (wide, wide_bits) = if lane_bits > 32 {
        (Int::U64, 64i32)
    } else {
        (Int::U32, 32i32)
    };
    let widened = asm.int_cast(lane, wide, ExtendKind::ZeroExtend);
    let raw = static_call(
        asm,
        bit_ops,
        "LeadingZeroCount",
        &[Type::Int(wide)],
        Type::Int(Int::I32),
        &[widened],
    );
    // raw counts leading zeros in `wide`; subtract the padding to get the lane's count.
    let corrected = if wide_bits == lane_bits {
        raw
    } else {
        let pad = asm.alloc_node(Const::I32(wide_bits - lane_bits));
        asm.biop(raw, pad, BinOp::Sub)
    };
    asm.int_cast(corrected, int, ExtendKind::ZeroExtend)
}

/// `simd_cttz`: per-lane trailing-zero count. Widening a sub-word lane to 32/64 bits introduces
/// high zero bits but does NOT change the trailing-zero count of a non-zero value. The only hazard
/// is the all-zero lane: `BitOperations.TrailingZeroCount(0)` returns the register width (32/64),
/// but Rust expects the lane width. We clamp the result to `lane_bits` via `Math.Min`. Mirrors
/// `ints::cttz`.
fn cttz_lane(asm: &mut Assembly, lane: Interned<CILNode>, elem: SIMDElem) -> Interned<CILNode> {
    let SIMDElem::Int(int) = elem else {
        todo!("simd_cttz on a float lane {elem:?}")
    };
    let bit_ops = ClassRef::bit_operations(asm);
    let lane_bits = int.bits().unwrap_or(64) as u32;
    let (wide, wide_bits) = if lane_bits > 32 {
        (Int::U64, 64u32)
    } else {
        (Int::U32, 32u32)
    };
    let widened = asm.int_cast(lane, wide, ExtendKind::ZeroExtend);
    let raw = static_call(
        asm,
        bit_ops,
        "TrailingZeroCount",
        &[Type::Int(wide)],
        Type::Int(Int::I32),
        &[widened],
    );
    let raw = asm.int_cast(raw, Int::U32, ExtendKind::ZeroExtend);
    // Clamp to the lane width: a zero lane reports `wide_bits` but must report `lane_bits`.
    let corrected = if wide_bits == lane_bits {
        raw
    } else {
        let math = ClassRef::math(asm);
        let cap = asm.alloc_node(Const::U32(lane_bits));
        static_call(
            asm,
            math,
            "Min",
            &[Type::Int(Int::U32), Type::Int(Int::U32)],
            Type::Int(Int::U32),
            &[raw, cap],
        )
    };
    asm.int_cast(corrected, int, ExtendKind::ZeroExtend)
}

/// `simd_bswap`: per-lane byte-swap via `BinaryPrimitives.ReverseEndianness`. A `u8`/`i8` lane is
/// the identity (single byte). For signed lanes we reverse the same-width unsigned representation
/// then reinterpret — `ReverseEndianness` is defined per-width, and the IR's `IntCast` between
/// same-width signed/unsigned is a no-op reinterpret.
fn bswap_lane(asm: &mut Assembly, lane: Interned<CILNode>, elem: SIMDElem) -> Interned<CILNode> {
    let SIMDElem::Int(int) = elem else {
        todo!("simd_bswap on a float lane {elem:?}")
    };
    // Single-byte lanes are unchanged.
    if matches!(int, Int::U8 | Int::I8) {
        return lane;
    }
    let bin_prim = ClassRef::binary_primitives(asm);
    static_call(
        asm,
        bin_prim,
        "ReverseEndianness",
        &[Type::Int(int)],
        Type::Int(int),
        &[lane],
    )
}

/// `simd_bitreverse`: per-lane bit reversal, reusing the scalar `bitreverse_<uN>` builtin bodies that
/// the non-SIMD `bitreverse` intrinsic registers (only `u32`/`u64`/`u128` exist). Sub-word lanes
/// (`u8`/`u16`) widen to `u32`, reverse the full 32-bit word, then logical-shift right by the padding
/// (`32 - lane_bits`) to bring the reversed lane bits into the low end — exactly the standard
/// sub-word bit-reverse. Reinterprets back to the lane's signedness afterwards.
fn bitreverse_lane(
    asm: &mut Assembly,
    lane: Interned<CILNode>,
    elem: SIMDElem,
) -> Interned<CILNode> {
    let SIMDElem::Int(int) = elem else {
        todo!("simd_bitreverse on a float lane {elem:?}")
    };
    let unsigned = int.as_unsigned();
    let lane_bits = int.bits().unwrap_or(64) as i32;
    let main = *asm.main_module();
    // Pick a backing reverse method that has a registered body (u32/u64/u128).
    let (work, work_bits) = match unsigned {
        Int::U8 | Int::U16 | Int::U32 => (Int::U32, 32i32),
        Int::U64 | Int::USize => (Int::U64, 64i32),
        Int::U128 => (Int::U128, 128i32),
        other => (other, lane_bits),
    };
    let sig = asm.sig([Type::Int(work)], Type::Int(work));
    let fn_name = format!("bitreverse_{}", work.name());
    let mref = asm.new_methodref(main, fn_name, sig, MethodKind::Static, vec![]);
    let widened = asm.int_cast(lane, work, ExtendKind::ZeroExtend);
    let reversed = asm.call(mref, &[widened], IsPure::NOT);
    // Bring the reversed sub-word bits down to the low end.
    let reversed = if work_bits == lane_bits {
        reversed
    } else {
        let pad = asm.alloc_node(Const::I32(work_bits - lane_bits));
        asm.biop(reversed, pad, BinOp::ShrUn)
    };
    asm.int_cast(reversed, int, ExtendKind::ZeroExtend)
}

/// Per-lane float unary call (`Floor`/`Ceiling`/`Truncate`/`Sqrt`, plus the `std_float` elementary
/// functions).  .NET exposes the same static surface on `System.Half` as on `MathF`/`Math`; using
/// it directly preserves the half-width rounding at each lane instead of widening through `f32`
/// and accidentally changing the Rust `f16` result.
fn float_unop_lane(
    asm: &mut Assembly,
    lane: Interned<CILNode>,
    elem: SIMDElem,
    method: &str,
) -> Interned<CILNode> {
    let SIMDElem::Float(float) = elem else {
        todo!("simd float op on an int lane {elem:?}")
    };
    let (class, ft) = match float {
        Float::F16 => (ClassRef::half(asm), Type::Float(Float::F16)),
        Float::F32 => (ClassRef::mathf(asm), Type::Float(Float::F32)),
        Float::F64 => (ClassRef::math(asm), Type::Float(Float::F64)),
        other => todo!("simd float {method} on {other:?}"),
    };
    // .NET 10 exposes `Exp2` on the primitive floating-point value types (`Single`/`Double` /
    // `Half`), while the corresponding `MathF.Exp2`/`Math.Exp2` method is absent from the runtime
    // shipped with the pinned SDK.  Select the primitive declaring type for this one API to avoid
    // emitting a metadata reference that resolves only against the reference pack.
    let class = if method == "Exp2" {
        match float {
            Float::F16 => ClassRef::half(asm),
            Float::F32 => ClassRef::single(asm),
            Float::F64 => ClassRef::double(asm),
            other => todo!("simd float {method} on {other:?}"),
        }
    } else {
        class
    };
    static_call(asm, class, method, &[ft], ft, &[lane])
}

/// Per-lane `Round`. `away` selects `MidpointRounding.AwayFromZero` (Rust `simd_round`) vs the
/// default banker's rounding (`simd_round_ties_even`). The `MidpointRounding` enum value is built by
/// reinterpreting the integer `1` (= `AwayFromZero`), mirroring `floats::roundf32`.
fn round_lane(
    asm: &mut Assembly,
    lane: Interned<CILNode>,
    elem: SIMDElem,
    away: bool,
) -> Interned<CILNode> {
    let SIMDElem::Float(float) = elem else {
        todo!("simd_round on an int lane {elem:?}")
    };
    let (class, ft) = match float {
        Float::F32 => (ClassRef::mathf(asm), Type::Float(Float::F32)),
        Float::F64 => (ClassRef::math(asm), Type::Float(Float::F64)),
        other => todo!("simd_round on {other:?}"),
    };
    if !away {
        // Banker's rounding: plain single-arg `Round`.
        return static_call(asm, class, "Round", &[ft], ft, &[lane]);
    }
    let rounding = ClassRef::midpoint_rounding(asm);
    let one = asm.alloc_node(Const::I32(1));
    let mode = asm.transmute_on_stack(Type::Int(Int::I32), Type::ClassRef(rounding), one);
    static_call(
        asm,
        class,
        "Round",
        &[ft, Type::ClassRef(rounding)],
        ft,
        &[lane, mode],
    )
}

/// `simd_fma` / `simd_relaxed_fma`: per-lane fused multiply-add `x*y + z` with a SINGLE rounding,
/// via `Math.FusedMultiplyAdd` / `MathF.FusedMultiplyAdd`. Using `x*y+z` (two roundings) would
/// mismatch Rust's `simd_fma` on inputs where the intermediate rounds differently — keep the fused
/// call. Clones `simd_binop`'s spill-and-index loop with a third source pointer.
fn simd_fma(name: &str, asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string(name);
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        let (elem, count) = super::binop::simd_lane_info(sig.inputs()[0], asm)
            .expect("simd_fma input is not a vector");
        let elem_tpe: Type = elem.into();
        let elem_idx = asm.alloc_type(elem_tpe);
        let SIMDElem::Float(float) = elem else {
            todo!("simd_fma on an int lane {elem:?}")
        };
        let (class, ft) = match float {
            Float::F16 => (ClassRef::half(asm), Type::Float(Float::F16)),
            Float::F32 => (ClassRef::mathf(asm), Type::Float(Float::F32)),
            Float::F64 => (ClassRef::math(asm), Type::Float(Float::F64)),
            other => todo!("simd_fma on {other:?}"),
        };

        let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
        let x = asm.alloc_node(CILNode::LdArgA(0));
        let x = asm.cast_ptr(x, elem_tpe);
        let y = asm.alloc_node(CILNode::LdArgA(1));
        let y = asm.cast_ptr(y, elem_tpe);
        let z = asm.alloc_node(CILNode::LdArgA(2));
        let z = asm.cast_ptr(z, elem_tpe);
        let mut roots = vec![];
        for idx in 0..count {
            let xs = asm.offset(x, Const::USize(idx), elem_tpe);
            let ys = asm.offset(y, Const::USize(idx), elem_tpe);
            let zs = asm.offset(z, Const::USize(idx), elem_tpe);
            let xv = asm.load(xs, elem_idx);
            let yv = asm.load(ys, elem_idx);
            let zv = asm.load(zs, elem_idx);
            let fused = static_call(
                asm,
                class,
                "FusedMultiplyAdd",
                &[ft, ft, ft],
                ft,
                &[xv, yv, zv],
            );
            let rp = asm.cast_ptr(res_ptr, elem_tpe);
            let rp = asm.offset(rp, Const::USize(idx), elem_tpe);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((rp, fused, elem_tpe, false)))));
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

/// `simd_shuffle<T, T, U> -> V`: `output[i] = concat(x, y)[IDX[i]]` where `concat` is the logical
/// `[x[0..n], y[0..n]]` and `IDX` is the third argument — a real `Simd<u32, N>` index *vector*
/// (rustc lowers the const index array to this concrete argument; line 359 of core simd/mod.rs). We
/// honor the compile-time mapping exactly: per output lane, read `sel = IDX[i]`; if `sel < n` read
/// `x[sel]`, else read `y[sel - n]`. A wrong branch or a wrong `sel - n` offset is a silent
/// miscompile, so the boundary is pinned by the asymmetric two-vector test.
fn simd_shuffle(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_shuffle");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        // Both ordinary CLR vectors and the fixed-array fallback (non-power-of-two widths,
        // sub-64-bit values, and >512-bit values) have the same contiguous lane layout. Recover
        // the shape representation-agnostically instead of assuming `SIMDVector`.
        let (out_elem_s, out_count) =
            super::binop::simd_lane_info(res, asm).expect("simd_shuffle result is not a vector");
        let out_elem: Type = out_elem_s.into();
        // Per the simd_shuffle contract the input element type equals the output element type.
        let (src_elem_s, src_count) = super::binop::simd_lane_info(sig.inputs()[0], asm)
            .expect("simd_shuffle source is not a vector");
        assert_eq!(
            src_elem_s, out_elem_s,
            "simd_shuffle source/result lane elements differ: {src_elem_s:?} vs {out_elem_s:?}"
        );
        // The shuffle index is `[u32; out_count]` (the rustc `simd_shuffle` contract). When
        // out_count > 16 the index vector exceeds 512 bits and is lowered to a fixed-array ClassRef
        // (type.rs >512-bit path), NOT a SIMDVector — so `as_simdvector()` is None and must not be
        // `.unwrap()`ed (the panic that blocked sha2/hmac/ed25519 + the RustCrypto x86 family). The
        // element is u32 regardless of representation, and idx_count is unused (the loop is
        // `0..out_count`), so derive the element representation-agnostically.
        let idx_elem: Type = match sig.inputs()[2].as_simdvector() {
            Some(v) => v.elem().into(),
            None => Type::Int(Int::U32),
        };

        let out_elem_idx = asm.alloc_type(out_elem);
        let idx_elem_idx = asm.alloc_type(idx_elem);
        let elem_ptr_ty = asm.nptr(out_elem_idx);

        let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
        let res_ptr = asm.cast_ptr(res_ptr, out_elem);
        let x = asm.alloc_node(CILNode::LdArgA(0));
        let x = asm.cast_ptr(x, out_elem);
        let y = asm.alloc_node(CILNode::LdArgA(1));
        let y = asm.cast_ptr(y, out_elem);
        let idx = asm.alloc_node(CILNode::LdArgA(2));
        let idx = asm.cast_ptr(idx, idx_elem);

        let mut roots = vec![];
        for i in 0..out_count {
            // sel = (usize)IDX[i]
            let sel_slot = asm.offset(idx, Const::USize(i), idx_elem);
            let sel = asm.load(sel_slot, idx_elem_idx);
            let sel = asm.int_cast(sel, Int::USize, ExtendKind::ZeroExtend);
            // in_first = sel < src_count  (unsigned)
            let n = asm.alloc_node(Const::USize(src_count));
            let in_first = asm.biop(sel, n, BinOp::LtUn);
            // base = in_first ? x : y
            let base = asm.select(elem_ptr_ty, x, y, in_first);
            // within = in_first ? sel : sel - src_count
            let n2 = asm.alloc_node(Const::USize(src_count));
            let sel_minus = asm.biop(sel, n2, BinOp::Sub);
            let within = asm.select(Type::Int(Int::USize), sel, sel_minus, in_first);
            // val = base[within]
            let slot = asm.offset(base, within, out_elem);
            let val = asm.load(slot, out_elem_idx);
            let r_slot = asm.offset(res_ptr, Const::USize(i), out_elem);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((r_slot, val, out_elem, false)))));
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

/// Recover the element pointer type and lane count for a vector of raw pointers.
///
/// Pointer vectors cannot use [`SIMDElem`], whose domain intentionally matches the CLR intrinsic
/// element set (`int`/`float`).  rustc therefore lowers them through the fixed-array fallback: a
/// generated value type with one explicitly-sized field of `*const T`/`*mut T`.  Keep this helper
/// representation-agnostic for one-lane vectors as well, where the type is already a scalar
/// pointer.  This is the pointer analogue of `simd_lane_info` in `binop.rs`.
fn simd_ptr_lane_info(tpe: Type, asm: &Assembly) -> Option<(Type, u64)> {
    if let Type::Ptr(inner) | Type::Ref(inner) = tpe {
        return Some((Type::Ptr(inner), 1));
    }
    let Type::ClassRef(cref) = tpe else {
        return None;
    };
    let def = asm.class_ref_to_def(cref)?;
    let def = &asm[def];
    let (elem, _, _) = *def.fields().first()?;
    if !matches!(elem, Type::Ptr(_) | Type::Ref(_)) {
        return None;
    }
    let total = u64::from(def.explict_size()?.get());
    let elem_size = u64::from(asm.sizeof_type(elem));
    (elem_size != 0).then_some((elem, total / elem_size))
}

/// Build the straight-line per-lane body for pointer-vector provenance/casting operations.
/// `transform` receives each loaded source pointer and returns the destination lane value.  The
/// destination may be another pointer vector or a `usize` vector; callers supply its lane type
/// explicitly because only pointer vectors fall outside `SIMDElem`.
fn simd_ptr_transform(
    name: &str,
    transform: impl Fn(&mut Assembly, Interned<CILNode>, Type, Type) -> Interned<CILNode> + 'static,
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
) {
    let name = asm.alloc_string(name);
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        let (src_elem, count) = match simd_ptr_lane_info(sig.inputs()[0], asm) {
            Some(info) => info,
            None => super::binop::simd_lane_info(sig.inputs()[0], asm)
                .map(|(elem, count)| (Type::from(elem), count))
                .expect("SIMD pointer transform input is not a vector"),
        };
        let (res_elem, res_count) = match simd_ptr_lane_info(res, asm) {
            Some(info) => info,
            None => super::binop::simd_lane_info(res, asm)
                .map(|(elem, count)| (Type::from(elem), count))
                .expect("SIMD pointer transform result is not a vector"),
        };
        assert_eq!(
            count, res_count,
            "SIMD pointer transform source/result lane counts differ"
        );

        let src_elem_idx = asm.alloc_type(src_elem);
        let src = asm.alloc_node(CILNode::LdArgA(0));
        let src = asm.cast_ptr(src, src_elem);
        let res_ptr = asm.alloc_node(CILNode::LdLocA(0));
        let res_ptr = asm.cast_ptr(res_ptr, res_elem);
        let mut roots = Vec::with_capacity(count as usize + 1);
        for idx in 0..count {
            let src_slot = asm.offset(src, Const::USize(idx), src_elem);
            let lane = asm.load(src_slot, src_elem_idx);
            let transformed = transform(asm, lane, src_elem, res_elem);
            let res_slot = asm.offset(res_ptr, Const::USize(idx), res_elem);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((
                res_slot,
                transformed,
                res_elem,
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

/// `simd_expose_provenance`: convert each raw-pointer lane to its native integer address.  CLR
/// pointers and `nuint` share the native-int representation, so the operation is an explicit
/// `PtrCast` per lane; provenance is intentionally erased just as the Rust intrinsic specifies.
fn simd_expose_provenance(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    simd_ptr_transform(
        "simd_expose_provenance",
        |asm, lane, src_elem, _res_elem| {
            assert!(matches!(src_elem, Type::Ptr(_) | Type::Ref(_)));
            asm.cast_ptr_to(lane, Type::Int(Int::USize))
        },
        asm,
        patcher,
    );
}

/// `simd_with_exposed_provenance`: reconstruct each pointer lane from a native integer address.
/// This is the inverse representation cast of `simd_expose_provenance`; the CLR has no separate
/// provenance token, so preserving the address bits is the complete target contract.
fn simd_with_exposed_provenance(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_with_exposed_provenance");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        let (res_elem, count) =
            simd_ptr_lane_info(res, asm).expect("SIMD with-exposed result is not a pointer vector");
        let (src_elem, src_count) = super::binop::simd_lane_info(sig.inputs()[0], asm)
            .expect("SIMD with-exposed input is not a usize vector");
        assert_eq!(
            count, src_count,
            "SIMD with-exposed source/result lane counts differ"
        );
        assert_eq!(src_elem, SIMDElem::Int(Int::USize));

        let src_elem_tpe: Type = src_elem.into();
        let src_elem_idx = asm.alloc_type(src_elem_tpe);
        let src_addr = asm.alloc_node(CILNode::LdArgA(0));
        let src = asm.cast_ptr(src_addr, src_elem_tpe);
        let res_addr = asm.alloc_node(CILNode::LdLocA(0));
        let res_ptr = asm.cast_ptr(res_addr, res_elem);
        let mut roots = Vec::with_capacity(count as usize + 1);
        for idx in 0..count {
            let src_slot = asm.offset(src, Const::USize(idx), src_elem_tpe);
            let address = asm.load(src_slot, src_elem_idx);
            let pointer = asm.cast_ptr_to(address, res_elem);
            let res_slot = asm.offset(res_ptr, Const::USize(idx), res_elem);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((
                res_slot, pointer, res_elem, false,
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

/// `simd_cast_ptr`: cast each pointer lane to the destination pointer type.  The cast is a type
/// relabel in CIL (no address arithmetic), matching the Rust pointer-vector cast intrinsic.
fn simd_cast_ptr(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    simd_ptr_transform(
        "simd_cast_ptr",
        |asm, lane, _src_elem, res_elem| asm.cast_ptr_to(lane, res_elem),
        asm,
        patcher,
    );
}

/// `simd_arith_offset`: apply wrapping pointer arithmetic lane-by-lane.  `Assembly::offset`
/// performs native modulo arithmetic; sign-preserving bit patterns make it correct for both
/// `isize` and `usize` offsets when the index is lowered to `nuint`.
fn simd_arith_offset(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_arith_offset");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        let (ptr_elem, count) = simd_ptr_lane_info(sig.inputs()[0], asm)
            .expect("SIMD arith offset input is not a pointer vector");
        let (res_elem, res_count) =
            simd_ptr_lane_info(res, asm).expect("SIMD arith offset result is not a pointer vector");
        assert_eq!(
            ptr_elem, res_elem,
            "SIMD arith offset changes pointer element type"
        );
        assert_eq!(
            count, res_count,
            "SIMD arith offset source/result lane counts differ"
        );
        let (offset_elem, offset_count) = super::binop::simd_lane_info(sig.inputs()[1], asm)
            .expect("SIMD arith offset input is not an integer vector");
        assert_eq!(
            count, offset_count,
            "SIMD arith offset pointer/offset lane counts differ"
        );
        assert!(matches!(
            offset_elem,
            SIMDElem::Int(Int::USize | Int::ISize)
        ));
        let offset_tpe: Type = offset_elem.into();
        let ptr_idx = asm.alloc_type(ptr_elem);
        let offset_idx = asm.alloc_type(offset_tpe);
        let ptr_addr = asm.alloc_node(CILNode::LdArgA(0));
        let ptr = asm.cast_ptr(ptr_addr, ptr_elem);
        let offsets_addr = asm.alloc_node(CILNode::LdArgA(1));
        let offsets = asm.cast_ptr(offsets_addr, offset_tpe);
        let result_addr = asm.alloc_node(CILNode::LdLocA(0));
        let result = asm.cast_ptr(result_addr, res_elem);
        let pointee = match ptr_elem {
            Type::Ptr(inner) | Type::Ref(inner) => asm[inner],
            _ => unreachable!(),
        };
        let mut roots = Vec::with_capacity(count as usize + 1);
        for idx in 0..count {
            let pslot = asm.offset(ptr, Const::USize(idx), ptr_elem);
            let oslot = asm.offset(offsets, Const::USize(idx), offset_tpe);
            let pointer = asm.load(pslot, ptr_idx);
            let offset = asm.load(oslot, offset_idx);
            let shifted = asm.offset(pointer, offset, pointee);
            let rslot = asm.offset(result, Const::USize(idx), res_elem);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((rslot, shifted, res_elem, false)))));
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

/// Build the per-lane body for `simd_masked_load`.
///
/// The CLR has no generic `Vector<T>` operation with Rust's exact masked-memory contract: a
/// disabled lane must not even evaluate its address.  A pointer-select followed by `ldind` would
/// be observably wrong for a poisoned/out-of-bounds disabled pointer, so this generator uses one
/// tiny conditional block per lane.  The passthrough vector is copied first; enabled lanes then
/// overwrite their slots.  This is deliberately a straight-line bounded CFG (rather than a
/// runtime loop), which keeps the generated method verifiable and lets the normal CFG cleanup
/// remove no-op branches for one-lane vectors.
fn simd_masked_load(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    simd_masked_memory("simd_masked_load", true, asm, patcher);
}

/// Shared straight-line CFG generator for masked load/store. `is_load` selects whether argument
/// 1 is read into the result local (load) or written to (store); both forms use argument 2 as the
/// value vector and argument 0 as the mask. Disabled lanes branch around the memory operation, so
/// their pointer is never evaluated.
fn simd_masked_memory(
    name: &str,
    is_load: bool,
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
) {
    let name = asm.alloc_string(name);
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        let vector = if is_load { res } else { sig.inputs()[2] };
        let (elem_s, count) = super::binop::simd_lane_info(vector, asm)
            .expect("simd_masked memory value is not a vector");
        let elem: Type = elem_s.into();
        let (mask_elem_s, mask_count) = super::binop::simd_lane_info(sig.inputs()[0], asm)
            .expect("simd_masked memory mask is not a vector");
        assert_eq!(
            count, mask_count,
            "simd_masked memory mask/value lane counts differ: {mask_count} vs {count}"
        );
        let mask_elem: Type = mask_elem_s.into();
        let elem_idx = asm.alloc_type(elem);
        let mask_idx = asm.alloc_type(mask_elem);

        let mask_addr = asm.alloc_node(CILNode::LdArgA(0));
        let mask_ptr = asm.cast_ptr(mask_addr, mask_elem);
        let value_addr = asm.alloc_node(CILNode::LdArgA(2));
        let value_ptr = asm.cast_ptr(value_addr, elem);
        let pointer_addr = asm.alloc_node(CILNode::LdArg(1));
        let pointer = asm.cast_ptr(pointer_addr, elem);
        let result_ptr = is_load.then(|| {
            let result_addr = asm.alloc_node(CILNode::LdLocA(0));
            asm.cast_ptr(result_addr, elem)
        });

        let done_id = 1 + (count as u32) * 2;
        let mut entry = Vec::with_capacity(count as usize + 1);
        if let Some(result_ptr) = result_ptr {
            // Start with the passthrough value. A fully-disabled mask never touches the source
            // pointer at all.
            for lane in 0..count {
                let src_slot = asm.offset(value_ptr, Const::USize(lane), elem);
                let value = asm.load(src_slot, elem_idx);
                let dst_slot = asm.offset(result_ptr, Const::USize(lane), elem);
                entry
                    .push(asm.alloc_root(CILRoot::StInd(Box::new((dst_slot, value, elem, false)))));
            }
        }

        let mask_lane = |asm: &mut Assembly, lane: u64| {
            let slot = asm.offset(mask_ptr, Const::USize(lane), mask_elem);
            let value = asm.load(slot, mask_idx);
            let value = asm.int_cast(value, Int::I32, ExtendKind::SignExtend);
            let zero = asm.alloc_node(Const::I32(0));
            (value, zero)
        };

        // Every check is represented in the canonical MIR shape: a conditional branch to the
        // enabled block followed by an unconditional jump for the disabled case. We intentionally
        // avoid the legacy `sub_target` encoding here; direct PE emits the first branch as a
        // normal conditional jump and the explicit second root makes both arms stable even if the
        // linker reorders blocks.
        let (mask_value, zero) = mask_lane(asm, 0);
        let first_load = 1;
        let first_false = if count > 1 { 2 } else { done_id };
        entry.push(asm.alloc_root(CILRoot::Branch(Box::new((
            first_load,
            0,
            Some(BranchCond::Ne(mask_value, zero)),
        )))));
        entry.push(asm.alloc_root(CILRoot::Branch(Box::new((first_false, 0, None)))));

        let mut blocks = vec![BasicBlock::new(entry, 0, None)];
        for lane in 0..count {
            let load_id = 1 + (lane as u32) * 2;
            let next_check = if lane + 1 < count {
                load_id + 1
            } else {
                done_id
            };
            let pointer_slot = asm.offset(pointer, Const::USize(lane), elem);
            let (destination_slot, value) = if is_load {
                let source = asm.load(pointer_slot, elem_idx);
                let result_ptr = result_ptr.expect("masked load has no result local");
                let destination = asm.offset(result_ptr, Const::USize(lane), elem);
                (destination, source)
            } else {
                let source = asm.offset(value_ptr, Const::USize(lane), elem);
                let value = asm.load(source, elem_idx);
                (pointer_slot, value)
            };
            let store = asm.alloc_root(CILRoot::StInd(Box::new((
                destination_slot,
                value,
                elem,
                false,
            ))));
            let jump = asm.alloc_root(CILRoot::Branch(Box::new((next_check, 0, None))));
            blocks.push(BasicBlock::new(vec![store, jump], load_id, None));

            if lane + 1 < count {
                let (mask_value, zero) = mask_lane(asm, lane + 1);
                let next_load = load_id + 2;
                let next_false = if lane + 2 < count {
                    load_id + 3
                } else {
                    done_id
                };
                let branch = asm.alloc_root(CILRoot::Branch(Box::new((
                    next_load,
                    0,
                    Some(BranchCond::Ne(mask_value, zero)),
                ))));
                let fallthrough = asm.alloc_root(CILRoot::Branch(Box::new((next_false, 0, None))));
                blocks.push(BasicBlock::new(
                    vec![branch, fallthrough],
                    load_id + 1,
                    None,
                ));
            }
        }

        let done = if is_load {
            CILRoot::Ret(asm.alloc_node(CILNode::LdLoc(0)))
        } else {
            CILRoot::VoidRet
        };
        blocks.push(BasicBlock::new(vec![asm.alloc_root(done)], done_id, None));
        MethodImpl::MethodBody {
            blocks,
            locals: is_load
                .then(|| (None, asm.alloc_type(res)))
                .into_iter()
                .collect(),
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// Build the per-lane body for `simd_masked_store`.
///
/// As with [`simd_masked_load`], each disabled lane is represented by a branch around the memory
/// write.  This preserves the intrinsic's no-access guarantee even when a disabled pointer is
/// invalid, and it also handles fixed-array SIMD representations used below 64 bits or above the
/// CLR vector width.
fn simd_masked_store(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    simd_masked_memory("simd_masked_store", false, asm, patcher);
}

/// Build `simd_select_bitmask<M, T>(mask, yes, no) -> T`.
///
/// Portable-SIMD uses a compact scalar bitmask for this intrinsic (u8/u16/u32/u64 depending on
/// lane count).  Expand the bits one at a time and select *addresses* before loading each lane;
/// selecting addresses keeps this valid for float and fixed-array vector representations without
/// requiring a value-level float select in the IR.
fn simd_select_bitmask(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("simd_select_bitmask");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let sig = asm[asm[mref].sig()].clone();
        let res = *sig.output();
        let (elem_s, count) = super::binop::simd_lane_info(res, asm)
            .expect("simd_select_bitmask result is not a vector");
        let elem: Type = elem_s.into();
        let elem_idx = asm.alloc_type(elem);
        let elem_ptr_ty = asm.nptr(elem_idx);

        let mask = asm.alloc_node(CILNode::LdArg(0));
        let mask = asm.int_cast(mask, Int::U64, ExtendKind::ZeroExtend);
        let yes_addr = asm.alloc_node(CILNode::LdArgA(1));
        let yes_ptr = asm.cast_ptr(yes_addr, elem);
        let no_addr = asm.alloc_node(CILNode::LdArgA(2));
        let no_ptr = asm.cast_ptr(no_addr, elem);
        let result_addr = asm.alloc_node(CILNode::LdLocA(0));
        let result_ptr = asm.cast_ptr(result_addr, elem);

        let mut roots = Vec::with_capacity(count as usize + 1);
        for lane in 0..count {
            let bit = asm.biop(mask, Const::U64(1_u64 << lane), BinOp::And);
            let zero = asm.alloc_node(Const::U64(0));
            let is_false = asm.biop(bit, zero, BinOp::Eq);
            let yes_slot = asm.offset(yes_ptr, Const::USize(lane), elem);
            let no_slot = asm.offset(no_ptr, Const::USize(lane), elem);
            let chosen = asm.select(elem_ptr_ty, no_slot, yes_slot, is_false);
            let value = asm.load(chosen, elem_idx);
            let result_slot = asm.offset(result_ptr, Const::USize(lane), elem);
            roots.push(asm.alloc_root(CILRoot::StInd(Box::new((result_slot, value, elem, false)))));
        }
        let ret_value = asm.alloc_node(CILNode::LdLoc(0));
        roots.push(asm.alloc_root(CILRoot::Ret(ret_value)));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals: vec![(None, asm.alloc_type(res))],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// Register all SIMD-tail per-lane ops.
pub(super) fn register_tail_ops(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    simd_shuffle(asm, patcher);
    simd_cast_ptr(asm, patcher);
    simd_arith_offset(asm, patcher);
    simd_expose_provenance(asm, patcher);
    simd_with_exposed_provenance(asm, patcher);
    simd_masked_load(asm, patcher);
    simd_masked_store(asm, patcher);
    simd_select_bitmask(asm, patcher);
    // Per-lane integer bit ops.
    simd_unary(ctpop_lane, "simd_ctpop", asm, patcher);
    simd_unary(ctlz_lane, "simd_ctlz", asm, patcher);
    simd_unary(cttz_lane, "simd_cttz", asm, patcher);
    simd_unary(bswap_lane, "simd_bswap", asm, patcher);
    simd_unary(bitreverse_lane, "simd_bitreverse", asm, patcher);
    // Per-lane float transcendentals / rounding (also used as the C fallback for floor/ceil/sqrt).
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Sqrt"),
        "simd_fsqrt",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Sin"),
        "simd_fsin",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Cos"),
        "simd_fcos",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Exp"),
        "simd_fexp",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Exp2"),
        "simd_fexp2",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Log"),
        "simd_flog",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Log2"),
        "simd_flog2",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Log10"),
        "simd_flog10",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Floor"),
        "simd_floor",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Ceiling"),
        "simd_ceil",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| float_unop_lane(a, l, e, "Truncate"),
        "simd_trunc",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| round_lane(a, l, e, true),
        "simd_round",
        asm,
        patcher,
    );
    simd_unary(
        |a, l, e| round_lane(a, l, e, false),
        "simd_round_ties_even",
        asm,
        patcher,
    );
    // Per-lane fused multiply-add.
    simd_fma("simd_fma", asm, patcher);
    simd_fma("simd_relaxed_fma", asm, patcher);
}
