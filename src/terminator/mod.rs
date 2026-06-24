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
use rustc_ast::{InlineAsmOptions, InlineAsmTemplatePiece};
use rustc_middle::{
    mir::{
        BasicBlock, InlineAsmOperand, Operand, Place, SwitchTargets, Terminator, TerminatorKind,
        UnwindTerminateReason,
    },
    ty::{Instance, InstanceKind, Ty, TyKind},
};
use rustc_span::Spanned;

mod call;
mod intrinsics;
/// Builds an unconditional branch root targeting `target`.
fn goto(ctx: &mut MethodCompileCtx<'_, '_>, target: u32) -> Root {
    ctx.alloc_root(CILRoot::Branch(Box::new((target, 0, None))))
}

/// Emit the hard-abort landing for a `nounwind`-boundary unwind — used by BOTH the `UnwindTerminate`
/// *terminator* and a synthetic handler for an `UnwindAction::Terminate` *edge* (see
/// `crate::basic_block`). Rust requires the process to terminate **uncatchably** here, so we map it to
/// `System.Environment.FailFast` (the managed no-catch / no-cleanup abort) — NOT a `ReThrow`, which
/// would let an outer `catch_unwind` wrongly absorb an abort Rust guarantees is final. The message
/// distinguishes the reason: `Abi` = a panic escaping a `nounwind`/`extern "C"` boundary
/// ("panic in a function that cannot unwind"); `InCleanup` = a panic in a destructor run while already
/// unwinding ("panic in a destructor during cleanup", a double panic).
pub(crate) fn emit_terminate(
    ctx: &mut MethodCompileCtx<'_, '_>,
    reason: UnwindTerminateReason,
) -> Vec<Root> {
    let msg = match reason {
        UnwindTerminateReason::Abi => {
            "Rust unwinding crossed a `nounwind` ABI boundary (panic in a function that cannot unwind); aborted."
        }
        UnwindTerminateReason::InCleanup => {
            "Rust panicked while running a destructor during unwinding (panic in a destructor during cleanup); aborted."
        }
    };
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
    // FailFast never returns; the trailing ReThrow only keeps the block well-formed (valid IL on a
    // path where an exception is in flight, but never executed).
    let rethrow = ctx.alloc_root(CILRoot::ReThrow);
    vec![abort, rethrow]
}

/// Materialize the `&core::panic::Location` value that a `#[track_caller]` callee — or the
/// `caller_location` intrinsic — must observe at this point in the function currently being compiled.
///
/// Mirrors rustc's `FunctionCx::get_caller_location` exactly. Two effects compose:
///   * **MIR-inlining scope walk** (delegated to `Body::caller_location_span`): release builds inline
///     `#[track_caller]` callees, so the location must be recovered by climbing the inlined source
///     scopes to the real outer call site rather than reading the (inlined) statement span.
///   * **Implicit-arg forwarding**: if the function being compiled is *itself* `#[track_caller]`, an
///     un-inlined chain forwards its own implicit trailing `&Location` argument, so a chain
///     `user_site → #[track_caller] a → #[track_caller] b → Location::caller()` reports `user_site`.
///
/// Only at the root of the chain (a non-track_caller frame) is a fresh `Location` constant
/// materialized from the (walked) span. Previously every site unconditionally materialized the local
/// statement span, so `Location::caller()` reported the body of `core::panic::Location::caller`
/// itself (`library/core/src/panic/location.rs`) instead of the real user call site.
pub(crate) fn get_caller_location<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    source_info: rustc_middle::mir::SourceInfo,
) -> Interned<CILNode> {
    let tcx = ctx.tcx();
    // rustc appends exactly one implicit `&Location` to the `FnAbi` of every track_caller fn, so it
    // is the last CIL argument. MIR arg locals `_1..=arg_count` map to `LdArg(0..arg_count-1)` (see
    // `rustc_codegen_clr_place::get::local_get`), so the implicit trailing arg is at `LdArg(arg_count)`.
    let own_caller_location = if ctx.instance().def.requires_caller_location(tcx) {
        let idx = u32::try_from(ctx.body().arg_count).expect("arg_count exceeds u32");
        Some(ctx.alloc_node(CILNode::LdArg(idx)))
    } else {
        None
    };
    // `body()` returns a `'tcx` reference, so it does not borrow `ctx` — the `from_span` closure is
    // free to take `&mut ctx` to materialize the constant.
    let body = ctx.body();
    body.caller_location_span(source_info, own_caller_location, tcx, |span| {
        let caller_loc = tcx.span_as_caller_location(span);
        let caller_loc_ty = tcx.caller_location_ty();
        rustc_codgen_clr_operand::constant::load_const_value(caller_loc, caller_loc_ty, ctx)
    })
}

