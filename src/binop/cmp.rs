use cilly::{
    cilnode::{IsPure, MethodKind},
    Assembly, BinOp, ClassRef, Float, Int, Interned, MethodRef, Type,
};
use crate::fn_ctx::MethodCompileCtx;
use crate::r#type::{get_type, utilis::is_fat_ptr};
use rustc_middle::ty::{FloatTy, IntTy, Ty, TyKind, UintTy};

type Node = Interned<cilly::ir::CILNode>;

pub fn ne_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    operand_a: Node,
    operand_b: Node,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    //vec![eq_unchecked(ty_a), CILOp::LdcI32(0), CILOp::Eq]
    let eq = eq_unchecked(ty_a, operand_a, operand_b, ctx);
    let f = ctx.alloc_node(false);
    ctx.biop(eq, f, BinOp::Eq)
}
pub fn eq_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    operand_a: Node,
    operand_b: Node,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    match ty_a.kind() {
        TyKind::Uint(uint) => match uint {
            UintTy::U128 => ctx.call_static(
                "eq_u128",
                [Type::Int(Int::U128), Type::Int(Int::U128)],
                Type::Bool,
                &[operand_a, operand_b],
            ),
            _ => ctx.biop(operand_a, operand_b, BinOp::Eq),
        },
        TyKind::Int(int) => match int {
            IntTy::I128 => ctx.call_static(
                "eq_i128",
                [Type::Int(Int::I128), Type::Int(Int::I128)],
                Type::Bool,
                &[operand_a, operand_b],
            ),
            _ => ctx.biop(operand_a, operand_b, BinOp::Eq),
        },
        TyKind::Bool | TyKind::Char | TyKind::Float(FloatTy::F32 | FloatTy::F64) => {
            ctx.biop(operand_a, operand_b, BinOp::Eq)
        }
        TyKind::RawPtr(_, _) | TyKind::FnPtr(_, _) => {
            if is_fat_ptr(ty_a, ctx.tcx(), ctx.instance()) {
                let tpe = get_type(ty_a, ctx).as_class_ref().unwrap();
                let f0 = Interned::data_ptr(ctx, tpe);
                let a0 = ctx.ld_field(operand_a, f0);
                let b0 = ctx.ld_field(operand_b, f0);
                let f0 = ctx.biop(a0, b0, BinOp::Eq);
                let f1 = Interned::metadata(ctx, tpe);
                let a1 = ctx.ld_field(operand_a, f1);
                let b1 = ctx.ld_field(operand_b, f1);
                let f1 = ctx.biop(a1, b1, BinOp::Eq);
                ctx.biop(f0, f1, BinOp::And)
            } else {
                ctx.biop(operand_a, operand_b, BinOp::Eq)
            }
        }
        TyKind::Float(FloatTy::F128) => ctx.call_static(
            "__eqtf2",
            [Type::Float(Float::F128), Type::Float(Float::F128)],
            Type::Bool,
            &[operand_a, operand_b],
        ),
        TyKind::Float(FloatTy::F16) => ctx.call_static(
            "eq_f16",
            [Type::Float(Float::F16), Type::Float(Float::F16)],
            Type::Bool,
            &[operand_a, operand_b],
        ),
        _ => panic!("Can't eq type  {ty_a:?}"),
    }
}
/// Calls the static `op_LessThan`/`op_GreaterThan` operator on .NET's `(U)Int128`, since the 128-bit
/// orderings have no native CIL instruction (unlike eq, which routes through a main-module helper).
fn call_int128_cmp(
    asm: &mut Assembly,
    class: Interned<ClassRef>,
    int: Int,
    op: &str,
    a: Node,
    b: Node,
) -> Node {
    let mref = MethodRef::new(
        class,
        asm.alloc_string(op),
        asm.sig([Type::Int(int), Type::Int(int)], Type::Bool),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    asm.call(mref, &[a, b], IsPure::NOT)
}

/// Generates an ordering comparison fn (`lt_unchecked`/`gt_unchecked`) from a table of the per-type
/// behavior. `lt` and `gt` share an identical `match ty_a.kind()` skeleton; only these parameters
/// differ. They are kept EXPLICIT at the invocation site (never inferred), because the
/// sign-agnostic-stack / unordered-float subtleties live here:
/// * `$op128` — the static .NET `(U)Int128` operator name for the 128-bit path.
/// * `$f128`/`$f16` — the main-module soft-float helpers for `f128`/`f16`.
/// * `$signed` — the `BinOp` for signed/scalar ordering (`Lt`/`Gt`).
/// * `$unsigned` — the `BinOp` for unsigned/pointer ordering (`LtUn`/`GtUn`).
/// * `$ptr_pat` — the exact pointer `TyKind` pattern routed through the unsigned op (`lt` includes
///   `FnPtr`; `gt` historically matched only `RawPtr`). Pattern order vs. the float arms is
///   irrelevant since the patterns are disjoint.
macro_rules! cmp_op {
    ($fn_name:ident, $op128:literal, $f128:literal, $f16:literal, $signed:expr, $unsigned:expr, $ptr_pat:pat) => {
        pub fn $fn_name(
            ty_a: Ty<'_>,
            operand_a: Node,
            operand_b: Node,
            asm: &mut Assembly,
        ) -> Node {
            match ty_a.kind() {
                TyKind::Uint(uint) => match uint {
                    UintTy::U128 => {
                        let class = ClassRef::uint_128(asm);
                        call_int128_cmp(asm, class, Int::U128, $op128, operand_a, operand_b)
                    }
                    _ => asm.biop(operand_a, operand_b, $unsigned),
                },
                TyKind::Int(int) => match int {
                    IntTy::I128 => {
                        let class = ClassRef::int_128(asm);
                        call_int128_cmp(asm, class, Int::I128, $op128, operand_a, operand_b)
                    }
                    _ => asm.biop(operand_a, operand_b, $signed),
                },
                // TODO: are chars considered signed or unsigned?
                TyKind::Bool | TyKind::Char | TyKind::Float(FloatTy::F32 | FloatTy::F64) => {
                    asm.biop(operand_a, operand_b, $signed)
                }
                $ptr_pat => asm.biop(operand_a, operand_b, $unsigned),
                TyKind::Float(FloatTy::F128) => asm.call_static(
                    $f128,
                    [Type::Float(Float::F128), Type::Float(Float::F128)],
                    Type::Bool,
                    &[operand_a, operand_b],
                ),
                TyKind::Float(FloatTy::F16) => asm.call_static(
                    $f16,
                    [Type::Float(Float::F16), Type::Float(Float::F16)],
                    Type::Bool,
                    &[operand_a, operand_b],
                ),
                _ => panic!("Can't eq type  {ty_a:?}"),
            }
        }
    };
}

cmp_op!(
    lt_unchecked,
    "op_LessThan",
    "__lttf2",
    "lt_f16",
    BinOp::Lt,
    BinOp::LtUn,
    TyKind::RawPtr(_, _) | TyKind::FnPtr(_, _)
);
cmp_op!(
    gt_unchecked,
    "op_GreaterThan",
    "__gttf2",
    "gt_f16",
    BinOp::Gt,
    BinOp::GtUn,
    TyKind::RawPtr(_, _)
);
