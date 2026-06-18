use cilly::{
    cilnode::{IsPure, MethodKind},
    Assembly, BinOp, ClassRef, Float, Int, Interned, MethodRef, Type,
};
use rustc_codegen_clr_ctx::MethodCompileCtx;
use rustc_codegen_clr_type::{r#type::get_type, utilis::is_fat_ptr};
use rustc_middle::ty::{FloatTy, IntTy, Ty, TyKind, UintTy};

type Node = Interned<cilly::v2::CILNode>;

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
            UintTy::U128 => {
                let main_module = *ctx.main_module();
                let mref = MethodRef::new(
                    main_module,
                    ctx.alloc_string("eq_u128"),
                    ctx.sig([Type::Int(Int::U128), Type::Int(Int::U128)], Type::Bool),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
            }
            _ => ctx.biop(operand_a, operand_b, BinOp::Eq),
        },
        TyKind::Int(int) => match int {
            IntTy::I128 => {
                let main_module = *ctx.main_module();
                let mref = MethodRef::new(
                    main_module,
                    ctx.alloc_string("eq_i128"),
                    ctx.sig([Type::Int(Int::I128), Type::Int(Int::I128)], Type::Bool),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
            }
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
        TyKind::Float(FloatTy::F128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("__eqtf2"),
                ctx.sig(
                    [Type::Float(Float::F128), Type::Float(Float::F128)],
                    Type::Bool,
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Float(FloatTy::F16) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("eq_f16"),
                ctx.sig(
                    [Type::Float(Float::F16), Type::Float(Float::F16)],
                    Type::Bool,
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        _ => panic!("Can't eq type  {ty_a:?}"),
    }
}
pub fn lt_unchecked(ty_a: Ty<'_>, operand_a: Node, operand_b: Node, asm: &mut Assembly) -> Node {
    //return CILOp::Lt;
    match ty_a.kind() {
        TyKind::Uint(uint) => match uint {
            UintTy::U128 => {
                let mref = MethodRef::new(
                    ClassRef::uint_128(asm),
                    asm.alloc_string("op_LessThan"),
                    asm.sig([Type::Int(Int::U128), Type::Int(Int::U128)], Type::Bool),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = asm.alloc_methodref(mref);
                asm.call(mref, &[operand_a, operand_b], IsPure::NOT)
            }
            _ => asm.biop(operand_a, operand_b, BinOp::LtUn),
        },
        TyKind::Int(int) => match int {
            IntTy::I128 => {
                let mref = MethodRef::new(
                    ClassRef::int_128(asm),
                    asm.alloc_string("op_LessThan"),
                    asm.sig([Type::Int(Int::I128), Type::Int(Int::I128)], Type::Bool),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = asm.alloc_methodref(mref);
                asm.call(mref, &[operand_a, operand_b], IsPure::NOT)
            }
            _ => asm.biop(operand_a, operand_b, BinOp::Lt),
        },
        // TODO: are chars considered signed or unsigned?
        TyKind::Bool | TyKind::Char | TyKind::Float(FloatTy::F32 | FloatTy::F64) => {
            asm.biop(operand_a, operand_b, BinOp::Lt)
        }
        TyKind::RawPtr(_, _) | TyKind::FnPtr(_, _) => asm.biop(operand_a, operand_b, BinOp::LtUn),
        TyKind::Float(FloatTy::F128) => {
            let mref = MethodRef::new(
                *asm.main_module(),
                asm.alloc_string("__lttf2"),
                asm.sig(
                    [Type::Float(Float::F128), Type::Float(Float::F128)],
                    Type::Bool,
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Float(FloatTy::F16) => {
            let mref = MethodRef::new(
                *asm.main_module(),
                asm.alloc_string("lt_f16"),
                asm.sig(
                    [Type::Float(Float::F16), Type::Float(Float::F16)],
                    Type::Bool,
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        _ => panic!("Can't eq type  {ty_a:?}"),
    }
}
pub fn gt_unchecked(ty_a: Ty<'_>, operand_a: Node, operand_b: Node, asm: &mut Assembly) -> Node {
    match ty_a.kind() {
        TyKind::Uint(uint) => match uint {
            UintTy::U128 => {
                let mref = MethodRef::new(
                    ClassRef::uint_128(asm),
                    asm.alloc_string("op_GreaterThan"),
                    asm.sig([Type::Int(Int::U128), Type::Int(Int::U128)], Type::Bool),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = asm.alloc_methodref(mref);
                asm.call(mref, &[operand_a, operand_b], IsPure::NOT)
            }
            _ => asm.biop(operand_a, operand_b, BinOp::GtUn),
        },
        TyKind::Int(int) => match int {
            IntTy::I128 => {
                let mref = MethodRef::new(
                    ClassRef::int_128(asm),
                    asm.alloc_string("op_GreaterThan"),
                    asm.sig([Type::Int(Int::I128), Type::Int(Int::I128)], Type::Bool),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = asm.alloc_methodref(mref);
                asm.call(mref, &[operand_a, operand_b], IsPure::NOT)
            }
            _ => asm.biop(operand_a, operand_b, BinOp::Gt),
        },
        // TODO: are chars considered signed or unsigned?
        TyKind::Bool | TyKind::Char | TyKind::Float(FloatTy::F32 | FloatTy::F64) => {
            asm.biop(operand_a, operand_b, BinOp::Gt)
        }
        TyKind::Float(FloatTy::F128) => {
            let mref = MethodRef::new(
                *asm.main_module(),
                asm.alloc_string("__gttf2"),
                asm.sig(
                    [Type::Float(Float::F128), Type::Float(Float::F128)],
                    Type::Bool,
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Float(FloatTy::F16) => {
            let mref = MethodRef::new(
                *asm.main_module(),
                asm.alloc_string("gt_f16"),
                asm.sig(
                    [Type::Float(Float::F16), Type::Float(Float::F16)],
                    Type::Bool,
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::RawPtr(_, _) => asm.biop(operand_a, operand_b, BinOp::GtUn),
        _ => panic!("Can't eq type  {ty_a:?}"),
    }
}
