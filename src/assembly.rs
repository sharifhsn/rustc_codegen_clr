use crate::{
    IString,
    basic_block::handler_for_block,
    codegen_error::CodegenError,
    utilis::{classify_magic_fn, is_comptime_entrypoint},
};
use cilly::{
    Access, Assembly, CILRoot, ExceptionRegion, Int, Interned, IntoAsmIndex, MethodDef, MethodRef,
    StaticFieldDesc, Type,
    cilnode::{MethodKind, PtrCastRes},
    ir::BasicBlock,
    ir::method::{LocalDef, MethodImpl},
    utilis::{self},
};

type Root = Interned<cilly::ir::CILRoot>;
use crate::abi::AbiPlan;
pub use crate::fn_ctx::MethodCompileCtx;
use crate::fn_ctx::fn_name_for_instance;
use crate::operand::static_data::{add_static, static_is_nested};
use crate::r#type::{GetTypeExt, adt::field_descrptor, get_type};
use rustc_middle::{
    middle::codegen_fn_attrs::CodegenFnAttrFlags,
    mir::{Local, LocalDecl, Statement, Terminator, interpret::GlobalAlloc},
    mono::MonoItem,
    ty::{Instance, TyCtxt, TyKind},
};
use rustc_structures::CrateType;
type LocalDefList = Vec<LocalDef>;
type ArgsDebugInfo = Vec<Option<Interned<IString>>>;

