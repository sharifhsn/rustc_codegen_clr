use crate::assembly::MethodCompileCtx;
use cilly::{
    cilnode::IsPure,
    Interned,
    {cilnode::MethodKind, ClassRef, MethodRef},
};
use crate::operand::handle_operand;
use crate::place::place_set;
use crate::r#type::GetTypeExt;
use rustc_middle::{
    mir::{Operand, Place},
    ty::{TyKind, UintTy},
};
use rustc_span::Spanned;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

pub fn bswap<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        1,
        "The intrinsic `bswap` MUST take in exactly 1 argument!"
    );
    let ty = args[0].node.ty(ctx.body(), ctx.tcx());
    let ty = ctx.monomorphize(ty);
    let tpe = ctx.type_from_cache(ty);
    let operand = handle_operand(&args[0].node, ctx);
    let value_calc: Node = match ty.kind() {
        TyKind::Uint(UintTy::U8) => operand,
        TyKind::Uint(_) | TyKind::Int(_) => {
            let mref = MethodRef::new(
                ClassRef::binary_primitives(ctx),
                ctx.alloc_string("ReverseEndianness"),
                ctx.sig([tpe], tpe),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand], IsPure::NOT)
        }

        _ => todo!("Can't bswap {tpe:?}"),
    };
    place_set(destination, value_calc, ctx)
}
