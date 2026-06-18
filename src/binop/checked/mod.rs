use crate::{assembly::MethodCompileCtx, casts};
use cilly::{
    cilnode::{ExtendKind, IsPure},
    BinOp, Interned, Type,
    {cilnode::MethodKind, Assembly, ClassRef, Int, MethodRef},
};
use rustc_codegen_clr_type::utilis::simple_tuple;
use rustc_codegen_clr_type::GetTypeExt;
use rustc_middle::ty::{IntTy, Ty, TyKind, UintTy};

type Node = Interned<cilly::ir::CILNode>;

pub fn result_tuple(tpe: Type, out_of_range: Node, val: Node, asm: &mut Assembly) -> Node {
    let tuple = simple_tuple(&[tpe, Type::Bool], asm);
    asm.ovf_check_tuple(tuple, out_of_range, val, tpe)
}
pub fn zero(ty: Ty, asm: &mut Assembly) -> Node {
    match ty.kind() {
        TyKind::Uint(UintTy::U8) => asm.alloc_node(0_u8),
        TyKind::Uint(UintTy::U16) => asm.alloc_node(0_u16),
        TyKind::Uint(UintTy::U32) => asm.alloc_node(0_u32),
        TyKind::Uint(UintTy::U64) => asm.alloc_node(0_u64),
        TyKind::Uint(UintTy::Usize) => asm.alloc_node(0_usize),
        TyKind::Int(IntTy::I8) => asm.alloc_node(0_i8),
        TyKind::Int(IntTy::I16) => asm.alloc_node(0_i16),
        TyKind::Int(IntTy::I32) => asm.alloc_node(0_i32),
        TyKind::Int(IntTy::I64) => asm.alloc_node(0_i64),
        TyKind::Int(IntTy::Isize) => asm.alloc_node(0_isize),
        TyKind::Uint(UintTy::U128) => asm.alloc_node(0_u128),
        TyKind::Int(IntTy::I128) => asm.alloc_node(0_i128),
        _ => todo!("Can't get zero of {ty:?}"),
    }
}
fn min(ty: Ty, asm: &mut Assembly) -> Node {
    match ty.kind() {
        TyKind::Uint(UintTy::U8) => asm.alloc_node(u8::MIN),
        TyKind::Uint(UintTy::U16) => asm.alloc_node(u16::MIN),
        TyKind::Uint(UintTy::U32) => asm.alloc_node(u32::MIN),
        TyKind::Uint(UintTy::U64) => asm.alloc_node(u64::MIN),
        TyKind::Int(IntTy::I8) => asm.alloc_node(i8::MIN),
        TyKind::Int(IntTy::I16) => asm.alloc_node(i16::MIN),
        TyKind::Int(IntTy::I32) => asm.alloc_node(i32::MIN),
        TyKind::Int(IntTy::I64) => asm.alloc_node(i64::MIN),
        TyKind::Uint(UintTy::Usize) => {
            let mref = MethodRef::new(
                ClassRef::usize_type(asm),
                asm.alloc_string("get_MinValue"),
                asm.sig([], Type::Int(Int::USize)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            const EMPTY: [Interned<cilly::ir::CILNode>; 0] = [];
            asm.call(mref, &EMPTY, IsPure::NOT)
        }
        TyKind::Int(IntTy::Isize) => {
            let mref = MethodRef::new(
                ClassRef::isize_type(asm),
                asm.alloc_string("get_MinValue"),
                asm.sig([], Type::Int(Int::ISize)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            const EMPTY: [Interned<cilly::ir::CILNode>; 0] = [];
            asm.call(mref, &EMPTY, IsPure::NOT)
        }
        TyKind::Uint(UintTy::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("get_MinValue"),
                asm.sig([], Type::Int(Int::U128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            const EMPTY: [Interned<cilly::ir::CILNode>; 0] = [];
            asm.call(mref, &EMPTY, IsPure::NOT)
        }
        TyKind::Int(IntTy::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("get_MinValue"),
                asm.sig([], Type::Int(Int::I128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            const EMPTY: [Interned<cilly::ir::CILNode>; 0] = [];
            asm.call(mref, &EMPTY, IsPure::NOT)
        }
        _ => todo!("Can't get min of {ty:?}"),
    }
}
fn max(ty: Ty, asm: &mut Assembly) -> Node {
    match ty.kind() {
        TyKind::Uint(UintTy::U8) => asm.alloc_node(u8::MAX),
        TyKind::Uint(UintTy::U16) => asm.alloc_node(u16::MAX),
        TyKind::Uint(UintTy::U32) => asm.alloc_node(u32::MAX),
        TyKind::Uint(UintTy::U64) => asm.alloc_node(u64::MAX),
        TyKind::Int(IntTy::I8) => asm.alloc_node(i8::MAX),
        TyKind::Int(IntTy::I16) => asm.alloc_node(i16::MAX),
        TyKind::Int(IntTy::I32) => asm.alloc_node(i32::MAX),
        TyKind::Int(IntTy::I64) => asm.alloc_node(i64::MAX),
        TyKind::Uint(UintTy::Usize) => {
            let mref = MethodRef::new(
                ClassRef::usize_type(asm),
                asm.alloc_string("get_MaxValue"),
                asm.sig([], Type::Int(Int::USize)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            const EMPTY: [Interned<cilly::ir::CILNode>; 0] = [];
            asm.call(mref, &EMPTY, IsPure::NOT)
        }
        TyKind::Int(IntTy::Isize) => {
            let mref = MethodRef::new(
                ClassRef::isize_type(asm),
                asm.alloc_string("get_MaxValue"),
                asm.sig([], Type::Int(Int::ISize)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            const EMPTY: [Interned<cilly::ir::CILNode>; 0] = [];
            asm.call(mref, &EMPTY, IsPure::NOT)
        }
        TyKind::Uint(UintTy::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("get_MaxValue"),
                asm.sig([], Type::Int(Int::U128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            const EMPTY: [Interned<cilly::ir::CILNode>; 0] = [];
            asm.call(mref, &EMPTY, IsPure::NOT)
        }
        TyKind::Int(IntTy::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("get_MaxValue"),
                asm.sig([], Type::Int(Int::I128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            const EMPTY: [Interned<cilly::ir::CILNode>; 0] = [];
            asm.call(mref, &EMPTY, IsPure::NOT)
        }
        _ => todo!("Can't get max of {ty:?}"),
    }
}

/// `conv_u8` mirror: zero-extend to U8.
fn cu8(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::U8, ExtendKind::ZeroExtend)
}
fn cu32(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::U32, ExtendKind::ZeroExtend)
}
fn cu64(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::U64, ExtendKind::ZeroExtend)
}
fn ci16(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::I16, ExtendKind::SignExtend)
}
fn ci32(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::I32, ExtendKind::SignExtend)
}
fn ci64(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::I64, ExtendKind::SignExtend)
}
fn ci8(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::I8, ExtendKind::SignExtend)
}

pub fn mul<'tcx>(
    ops_a: Node,
    ops_b: Node,
    ty: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    //(b > 0 && a < INT_MIN + b) || (b < 0 && a > INT_MAX + b);
    let tpe = ctx.type_from_cache(ty);
    let mul = super::mul_unchecked(ty, ctx, ops_a, ops_b);
    let ovf = match ty.kind() {
        // Work without promotions
        TyKind::Uint(UintTy::U8) => {
            let a = cu8(ctx, ops_a);
            let b = cu8(ctx, ops_b);
            let mul = ctx.biop(a, b, BinOp::Mul);
            let mx = max(ty, ctx);
            let mx = cu8(ctx, mx);
            ctx.biop(mul, mx, BinOp::GtUn)
        }
        TyKind::Uint(UintTy::U16) => {
            let a = cu32(ctx, ops_a);
            let b = cu32(ctx, ops_b);
            let mul = ctx.biop(a, b, BinOp::Mul);
            let mx = max(ty, ctx);
            let mx = cu32(ctx, mx);
            ctx.biop(mul, mx, BinOp::GtUn)
        }
        TyKind::Int(IntTy::I8) => {
            let a = ci16(ctx, ops_a);
            let b = ci16(ctx, ops_b);
            let mul = ctx.biop(a, b, BinOp::Mul);
            let mx = max(ty, ctx);
            let mx = ci16(ctx, mx);
            let gt = ctx.biop(mul, mx, BinOp::Gt);
            let mn = min(ty, ctx);
            let mn = ci16(ctx, mn);
            let lt = ctx.biop(mul, mn, BinOp::Lt);
            ctx.biop(gt, lt, BinOp::Or)
        }
        TyKind::Int(IntTy::I16) => {
            let a = ci32(ctx, ops_a);
            let b = ci32(ctx, ops_b);
            let mul = ctx.biop(a, b, BinOp::Mul);
            let mx = max(ty, ctx);
            let mx = ci32(ctx, mx);
            let gt = ctx.biop(mul, mx, BinOp::Gt);
            let mn = min(ty, ctx);
            let mn = ci32(ctx, mn);
            let lt = ctx.biop(mul, mn, BinOp::Lt);
            ctx.biop(gt, lt, BinOp::Or)
        }
        // Works with 32 -> 64 size promotions
        TyKind::Uint(UintTy::U32) => {
            let a = cu64(ctx, ops_a);
            let b = cu64(ctx, ops_b);
            let mul = ctx.biop(a, b, BinOp::Mul);
            let mx = max(ty, ctx);
            let mx = cu64(ctx, mx);
            ctx.biop(mul, mx, BinOp::GtUn)
        }
        TyKind::Int(IntTy::I32) => {
            let a = ci64(ctx, ops_a);
            let b = ci64(ctx, ops_b);
            let mul = ctx.biop(a, b, BinOp::Mul);
            let mx = max(ty, ctx);
            let mx = ci64(ctx, mx);
            let gt = ctx.biop(mul, mx, BinOp::Gt);
            let mn = min(ty, ctx);
            let mn = ci64(ctx, mn);
            let lt = ctx.biop(mul, mn, BinOp::Lt);
            ctx.biop(gt, lt, BinOp::Or)
        }
        // Use 128 bit ints, not supported in mono.
        TyKind::Uint(UintTy::U64) => {
            let op_mul = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("op_Multiply"),
                ctx.sig(
                    [Type::Int(Int::U128), Type::Int(Int::U128)],
                    Type::Int(Int::U128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let a = casts::int_to_int(Type::Int(Int::U64), Type::Int(Int::U128), ops_a, ctx);
            let b = casts::int_to_int(Type::Int(Int::U64), Type::Int(Int::U128), ops_b, ctx);
            let op_mul = ctx.alloc_methodref(op_mul);
            let mul = ctx.call(op_mul, &[a, b], IsPure::NOT);
            let op_gt = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("op_GreaterThan"),
                ctx.sig([Type::Int(Int::U128), Type::Int(Int::U128)], Type::Bool),
                MethodKind::Static,
                vec![].into(),
            );
            let mx = max(ty, ctx);
            let mx = casts::int_to_int(Type::Int(Int::U64), Type::Int(Int::U128), mx, ctx);
            let op_gt = ctx.alloc_methodref(op_gt);
            ctx.call(op_gt, &[mul, mx], IsPure::NOT)
        }
        TyKind::Int(IntTy::I64) => {
            let main_module = *ctx.main_module();
            let op_mul = MethodRef::new(
                main_module,
                ctx.alloc_string("mul_i128"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let a = casts::int_to_int(Type::Int(Int::I64), Type::Int(Int::I128), ops_a, ctx);
            let b = casts::int_to_int(Type::Int(Int::I64), Type::Int(Int::I128), ops_b, ctx);
            let op_mul = ctx.alloc_methodref(op_mul);
            let mul = ctx.call(op_mul, &[a, b], IsPure::NOT);
            let op_greater_than = MethodRef::new(
                main_module,
                ctx.alloc_string("gt_i128"),
                ctx.sig([Type::Int(Int::I128), Type::Int(Int::I128)], Type::Bool),
                MethodKind::Static,
                vec![].into(),
            );
            let mx = max(ty, ctx);
            let mx = casts::int_to_int(Type::Int(Int::I64), Type::Int(Int::I128), mx, ctx);
            let op_greater_than = ctx.alloc_methodref(op_greater_than);
            let gt = ctx.call(op_greater_than, &[mul, mx], IsPure::NOT);
            let op_lt = MethodRef::new(
                main_module,
                ctx.alloc_string("lt_i128"),
                ctx.sig([Type::Int(Int::I128), Type::Int(Int::I128)], Type::Bool),
                MethodKind::Static,
                vec![].into(),
            );
            let mn = min(ty, ctx);
            let mn = casts::int_to_int(Type::Int(Int::I64), Type::Int(Int::I128), mn, ctx);
            let op_lt = ctx.alloc_methodref(op_lt);
            let lt = ctx.call(op_lt, &[mul, mn], IsPure::NOT);
            ctx.biop(gt, lt, BinOp::Or)
        }

        TyKind::Uint(UintTy::Usize) => {
            let main_module = *ctx.main_module();
            let op_mul = MethodRef::new(
                main_module,
                ctx.alloc_string("mul_u128"),
                ctx.sig(
                    [Type::Int(Int::U128), Type::Int(Int::U128)],
                    Type::Int(Int::U128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let a = casts::int_to_int(Type::Int(Int::USize), Type::Int(Int::U128), ops_a, ctx);
            let b = casts::int_to_int(Type::Int(Int::USize), Type::Int(Int::U128), ops_b, ctx);
            let op_mul = ctx.alloc_methodref(op_mul);
            let mul = ctx.call(op_mul, &[a, b], IsPure::NOT);
            let op_gt = MethodRef::new(
                main_module,
                ctx.alloc_string("gt_u128"),
                ctx.sig([Type::Int(Int::U128), Type::Int(Int::U128)], Type::Bool),
                MethodKind::Static,
                vec![].into(),
            );
            let mx = max(ty, ctx);
            let mx = casts::int_to_int(Type::Int(Int::USize), Type::Int(Int::U128), mx, ctx);
            let op_gt = ctx.alloc_methodref(op_gt);
            ctx.call(op_gt, &[mul, mx], IsPure::NOT)
        }
        TyKind::Int(IntTy::Isize) => {
            let main_module = *ctx.main_module();
            let op_mul = MethodRef::new(
                main_module,
                ctx.alloc_string("mul_i128"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let a = casts::int_to_int(Type::Int(Int::ISize), Type::Int(Int::I128), ops_a, ctx);
            let b = casts::int_to_int(Type::Int(Int::ISize), Type::Int(Int::I128), ops_b, ctx);
            let op_mul = ctx.alloc_methodref(op_mul);
            let mul = ctx.call(op_mul, &[a, b], IsPure::NOT);
            let op_greater_than = MethodRef::new(
                main_module,
                ctx.alloc_string("gt_i128"),
                ctx.sig([Type::Int(Int::I128), Type::Int(Int::I128)], Type::Bool),
                MethodKind::Static,
                vec![].into(),
            );
            let mx = max(ty, ctx);
            let mx = casts::int_to_int(Type::Int(Int::ISize), Type::Int(Int::I128), mx, ctx);
            let op_greater_than = ctx.alloc_methodref(op_greater_than);
            let gt = ctx.call(op_greater_than, &[mul, mx], IsPure::NOT);
            let op_lt = MethodRef::new(
                main_module,
                ctx.alloc_string("lt_i128"),
                ctx.sig([Type::Int(Int::I128), Type::Int(Int::I128)], Type::Bool),
                MethodKind::Static,
                vec![].into(),
            );
            let mn = min(ty, ctx);
            let mn = casts::int_to_int(Type::Int(Int::ISize), Type::Int(Int::I128), mn, ctx);
            let op_lt = ctx.alloc_methodref(op_lt);
            let lt = ctx.call(op_lt, &[mul, mn], IsPure::NOT);
            ctx.biop(gt, lt, BinOp::Or)
        }
        TyKind::Int(IntTy::I128) => {
            let op_mul = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("i128_mul_ovf_check"),
                ctx.sig([Type::Int(Int::I128), Type::Int(Int::I128)], Type::Bool),
                MethodKind::Static,
                vec![].into(),
            );
            let op_mul = ctx.alloc_methodref(op_mul);
            let called = ctx.call(op_mul, &[ops_a, ops_b], IsPure::NOT);
            let f = ctx.alloc_node(false);
            ctx.biop(called, f, BinOp::Eq)
        }
        TyKind::Uint(UintTy::U128) => {
            let op_mul = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("u128_mul_ovf_check"),
                ctx.sig([Type::Int(Int::U128), Type::Int(Int::U128)], Type::Bool),
                MethodKind::Static,
                vec![].into(),
            );
            let op_mul = ctx.alloc_methodref(op_mul);
            let called = ctx.call(op_mul, &[ops_a, ops_b], IsPure::NOT);
            let f = ctx.alloc_node(false);
            ctx.biop(called, f, BinOp::Eq)
        }
        _ => {
            eprintln!("WARINING: can't checked mul type {ty:?}");
            ctx.alloc_node(false)
        }
    };
    result_tuple(tpe, ovf, mul, ctx)
}
pub fn sub_signed<'tcx>(
    ops_a: Node,
    ops_b: Node,
    ty: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    let tpe = ctx.type_from_cache(ty);
    let min = min(ty, ctx);
    let max = max(ty, ctx);
    // (b > 0 && a < MIN + b) || (b < 0 && a > MAX + b)
    let z = zero(ty, ctx);
    let b_gt_zero = super::cmp::gt_unchecked(ty, ops_b, z, ctx);
    let min_plus_b = super::add_unchecked(ty, ty, ctx, min, ops_b);
    let a_lt = super::cmp::lt_unchecked(ty, ops_a, min_plus_b, ctx);
    let left = ctx.biop(b_gt_zero, a_lt, BinOp::And);

    let z = zero(ty, ctx);
    let b_lt_zero = super::cmp::lt_unchecked(ty, ops_b, z, ctx);
    let max_plus_b = super::add_unchecked(ty, ty, ctx, max, ops_b);
    let a_gt = super::cmp::gt_unchecked(ty, ops_a, max_plus_b, ctx);
    let right = ctx.biop(b_lt_zero, a_gt, BinOp::And);

    let out_of_range = ctx.biop(left, right, BinOp::Or);
    let val = super::sub_unchecked(ty, ty, ctx, ops_a, ops_b);
    result_tuple(tpe, out_of_range, val, ctx)
}
pub fn sub_unsigned<'tcx>(
    ops_a: Node,
    ops_b: Node,
    ty: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    let tpe = ctx.type_from_cache(ty);
    let out_of_range = super::cmp::lt_unchecked(ty, ops_a, ops_b, ctx);
    let val = super::sub_unchecked(ty, ty, ctx, ops_a, ops_b);
    result_tuple(tpe, out_of_range, val, ctx)
}
pub fn add_unsigned<'tcx>(
    ops_a: Node,
    ops_b: Node,
    ty: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    let tpe = ctx.type_from_cache(ty);
    let res = super::add_unchecked(ty, ty, ctx, ops_a, ops_b);

    let out_of_range = super::cmp::lt_unchecked(ty, res, ops_a, ctx);
    result_tuple(tpe, out_of_range, res, ctx)
}
pub fn add_signed<'tcx>(
    ops_a: Node,
    ops_b: Node,
    ty: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    let tpe = ctx.type_from_cache(ty);
    match ty.kind() {
        TyKind::Int(IntTy::I8) => {
            let a = ci16(ctx, ops_a);
            let b = ci16(ctx, ops_b);
            let sum = ctx.biop(a, b, BinOp::Add);
            let lo = ctx.alloc_node(i16::from(i8::MIN));
            let lt = ctx.biop(sum, lo, BinOp::Lt);
            let hi = ctx.alloc_node(i16::from(i8::MAX));
            let gt = ctx.biop(sum, hi, BinOp::Gt);
            let out_of_range = ctx.biop(lt, gt, BinOp::Or);
            let val = ci8(ctx, sum);
            return result_tuple(tpe, out_of_range, val, ctx);
        }
        TyKind::Int(IntTy::I16) => {
            let a = ci32(ctx, ops_a);
            let b = ci32(ctx, ops_b);
            let sum = ctx.biop(a, b, BinOp::Add);
            let lo = ctx.alloc_node(i32::from(i16::MIN));
            let lt = ctx.biop(sum, lo, BinOp::Lt);
            let hi = ctx.alloc_node(i32::from(i16::MAX));
            let gt = ctx.biop(sum, hi, BinOp::Gt);
            let out_of_range = ctx.biop(lt, gt, BinOp::Or);
            let val = ci16(ctx, sum);
            return result_tuple(tpe, out_of_range, val, ctx);
        }
        TyKind::Int(IntTy::I32) => {
            let a = ci64(ctx, ops_a);
            let b = ci64(ctx, ops_b);
            let sum = ctx.biop(a, b, BinOp::Add);
            let lo = ctx.alloc_node(i32::MIN);
            let lo = ci64(ctx, lo);
            let lt = ctx.biop(sum, lo, BinOp::Lt);
            let hi = ctx.alloc_node(i32::MAX);
            let hi = ci64(ctx, hi);
            let gt = ctx.biop(sum, hi, BinOp::Gt);
            let out_of_range = ctx.biop(lt, gt, BinOp::Or);
            let val = ci32(ctx, sum);
            return result_tuple(tpe, out_of_range, val, ctx);
        }
        _ => (),
    }
    let res = super::add_unchecked(ty, ty, ctx, ops_a, ops_b);
    // (a < 0 && b < 0 && res > 0) || (a > 0 && b > 0 && res < 0)
    let z = zero(ty, ctx);
    let a_lt = super::lt_unchecked(ty, ops_a, z, ctx);
    let z = zero(ty, ctx);
    let b_lt = super::lt_unchecked(ty, ops_b, z, ctx);
    let z = zero(ty, ctx);
    let res_gt = super::gt_unchecked(ty, res, z, ctx);
    let inner_left = ctx.biop(b_lt, res_gt, BinOp::And);
    let left = ctx.biop(a_lt, inner_left, BinOp::And);

    let z = zero(ty, ctx);
    let a_gt = super::gt_unchecked(ty, ops_a, z, ctx);
    let z = zero(ty, ctx);
    let b_gt = super::gt_unchecked(ty, ops_b, z, ctx);
    let z = zero(ty, ctx);
    let res_lt = super::lt_unchecked(ty, res, z, ctx);
    let inner_right = ctx.biop(b_gt, res_lt, BinOp::And);
    let right = ctx.biop(a_gt, inner_right, BinOp::And);

    let out_of_range = ctx.biop(left, right, BinOp::Or);
    result_tuple(tpe, out_of_range, res, ctx)
}