/// Runtime support symbols use a reserved prefix: they must be rooted like native exports because
/// generated wrappers resolve them by stable name, but they are implementation details rather than
/// managed API. The shipped container macros predate that convention and intentionally expose their
/// stable `rcl_vec_*`, `rcl_map_*`, and `rcl_str_*` entry points to C#.
fn is_reserved_runtime_symbol(name: &str) -> bool {
    name.starts_with("rcl_")
        && !["rcl_vec_", "rcl_map_", "rcl_str_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

/// Whether a non-Rust-ABI local function is part of the crate's native-facing public surface.
///
/// Merely using `extern "C"` does not make a function an external boundary: Mycorrhiza generates
/// private C-ABI trampolines whose function pointers are consumed by managed delegate shims. Only
/// an explicitly named export in a final library artifact can escape to a native caller. Keep this
/// predicate shared with managed-storage validation so accessibility and GC-reference escape rules
/// cannot drift apart.
pub(crate) fn is_explicit_local_export<'tcx>(
    tcx: TyCtxt<'tcx>,
    instance: Instance<'tcx>,
    name: &str,
) -> bool {
    let is_final_library_artifact = tcx.crate_types().iter().any(|crate_type| {
        matches!(
            crate_type,
            CrateType::Cdylib | CrateType::Dylib | CrateType::StaticLib
        )
    });
    if !is_final_library_artifact
        || !instance.def_id().is_local()
        || is_reserved_runtime_symbol(name)
    {
        return false;
    }
    let attrs = tcx.codegen_fn_attrs(instance.def_id());
    attrs.flags.contains(CodegenFnAttrFlags::NO_MANGLE) || attrs.symbol_name.is_some()
}

/// Returns the list of all local variables within MIR of a function, and converts them to the internal type represenation `Type`
fn locals_from_mir<'tcx>(
    locals: &rustc_index::IndexVec<Local, LocalDecl<'tcx>>,
    argc: usize,
    var_debuginfo: &[rustc_middle::mir::VarDebugInfo<'tcx>],
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> (ArgsDebugInfo, LocalDefList) {
    use rustc_middle::mir::VarDebugInfoContents;
    let mut local_types: Vec<LocalDef> = Vec::with_capacity(locals.len());
    for (local_id, local) in locals.iter().enumerate() {
        if local_id == 0 || local_id > argc {
            let ty = ctx.monomorphize(local.ty);
            if crate::config::current().print_local_types() {
                println!(
                    "Local type {ty:?},non-morphic: {non_morph}",
                    non_morph = local.ty
                );
            }
            let name = None;
            let tpe = ctx.type_from_cache(ty);
            let tpe = ctx.alloc_type(tpe);
            local_types.push((name, tpe));
        }
    }
    let mut arg_names: Vec<Option<Interned<IString>>> = (0..argc).map(|_| None).collect();
    for var in var_debuginfo {
        let mir_local = match var.value {
            VarDebugInfoContents::Place(place) => {
                // Check if this is just a "naked" local(eg. just a local varaible, with no indirction)
                if !place.projection.is_empty() {
                    continue;
                }
                place.local.as_usize()
            }
            VarDebugInfoContents::Const(_) => continue,
        };
        if mir_local == 0 {
            local_types[0].0 = Some(var.name.to_string().into_idx(ctx));
        } else if mir_local > argc {
            local_types[mir_local - argc].0 = Some(var.name.to_string().into_idx(ctx));
        } else {
            arg_names[mir_local - 1] = Some(var.name.to_string().into_idx(ctx));
        }
    }
    (arg_names, local_types)
}

/// Real Rust-source parameter names for a comptime-lifted "carrier" fn's `Param` rows
/// (docs/RUST_PARITY_ROADMAP.md Tier 0 item 4). `src/comptime.rs`'s `PendingClass::methods` /
/// `static_methods` / `abstract_methods` / `static_abstract_methods` / `default_methods` loops each
/// build a `MethodDef` that aliases (or, for an abstract interface member, only borrows the signature
/// of) an ordinary, separately-typechecked Rust fn `carrier` -- but unlike `add_fn`'s normal path
/// (`locals_from_mir` above, which threads `var_debug_info` into the `Param` names it emits), those
/// loops never looked at `carrier`'s MIR debug info at all and passed an all-`None` name list, so the
/// exported class member's `Param` table rows carried a signature but no name -- confirmed via
/// reflection (`ParameterInfo.Name == ""`), which broke ASP.NET Core's `RequestDelegateFactory` (keys
/// its parameter-binding dictionary by name; 2+ unnamed params collide) the moment a Rust-defined
/// static method was passed directly as a route-handler delegate (`cargo_tests/cd_mvc`).
///
/// `count` is the number of `Param` rows the caller is about to emit (i.e. `fn_sig.inputs().len()`
/// after any receiver-slicing/byref conversion already applied to the caller's `fn_sig`); `skip` is
/// how many of `carrier`'s OWN leading MIR args (1-based locals) were dropped to get there (1 for an
/// instance member's receiver, 0 for a static one) -- mirrors the skip each comptime.rs call site
/// already applies when deriving `fn_sig` itself, so the two stay index-consistent.
pub fn carrier_arg_names<'tcx>(
    carrier: rustc_middle::ty::Instance<'tcx>,
    skip: usize,
    count: usize,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> ArgsDebugInfo {
    use rustc_middle::mir::VarDebugInfoContents;
    let mir = ctx.tcx().instance_mir(carrier.def);
    let mut arg_names: Vec<Option<Interned<IString>>> = (0..count).map(|_| None).collect();
    for var in &mir.var_debug_info {
        let mir_local = match var.value {
            VarDebugInfoContents::Place(place) => {
                if !place.projection.is_empty() {
                    continue;
                }
                place.local.as_usize()
            }
            VarDebugInfoContents::Const(_) => continue,
        };
        if mir_local > skip && mir_local <= skip + count {
            arg_names[mir_local - skip - 1] = Some(var.name.to_string().into_idx(ctx));
        }
    }
    arg_names
}

/// Turns a terminator into ops, if `ABORT_ON_ERROR` set to false, will handle and recover from errors.
pub fn terminator_to_ops<'tcx>(
    term: &Terminator<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    // Whether the MIR block being lowered is a `cleanup` block — threaded to `handle_terminator` so
    // a `Drop` carrying an `UnwindAction::Terminate` edge can be wrapped in an inline abort guard.
    is_cleanup_block: bool,
) -> Result<Vec<Root>, CodegenError> {
    let terminator = if crate::config::current().abort_on_error() {
        crate::terminator::handle_terminator(term, ctx, is_cleanup_block)
    } else {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::terminator::handle_terminator(term, ctx, is_cleanup_block)
        })) {
            Ok(ok) => ok,
            Err(payload) => {
                let msg = if let Some(msg) =
                    crate::codegen_error::panic_payload_msg(payload.as_ref())
                {
                    rustc_middle::ty::print::with_no_trimmed_paths! {
                    format!("Tried to execute terminator {term:?} whose compialtion message {msg:?}!")}
                } else {
                    eprintln!(
                        "handle_terminator panicked with a non-string message when trying to compile {term:?} !"
                    );
                    rustc_middle::ty::print::with_no_trimmed_paths! {
                    format!("Tried to execute terminator {term:?} whose compialtion failed with a no-string message!")
                    }
                };
                vec![ctx.throw_msg(&msg)]
            }
        }
    };

    Ok(terminator)
}
/// Turns a statement into ops, if `ABORT_ON_ERROR` set to false, will handle and recover from errors.
pub fn statement_to_ops<'tcx>(
    statement: &Statement<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Result<Vec<Root>, CodegenError> {
    ctx.set_span(statement.source_info.span);
    if crate::config::current().abort_on_error() {
        Ok(crate::statement::handle_statement(statement, ctx))
    } else {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::statement::handle_statement(statement, ctx)
        })) {
            Ok(success) => Ok(success),
            Err(payload) => {
                if let Some(msg) = crate::codegen_error::panic_payload_msg(payload.as_ref()) {
                    Err(crate::codegen_error::CodegenError::from_panic_message(msg))
                } else {
                    Err(crate::codegen_error::CodegenError::from_panic_message(
                        "statement_to_ops panicked with a non-string message!",
                    ))
                }
            }
        }
    }
}
/// Adds a rust MIR function to the assembly.
pub fn add_fn<'tcx, 'asm, 'a: 'asm>(
    name: &str,
    ctx: &'a mut MethodCompileCtx<'tcx, 'asm>,
) -> Result<(), CodegenError> {
    let kind = ctx
        .instance()
        .ty(
            ctx.tcx(),
            rustc_middle::ty::TypingEnv::fully_monomorphized(),
        )
        .kind();
    if let TyKind::FnDef(_, _) = kind {
        //ALL OK.
    } else if let TyKind::Closure(_, _) = kind {
    } else if let TyKind::Coroutine(_, _) = kind {
    } else {
        println!(
            "fn item {instance:?} is not a function definition type or a closure. Skippping.",
            instance = ctx.instance()
        );
        return Ok(());
    }
    let mir = ctx.tcx().instance_mir(ctx.instance().def);
    // DUMP_MIR=<substr>: print the EXACT optimized MIR the backend is about to translate, for any
    // function whose (mangled) name contains <substr>. This is the ground truth for MIR↔CIL diffing —
    // the monomorphized `instance_mir` is otherwise hard to obtain (library generics like
    // `Vec::<u32>::extend_with` are instantiated at codegen, not emitted by `--emit=mir`). Pairs with
    // `INSERT_MIR_DEBUG_COMMENTS=1` (which annotates the emitted CIL with these same statements).
    if let Some(filter) = crate::config::current().dump_mir()
        && name.contains(filter)
    {
        use std::fmt::Write as _;
        use std::io::Write as _;
        // APPEND to a file (default /tmp/dump_mir.txt, override DUMP_MIR_OUT) rather than stderr:
        // cargo-dotnet codegens std/alloc in a discarded warm pass, so library generics like
        // `Vec::<u32>::extend_with` would be lost from stderr. A file survives every pass.
        let path = crate::config::current().dump_mir_out();
        let mut out = format!("\n===DUMP_MIR_BEGIN {name}\n");
        for (local, decl) in mir.local_decls.iter_enumerated() {
            let _ = writeln!(out, "  let {local:?}: {:?};", decl.ty);
        }
        for (bb, data) in mir.basic_blocks.iter_enumerated() {
            let _ = writeln!(out, "  {bb:?}:");
            for stmt in &data.statements {
                let _ = writeln!(out, "    {stmt:?};");
            }
            if let Some(term) = &data.terminator {
                let _ = writeln!(out, "    {:?};", term.kind);
            }
        }
        let _ = writeln!(out, "===DUMP_MIR_END {name}");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = f.write_all(out.as_bytes());
        }
    }
    let mut ctx = ctx.with_body(mir);
    let ctx = &mut ctx;
    // A generated managed-type definition opts into interpretation with an exact marker. Symbol
    // substrings are not identity: an ordinary same-named function/module must compile normally.
    if is_comptime_entrypoint(ctx.tcx(), ctx.instance().def_id()) {
        crate::comptime::interpret(ctx, mir);
        // The generated `#[used]` retention static still references this carrier function after
        // its MIR has been interpreted into metadata. Leaving the carrier undefined therefore
        // creates a reachable `MethodImpl::Missing`, which the fatal post-link verifier correctly
        // rejects. Materialize a harmless void body: the function has already done its only job at
        // compile time, but remains a valid target if metadata reachability keeps the symbol alive.
        let sig = AbiPlan::from_instance(ctx.instance(), ctx)
            .signature()
            .clone();
        assert!(
            sig.inputs().is_empty() && *sig.output() == Type::Void,
            "comptime entrypoint must have signature fn()"
        );
        let sig = ctx.alloc_sig(sig);
        let class = ctx.main_module();
        let name = ctx.alloc_string(name);
        let ret = ctx.alloc_root(CILRoot::VoidRet);
        let method = MethodDef::new(
            Access::Assembly,
            class,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![],
        );
        ctx.new_method(method);
        return Ok(());
    }
    if classify_magic_fn(ctx.tcx(), ctx.instance().def_id()).is_some() {
        return Ok(());
    }

    crate::managed_storage::validate_body(ctx)?;

    let timer = ctx.tcx().prof.generic_activity_with_arg("codegen fn", name);
    let attrs = ctx.tcx().codegen_fn_attrs(ctx.instance().def_id());
    // Only an explicit `#[unsafe(no_mangle)]` or `#[unsafe(export_name = "...")]` on the user's
    // crate authorizes a public managed export. rustc's broader `contains_extern_indicator()` also
    // includes cross-CGU linkage, compiler-builtins, and standard-library implementation symbols;
    // treating that native-linker metadata as CLR accessibility leaked hundreds of implementation
    // methods through `MainModule` reflection. Dependency and ordinary Rust methods remain
    // assembly-visible so generated wrappers can call them without exposing them to C#.
    //
    // Explicit exports map to `Access::Extern`, which the linker's dead-code pass treats as a ROOT
    // (`eliminate_dead_fns`). This is what lets a **library** crate keep its public API: a library has
    // no entrypoint to root the call graph, so without this every method would be eliminated. (For a
    // binary it is a no-op beyond keeping unused exports, which is the correct semantics.)
    let is_explicit_local_export = is_explicit_local_export(ctx.tcx(), ctx.instance(), name);
    let access = if is_explicit_local_export {
        Access::Extern
    } else if attrs.contains_extern_indicator() {
        Access::InternalExtern
    } else {
        Access::Assembly
    };
    #[allow(deprecated)]
    let export_nullability = (access == Access::Extern)
        .then(|| {
            ctx.tcx()
                .get_all_attrs(ctx.instance().def_id())
                .iter()
                .filter_map(|attribute| attribute.doc_str())
                .find_map(|doc| {
                    doc.as_str()
                        .strip_prefix("__rustc_codegen_clr_nullability:")
                        .map(str::to_owned)
                })
        })
        .flatten();
    #[allow(deprecated)]
    let export_custom_attrs: Vec<String> = if access == Access::Extern {
        ctx.tcx()
            .get_all_attrs(ctx.instance().def_id())
            .iter()
            .filter_map(|attribute| attribute.doc_str())
            .filter_map(|doc| {
                doc.as_str()
                    .strip_prefix("__rustc_codegen_clr_custom_attr:")
                    .map(str::to_owned)
            })
            .collect()
    } else {
        vec![]
    };
    // Handle the function signature
    let abi = AbiPlan::from_instance(ctx.instance(), ctx);
    let sig = abi.signature().clone();

    // The current core implementation uses a generic element-wise loop for
    // `SlicePartialEq::equal_same_length` when the element type does not opt
    // into `BytewiseEq`.  That is correct for ordinary values, but the unit
    // specialization is a degenerate case: every element is equal and the
    // official coretests intentionally compare slices with `usize::MAX`
    // elements.  Lowering that loop literally would spend effectively
    // forever visiting the same zero-sized value.  Keep this backend-owned
    // specialization narrow (the exact monomorphization is visible in the
    // demangled symbol) and return the contractually correct result directly.
    let demangled = format!("{:#}", rustc_demangle::demangle(name));
    if demangled.contains("<() as core::slice::cmp::SlicePartialEq<()>>::equal_same_length") {
        let sig_idx = ctx.alloc_sig(sig.clone());
        let true_value = ctx.alloc_node(cilly::Const::Bool(true));
        let ret = ctx.alloc_root(CILRoot::Ret(true_value));
        let method = MethodDef::new(
            Access::Assembly,
            cilly::class::ClassDefIdx(*ctx.main_module()),
            ctx.alloc_string(name),
            sig_idx,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            (0..sig.inputs().len()).map(|_| None).collect(),
        );
        ctx.new_method(method);
        return Ok(());
    }

    // Get locals
    let (arg_names, mut locals) =
        locals_from_mir(&mir.local_decls, mir.arg_count, &mir.var_debug_info, ctx);
    let mut arg_names = abi.physical_argument_names(arg_names, mir, ctx);
    if let Some(slot) = abi.caller_location_slot() {
        arg_names[slot] = Some("panic_location".into_idx(ctx));
    }
    assert_eq!(arg_names.len(), sig.inputs().len());

    let blocks = &mir.basic_blocks;
    let mut normal_bbs = Vec::new();
    let mut cleanup_bbs = Vec::new();
    let mut exception_regions = Vec::new();
    // Synthetic terminate-handler ids (one/two past the last MIR block) referenced by
    // `UnwindAction::Terminate` edges; the matching `FailFast` cleanup blocks are materialized after
    // the loop (see `basic_block::terminate_handler_id` / `terminator::emit_terminate`).
    let n_blocks = u32::try_from(blocks.len()).expect("function has more than 2^32 basic blocks");
    let mut used_terminate: std::collections::HashSet<u32> = std::collections::HashSet::new();
    // Used for funcrions with the rust_call ABI
    let mut repack_cil = if let Some(spread_arg) = mir.spread_arg {
        // Prepare for repacking the argument tuple, by allocating a local
        let repacked = u32::try_from(locals.len()).expect("More than 2^32 arguments of a function");
        let repacked_ty: rustc_middle::ty::Ty = ctx.monomorphize(mir.local_decls[spread_arg].ty);
        let repacked_tpe = get_type(repacked_ty, ctx);
        locals.push((
            Some("repacked_arg".into_idx(ctx)),
            ctx.alloc_type(repacked_tpe),
        ));
        let mut repack_cil: Vec<Root> = Vec::new();
        // For each element of the tuple, get the argument spread_arg + n
        let spread_fields = abi.rust_call_tuple_fields(mir, ctx);
        for (field_id, (physical_slot, ty)) in spread_fields.into_iter().enumerate() {
            if ctx.type_from_cache(ty) == Type::Void {
                continue;
            }
            let arg_field = field_descrptor(repacked_ty, field_id.try_into().unwrap(), ctx);
            let arg = ctx.alloc_node(cilly::ir::CILNode::LdArg(
                u32::try_from(physical_slot).expect("ABI argument index exceeds u32"),
            ));
            let repacked = ctx.alloc_node(cilly::ir::CILNode::LdLocA(repacked));
            repack_cil.push(ctx.alloc_root(cilly::CILRoot::SetField(Box::new((
                arg_field, repacked, arg,
            )))));
        }
        repack_cil
    } else {
        vec![]
    };
    let cpuid_scratch_count = crate::terminator::cpuid_scratch_local_count(mir);
    if cpuid_scratch_count != 0 {
        let scratch_start = u32::try_from(locals.len()).expect("more than 2^32 method locals");
        let result_type = Type::ClassRef(crate::terminator::cpuid_result_class(ctx));
        let result_type = ctx.alloc_type(result_type);
        locals.extend((0..cpuid_scratch_count).map(|_| (None, result_type)));
        ctx.reserve_synthetic_local_range(
            scratch_start,
            u32::try_from(cpuid_scratch_count).expect("more than 2^32 CPUID sites"),
        );
    }
    let sig_idx = ctx.alloc_sig(sig.clone());
    // If any statement fails to compile, the per-statement `throw` recovery below would leave the
    // surrounding block structure (branches/handlers) intact while the failed statement no longer
    // produces the value later blocks expect — which can emit branches to blocks the optimizer then
    // drops, yielding invalid CIL (`ilasm: Undefined Label: bbN`). To keep recovery sound, we record
    // the first failure and, after assembling the blocks, replace the *whole* method body with a
    // single throwing stub (no branches → always-valid IL). The method then throws if ever called.
    let mut compile_failed: Option<String> = None;
    // TRACE_FN=<substr>: inject a runtime `Console.WriteLine` at each basic-block entry (and at every
    // `SwitchInt`, via `handle_switch`) for any function whose (mangled) name contains <substr>. Unlike
    // `DUMP_MIR` (which dumps at *codegen* time and is blind to which cargo-dotnet build pass actually
    // runs), this fires at *runtime*, so it reveals the actually-executed control-flow path — exactly the
    // static→runtime gap that defeats hand-reading the `.il`. Output is greppable via the ">>T" prefix.
    // Keep the filter narrow (one type/fn) to avoid flooding hot loops. See feasibility/rcc-debug.
    let trace_this_fn = crate::config::current()
        .trace_fn()
        .is_some_and(|filter| name.contains(filter));
    // Compact, stable per-function tag for trace lines (tail of the mangled name fits the symbol identity).
    let trace_tag: String = name
        .chars()
        .rev()
        .take(48)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    // Used for type-checking the CIL to ensure its validity.
    for (last_bb_id, block_data) in blocks.into_iter().enumerate() {
        let mut trees: Vec<Root> = Vec::new();
        if trace_this_fn {
            let dbg = ctx.debug_msg(&format!(">>T bb{last_bb_id} {trace_tag}"));
            trees.push(dbg);
        }
        for statement in &block_data.statements {
            if crate::config::current().insert_mir_debug_comments() {
                let msg = rustc_middle::ty::print::with_no_trimmed_paths!(format!("{statement:?}"));
                let dbg = ctx.debug_msg(&msg);
                trees.push(dbg);
                let msg = source_info_diagnostic_location(ctx, statement.source_info);
                let dbg = ctx.debug_msg(&msg);
                trees.push(dbg);
            }

            let statement_tree = match statement_to_ops(statement, ctx) {
                Ok(ops) => ops,
                Err(err) => {
                    rustc_middle::ty::print::with_no_trimmed_paths! {eprintln!(
                        "Method \"{name}\" failed to compile statement {statement:?} with message {err:?}\n"
                    )};
                    if compile_failed.is_none() {
                        compile_failed = rustc_middle::ty::print::with_no_trimmed_paths! {Some(format!(
                            "Method \"{name}\" could not be compiled: statement {statement:?} failed with {err:?}."
                        ))};
                    }
                    rustc_middle::ty::print::with_no_trimmed_paths! {vec![ctx.throw_msg(&format!("Tired to run a statement {statement:?} which failed to compile with error message {err:?}."))]}
                }
            };
            // Typecheck each produced root, warning (not failing) on errors.
            for root in &statement_tree {
                if let Err(err) = ctx.get_root(*root).clone().typecheck(sig_idx, &locals, ctx) {
                    ctx.tcx().dcx().span_warn(
                        statement.source_info.span,
                        format!("Typecheck failed:{err:?}"),
                    );
                }
            }
            // Only save debuginfo for statements which result in ops.
            if !statement_tree.is_empty() {
                let sfi = span_source_info(ctx, statement.source_info);
                trees.push(sfi);
            }
            trees.extend(statement_tree);
        }
        if let Some(term) = &block_data.terminator {
            if crate::config::current().insert_mir_debug_comments() {
                let msg = rustc_middle::ty::print::with_no_trimmed_paths!(format!("{term:?}"));
                let dbg = ctx.debug_msg(&msg);
                trees.push(dbg);
            }
            let term_trees =
                terminator_to_ops(term, ctx, block_data.is_cleanup).unwrap_or_else(|err| {
                    panic!("Could not compile terminator {term:?} because {err:?}")
                });
            for root in &term_trees {
                if let Err(err) = ctx.get_root(*root).clone().typecheck(sig_idx, &locals, ctx) {
                    ctx.tcx()
                        .dcx()
                        .span_warn(term.source_info.span, format!("Typecheck failed:{err:?}"));
                }
            }
            if !term_trees.is_empty() {
                let sfi = span_source_info(ctx, term.source_info);
                trees.push(sfi);
            }
            trees.extend(term_trees);
        }
        let handler_id = handler_for_block(
            block_data,
            &mir.basic_blocks,
            ctx.tcx(),
            &ctx.instance(),
            mir,
        );
        // A handler id past the last real MIR block is a synthetic terminate handler — record it so
        // the matching FailFast cleanup block is emitted below.
        if let Some(h) = handler_id
            && h >= n_blocks
        {
            used_terminate.insert(h);
        }
        let block_id = u32::try_from(last_bb_id).unwrap();
        let bb = BasicBlock::new(trees, block_id, None);
        if block_data.is_cleanup {
            cleanup_bbs.push(bb);
        } else {
            if let Some(handler_entry) = handler_id {
                exception_regions.push(ExceptionRegion::new(block_id, handler_entry));
            }
            normal_bbs.push(bb);
        }
    }

    // Materialize the synthetic terminate-handler cleanup blocks referenced by `UnwindAction::Terminate`
    // edges: a hard, uncatchable `FailFast` abort (NOT a rethrow that an outer `catch_unwind` could
    // absorb). Built here, after the block loop, because `emit_terminate` needs `&mut ctx`. If a
    // statement failed to compile, the `compile_failed` reset below wipes these too — consistent.
    for (reason, id) in [
        (rustc_middle::mir::UnwindTerminateReason::Abi, n_blocks),
        (
            rustc_middle::mir::UnwindTerminateReason::InCleanup,
            n_blocks.saturating_add(1),
        ),
    ] {
        if used_terminate.contains(&id) {
            let roots = crate::terminator::emit_terminate(ctx, reason);
            cleanup_bbs.push(BasicBlock::new(roots, id, None));
        }
    }

    // A statement failed to compile: discard the partially-built (and potentially branch-inconsistent)
    // body and emit a single throwing block, guaranteeing valid CIL. The method throws if called.
    if let Some(reason) = compile_failed {
        let throw = ctx.throw_msg(&reason);
        normal_bbs = vec![BasicBlock::new(vec![throw], 0, None)];
        cleanup_bbs = Vec::new();
        exception_regions = Vec::new();
        repack_cil = Vec::new();
    }
    // Get the first bb, and append repack_cil at its start
    let first_bb: &mut BasicBlock = &mut normal_bbs[0];
    repack_cil.append(first_bb.roots_mut());
    *first_bb.roots_mut() = repack_cil;

    let main_module = ctx.main_module();
    let mut method = MethodDef::from_region_blocks(
        access,
        main_module,
        name,
        sig_idx,
        MethodKind::Static,
        normal_bbs,
        cleanup_bbs,
        exception_regions,
        locals,
        arg_names,
        ctx,
    );
    if let Some(spec) = export_nullability {
        if let Some((context, return_flag, parameter_flags)) =
            crate::comptime::parse_nullability_spec(&spec, sig.inputs().len())
        {
            method = method.with_nullability(context, return_flag, parameter_flags);
        }
    }
    if !export_custom_attrs.is_empty() {
        let mut method_attributes = Vec::new();
        let mut return_attributes = Vec::new();
        let mut parameter_attributes = vec![Vec::new(); sig.inputs().len()];
        for marker in export_custom_attrs {
            let (target, packed) = marker
                .split_once(':')
                .expect("dotnet export custom-attribute marker is malformed");
            match target {
                "m" => method_attributes.push(crate::comptime::pending_custom_attr_to_cilly(
                    ctx,
                    &crate::comptime::decode_custom_attr_spec(packed),
                )),
                "r" => return_attributes.push(crate::comptime::pending_custom_attr_to_cilly(
                    ctx,
                    &crate::comptime::decode_custom_attr_spec(packed),
                )),
                "p" => {
                    let (index, packed) = packed
                        .split_once(':')
                        .expect("dotnet export parameter-attribute marker is malformed");
                    let index = index
                        .parse::<usize>()
                        .expect("dotnet export parameter-attribute index is malformed");
                    let destination = parameter_attributes.get_mut(index).unwrap_or_else(|| {
                        panic!(
                            "dotnet export parameter-attribute index {index} exceeds {} parameters",
                            sig.inputs().len()
                        )
                    });
                    destination.push(crate::comptime::pending_custom_attr_to_cilly(
                        ctx,
                        &crate::comptime::decode_custom_attr_spec(packed),
                    ));
                }
                other => panic!("unknown dotnet export custom-attribute target {other:?}"),
            }
        }
        method = method.with_custom_attributes(
            method_attributes,
            return_attributes,
            parameter_attributes,
        );
    }
    if let Err(err) = method.typecheck(ctx) {
        ctx.tcx()
            .dcx()
            .span_warn(ctx.body().span, format!("Typecheck failed {err:?}"));
    };
    ctx.new_method(method);
    drop(timer);
    Ok(())
}
/// This is used *ONLY* to catch uncaught errors.
pub fn checked_add_fn<'a: 'c, 'b: 'c, 'c>(
    ctx: &'a mut MethodCompileCtx<'b, 'c>,
    name: &str,
) -> Result<(), CodegenError> {
    add_fn(name, ctx)
}
/// Adds a MIR item (method,inline assembly code, etc.) to the assembly.
#[allow(clippy::similar_names)]
pub fn add_item<'tcx>(
    asm: &mut Assembly,
    item: MonoItem<'tcx>,
    tcx: TyCtxt<'tcx>,
) -> Result<(), CodegenError> {
    match item {
        MonoItem::Fn(instance) => {
            let symbol_name: IString = fn_name_for_instance(tcx, instance).into();
            let mut ctx = MethodCompileCtx::new(tcx, None, instance, asm);
            let fn_timer = tcx
                .prof
                .generic_activity_with_arg("compile function", item.symbol_name(tcx).to_string());
            rustc_middle::ty::print::with_no_trimmed_paths! {
                checked_add_fn(&mut ctx, &symbol_name)?
            };
            drop(fn_timer);
            Ok(())
        }
        MonoItem::GlobalAsm(asm) => Err(CodegenError::unsupported(
            "global_asm",
            format!("the .NET backend cannot emit this global assembly item: {asm:?}"),
        )),
        MonoItem::Static(stotic) => {
            let static_timer = tcx.prof.generic_activity_with_arg(
                "compile static initializer",
                item.symbol_name(tcx).to_string(),
            );

            let instance =
                rustc_middle::ty::Instance::new_raw(stotic, rustc_middle::ty::List::empty());
            let mut ctx = MethodCompileCtx::new(tcx, None, instance, asm);
            // Anonymous nested statics have no queryable Rust type (see `add_static`); their bytes
            // come only from a const allocation and therefore cannot contain a runtime CLR ref.
            if !static_is_nested(tcx, stotic) {
                let static_ty = tcx
                    .type_of(stotic)
                    .instantiate_identity()
                    .skip_normalization();
                if let Some(violation) =
                    crate::managed_storage::static_storage_violation(static_ty, &ctx)
                {
                    return Err(CodegenError::unsupported(
                        "managed_reference_storage",
                        format!(
                            "static {stotic:?} stores {} at {} in native Rust storage (type {static_ty:?}); use a GCHandle-backed wrapper",
                            violation.kind.description(),
                            violation.path,
                        ),
                    ));
                }
            }
            let alloc = tcx.eval_static_initializer(stotic).unwrap();
            let attrs = tcx.codegen_fn_attrs(stotic);
            let int8_ptr = ctx.nptr(Type::Int(Int::I8));
            let int8_ptr_ptr = ctx.nptr(int8_ptr);
            if let Some(section) = attrs.link_section {
                if section.to_string().contains(".init_array") {
                    let argc = utilis::argc_argv_init_method(&mut ctx);
                    let init_argc = ctx.alloc_root(cilly::CILRoot::call(argc, []));

                    ctx.add_user_init(&[init_argc]);
                    let get_environ: Interned<MethodRef> = utilis::get_environ(&mut ctx);
                    let fn_ptr = alloc.0.provenance().ptrs().iter().next().unwrap();
                    let fn_ptr = tcx.global_alloc(fn_ptr.1.alloc_id());
                    let init_call_site = if let GlobalAlloc::Function {
                        instance: finstance,
                    } = fn_ptr
                    {
                        let mut ctx = MethodCompileCtx::new(tcx, None, finstance, &mut ctx);
                        // If it is a function, patch its pointer up.
                        let abi = AbiPlan::from_instance(finstance, &mut ctx);
                        let function_name = fn_name_for_instance(tcx, finstance);
                        MethodRef::new(
                            *ctx.main_module(),
                            ctx.alloc_string(function_name),
                            ctx.alloc_sig(abi.signature().clone()),
                            MethodKind::Static,
                            vec![].into(),
                        )
                    } else {
                        panic!()
                    };

                    let argv = ctx.alloc_string("argv");
                    let argc = ctx.alloc_string("argc");
                    let main_module = ctx.main_module();
                    let mref = ctx.alloc_methodref(init_call_site);
                    let argv =
                        ctx.alloc_sfld(StaticFieldDesc::new(*main_module, argv, int8_ptr_ptr));
                    let argc = ctx.alloc_sfld(StaticFieldDesc::new(
                        *main_module,
                        argc,
                        Type::Int(Int::I32),
                    ));
                    let argv = ctx.alloc_node(cilly::CILNode::LdStaticField(argv));
                    let uint8_ptr = ctx.nptr(Int::U8);
                    let uint8_ptr_idx = ctx.alloc_type(uint8_ptr);
                    let args = [
                        ctx.alloc_node(cilly::CILNode::LdStaticField(argc)),
                        ctx.alloc_node(cilly::CILNode::PtrCast(
                            argv,
                            Box::new(PtrCastRes::Ptr(uint8_ptr_idx)),
                        )),
                        ctx.alloc_node(cilly::CILNode::call(get_environ, [])),
                    ];
                    let root = ctx.alloc_root(cilly::CILRoot::call(mref, args));
                    ctx.add_user_init(&[root]);
                } else {
                    panic!("Unsuported link section {section}.")
                }
            }

            add_static(stotic, &mut ctx);

            drop(static_timer);

            Ok(())
        }
    }
}

