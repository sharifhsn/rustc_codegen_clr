//! Correctly rounded integer-to-`f32` conversions.
//!
//! ECMA-335 exposes an unsigned integer conversion through `conv.r.un`, but the usual
//! `conv.r.un; conv.r4` sequence is allowed to round through an intermediate `f64`.  That is a
//! double-rounding trap for values near an `f32` midpoint.  Rust's compiler-builtins routines
//! instead construct the IEEE-754 bits directly.  The small helpers in this module mirror those
//! routines for the wide integer types that can trigger the trap (`u64`, `u128`, and `i128`).
//!
//! The helpers intentionally use the canonical compiler-builtins symbol names.  When a linked
//! compiler-builtins assembly already defines one of those symbols, normal linker resolution uses
//! that implementation.  The patcher bodies below are the self-contained fallback for programs
//! whose sysroot does not carry the symbol.

use crate::{
    Assembly, BasicBlock, BinOp, BranchCond, CILNode, CILRoot, ClassRef, Const, Float, Int,
    Interned, MethodImpl, Type,
    asm::MissingMethodPatcher,
    cilnode::{ExtendKind, IsPure},
    hashable::HashableF32,
};

type Node = Interned<CILNode>;

fn leading_zeroes_u64(asm: &mut Assembly, value: Node) -> Node {
    let class = ClassRef::bit_operations(asm);
    let mref = asm[class].clone().static_mref(
        &[Type::Int(Int::U64)],
        Type::Int(Int::I32),
        asm.alloc_string("LeadingZeroCount"),
        asm,
    );
    asm.call(mref, &[value], IsPure::PURE)
}

fn u128_binop(asm: &mut Assembly, name: &str, lhs: Node, rhs: Node) -> Node {
    asm.call_static(
        name,
        [Type::Int(Int::U128), Type::Int(Int::U128)],
        Type::Int(Int::U128),
        &[lhs, rhs],
    )
}

fn u128_shift(asm: &mut Assembly, name: &str, value: Node, amount: Node) -> Node {
    asm.call_static(
        name,
        [Type::Int(Int::U128), Type::Int(Int::I32)],
        Type::Int(Int::U128),
        &[value, amount],
    )
}

fn u128_eq(asm: &mut Assembly, lhs: Node, rhs: Node) -> Node {
    asm.call_static(
        "eq_u128",
        [Type::Int(Int::U128), Type::Int(Int::U128)],
        Type::Bool,
        &[lhs, rhs],
    )
}

fn u128_to_int(asm: &mut Assembly, value: Node, target: Int) -> Node {
    let class = ClassRef::uint_128(asm);
    let mref = asm[class].clone().static_mref(
        &[Type::Int(Int::U128)],
        Type::Int(target),
        asm.alloc_string("op_Explicit"),
        asm,
    );
    asm.call(mref, &[value], IsPure::PURE)
}

fn round_mantissa(asm: &mut Assembly, base: Node, dropped: Node) -> Node {
    // compiler-builtins' `m_adj::<f32>`:
    //   (dropped - ((dropped >> 31) & !base)) >> 31
    // This is round-to-nearest, ties-to-even, represented as a branchless 0/1 increment.
    let shift = asm.alloc_node(Const::I32(31));
    let top_bit = asm.biop(dropped, shift, BinOp::ShrUn);
    let not_base = asm.not(base);
    let correction = asm.biop(top_bit, not_base, BinOp::And);
    let adjusted = asm.biop(dropped, correction, BinOp::Sub);
    let increment = asm.biop(adjusted, shift, BinOp::ShrUn);
    asm.biop(base, increment, BinOp::Add)
}

fn pack_f32(asm: &mut Assembly, exponent: Node, mantissa: Node) -> Node {
    let shift = asm.alloc_node(Const::I32(23));
    let shifted = asm.biop(exponent, shift, BinOp::Shl);
    let bits = asm.biop(shifted, mantissa, BinOp::Add);
    asm.transmute_on_stack(Type::Int(Int::U32), Type::Float(Float::F32), bits)
}

