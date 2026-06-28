//! A *general* single-block inliner — the keystone for making Rust's zero-cost abstractions
//! (iterators, closures, small wrappers) actually zero-cost on the .NET/C/JVM backends.
//!
//! The trivial inliner ([`super::inline`]) only inlines a callee that is a single `Ret(expr)` with
//! **zero locals**. That excludes almost everything real: a closure body like `map_fold` is one
//! block but has locals (temporaries, the arg-passing tuple), so it survives as a per-element call —
//! the dominant iterator-codegen cost. Native Rust is fast here only because LLVM inlines the whole
//! adapter chain.
//!
//! This pass generalizes to **any handler-free, straight-line (single-block, returning) static
//! callee, locals allowed**. Because such a callee has no internal control flow, inlining it needs
//! no block/label surgery: we (1) evaluate each call argument once into a fresh caller local,
//! (2) renumber the callee's locals into fresh caller locals, (3) splice the callee's roots in
//! place with `LdArg(i)`/`LdLoc(n)` rewritten to those temps, and (4) turn the callee's terminating
//! `Ret(v)` into the caller's effect at the call site (`StLoc`/`Ret`/`Pop`). The optimizer runs to a
//! fuel-bounded fixpoint, so a chain `map_fold -> call_mut -> wrapping_add` peels one layer per
//! pass; the existing copy-propagation + dead-store elimination then dissolve the temps.
//!
//! Safety: only *top-level* calls (the whole value of a `StLoc`/`Ret`/`Pop`, or a bare statement
//! `Call`) are inlined, so hoisting the callee's effects before the call site never reorders
//! anything evaluated earlier in the same root. Argument evaluation is preserved (each arg is
//! evaluated exactly once, in order, before the body). The fatal CIL typechecker validates the
//! spliced result, and the differential test corpus + `::stable` gate guard behaviour.

use super::OptFuel;
use crate::{
    bimap::Interned, cilnode::MethodKind, method::LocalDef, Assembly, BasicBlock, CILNode, CILRoot,
    MethodImpl, MethodRef,
};

/// Callees larger than this many roots are not inlined (guards against code-size blowup).
const MAX_CALLEE_ROOTS: usize = 48;

/// Whether the general inliner is enabled. **Opt-in (default OFF)** — set `INLINE_BLOCKS=1`.
///
/// Status: TYPE-safety is solved. The assembly-level snapshot/verify/revert net in `Assembly::opt`
/// (typecheck every method at its final state, revert any inlining left ill-typed to its trusted
/// un-optimized body) plus the `.cctor`/`.tcctor` skip make the inliner produce only type-valid CIL
/// that compiles, links, and runs. The remaining blocker is a residual *semantic* miscompile in the
/// splice: a type-valid but behaviourally-wrong inline (e.g. collecting `(0..n).map(f)` into a `Vec`
/// faults). The type verifier cannot catch this by construction; it needs differential debugging
/// (run with `INLINE_BLOCKS=1` under the diff oracle, bisect to the miscompiled method, dump its
/// pre/post-inline IR). Suspects: argument-by-value vs by-address evaluation, or the `Ret`->`StLoc`
/// tail when the returned value aliases a caller local.
#[must_use]
pub fn inlining_enabled() -> bool {
    matches!(std::env::var("INLINE_BLOCKS").as_deref(), Ok("1"))
}

/// How the callee's terminating `Ret(v)`/`VoidRet` becomes the caller's effect at the call site.
#[derive(Clone, Copy)]
enum Tail {
    /// The call result flowed into a local: `StLoc(loc, v)`.
    StLoc(u32),
    /// The call was the caller's returned value (tail position): `Ret(v)`.
    Ret,
    /// The result is discarded: `Pop(v)` (or nothing, for a void callee).
    Drop,
}