pub(crate) fn span_source_info<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    source_info: rustc_middle::mir::SourceInfo,
) -> Interned<CILRoot> {
    let (file, lstart, cstart, lend, mut cend) = source_info_location(ctx, source_info);
    if cstart >= cend {
        cend = cstart + 1;
    }
    // Emit the source-file-info root directly. The line range is `lstart..lend`, the column
    // range is `cstart..cend` (with `cstart < cend` guaranteed by the clamp above).
    let line_start = u32::try_from((lstart as u64).min(u64::from(u32::MAX))).unwrap();
    let line_end = u32::try_from((lend as u64).min(u64::from(u32::MAX))).unwrap();
    let line_len = u16::try_from((line_end - line_start).min(u32::from(u16::MAX))).unwrap();
    let col_start = u16::try_from((cstart as u64).min(u64::from(u16::MAX))).unwrap();
    let col_end = u16::try_from((cend as u64).min(u64::from(u16::MAX))).unwrap();
    let col_len = col_end - col_start;
    let file = ctx.alloc_string(file);
    ctx.alloc_root(CILRoot::SourceFileInfo {
        line_start,
        line_len,
        col_start,
        col_len,
        file,
    })
}

/// Formats a MIR source location without rustc's session-local hygiene/scope indices.
///
/// `Debug` formatting a `Span` appends values such as `(#510)`. The same upstream inline MIR can
/// acquire a different number when instantiated in another crate, which made otherwise identical
/// CIL method bodies fail strict cross-shard comparison. File/line/column coordinates are the
/// stable, user-relevant part of that diagnostic.
pub(crate) fn source_info_diagnostic_location<'tcx>(
    ctx: &MethodCompileCtx<'tcx, '_>,
    source_info: rustc_middle::mir::SourceInfo,
) -> String {
    let (file, line_start, col_start, line_end, col_end) = source_info_location(ctx, source_info);
    format!("{file}:{line_start}:{col_start}: {line_end}:{col_end}")
}

