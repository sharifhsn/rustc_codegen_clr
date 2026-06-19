use crate::assembly::MethodCompileCtx;
use cilly::{
    cilnode::{IsPure, MethodKind}, BinOp,
    BranchCond, CILNode, CILRoot, ClassRef, Const, FieldDesc, FnSig, Int, Interned, MethodRef, Type,
};

type Root = Interned<cilly::ir::CILRoot>;
use rustc_codegen_clr_ctx::function_name;
use rustc_codegen_clr_place::{place_address, place_set};
use rustc_codegen_clr_type::GetTypeExt;
use rustc_middle::mir::AssertKind;

use rustc_codgen_clr_operand::{
    constant::{load_const_int, load_const_uint},
    handle_operand,
};
use rustc_middle::{
    mir::{BasicBlock, Operand, Place, SwitchTargets, Terminator, TerminatorKind},
    ty::{Instance, InstanceKind, Ty, TyKind},
};
use rustc_span::Spanned;

mod call;
mod intrinsics;
/// Builds an unconditional branch root targeting `target`.
fn goto(ctx: &mut MethodCompileCtx<'_, '_>, target: u32) -> Root {
    ctx.alloc_root(CILRoot::Branch(Box::new((target, 0, None))))
}
pub fn handle_call_terminator<'tycxt>(
    terminator: &Terminator<'tycxt>,
    ctx: &mut MethodCompileCtx<'tycxt, '_>,
    args: &[Spanned<Operand<'tycxt>>],
    destination: &Place<'tycxt>,
    func: &Operand<'tycxt>,
    target: Option<BasicBlock>,
) -> Vec<Root> {
    let mut trees = Vec::new();

    let func_ty = func.ty(ctx.body(), ctx.tcx());
    let fn_ty = ctx.monomorphize(func_ty);
    // Get the pointed type, if byref;
    let func_ty = match func_ty.builtin_deref(true) {
        None => func_ty,
        Some(inner) => inner,
    };
    match func_ty.kind() {
        TyKind::FnDef(_, _) => {
            assert!(
                fn_ty.is_fn(),
                "fn_ty{fn_ty:?} in call is not a function type!"
            );
            let fn_ty = ctx.monomorphize(fn_ty);
            let call_ops = call::call(fn_ty, ctx, args, destination, terminator.source_info.span);
            //eprintln!("\nCalling FnDef:{fn_ty:?}. call_ops:{call_ops:?}");
            trees.extend(call_ops);
        }
        TyKind::FnPtr(sig, _) => {
            //eprintln!("Calling FnPtr:{func_ty:?}");

            let sig = ctx.tcx().instantiate_bound_regions_with_erased(*sig);
            let sig = crate::function_sig::from_poly_sig(ctx, sig);
            let mut arg_operands = Vec::new();
            for arg in args {
                arg_operands.push(handle_operand(&arg.node, ctx));
            }
            let called_operand = handle_operand(func, ctx);
            let sig_idx = ctx.alloc_sig(sig.clone());
            if *sig.output() == cilly::Type::Void {
                let root = ctx.call_indirect_root(sig_idx, called_operand, arg_operands);
                trees.push(root);
            } else {
                let call = ctx.call_indirect(sig_idx, called_operand, arg_operands);
                let root = place_set(destination, call, ctx);
                trees.push(root);
            }
        }
        _ => todo!("Can't call type {func_ty:?}"),
    }
    // Final Jump
    if let Some(target) = target {
        let goto = goto(ctx, target.as_u32());
        trees.push(goto);
    } else {
        let root = ctx.throw_msg("Function returning `Never` returned!");
        trees.push(root);
    }
    trees
}
pub fn handle_terminator<'tcx>(
    terminator: &Terminator<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Vec<Root> {
    let res = match &terminator.kind {
        TerminatorKind::Call {
            func,
            args,
            destination,
            target,
            unwind: _,
            call_source: _,
            fn_span: _,
        } => handle_call_terminator(terminator, ctx, args, destination, func, *target),
        TerminatorKind::TailCall { .. } => todo!(),
        TerminatorKind::Return => {
            let ret = ctx.monomorphize(ctx.body().return_ty());
            if ctx.type_from_cache(ret) == cilly::Type::Void {
                vec![ctx.alloc_root(CILRoot::VoidRet)]
            } else {
                let ld = ctx.alloc_node(cilly::CILNode::LdLoc(0));
                vec![ctx.alloc_root(CILRoot::Ret(ld))]
            }
        }
        TerminatorKind::SwitchInt { discr, targets } => {
            let ty = ctx.monomorphize(discr.ty(ctx.body(), ctx.tcx()));
            let discr = handle_operand(discr, ctx);
            handle_switch(ty, discr, targets, ctx)
        }
        TerminatorKind::Assert {
            cond,
            expected,
            msg,
            target,
            unwind: _,
        } => {
            let cond = if *expected {
                handle_operand(cond, ctx)
            } else {
                let c = handle_operand(cond, ctx);
                let e = ctx.alloc_node(*expected);
                ctx.biop(c, e, BinOp::Eq)
            };
            // FIXME: propelrly handle *all* assertion messages.
            let main = ctx.main_module();

            let name = match msg.as_ref() 
            {
                AssertKind::InvalidEnumConstruction(_)=>{
               
                    format!("assert_iec")
                }
                AssertKind::Overflow(op, _, _) => {
                    let op: BinOp = crate::map_binop(op);
                    format!("assert_{}", op.name())
                }
                AssertKind::OverflowNeg(_) => "assert_neg_overflow".into(),
                AssertKind::BoundsCheck { .. } => {
                    // The surrogate `assert_bounds_check` only takes the precomputed `cond` bool;
                    // the `len`/`index` operands are not part of its ABI.
                    let sig = ctx.sig([Type::Bool], Type::Void);
                    let site = ctx.new_methodref(
                        *main,
                        "assert_bounds_check",
                        sig,
                        MethodKind::Static,
                        vec![],
                    );
                    let call = ctx.call_root(site, &[cond], IsPure::NOT);
                    let goto = goto(ctx, target.as_u32());
                    return vec![call, goto];
                }
                AssertKind::NullPointerDereference => "assert_notnull".into(),
                AssertKind::MisalignedPointerDereference {
                    required: _,
                    found: _,
                } => "assert_ptr_align".into(),
                AssertKind::DivisionByZero(_) => "assert_zero_div".into(),
                AssertKind::RemainderByZero(_) => "assert_zero_rem".into(),
                AssertKind::ResumedAfterReturn(_) => "assert_coroutine_resume_after_return".into(),
                AssertKind::ResumedAfterPanic(_) => "assert_coroutine_resume_after_panic".into(),
                AssertKind::ResumedAfterDrop(_) => "assert_coroutine_resume_after_drop".into(),
            };
            let sig = ctx.sig([Type::Bool], Type::Void);
            let site = ctx.new_methodref(*main, name, sig, MethodKind::Static, vec![]);
            let call = ctx.call_root(site, &[cond], IsPure::NOT);
            let goto = goto(ctx, target.as_u32());
            vec![call, goto]
        }
        TerminatorKind::Goto { target } => vec![goto(ctx, target.as_u32())],
        TerminatorKind::UnwindResume => {
            vec![ctx.alloc_root(CILRoot::ReThrow)]
        }
        TerminatorKind::Drop {
            place,
            target,
            unwind: _,
            replace: _,
            //TODO: figure out what the hell those fields are doing.
            drop: _,
        } => {
            let ty = ctx.monomorphize(place.ty(ctx.body(), ctx.tcx()).ty);

            let drop_instance = Instance::resolve_drop_glue(ctx.tcx(), ty);
            if let InstanceKind::DropGlue(_, None) = drop_instance.def {
                //Empty drop, nothing needs to happen.
                vec![goto(ctx, target.as_u32())]
            } else {
                match ty.kind() {
                    TyKind::Dynamic(_, _) => {
                        let fat_ptr_address = place_address(place, ctx);
                        let fat_ptr_type = ctx.type_from_cache(Ty::new_ptr(
                            ctx.tcx(),
                            ty,
                            rustc_middle::ty::Mutability::Mut,
                        ));
                        let desc = FieldDesc::new(
                            fat_ptr_type.as_class_ref().unwrap(),
                            ctx.alloc_string(crate::METADATA),
                            Type::Int(Int::USize),
                        );
                        // Get the vtable
                        let vtable_desc = ctx.alloc_field(desc);
                        let vtable_ptr = ctx.ld_field(fat_ptr_address, vtable_desc);
                        let void_ptr = ctx.nptr(Type::Void);
                        // Get the addres of the object
                        let desc = FieldDesc::new(
                            fat_ptr_type.as_class_ref().unwrap(),
                            ctx.alloc_string(crate::DATA_PTR),
                            void_ptr,
                        );
                        let obj_desc = ctx.alloc_field(desc);
                        let obj_ptr = ctx.ld_field(fat_ptr_address, obj_desc);
                        // We asusme the drop is the first method in the vtable
                        assert_eq!(
                            rustc_middle::ty::vtable::COMMON_VTABLE_ENTRIES_DROPINPLACE,
                            0
                        );
                        let sig = ctx.sig([void_ptr], Type::Void);
                        // `vtable_ptr` is the address of the vtable slot holding the drop fn ptr, so
                        // it must be cast to a pointer-to-`FnPtr` before loading. `cast_ptr` already
                        // adds the `Ptr` level, so the pointee passed is the bare `FnPtr(sig)` — not
                        // `nptr(FnPtr(sig))`, which would build a `Ptr(Ptr(FnPtr))` and make the
                        // following load deref a data `Ptr` (the `DerfWrongPtr` / Bad IL bug).
                        let casted = ctx.cast_ptr(vtable_ptr, Type::FnPtr(sig));
                        let drop_fn_ptr = ctx.load(casted, Type::FnPtr(sig));
                        let cmp_a = ctx.cast_ptr_to(drop_fn_ptr, Type::Int(Int::USize));
                        let cmp_b = ctx.alloc_node(Const::USize(0));
                        let fn_sig = ctx.alloc_sig(FnSig::new([void_ptr], Type::Void));
                        let calli = ctx.call_indirect_root(fn_sig, drop_fn_ptr, [obj_ptr]);
                        let beq = ctx.alloc_root(CILRoot::Branch(Box::new((
                            target.as_u32(),
                            0,
                            Some(BranchCond::Eq(cmp_a, cmp_b)),
                        ))));
                        let goto = goto(ctx, target.as_u32());
                        vec![beq, calli, goto]
                    }

                    _ => {
                        let sig =
                            crate::function_sig::sig_from_instance_(drop_instance, ctx).unwrap();
                        let function_name = function_name(ctx.tcx().symbol_name(drop_instance));
                        let mref = MethodRef::new(
                            *ctx.main_module(),
                            ctx.alloc_string(function_name),
                            ctx.alloc_sig(sig),
                            MethodKind::Static,
                            vec![].into(),
                        );
                        let site = ctx.alloc_methodref(mref);
                        let addr = place_address(place, ctx);
                        let call = ctx.call_root(site, &[addr], IsPure::NOT);
                        let goto = goto(ctx, target.as_u32());
                        vec![call, goto]
                    }
                }
            }
        }
        TerminatorKind::Unreachable => {
            let loc = terminator.source_info.span;
            let msg = ctx.alloc_string(format!("Unreachable reached at {loc:?}!"));

            vec![
                rustc_middle::ty::print::with_no_trimmed_paths! {ctx.alloc_root(cilly::CILRoot::Unreachable(msg))},
            ]
        }
        TerminatorKind::InlineAsm {
            template: _,
            operands: _,
            options: _,
            line_spans: _,
            unwind: _,
            targets: _,
            asm_macro: _,
        } => {
            eprintln!("Inline assembly is not yet supported!");
            let root = ctx.throw_msg("Inline assembly is not yet supported!");
            vec![root]
        }
        TerminatorKind::UnwindTerminate(_) => {
            // The `abort()` landing pad — reached when unwinding would cross a `nounwind` boundary (a
            // double panic, or a panic escaping a `Drop` run during unwinding). Rust requires a hard
            // process termination here; the previous `ReThrow` incorrectly *continued* unwinding. Map
            // it to `System.Environment.FailFast`, the managed no-catch / no-cleanup abort.
            let loc = terminator.source_info.span;
            let msg = format!("Rust unwinding reached a nounwind boundary and was aborted (at {loc:?}).");
            let msg = ctx.alloc_string(msg);
            let msg = ctx.alloc_node(CILNode::Const(Box::new(Const::PlatformString(msg))));
            let fail_fast = MethodRef::new(
                ClassRef::enviroment(ctx),
                ctx.alloc_string("FailFast"),
                ctx.sig([Type::PlatformString], Type::Void),
                MethodKind::Static,
                vec![].into(),
            );
            let fail_fast = ctx.alloc_methodref(fail_fast);
            let abort = ctx.alloc_root(CILRoot::call(fail_fast, vec![msg]));
            // FailFast never returns; the trailing ReThrow only keeps the cleanup block well-formed
            // (an exception is in flight on this path, so it is valid IL but never executed).
            let rethrow = ctx.alloc_root(CILRoot::ReThrow);
            vec![abort, rethrow]
        }
        TerminatorKind::FalseEdge {
            real_target,
            imaginary_target: _,
        } => {
            // imaginary_target is ignored becase you can't jump to it.
            vec![goto(ctx, real_target.as_u32())]
        }
        // Really just a goto, since it can never unwind.
        TerminatorKind::FalseUnwind {
            real_target,
            unwind: _,
        } => {
            // unwind is ignored becase it can't happen.
            vec![goto(ctx, real_target.as_u32())]
        }
        TerminatorKind::CoroutineDrop {} => todo!("Can't drop corutines yet!"),
        TerminatorKind::Yield {
            value: _,
            resume: _,
            resume_arg: _,
            drop: _,
        } => todo!("Can't yeld yet!"), //_ => todo!("Unhandled terminator kind {kind:?}", kind = terminator.kind),
    };
    // Every terminator must produce at least one root.
    assert!(
        !res.is_empty(),
        "A terminator did not produce any roots!."
    );
    res
}

fn handle_switch<'tcx>(
    ty: Ty<'tcx>,
    discr: Interned<cilly::ir::CILNode>,
    switch: &SwitchTargets,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Vec<Root> {
    let mut trees = Vec::new();
    for (value, target) in switch.iter() {
        //ops.extend(CILOp::debug_msg("Switchin"));

        let const_val = match ty.kind() {
            TyKind::Int(int) => load_const_int(value, *int, ctx),
            TyKind::Uint(uint) => load_const_uint(value, *uint, ctx),
            TyKind::Bool => ctx.alloc_node(value != 0),
            TyKind::Char => load_const_uint(value, rustc_middle::ty::UintTy::U32, ctx),
            _ => todo!("Unsuported switch discriminant type {ty:?}"),
        };
        //ops.push(CILOp::LdcI64(value as i64));
        let cond = crate::binop::cmp::eq_unchecked(ty, discr, const_val, ctx);
        trees.push(ctx.alloc_root(CILRoot::Branch(Box::new((
            target.into(),
            0,
            Some(BranchCond::True(cond)),
        )))));
    }
    trees.push(goto(ctx, switch.otherwise().into()));
    trees
}