/// Inline every eligible top-level call in every block of `blocks`. Returns whether anything changed.
pub fn inline_single_block_calls(
    blocks: &mut [BasicBlock],
    locals: &mut Vec<LocalDef>,
    self_ref: Interned<MethodRef>,
    asm: &mut Assembly,
    fuel: &mut OptFuel,
) -> bool {
    let mut changed = false;
    for block in blocks.iter_mut() {
        // Only the block's own roots (not handler bodies) — hot loops live in normal blocks, and
        // skipping handlers keeps the EH model untouched.
        let old: Vec<Interned<CILRoot>> = std::mem::take(block.roots_mut());
        let mut new_roots = Vec::with_capacity(old.len());
        for root in old {
            match try_inline_root(root, self_ref, locals, asm, fuel) {
                Some(spliced) => {
                    new_roots.extend(spliced);
                    changed = true;
                }
                None => new_roots.push(root),
            }
        }
        *block.roots_mut() = new_roots;
    }
    changed
}

/// If `root_idx` is a top-level call to an eligible callee, return the spliced replacement roots.
fn try_inline_root(
    root_idx: Interned<CILRoot>,
    self_ref: Interned<MethodRef>,
    caller_locals: &mut Vec<LocalDef>,
    asm: &mut Assembly,
    fuel: &mut OptFuel,
) -> Option<Vec<Interned<CILRoot>>> {
    let root = asm.get_root(root_idx).clone();
    // Identify the call value + how its result is consumed (the four order-safe top-level forms).
    let (mref, args, tail): (Interned<MethodRef>, Vec<Interned<CILNode>>, Tail) = match &root {
        CILRoot::StLoc(loc, val) => match asm.get_node(*val) {
            CILNode::Call(info) => (info.0, info.1.to_vec(), Tail::StLoc(*loc)),
            _ => return None,
        },
        CILRoot::Ret(val) => match asm.get_node(*val) {
            CILNode::Call(info) => (info.0, info.1.to_vec(), Tail::Ret),
            _ => return None,
        },
        CILRoot::Pop(val) => match asm.get_node(*val) {
            CILNode::Call(info) => (info.0, info.1.to_vec(), Tail::Drop),
            _ => return None,
        },
        CILRoot::Call(info) => (info.0, info.1.to_vec(), Tail::Drop),
        _ => return None,
    };
    // Don't inline a method into itself (direct recursion). Mutual recursion is bounded by fuel.
    if mref == self_ref {
        return None;
    }
    let def = asm.method_def_from_ref(mref).cloned()?;
    if def.kind() != MethodKind::Static {
        return None;
    }
    // Extract the callee's single straight-line block + its locals (cloned out of the borrow).
    let (croots, clocals) = {
        let MethodImpl::MethodBody {
            blocks: cblocks,
            locals: clocals,
        } = def.resolved_implementation(asm)
        else {
            return None;
        };
        let [cblock] = &cblocks[..] else {
            return None;
        };
        if cblock.handler().is_some() {
            return None;
        }
        (cblock.roots().to_vec(), clocals.clone())
    };
    if croots.len() > MAX_CALLEE_ROOTS || !is_straightline_returning(&croots, asm) {
        return None;
    }
    // Arg count must match the signature (static call: args == params).
    let sig_inputs: Vec<crate::Type> = asm[def.sig()].inputs().to_vec();
    if sig_inputs.len() != args.len() {
        return None;
    }
    // No per-site type guards: soundness is enforced at the *method* level. The IR-with-lenient-puns
    // model means a representation pun (e.g. core `dec2flt` computing an `Option<&u8>` niche as a raw
    // `u64`) can be moved by a later `propagate_locals` pass across the boundary between a
    // pun-accepting and a pun-rejecting context — which no per-boundary guard here can see. Instead,
    // `MethodDef::optimize` snapshots the method, inlines + runs the normal passes, then typechecks
    // the result and reverts the whole method to its (trusted) pre-inline form if it became
    // ill-typed. So this pass may freely splice; only inlines that survive the full optimize +
    // verify are kept.
    // Budget: cost ~ body size + arg materialization.
    if !fuel.consume((croots.len() + args.len() + 4) as u32) {
        return None;
    }
    Some(splice(
        &croots,
        &clocals,
        &args,
        &sig_inputs,
        tail,
        caller_locals,
        asm,
    ))
}

