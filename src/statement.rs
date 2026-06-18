use crate::assembly::MethodCompileCtx;
use crate::rvalue::is_rvalue_unint;
use crate::utilis::adt::set_discr;
use rustc_codegen_clr_place::{place_address, place_get, place_set};
use rustc_codegen_clr_type::utilis::is_zst;
use rustc_codegen_clr_type::GetTypeExt;

use cilly::cilnode::ExtendKind;
use cilly::{BinOp, Int, Interned};

use rustc_codgen_clr_operand::handle_operand;
use rustc_middle::mir::{CopyNonOverlapping, NonDivergingIntrinsic, Statement, StatementKind};

type Root = Interned<cilly::v2::CILRoot>;
#[allow(clippy::match_same_arms)]
pub fn handle_statement<'tcx>(
    statement: &Statement<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Vec<Root> {
    let kind = &statement.kind;
    match kind {
        StatementKind::StorageLive(_local) => vec![],
        StatementKind::StorageDead(_local) => vec![],
        StatementKind::SetDiscriminant {
            place,
            variant_index,
        } => {
            let owner_ty = place.ty(ctx.body(), ctx.tcx()).ty;
            let owner_ty = ctx.monomorphize(owner_ty);
            let owner = ctx.type_from_cache(owner_ty);

            let layout = ctx.layout_of(owner_ty);
            //let (disrc_type, _) = adt::enum_tag_info(&layout.layout, tcx);
            let cilly::Type::ClassRef(owner) = owner else {
                panic!("Nonsense operation: attempted to set the discriminant of type {owner_ty:?}, which is not valid.");
            };
            //ops.push();

            let addr = place_address(place, ctx);
            let root = set_discr(layout.layout, *variant_index, addr, owner, owner_ty, ctx);
            vec![root]
        }
        StatementKind::Assign(place_rvalue) => {
            if is_rvalue_unint(&place_rvalue.as_ref().1, ctx) {
                return vec![];
            }
            let place = place_rvalue.as_ref().0;
            let rvalue = &place_rvalue.as_ref().1;
            let ty = ctx.monomorphize(place.ty(ctx.body(), ctx.tcx()).ty);
            // Skip void assignments. Assigining to or from void type is a NOP.
            if is_zst(ctx.monomorphize(ty), ctx.tcx()) {
                return vec![];
            }
            let tpe = ctx.type_from_cache(ty);
            let tpe = ctx.alloc_type(tpe);
            if crate::rvalue::is_rvalue_const_0(rvalue, ctx) {
                let addr = place_address(&place, ctx);
                let root = ctx.init_obj(addr, tpe);
                return vec![root];
            }
            let (mut trees, value_calc) = crate::rvalue::handle_rvalue(rvalue, &place, ctx);
            trees.push(place_set(&place, value_calc, ctx));
            trees
        }
        StatementKind::Intrinsic(non_diverging_intirinsic) => {
            match non_diverging_intirinsic.as_ref() {
                NonDivergingIntrinsic::Assume(_) => vec![],
                NonDivergingIntrinsic::CopyNonOverlapping(CopyNonOverlapping {
                    src,
                    dst,
                    count,
                }) => {
                    let dst_op = handle_operand(dst, ctx);
                    let src_op = handle_operand(src, ctx);
                    let count_op = handle_operand(count, ctx);
                    let src_ty = src.ty(ctx.body(), ctx.tcx());
                    let src_ty = ctx.monomorphize(src_ty);
                    let ptr_type = ctx.type_from_cache(src_ty);
                    let cilly::Type::Ptr(pointed) = ptr_type else {
                        rustc_middle::ty::print::with_no_trimmed_paths! { panic!("Copy nonoverlaping called with non-pointer type {src_ty:?}")};
                    };

                    let size = ctx.size_of(pointed);
                    let size = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
                    let len = ctx.biop(count_op, size, BinOp::Mul);
                    let root = ctx.cp_blk(dst_op, src_op, len);
                    vec![root]
                }
            }
        }
        StatementKind::FakeRead(_) => {
            panic!("Fake reads should not be passed from the backend to the forntend!")
        }
        rustc_middle::mir::StatementKind::BackwardIncompatibleDropHint { .. } => todo!(),
        StatementKind::PlaceMention(place) => {
            let val = place_get(place, ctx);
            let root = ctx.pop(val);
            vec![root]
        }

        //TODO: consider adding some .NET specific coverage info(Is that even possible?).
        StatementKind::Coverage(_) => vec![],
        // A no-op in non-const scenarions, so safe to do nothing.
        StatementKind::ConstEvalCounter => vec![],
        // A no-op does nothing, so safe to do... nothing.
        StatementKind::Nop => vec![],
        // `StatementKind::Retag` no longer exists in MIR for this rustc version (retags are now
        // tracked via `WithRetag` on `Rvalue::Use`); there is nothing to handle here.
        // A no-op at runtime.
        StatementKind::AscribeUserType(_, _) => vec![],
    }
}