fn finish_f32_body(
    asm: &mut Assembly,
    n: Node,
    i_m: Node,
    i_m_type: Int,
    m_base: Node,
    dropped: Node,
    mantissa: Node,
    exponent: Node,
    packed: Node,
    is_zero: Node,
) -> MethodImpl {
    let branch = asm.alloc_root(CILRoot::Branch(Box::new((
        1,
        2,
        Some(BranchCond::True(is_zero)),
    ))));
    let zero_f32 = asm.alloc_node(Const::F32(HashableF32(0.0)));
    let ret_zero = asm.alloc_root(CILRoot::Ret(zero_f32));
    let ret_value = asm.alloc_root(CILRoot::Ret(packed));

    MethodImpl::MethodBody {
        blocks: vec![
            BasicBlock::new(
                vec![
                    asm.alloc_root(CILRoot::StLoc(0, n)),
                    asm.alloc_root(CILRoot::StLoc(1, i_m)),
                    asm.alloc_root(CILRoot::StLoc(2, m_base)),
                    asm.alloc_root(CILRoot::StLoc(3, dropped)),
                    asm.alloc_root(CILRoot::StLoc(4, mantissa)),
                    asm.alloc_root(CILRoot::StLoc(5, exponent)),
                    branch,
                ],
                0,
                None,
            ),
            BasicBlock::new(vec![ret_zero], 1, None),
            BasicBlock::new(vec![ret_value], 2, None),
        ],
        locals: vec![
            (
                Some(asm.alloc_string("n")),
                asm.alloc_type(Type::Int(Int::I32)),
            ),
            (
                Some(asm.alloc_string("i_m")),
                asm.alloc_type(Type::Int(i_m_type)),
            ),
            (
                Some(asm.alloc_string("m_base")),
                asm.alloc_type(Type::Int(Int::U32)),
            ),
            (
                Some(asm.alloc_string("dropped")),
                asm.alloc_type(Type::Int(Int::U32)),
            ),
            (
                Some(asm.alloc_string("mantissa")),
                asm.alloc_type(Type::Int(Int::U32)),
            ),
            (
                Some(asm.alloc_string("exponent")),
                asm.alloc_type(Type::Int(Int::U32)),
            ),
        ],
    }
}

fn u64_body(asm: &mut Assembly) -> MethodImpl {
    let arg = asm.alloc_node(CILNode::LdArg(0));
    let n = leading_zeroes_u64(asm, arg);
    let i_m = asm.biop(arg, n, BinOp::Shl);
    let shift_40 = asm.alloc_node(Const::I32(40));
    let m_base_wide = asm.biop(i_m, shift_40, BinOp::ShrUn);
    let m_base = asm.int_cast(m_base_wide, Int::U32, ExtendKind::ZeroExtend);
    let lower_mask = asm.alloc_node(Const::U64(0xffff));
    let lower = asm.biop(i_m, lower_mask, BinOp::And);
    let shift_8 = asm.alloc_node(Const::I32(8));
    let upper = asm.biop(i_m, shift_8, BinOp::ShrUn);
    let dropped_wide = asm.biop(upper, lower, BinOp::Or);
    let dropped = asm.int_cast(dropped_wide, Int::U32, ExtendKind::ZeroExtend);
    let mantissa = round_mantissa(asm, m_base, dropped);

    let exponent_base = asm.alloc_node(Const::I32(189));
    let exponent_i32 = asm.biop(exponent_base, n, BinOp::Sub);
    let exponent = asm.int_cast(exponent_i32, Int::U32, ExtendKind::ZeroExtend);
    let packed = pack_f32(asm, exponent, mantissa);

    let zero = asm.alloc_node(Const::U64(0));
    let is_zero = asm.biop(arg, zero, BinOp::Eq);
    finish_f32_body(
        asm,
        n,
        i_m,
        Int::U64,
        m_base,
        dropped,
        mantissa,
        exponent,
        packed,
        is_zero,
    )
}

fn u128_body(asm: &mut Assembly) -> MethodImpl {
    let arg = asm.alloc_node(CILNode::LdArg(0));
    let class = ClassRef::uint_128(asm);
    let lz = asm[class].clone().static_mref(
        &[Type::Int(Int::U128)],
        Type::Int(Int::U128),
        asm.alloc_string("LeadingZeroCount"),
        asm,
    );
    let n128 = asm.call(lz, &[arg], IsPure::PURE);
    let n = u128_to_int(asm, n128, Int::I32);
    let i_m = u128_shift(asm, "shl_u128", arg, n);
    let shift_96 = asm.alloc_node(Const::I32(96));
    let m_base_wide = u128_shift(asm, "shr_u128", i_m, shift_96);
    let m_base = u128_to_int(asm, m_base_wide, Int::U32);

    // `d1 = (i_m >> 72) as u32` and `d2 = ((i_m << 32 >> 32) != 0)` from
    // compiler-builtins' `u128_to_f32_bits`.
    let shift_72 = asm.alloc_node(Const::I32(72));
    let d1_wide = u128_shift(asm, "shr_u128", i_m, shift_72);
    let d1 = u128_to_int(asm, d1_wide, Int::U32);
    let shift_32 = asm.alloc_node(Const::I32(32));
    let high_shifted = u128_shift(asm, "shl_u128", i_m, shift_32);
    let low = u128_shift(asm, "shr_u128", high_shifted, shift_32);
    let zero_u128 = asm.alloc_node(Const::U128(0));
    let d2_bool = u128_eq(asm, low, zero_u128);
    let false_node = asm.alloc_node(Const::Bool(false));
    let d2_nonzero = asm.biop(d2_bool, false_node, BinOp::Eq);
    let d2 = asm.int_cast(d2_nonzero, Int::U32, ExtendKind::ZeroExtend);
    let dropped = asm.biop(d1, d2, BinOp::Or);
    let mantissa = round_mantissa(asm, m_base, dropped);

    let exponent_base = asm.alloc_node(Const::I32(253));
    let exponent_i32 = asm.biop(exponent_base, n, BinOp::Sub);
    let exponent = asm.int_cast(exponent_i32, Int::U32, ExtendKind::ZeroExtend);
    let packed = pack_f32(asm, exponent, mantissa);

    let zero = asm.alloc_node(Const::U128(0));
    let is_zero = u128_eq(asm, arg, zero);
    finish_f32_body(
        asm,
        n,
        i_m,
        Int::U128,
        m_base,
        dropped,
        mantissa,
        exponent,
        packed,
        is_zero,
    )
}

