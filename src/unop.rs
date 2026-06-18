use crate::assembly::MethodCompileCtx;

use cilly::cilnode::{ExtendKind, IsPure, MethodKind};
use cilly::{BinOp, Interned, Type};
use cilly::{ClassRef, FieldDesc, Int, MethodRef};

use rustc_codegen_clr_type::r#type::get_type;

use rustc_codgen_clr_operand::handle_operand;
use rustc_middle::mir::Rvalue;
use rustc_middle::mir::{Operand, UnOp};
use rustc_middle::ty::{IntTy, TyKind, UintTy};
/// Implements an unary operation, such as negation.
pub fn unop<'tcx>(
    unnop: UnOp,
    operand: &Operand<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    rvalue: &Rvalue<'tcx>,
) -> Interned<cilly::ir::CILNode> {
    let parrent_node = handle_operand(operand, ctx);
    let ty = operand.ty(&ctx.body().local_decls, ctx.tcx());
    match unnop {
        UnOp::Neg => match ty.kind() {
            TyKind::Int(IntTy::I128) => {
                let mref = MethodRef::new(
                    ClassRef::int_128(ctx),
                    ctx.alloc_string("op_UnaryNegation"),
                    ctx.sig([Type::Int(Int::I128)], Type::Int(Int::I128)),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[parrent_node], IsPure::NOT)
            }
            TyKind::Int(IntTy::I8) => {
                let conv = ctx.int_cast(parrent_node, Int::I8, ExtendKind::SignExtend);
                ctx.neg(conv)
            }
            TyKind::Int(IntTy::I16) => {
                let conv = ctx.int_cast(parrent_node, Int::I16, ExtendKind::SignExtend);
                ctx.neg(conv)
            }
            TyKind::Uint(UintTy::U128) => {
                let mref = MethodRef::new(
                    ClassRef::uint_128(ctx),
                    ctx.alloc_string("op_UnaryNegation"),
                    ctx.sig([Type::Int(Int::U128)], Type::Int(Int::U128)),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[parrent_node], IsPure::NOT)
            }
            _ => ctx.neg(parrent_node),
        },
        UnOp::Not => match ty.kind() {
            TyKind::Bool => {
                let f = ctx.alloc_node(false);
                ctx.biop(f, parrent_node, BinOp::Eq)
            }
            TyKind::Uint(UintTy::U128) => {
                let mref = MethodRef::new(
                    ClassRef::uint_128(ctx),
                    ctx.alloc_string("op_OnesComplement"),
                    ctx.sig([Type::Int(Int::U128)], Type::Int(Int::U128)),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[parrent_node], IsPure::NOT)
            }
            TyKind::Int(IntTy::I128) => {
                let mref = MethodRef::new(
                    ClassRef::int_128(ctx),
                    ctx.alloc_string("op_OnesComplement"),
                    ctx.sig([Type::Int(Int::I128)], Type::Int(Int::I128)),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[parrent_node], IsPure::NOT)
            }
            _ => ctx.not(parrent_node),
        },
        rustc_middle::mir::UnOp::PtrMetadata => {
            let tpe = get_type(ty, ctx);
            let class = tpe.as_class_ref().expect("Invalid pointer type");

            // Check what type ty is - dyn, slices and "other" types have different behaviour.
            match ty
                .builtin_deref(true)
                .expect("Non-ptr type in PtrMetadata.")
                .kind()
            {
                TyKind::Slice(_) | TyKind::Str => {
                    let metadata = ctx.alloc_string(crate::METADATA);
                    let field = ctx.alloc_field(FieldDesc::new(
                        class,
                        metadata,
                        cilly::Type::Int(cilly::Int::USize),
                    ));
                    ctx.ld_field(parrent_node, field)
                }
                TyKind::Dynamic(..) => {
                    let metadata = ctx.alloc_string(crate::METADATA);
                    let target = get_type(rvalue.ty(ctx.body(), ctx.tcx()), ctx);
                    let field = ctx.alloc_field(FieldDesc::new(
                        class,
                        metadata,
                        cilly::Type::Int(cilly::Int::USize),
                    ));
                    let meta = ctx.ld_field(parrent_node, field);
                    ctx.transmute_on_stack(Type::Int(Int::USize), target, meta)
                }
                _ => ctx.uninit_val(Type::Void),
            }
        }
    }
}
