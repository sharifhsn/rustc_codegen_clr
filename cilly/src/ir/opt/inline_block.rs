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
    FnSig, MethodImpl, MethodRef,
};

/// Callees larger than this many roots are not inlined (guards against code-size blowup).
const MAX_CALLEE_ROOTS: usize = 48;

/// Whether the general inliner is enabled. **Opt-in (default OFF)** while a residual soundness issue
/// is resolved: the per-site arg/return type guards below are necessary but not yet sufficient — a
/// representation pun can still be surfaced onto a spliced store by a *later* pass (`propagate_locals`
/// folding a niche-encoded value through the fresh temps), which the fatal CIL type-verifier then
/// (correctly) rejects, so `core`/`std` fail to compile with it on. The splice machinery + guards are
/// complete and compile-verified; the remaining work is to make the inline atomic w.r.t. the
/// downstream passes (e.g. typecheck the whole method after splicing and roll back on failure, or
/// only inline returns/args that are type-faithful *carriers* — `Call`/`LdLoc`/`LdArg` — never
/// computed expressions). Set `INLINE_BLOCKS=1` to experiment.
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
    caller_sig: Interned<FnSig>,
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
            match try_inline_root(root, self_ref, caller_sig, locals, asm, fuel) {
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
    caller_sig: Interned<FnSig>,
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
    // SOUNDNESS GUARD for the `StLoc` tail. `Ret`/`Drop` tails are always safe: turning the callee's
    // `Ret(v)` into the caller's `Ret(v)` reuses the same lenient return-boundary check that already
    // accepted the callee, and `Pop(v)` discards `v` with no type constraint. But `StLoc(dest, v)` is
    // checked strictly, and `v` may be a representation pun the callee only ever exposes across the
    // return boundary (e.g. computing an `Option<&u8>` niche as a raw `u64` and returning it). So for
    // an `StLoc` tail, only inline when the callee's returned value is *assignable* to the
    // destination local — which keeps the type-faithful cases (the iterator hot path: a `u64`-typed
    // closure result into a `u64` accumulator) and skips the punning ones.
    if let Tail::StLoc(loc) = tail {
        let ret_val = croots.iter().rev().find_map(|&r| match asm.get_root(r) {
            CILRoot::Ret(v) => Some(*v),
            _ => None,
        });
        let Some(rv) = ret_val else { return None };
        let Ok(got) = asm.get_node(rv).clone().typecheck(def.sig(), &clocals, asm) else {
            return None;
        };
        let expected = asm[caller_locals[loc as usize].1];
        // EXACT equality (not merely assignable): `is_assignable_to` accepts representation puns
        // (e.g. a `u64` niche into an `Option<&u8>`) that the strict `StLoc` check later rejects, and
        // a punned value can be surfaced onto this store by `propagate_locals`. Exact types are
        // pun-free and survive the downstream passes. The iterator hot path is exact (`u64`->`u64`).
        if got != expected {
            return None;
        }
    }
    // Arg count must match the signature (static call: args == params).
    let sig_inputs: Vec<crate::Type> = asm[def.sig()].inputs().to_vec();
    if sig_inputs.len() != args.len() {
        return None;
    }
    // SOUNDNESS GUARD for argument materialization (symmetric to the `StLoc`-tail guard): we evaluate
    // each arg into a fresh local typed as the *parameter*, so the store `StLoc(temp:param, arg)`
    // must typecheck. A caller can pun across the call boundary too (pass a raw word where the param
    // is a niche type), so require each arg's type to be assignable to its parameter type. The
    // iterator hot path passes only `void*`/`u64` matching the params, so it is unaffected.
    for (i, &arg) in args.iter().enumerate() {
        let Ok(got) = asm.get_node(arg).clone().typecheck(caller_sig, caller_locals, asm) else {
            return None;
        };
        // Exact equality (see the StLoc-tail guard): a punned arg would store a mistyped value into
        // the param temp, which `propagate_locals` can then surface as an ill-typed assignment.
        if got != sig_inputs[i] {
            return None;
        }
    }
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
