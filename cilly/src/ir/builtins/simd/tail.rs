//! The "SIMD tail" builtins: per-lane scalar ops that have no guaranteed-correct generic-static BCL
//! `Vector{bits}` method, plus `simd_shuffle`. All of these use the target-agnostic spill-and-index
//! idiom (mirror `simd_binop`/`simd_cast`): cast the source/result vector locals to element pointers,
//! walk lanes, apply the scalar op per lane, and store into the result local. Because they touch no
//! mask convention and no BCL vector intrinsic, the same body is correct on both the .NET and C
//! targets, so they are all registered in `register_value_lane_ops`.
use crate::{
    asm::MissingMethodPatcher,
    cilnode::{ExtendKind, IsPure, MethodKind},
    tpe::simd::SIMDElem,
    Assembly, BasicBlock, BinOp, CILNode, CILRoot, ClassRef, Const, Float, Int, Interned,
    MethodImpl, MethodRef, Type,
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
        let sig = &asm[asm[mref].sig()];
        let res = *sig.output();
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
            let transformed = op(asm, lane, src_elem);
            let res_ptr = asm.cast_ptr(res_ptr, src_elem_tpe);
            let res_ptr = asm.offset(res_ptr, Const::USize(idx as u64), src_elem_tpe);
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
    let (wide, wide_bits) = if lane_bits > 32 { (Int::U64, 64i32) } else { (Int::U32, 32i32) };
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
    let (wide, wide_bits) = if lane_bits > 32 { (Int::U64, 64u32) } else { (Int::U32, 32u32) };
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
fn bitreverse_lane(asm: &mut Assembly, lane: Interned<CILNode>, elem: SIMDElem) -> Interned<CILNode> {
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

/// Per-lane float `MathF`/`Math` unary call (`Floor`/`Ceiling`/`Truncate`/`Sqrt`).
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
        Float::F32 => (ClassRef::mathf(asm), Type::Float(Float::F32)),
        Float::F64 => (ClassRef::math(asm), Type::Float(Float::F64)),
        other => todo!("simd float {method} on {other:?}"),
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
        let sig = &asm[asm[mref].sig()];
        let res = *sig.output();
        let vec = sig.inputs()[0].as_simdvector().unwrap().clone();
        let elem = vec.elem();
        let elem_tpe: Type = elem.into();
        let elem_idx = asm.alloc_type(elem_tpe);
        let SIMDElem::Float(float) = elem else {
            todo!("simd_fma on an int lane {elem:?}")
        };
        let (class, ft) = match float {
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
        for idx in 0..vec.count() {
            let xs = asm.offset(x, Const::USize(idx as u64), elem_tpe);
            let ys = asm.offset(y, Const::USize(idx as u64), elem_tpe);
            let zs = asm.offset(z, Const::USize(idx as u64), elem_tpe);
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
            let rp = asm.offset(rp, Const::USize(idx as u64), elem_tpe);
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
        let sig = &asm[asm[mref].sig()];
        let res = *sig.output();
        let out_vec = res.as_simdvector().unwrap().clone();
        let out_elem: Type = out_vec.elem().into();
        let out_count = out_vec.count();
        // Per the simd_shuffle contract the input element type equals the output element type.
        let src_vec = sig.inputs()[0].as_simdvector().unwrap().clone();
        let src_count = src_vec.count() as u64;
        let idx_vec = sig.inputs()[2].as_simdvector().unwrap().clone();
        let idx_elem: Type = idx_vec.elem().into();

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
            let sel_slot = asm.offset(idx, Const::USize(i as u64), idx_elem);
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
            let r_slot = asm.offset(res_ptr, Const::USize(i as u64), out_elem);
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

/// Register all SIMD-tail per-lane ops. Called from `register_value_lane_ops`, so they serve both
/// the .NET (`simd`) and C (`fallback_simd`) builtin sets.
pub(super) fn register_tail_ops(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    simd_shuffle(asm, patcher);
    // Per-lane integer bit ops.
    simd_unary(ctpop_lane, "simd_ctpop", asm, patcher);
    simd_unary(ctlz_lane, "simd_ctlz", asm, patcher);
    simd_unary(cttz_lane, "simd_cttz", asm, patcher);
    simd_unary(bswap_lane, "simd_bswap", asm, patcher);
    simd_unary(bitreverse_lane, "simd_bitreverse", asm, patcher);
    // Per-lane float transcendentals / rounding (also used as the C fallback for floor/ceil/sqrt).
    simd_unary(|a, l, e| float_unop_lane(a, l, e, "Sqrt"), "simd_fsqrt", asm, patcher);
    simd_unary(|a, l, e| float_unop_lane(a, l, e, "Floor"), "simd_floor", asm, patcher);
    simd_unary(|a, l, e| float_unop_lane(a, l, e, "Ceiling"), "simd_ceil", asm, patcher);
    simd_unary(|a, l, e| float_unop_lane(a, l, e, "Truncate"), "simd_trunc", asm, patcher);
    simd_unary(|a, l, e| round_lane(a, l, e, true), "simd_round", asm, patcher);
    simd_unary(|a, l, e| round_lane(a, l, e, false), "simd_round_ties_even", asm, patcher);
    // Per-lane fused multiply-add.
    simd_fma("simd_fma", asm, patcher);
    simd_fma("simd_relaxed_fma", asm, patcher);
}