/// A block is inlinable only if it is straight-line and returns: exactly one trailing `Ret`/`VoidRet`
/// and no internal control flow (`Branch`/`ExitSpecialRegion`) or unconditional divergence
/// (`Throw`/`ReThrow`), which would have no place to land once spliced.
fn is_straightline_returning(roots: &[Interned<CILRoot>], asm: &Assembly) -> bool {
    let mut terminator = false;
    for &r in roots {
        match asm.get_root(r) {
            CILRoot::Nop | CILRoot::SourceFileInfo { .. } => {}
            CILRoot::Ret(_) | CILRoot::VoidRet => {
                if terminator {
                    return false;
                }
                terminator = true;
            }
            CILRoot::Branch(_)
            | CILRoot::ExitSpecialRegion { .. }
            | CILRoot::Throw(_)
            | CILRoot::ReThrow
            | CILRoot::Unreachable(_) => return false,
            _ => {
                // Any ordinary side-effecting root after the terminator would be dead/ill-formed.
                if terminator {
                    return false;
                }
            }
        }
    }
    terminator
}

/// Build the spliced replacement: arg temps, then the callee body with locals/args renumbered and
/// its `Ret`/`VoidRet` rewritten per `tail`.
fn splice(
    croots: &[Interned<CILRoot>],
    clocals: &[LocalDef],
    args: &[Interned<CILNode>],
    sig_inputs: &[crate::Type],
    tail: Tail,
    caller_locals: &mut Vec<LocalDef>,
    asm: &mut Assembly,
) -> Vec<Interned<CILRoot>> {
    let mut out = Vec::with_capacity(croots.len() + args.len());

    // (1) Evaluate each argument exactly once into a fresh caller local (copy-prop later removes the
    // temp when the arg was already a simple value).
    let mut arg_temps = Vec::with_capacity(args.len());
    for (i, &arg) in args.iter().enumerate() {
        let ty = asm.alloc_type(sig_inputs[i]);
        let temp = caller_locals.len() as u32;
        caller_locals.push((None, ty));
        out.push(asm.alloc_root(CILRoot::StLoc(temp, arg)));
        arg_temps.push(temp);
    }

    // (2) Reserve fresh caller locals for the callee's locals (contiguous, so `n -> base + n`).
    let base = caller_locals.len() as u32;
    for l in clocals {
        caller_locals.push((None, l.1));
    }

    // (3) Splice the body, rewriting LdArg/LdLoc -> the fresh temps and the terminator per `tail`.
    for &cr in croots {
        let root = asm.get_root(cr).clone();
        let mapped = root.map(
            asm,
            &mut |r, _| r,
            &mut |node, _| match node {
                CILNode::LdArg(a) => CILNode::LdLoc(arg_temps[a as usize]),
                CILNode::LdArgA(a) => CILNode::LdLocA(arg_temps[a as usize]),
                CILNode::LdLoc(n) => CILNode::LdLoc(base + n),
                CILNode::LdLocA(n) => CILNode::LdLocA(base + n),
                other => other,
            },
        );
        match mapped {
            CILRoot::Ret(v) => {
                let r = match tail {
                    Tail::StLoc(loc) => CILRoot::StLoc(loc, v),
                    Tail::Ret => CILRoot::Ret(v),
                    Tail::Drop => CILRoot::Pop(v),
                };
                out.push(asm.alloc_root(r));
            }
            // Void callee: nothing to store/return. (A void call is only ever `Tail::Drop`.)
            CILRoot::VoidRet => {}
            other => out.push(asm.alloc_root(other)),
        }
    }
    out
}