fn i128_body(asm: &mut Assembly) -> MethodImpl {
    let arg = asm.alloc_node(CILNode::LdArg(0));
    let zero_i128 = asm.alloc_node(Const::I128(0));
    let sign = asm.call_static(
        "lt_i128",
        [Type::Int(Int::I128), Type::Int(Int::I128)],
        Type::Bool,
        &[arg, zero_i128],
    );
    let raw = asm.transmute_on_stack(Type::Int(Int::I128), Type::Int(Int::U128), arg);
    let zero_u128 = asm.alloc_node(Const::U128(0));
    let neg_raw = u128_binop(asm, "sub_u128", zero_u128, raw);
    let branch_sign = asm.alloc_root(CILRoot::Branch(Box::new((
        1,
        2,
        Some(BranchCond::True(sign)),
    ))));
    let branch_join_from_neg = asm.alloc_root(CILRoot::Branch(Box::new((3, 3, None))));
    let branch_join_from_raw = asm.alloc_root(CILRoot::Branch(Box::new((3, 3, None))));

    let magnitude = asm.alloc_node(CILNode::LdLoc(2));
    let converted = asm.call_static(
        "__floatuntisf",
        [Type::Int(Int::U128)],
        Type::Float(Float::F32),
        &[magnitude],
    );
    let bits = asm.transmute_on_stack(Type::Float(Float::F32), Type::Int(Int::U32), converted);
    let sign_local = asm.alloc_node(CILNode::LdLoc(0));
    let branch_output = asm.alloc_root(CILRoot::Branch(Box::new((
        4,
        5,
        Some(BranchCond::True(sign_local)),
    ))));
    let sign_mask = asm.alloc_node(Const::U32(0x8000_0000));
    let signed_bits = asm.biop(bits, sign_mask, BinOp::Or);
    let signed_value =
        asm.transmute_on_stack(Type::Int(Int::U32), Type::Float(Float::F32), signed_bits);
    let ret_signed = asm.alloc_root(CILRoot::Ret(signed_value));
    let unsigned_value = asm.transmute_on_stack(Type::Int(Int::U32), Type::Float(Float::F32), bits);
    let ret_unsigned = asm.alloc_root(CILRoot::Ret(unsigned_value));

    MethodImpl::MethodBody {
        blocks: vec![
            BasicBlock::new(
                vec![
                    asm.alloc_root(CILRoot::StLoc(0, sign)),
                    asm.alloc_root(CILRoot::StLoc(1, raw)),
                    branch_sign,
                ],
                0,
                None,
            ),
            BasicBlock::new(
                vec![
                    asm.alloc_root(CILRoot::StLoc(2, neg_raw)),
                    branch_join_from_neg,
                ],
                1,
                None,
            ),
            BasicBlock::new(
                vec![asm.alloc_root(CILRoot::StLoc(2, raw)), branch_join_from_raw],
                2,
                None,
            ),
            BasicBlock::new(
                vec![asm.alloc_root(CILRoot::StLoc(3, bits)), branch_output],
                3,
                None,
            ),
            BasicBlock::new(vec![ret_signed], 4, None),
            BasicBlock::new(vec![ret_unsigned], 5, None),
        ],
        locals: vec![
            (Some(asm.alloc_string("sign")), asm.alloc_type(Type::Bool)),
            (
                Some(asm.alloc_string("raw")),
                asm.alloc_type(Type::Int(Int::U128)),
            ),
            (
                Some(asm.alloc_string("magnitude")),
                asm.alloc_type(Type::Int(Int::U128)),
            ),
            (
                Some(asm.alloc_string("bits")),
                asm.alloc_type(Type::Int(Int::U32)),
            ),
        ],
    }
}

/// Register the canonical wide integer-to-`f32` fallbacks with the linker's missing-method
/// patcher.  Existing compiler-builtins definitions win during normal method resolution.
pub fn insert_int_to_float(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("__floatundisf");
    patcher.insert(name, Box::new(|_, asm| u64_body(asm)));

    let name = asm.alloc_string("__floatuntisf");
    patcher.insert(name, Box::new(|_, asm| u128_body(asm)));

    let name = asm.alloc_string("__floattisf");
    patcher.insert(name, Box::new(|_, asm| i128_body(asm)));
}