/// Emit a call to a `#[lang]`-item panic function (e.g. `panic_bounds_check`), supplying `args`
/// and — because every such lang item is `#[track_caller]` — the materialized caller `Location`.
///
/// This lowers a checked-failure terminator (`Assert`) to the *exact* panic the native Rust
/// codegen would emit, so the panic message (`"index out of bounds: the len is N but the index is
/// M"`) and the unwinding behaviour match native. The previous surrogate (`assert_bounds_check`)
/// discarded the `len`/`index` operands and called an unbodied `abort`, which crashed the program
/// with "missing method abort" instead of producing the correct, catchable panic.
fn call_panic_lang_item<'tcx>(
    lang: rustc_hir::lang_items::LangItem,
    args: &[Interned<CILNode>],
    source_info: rustc_middle::mir::SourceInfo,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Vec<Root> {
    let span = source_info.span;
    let def_id = ctx.tcx().require_lang_item(lang, span);
    let instance = Instance::expect_resolve(
        ctx.tcx(),
        rustc_middle::ty::TypingEnv::fully_monomorphized(),
        def_id,
        rustc_middle::ty::List::empty(),
        span,
    );
    let call_info = rustc_codegen_clr_call::CallInfo::sig_from_instance_(instance, ctx);
    let signature = call_info.sig().clone();
    let name = function_name(ctx.tcx().symbol_name(instance));
    let mut call_args: Vec<Interned<CILNode>> = args.to_vec();
    // The lang item is `#[track_caller]`: rustc appends an implicit `&core::panic::Location` param
    // that the call site must supply (FnSig ≠ FnAbi). Supply the correct caller location — forwarded
    // from our own implicit arg if we are track_caller, else materialized from `span`.
    if call_args.len() < signature.inputs().len() {
        let location = get_caller_location(ctx, source_info);
        call_args.push(location);
    }
    let main = ctx.main_module();
    let site = MethodRef::new(
        *main,
        ctx.alloc_string(name),
        ctx.alloc_sig(signature),
        MethodKind::Static,
        vec![].into(),
    );
    let site = ctx.alloc_methodref(site);
    // `panic_*` lang items return `!`; the call diverges. The guard throw is unreachable but keeps
    // the block well-formed (same pattern as a normal diverging `panic!` call site).
    let call = ctx.call_root(site, &call_args, IsPure::NOT);
    let guard = ctx.throw_msg("panic lang item returned!");
    vec![call, guard]
}

