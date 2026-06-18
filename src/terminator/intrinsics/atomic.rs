use crate::assembly::MethodCompileCtx;
use cilly::{
    cilnode::{ExtendKind, IsPure, MethodKind},
    ClassRef, Int, Interned, MethodRef, Type,
};
use rustc_codegen_clr_place::{place_address, place_set};
use rustc_codegen_clr_type::adt::field_descrptor;
use rustc_codegen_clr_type::GetTypeExt;
use rustc_codgen_clr_operand::handle_operand;
use rustc_middle::mir::{Operand, Place};
use rustc_span::Spanned;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

pub fn xchg<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let interlocked = ClassRef::interlocked(ctx);
    // *T
    let dst = handle_operand(&args[0].node, ctx);
    // T
    let new = handle_operand(&args[1].node, ctx);

    debug_assert_eq!(
        args.len(),
        2,
        "The intrinsic `atomic_xchg` MUST take in exactly 3 argument!"
    );
    let src_type = ctx.monomorphize(args[1].node.ty(ctx.body(), ctx.tcx()));
    let src_type = ctx.type_from_cache(src_type);
    let uint8_ref = ctx.nref(Type::Int(Int::U8));
    let xchng = MethodRef::new(
        *ctx.main_module(),
        ctx.alloc_string("atomic_xchng_u8"),
        ctx.sig([uint8_ref, Type::Int(Int::U8)], Type::Int(Int::U8)),
        MethodKind::Static,
        vec![].into(),
    );
    match src_type {
        Type::Int(Int::U8) => {
            let xchng = ctx.alloc_methodref(xchng);
            let call = ctx.call(xchng, &[dst, new], IsPure::NOT);
            return place_set(destination, call, ctx);
        }
        Type::Ptr(_) => {
            let usize_ref = ctx.nref(Type::Int(Int::USize));
            let call_site = MethodRef::new(
                interlocked,
                ctx.alloc_string("Exchange"),
                ctx.sig([usize_ref, Type::Int(Int::USize)], Type::Int(Int::USize)),
                MethodKind::Static,
                vec![].into(),
            );
            let call_site = ctx.alloc_methodref(call_site);
            let usize_ptr = ctx.nref(Type::Int(Int::USize));
            let dst = ctx.cast_ptr_to(dst, usize_ptr);
            let new = ctx.int_cast(new, Int::USize, ExtendKind::ZeroExtend);
            let call = ctx.call(call_site, &[dst, new], IsPure::NOT);
            let call = ctx.cast_ptr_to(call, src_type);
            return place_set(destination, call, ctx);
        }
        Type::Int(Int::I8 | Int::U16 | Int::I16) | Type::Bool | Type::PlatformChar => {
            todo!("can't atomic_xchg {src_type:?}")
        }
        _ => (),
    }
    let src_ref = ctx.nref(src_type);
    let call_site = MethodRef::new(
        interlocked,
        ctx.alloc_string("Exchange"),
        ctx.sig([src_ref, src_type], src_type),
        MethodKind::Static,
        vec![].into(),
    );
    let call_site = ctx.alloc_methodref(call_site);
    // T
    let call = ctx.call(call_site, &[dst, new], IsPure::NOT);
    place_set(destination, call, ctx)
}
pub fn cxchg<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> [Root; 2] {
    let interlocked = ClassRef::interlocked(ctx);
    // *T
    let dst = handle_operand(&args[0].node, ctx);
    // T
    let comparand = handle_operand(&args[1].node, ctx);
    // T
    let src = handle_operand(&args[2].node, ctx);
    debug_assert_eq!(
        args.len(),
        3,
        "The intrinsic `atomic_cxchgweak_acquire_acquire` MUST take in exactly 3 argument!"
    );
    let src_type = ctx.monomorphize(args[2].node.ty(ctx.body(), ctx.tcx()));
    let src_type = ctx.type_from_cache(src_type);

    let value = src;

    #[allow(clippy::single_match_else)]
    let exchange_res = match &src_type {
        Type::Ptr(_) => {
            let usize_ref = ctx.nref(Type::Int(Int::USize));
            let call_site = MethodRef::new(
                interlocked,
                ctx.alloc_string("CompareExchange"),
                ctx.sig(
                    [usize_ref, Type::Int(Int::USize), Type::Int(Int::USize)],
                    Type::Int(Int::USize),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let call_site = ctx.alloc_methodref(call_site);
            let dst = ctx.cast_ptr_to(dst, usize_ref);
            let value = ctx.int_cast(value, Int::USize, ExtendKind::ZeroExtend);
            let comparand = ctx.int_cast(comparand, Int::USize, ExtendKind::ZeroExtend);
            let call = ctx.call(call_site, &[dst, value, comparand], IsPure::NOT);
            ctx.cast_ptr_to(call, src_type)
        }
        // TODO: this is a bug, on purpose. The 1 byte compare exchange is not supported untill .NET 9. Remove after November, when .NET 9 Releases.
        Type::Int(Int::U8) => comparand,
        _ => {
            let src_ref = ctx.nref(src_type);
            let call_site = MethodRef::new(
                interlocked,
                ctx.alloc_string("CompareExchange"),
                ctx.sig([src_ref, src_type, src_type], src_type),
                MethodKind::Static,
                vec![].into(),
            );
            let call_site = ctx.alloc_methodref(call_site);
            ctx.call(call_site, &[dst, value, comparand], IsPure::NOT)
        }
    };
    let dst_ty = destination.ty(ctx.body(), ctx.tcx());
    let val_desc = field_descrptor(dst_ty.ty, 0, ctx);
    let flag_desc = field_descrptor(dst_ty.ty, 1, ctx);
    cxchng_res_val(
        exchange_res,
        comparand,
        place_address(destination, ctx),
        val_desc,
        flag_desc,
        ctx,
    )
}
/// Builds the two roots that store the compare-exchange result: the loaded `old_val`
/// into the value field, and the `old_val == expected` flag into the flag field.
fn cxchng_res_val(
    old_val: Node,
    expected: Node,
    destination_addr: Node,
    val_desc: Interned<cilly::FieldDesc>,
    flag_desc: Interned<cilly::FieldDesc>,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> [Root; 2] {
    // Set the value of the result.
    let set_val = ctx.set_field(val_desc, destination_addr, old_val);
    // Get the result back
    let val = ctx.ld_field(destination_addr, val_desc);
    let cmp = ctx.biop(val, expected, cilly::BinOp::Eq);
    let set_flag = ctx.set_field(flag_desc, destination_addr, cmp);
    [set_val, set_flag]
}
