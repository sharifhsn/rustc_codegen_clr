use rustc_codegen_clr_type::utilis::monomorphize;
use rustc_middle::mir::{UnwindAction, UnwindTerminateReason};
use rustc_middle::mir::{BasicBlock, BasicBlockData};
use rustc_middle::{
    mir::{BasicBlocks, Body, TerminatorKind},
    ty::{Instance, InstanceKind, TyCtxt},
};

/// Returns the *unresolved* exception-handler block id of a MIR block, if any. Consumed by
/// `BasicBlock::new_raw` / `resolve_exception_handlers`.
pub(crate) fn handler_for_block<'tcx>(
    block_data: &BasicBlockData,
    blocks: &BasicBlocks<'tcx>,
    tcx: TyCtxt<'tcx>,
    method_instance: &Instance<'tcx>,
    method: &Body<'tcx>,
) -> Option<u32> {
    let term = block_data.terminator.as_ref()?;
    let unwind = term.unwind()?;
    if *crate::config::NO_UNWIND {
        return None;
    }
    // `UnwindAction::Terminate` has no MIR cleanup block to point at; route it to a SYNTHETIC
    // terminate handler (built + appended to the cleanup blocks by `assembly::add_fn`) that hard-aborts
    // via `FailFast`. Returning `None` here (the pre-P2-S4 behavior) let an outer `catch_unwind` absorb
    // an abort Rust guarantees is uncatchable. Short-circuited BEFORE `simplify_handler`, which only
    // understands real MIR block indices.
    if let UnwindAction::Terminate(reason) = unwind {
        // A `Terminate` edge on a CLEANUP block whose terminator is a `Drop` is now handled by an
        // inline `TerminateRegion` abort guard wrapping the drop call itself (see
        // `terminator::handle_terminator`'s `Drop` arm). Returning a synthetic handler id here would
        // additionally materialize a now-DEAD `FailFast` cleanup block (assembly.rs) and, worse,
        // cleanup blocks are never run through `resolve_exception_handlers` anyway — so the
        // synthetic route never actually wired the abort for the InCleanup edge (that was the bug).
        // Returning `None` lets the inline guard be the sole, correct mechanism. The NORMAL-block
        // `Terminate(Abi)` edge (P2-S4) is left byte-identical: it keeps the synthetic-handler
        // route. We gate strictly on `is_cleanup` + `Drop` so no other Terminate edge is affected.
        if block_data.is_cleanup
            && matches!(
                term.kind,
                rustc_middle::mir::TerminatorKind::Drop { .. }
            )
        {
            return None;
        }
        return Some(terminate_handler_id(*reason, blocks));
    }
    simplify_handler(
        handler_from_action(*unwind),
        blocks,
        tcx,
        method_instance,
        method,
    )
}

/// The synthetic block id of the terminate handler for `reason`. MIR block indices are dense
/// (`0..blocks.len()`), so placing the handlers one/two past the end never collides with a real block.
/// `assembly::add_fn` materializes the matching cleanup block(s) on demand (see `emit_terminate`).
pub(crate) fn terminate_handler_id(reason: UnwindTerminateReason, blocks: &BasicBlocks) -> u32 {
    let base = u32::try_from(blocks.len()).expect("function has more than 2^32 basic blocks");
    match reason {
        UnwindTerminateReason::Abi => base,
        UnwindTerminateReason::InCleanup => base + 1,
    }
}
#[allow(clippy::match_same_arms)]
fn simplify_handler<'tcx>(
    handler: Option<u32>,
    blocks: &BasicBlocks<'tcx>,
    tcx: TyCtxt<'tcx>,
    method_instance: &Instance<'tcx>,
    method: &Body<'tcx>,
) -> Option<u32> {
    if *crate::config::NO_UNWIND {
        return None;
    }
    let handler = handler?;
    if !blocks[BasicBlock::from_u32(handler)].statements.is_empty() {
        return Some(handler);
    }
    match blocks[BasicBlock::from_u32(handler)]
        .terminator
        .as_ref()?
        .kind
    {
        TerminatorKind::TailCall { .. } => None,
        TerminatorKind::Goto { target } => {
            simplify_handler(Some(target.as_u32()), blocks, tcx, method_instance, method)
        }
        // Reaching Unreachable is UB, so we can do whatever, including doing nothing :).
        TerminatorKind::UnwindResume | TerminatorKind::Unreachable => None,
        TerminatorKind::Return => panic!("Interal error: cleanup(unwind) block returns!"),
        // This block drops, so we **have** to execute it
        TerminatorKind::Drop {
            place,
            target,
            unwind: _,
            replace: _,
            drop: _,
        } => {
            let ty = monomorphize(method_instance, place.ty(method, tcx).ty, tcx);

            let drop_instance = Instance::resolve_drop_glue(tcx, ty);
            if let InstanceKind::DropGlue(_, None) = drop_instance.def {
                //Empty drop, nothing needs to happen.
                simplify_handler(Some(target.as_u32()), blocks, tcx, method_instance, method)
            } else {
                Some(handler)
            }
        }
        TerminatorKind::CoroutineDrop { .. } => Some(handler),
        // This block calls, so we **have** to execute it
        // TODO: consider checking if this call has side effects!
        TerminatorKind::Call { .. } => Some(handler),
        // This block asserts, so it *could* double-panics, so we **have** to execute it
        TerminatorKind::Assert { .. } => Some(handler),
        TerminatorKind::Yield { .. } => {
            panic!("Interal error: cleanup(unwind) block yelds(returns)!")
        }
        // False targets should not be present.
        TerminatorKind::FalseEdge { .. } | TerminatorKind::FalseUnwind { .. } => {
            panic!("False bb termiantor after drop elaboration!")
        }
        // Iniline ASM could do **anything** so it can never be skipped.
        TerminatorKind::InlineAsm { .. } => Some(handler),
        // We *don't* know which target is taken, so we can't skip it
        // TODO: consider checking all sub-targets and removing impossible ones?
        TerminatorKind::SwitchInt { .. } => Some(handler),
        // We can't skip a termiantor which aborts.
        TerminatorKind::UnwindTerminate(_) => Some(handler),
    }
}
/// Convert an `UnwindAction` into an id of the block this will jump into during an exception.
//  We match same arms on purpose here.
#[allow(clippy::match_same_arms)]
pub(crate) fn handler_from_action(action: UnwindAction) -> Option<u32> {
    match action {
        UnwindAction::Continue => None,
        UnwindAction::Cleanup(handler) => Some(handler.as_u32()),
        // Double panics / panics crossing a `nounwind` (FFI) boundary. Handled UPSTREAM in
        // `handler_for_block`, which routes `Terminate` to a synthetic `FailFast` handler before this
        // function is reached — so this arm is effectively unreachable. (Kept exhaustive + `None` as a
        // defensive fallback: continuing to unwind is still less wrong than mis-indexing a block.)
        UnwindAction::Terminate(_reason) => None,
        // Reaching this is UB, so we can do whatever here
        // continuing unwinding seems like an OK option.
        UnwindAction::Unreachable => None,
    }
}