/// Strip C-style block comments (`/* ... */`) and line comments (`// ...`) from an asm template
/// piece. Used to recognize comment-only optimization barriers (e.g. `asm!("/* {} */", ...)`),
/// which carry no real instructions and can be lowered to a no-op. Conservative: an unterminated
/// `/*` consumes the rest of the string, and only the comment delimiters are removed (instruction
/// text, if any, survives so the caller's emptiness check fails and we fall through to a throw).
fn strip_asm_comments(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            // Block comment: skip to the closing "*/" (or end of string).
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            // Line comment: skip to end of line (or end of string).
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Recognize a small set of `asm!` templates the .NET backend can faithfully lower instead of
/// throwing at runtime. Returns `Some(roots)` (always including the fall-through branch) when a
/// template is recognized; returns `None` to let the caller keep the generic "unsupported inline
/// asm" throw. Never silently miscompiles an unrecognized template.
///
/// Match precedence: (A) `cpuid` -> (B) empty/barrier -> (C) num-bigint `div` -> `None`.
fn lower_inline_asm<'tcx>(
    template: &[InlineAsmTemplatePiece],
    operands: &[InlineAsmOperand<'tcx>],
    targets: &[BasicBlock],
    options: InlineAsmOptions,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Option<Vec<Root>> {
    // A `noreturn` asm legitimately diverges — keep the throw, never add a fall-through goto. Also
    // bail if there is no fall-through target to branch to.
    if options.contains(InlineAsmOptions::NORETURN) || targets.is_empty() {
        return None;
    }
    // By MIR contract `targets[0]` is the fall-through block.
    let after = goto(ctx, targets[0].as_u32());

    // The textual pieces of the template, ignoring `{N}` placeholders.
    let str_pieces: Vec<&str> = template
        .iter()
        .filter_map(|p| {
            if let InlineAsmTemplatePiece::String(s) = p {
                Some(s.as_ref())
            } else {
                None
            }
        })
        .collect();

    // (A) CPUID — stdarch `__cpuid`/`__cpuid_count` (used by std `is_x86_feature_detected!`, the
    // `cpufeatures` crate behind all RustCrypto x86 backends, and memchr's avx2 probe). The
    // x86_64 template is ["mov {0:r}, rbx", "cpuid", "xchg {0:r}, rbx"]; the bare "cpuid" piece
    // matches. Lowering: write 0 to every output operand. A cpuid that reports an all-zero result
    // makes std_detect see no features (max_basic_leaf < 1 early-returns the empty feature set),
    // so the portable/scalar backend is selected everywhere. Strictly safe — can only force the
    // safe scalar path.
    if str_pieces.iter().any(|s| s.trim().eq_ignore_ascii_case("cpuid")) {
        let mut roots = Vec::new();
        for op in operands {
            let out = match op {
                InlineAsmOperand::Out { place: Some(p), .. }
                | InlineAsmOperand::InOut {
                    out_place: Some(p), ..
                } => p,
                // In / discarded outs (place None) / Const / Sym* / Label: nothing to write.
                _ => continue,
            };
            // cpuid outputs are all u32.
            let zero = load_const_uint(0, rustc_middle::ty::UintTy::U32, ctx);
            roots.push(place_set(out, zero, ctx));
        }
        roots.push(after);
        return Some(roots);
    }

    // (B) EMPTY / BARRIER — optimization-barrier asm!s whose template carries no actual
    // instructions: pure fences, empty templates, and comment-only barriers such as the
    // `asm!("/* {} */", ...)` black-box pattern used by ryu/float-formatting crates (e.g. the
    // `to_decimal_fast` path reached via serde_json -> write_f64). A comment can straddle a
    // placeholder (`["/* ", " */"]` around a `{}`), so we strip comments over the CONCATENATION of
    // all String pieces, not piece-by-piece. Placeholders are dropped from the concatenation: in a
    // barrier they sit inside the comment (and vanish with it); in a real instruction they are
    // always flanked by non-comment String text that survives stripping, so the template is
    // correctly seen as non-empty. If the stripped concatenation is whitespace-only, it is a
    // barrier. Lower to a no-op that threads each InOut's in-value straight through to its
    // out-place; pure Out/In barriers have no effect. (core::hint::black_box itself is a
    // `#[rustc_intrinsic]` handled on the call path and never reaches here — this covers the
    // third-party crate barriers that do.)
    let joined_template: String = str_pieces.concat();
    if !str_pieces.is_empty() && strip_asm_comments(&joined_template).trim().is_empty() {
        let mut roots = Vec::new();
        for op in operands {
            if let InlineAsmOperand::InOut {
                in_value,
                out_place: Some(p),
                ..
            } = op
            {
                let v = handle_operand(in_value, ctx);
                roots.push(place_set(p, v, ctx));
            }
        }
        roots.push(after);
        return Some(roots);
    }

    // (C) NUM-BIGINT DIV (stretch) — num-bigint's `div_wide` (64-bit `BigDigit=u64` arm), reached
    // by `to_str_radix` -> `div_rem_digit`. The template is `"div {0}"`, which lowers to the String
    // pieces ["div ", ""] flanking a `{0}` placeholder, so we test the comment-free concatenation
    // (`"div "`) for a leading "div". Operand shape is In(reg-class)=divisor,
    // InOut("rdx"/"dx")=hi=>rem, InOut("rax"/"ax")=lo=>quot. Compute (hi:lo) / d and (hi:lo) % d
    // via u128 BCL div/rem builtins. If the shape is not an EXACT match, bail to None (keep
    // throwing) — never emit a wrong-width div.
    let div_template = strip_asm_comments(&joined_template);
    let div_template = div_template.trim();
    if div_template == "div" || div_template.starts_with("div ") {
        if let Some(roots) = lower_x86_div(operands, after, ctx) {
            return Some(roots);
        }
        return None;
    }

    None
}

/// Helper for case (C): match the exact `div` operand shape and emit the equivalent
/// 128-by-64 unsigned division. Returns `None` (caller keeps throwing) on any shape mismatch.
fn lower_x86_div<'tcx>(
    operands: &[InlineAsmOperand<'tcx>],
    after: Root,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Option<Vec<Root>> {
    use rustc_target::asm::InlineAsmRegOrRegClass;

    // Returns the explicit register's name (lowercased) for an InlineAsmRegOrRegClass::Reg.
    fn reg_name(r: &InlineAsmRegOrRegClass) -> Option<String> {
        if let InlineAsmRegOrRegClass::Reg(reg) = r {
            Some(reg.name().to_ascii_lowercase())
        } else {
            None
        }
    }

    let mut divisor: Option<&Operand<'tcx>> = None;
    let mut hi: Option<(&Operand<'tcx>, &Place<'tcx>)> = None; // (in, rem out) — rdx/dx
    let mut lo: Option<(&Operand<'tcx>, &Place<'tcx>)> = None; // (in, quot out) — rax/ax

    for op in operands {
        match op {
            // The divisor: an `in(reg)` register-class operand.
            InlineAsmOperand::In { reg, value } => {
                if matches!(reg, InlineAsmRegOrRegClass::RegClass(_)) && divisor.is_none() {
                    divisor = Some(value);
                } else {
                    return None;
                }
            }
            // hi/lo: explicit-register inout operands with an out place.
            InlineAsmOperand::InOut {
                reg,
                in_value,
                out_place: Some(out),
                ..
            } => {
                let name = reg_name(reg)?;
                match name.as_str() {
                    "rdx" | "dx" => {
                        if hi.is_some() {
                            return None;
                        }
                        hi = Some((in_value, out));
                    }
                    "rax" | "ax" => {
                        if lo.is_some() {
                            return None;
                        }
                        lo = Some((in_value, out));
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
    }

    let (divisor, (hi_in, rem_out), (lo_in, quot_out)) = match (divisor, hi, lo) {
        (Some(d), Some(h), Some(l)) => (d, h, l),
        _ => return None,
    };

    // Build the 128-bit dividend (hi << 64) | lo, divide by the widened divisor, and truncate the
    // quotient/remainder back to u64. The hi/lo/divisor operands are u64. .NET has no `conv` to a
    // 128-bit primitive (UInt128 is a struct), so every 64<->128 conversion and the u128 shift/or
    // go through the BCL operator helpers used elsewhere in the backend (see src/casts.rs and
    // src/binop/{shift,bitop}.rs), NOT raw `IntCast`/`BinOp`.
    let hi = handle_operand(hi_in, ctx);
    let lo = handle_operand(lo_in, ctx);
    let d = handle_operand(divisor, ctx);

    let u64_ty = Type::Int(Int::U64);
    let u128_ty = Type::Int(Int::U128);
    let hi128 = crate::casts::int_to_int(u64_ty, u128_ty, hi, ctx);
    let lo128 = crate::casts::int_to_int(u64_ty, u128_ty, lo, ctx);
    let d128 = crate::casts::int_to_int(u64_ty, u128_ty, d, ctx);

    // hi128 << 64 via UInt128.op_LeftShift(UInt128, i32).
    let shl_ref = MethodRef::new(
        ClassRef::uint_128(ctx),
        ctx.alloc_string("op_LeftShift"),
        ctx.sig([u128_ty, Type::Int(Int::I32)], u128_ty),
        MethodKind::Static,
        vec![].into(),
    );
    let shl_ref = ctx.alloc_methodref(shl_ref);
    let sh = ctx.alloc_node(Const::I32(64));
    let hi_sh = ctx.call(shl_ref, &[hi128, sh], IsPure::NOT);

    // (hi128 << 64) | lo128 via UInt128.op_BitwiseOr(UInt128, UInt128).
    let or_ref = MethodRef::new(
        ClassRef::uint_128(ctx),
        ctx.alloc_string("op_BitwiseOr"),
        ctx.sig([u128_ty, u128_ty], u128_ty),
        MethodKind::Static,
        vec![].into(),
    );
    let or_ref = ctx.alloc_methodref(or_ref);
    let dividend = ctx.call(or_ref, &[hi_sh, lo128], IsPure::NOT);

    // u128 div/rem are linker builtins (`div_u128` / `mod_u128`), NOT `BinOp::DivUn`.
    let u128_sig = ctx.sig([u128_ty, u128_ty], u128_ty);
    let div_ref = MethodRef::new(
        *ctx.main_module(),
        ctx.alloc_string("div_u128"),
        u128_sig,
        MethodKind::Static,
        vec![].into(),
    );
    let div_ref = ctx.alloc_methodref(div_ref);
    let quot128 = ctx.call(div_ref, &[dividend, d128], IsPure::NOT);

    let mod_ref = MethodRef::new(
        *ctx.main_module(),
        ctx.alloc_string("mod_u128"),
        u128_sig,
        MethodKind::Static,
        vec![].into(),
    );
    let mod_ref = ctx.alloc_methodref(mod_ref);
    let rem128 = ctx.call(mod_ref, &[dividend, d128], IsPure::NOT);

    // Truncate u128 -> u64 via UInt128.op_Explicit(UInt128) -> u64.
    let quot = crate::casts::int_to_int(u128_ty, u64_ty, quot128, ctx);
    let rem = crate::casts::int_to_int(u128_ty, u64_ty, rem128, ctx);

    let mut roots = Vec::new();
    roots.push(place_set(quot_out, quot, ctx));
    roots.push(place_set(rem_out, rem, ctx));
    roots.push(after);
    Some(roots)
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
            let call_ops = call::call(fn_ty, ctx, args, destination, terminator.source_info);
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
        // `become` / guaranteed tail calls are gated behind the unstable, incomplete
        // `explicit_tail_calls` feature, so this cannot appear in std/tokio or any stable crate. A
        // correct (un-optimized) lowering would be `call` + `Ret` of the result (no `.tail` prefix is
        // needed for correctness); left unimplemented until a real `become`-using crate appears.
        TerminatorKind::TailCall { .. } => {
            todo!("TailCall (`become`) requires the unstable `explicit_tail_calls` feature")
        }
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
            // `cond` is the "no-panic" condition: when it holds, control continues to `target`;
            // otherwise the assertion failed and we must panic. Mirror native rustc codegen
            // (`codegen_assert_terminator`): branch to `target` on the no-panic condition, and on
            // the failing path call the *exact* panic lang item the native backend would, with the
            // same operands. This makes the panic message and unwinding match native. The previous
            // implementation routed every kind through a surrogate `assert_*` builtin that discarded
            // the operands and called an unbodied `abort`, which crashed the program with
            // "missing method abort" instead of producing the correct, catchable Rust panic.
            let cond = if *expected {
                handle_operand(cond, ctx)
            } else {
                let c = handle_operand(cond, ctx);
                let e = ctx.alloc_node(*expected);
                ctx.biop(c, e, BinOp::Eq)
            };
            // Branch to the success block when the no-panic condition holds.
            let branch_ok = ctx.alloc_root(CILRoot::Branch(Box::new((
                target.as_u32(),
                0,
                Some(BranchCond::True(cond)),
            ))));
            // Otherwise (fall through) call the matching panic lang item. The special-cased kinds
            // take extra operands before the implicit `#[track_caller]` Location; all others take
            // just the Location (supplied inside `call_panic_lang_item`).
            use rustc_hir::lang_items::LangItem;
            let (lang_item, extra_args): (LangItem, Vec<Interned<CILNode>>) = match msg.as_ref() {
                AssertKind::BoundsCheck { len, index } => {
                    // `fn panic_bounds_check(index: usize, len: usize)`
                    let index = handle_operand(index, ctx);
                    let len = handle_operand(len, ctx);
                    (LangItem::PanicBoundsCheck, vec![index, len])
                }
                AssertKind::MisalignedPointerDereference { required, found } => {
                    // `fn panic_misaligned_pointer_dereference(required: usize, found: usize)`
                    let required = handle_operand(required, ctx);
                    let found = handle_operand(found, ctx);
                    (LangItem::PanicMisalignedPointerDereference, vec![required, found])
                }
                AssertKind::InvalidEnumConstruction(source) => {
                    // `fn panic_invalid_enum_construction(source: u128)`
                    let source = handle_operand(source, ctx);
                    (LangItem::PanicInvalidEnumConstruction, vec![source])
                }
                // Overflow / OverflowNeg / DivisionByZero / RemainderByZero / NullPointerDereference
                // / coroutine-resume kinds: a parameterless `panic_*()` (+ implicit Location).
                other => (other.panic_function(), vec![]),
            };
            let mut roots = vec![branch_ok];
            roots.extend(call_panic_lang_item(
                lang_item,
                &extra_args,
                terminator.source_info,
                ctx,
            ));
            roots
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
            template,
            operands,
            options,
            targets,
            ..
        } => match lower_inline_asm(template, operands, targets, *options, ctx) {
            Some(roots) => roots,
            None => {
                // Keep a clear diagnostic naming the unrecognized template — never a silent
                // miscompile.
                let joined: String = template
                    .iter()
                    .filter_map(|p| {
                        if let InlineAsmTemplatePiece::String(s) = p {
                            Some(s.as_ref())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                eprintln!("Unsupported inline assembly template: {joined}");
                vec![ctx.throw_msg(&format!("Unsupported inline assembly: {joined}"))]
            }
        },
        TerminatorKind::UnwindTerminate(reason) => {
            // The `abort()` landing pad — reached when unwinding would cross a `nounwind` boundary (a
            // double panic, or a panic escaping a `Drop`/`extern "C"` run during unwinding). Rust
            // requires a hard process termination here; a `ReThrow` would incorrectly *continue*
            // unwinding (catchable). Shared with the `UnwindAction::Terminate` *edge* handler.
            emit_terminate(ctx, *reason)
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
        // `Yield` and `CoroutineDrop` are removed by rustc's `coroutine::StateTransform` pass before
        // `instance_mir` ever hands us a coroutine body: `Yield` becomes a discriminant write + a
        // `Return`, resume becomes a `SwitchInt` on the discriminant, and coroutine drop is lowered
        // into a separate sync drop shim (reached through the ordinary `Drop` terminator via
        // `InstanceKind::DropGlue`). So the backend only ever sees the poll-style switch form — these
        // arms are genuinely unreachable. (Async/await and dropping an incomplete `Future` both work
        // through that machinery; see `cargo_tests/pal_async`.) An accurate invariant assertion is the
        // correct, complete handling here — if a future rustc exposed pre-`StateTransform` MIR, this
        // would fire loudly with a precise message rather than miscompile.
        TerminatorKind::CoroutineDrop {} => unreachable!(
            "CoroutineDrop is lowered away by rustc's coroutine::StateTransform before codegen; \
             reaching it means the backend was handed pre-StateTransform MIR"
        ),
        TerminatorKind::Yield { .. } => unreachable!(
            "Yield is lowered to a discriminant write + Return by rustc's coroutine::StateTransform \
             before codegen; reaching it means the backend was handed pre-StateTransform MIR"
        ),
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