fn source_info_location<'tcx>(
    ctx: &MethodCompileCtx<'tcx, '_>,
    source_info: rustc_middle::mir::SourceInfo,
) -> (String, usize, usize, usize, usize) {
    let span = outermost_inlined_callsite_span(ctx.body(), source_info);
    let (file, line_start, col_start, line_end, col_end) =
        ctx.tcx().sess.source_map().span_to_location_info(span);
    let file = file.map_or(String::new(), |file| debuginfo_file_name(&file));
    (file, line_start, col_start, line_end, col_end)
}

fn outermost_inlined_callsite_span<'tcx>(
    body: &rustc_middle::mir::Body<'tcx>,
    mut source_info: rustc_middle::mir::SourceInfo,
) -> rustc_span::Span {
    let mut outermost_callsite = None;
    loop {
        let scope_data = &body.source_scopes[source_info.scope];
        if let Some((_, callsite_span)) = scope_data.inlined {
            outermost_callsite = Some(callsite_span);
        }
        match scope_data.inlined_parent_scope {
            Some(parent) => source_info.scope = parent,
            None => break,
        }
    }
    outermost_callsite.unwrap_or(source_info.span)
}

fn debuginfo_file_name(file: &rustc_span::SourceFile) -> String {
    match &file.name {
        rustc_span::FileName::Real(name) => {
            let (working_dir, embeddable_name) =
                name.embeddable_name(rustc_span::RemapPathScopeComponents::DEBUGINFO);
            let path = if embeddable_name.is_absolute() || working_dir.as_os_str().is_empty() {
                embeddable_name.to_path_buf()
            } else {
                working_dir.join(embeddable_name)
            };
            path.to_string_lossy().into_owned()
        }
        name => name
            .display(rustc_span::RemapPathScopeComponents::DEBUGINFO)
            .to_string(),
    }
}
