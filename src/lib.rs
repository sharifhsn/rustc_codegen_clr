#![feature(rustc_private)]
#![feature(alloc_error_hook)]
#![warn(clippy::pedantic)]
// Used for handling some configs. Will be refactored later.
#![allow(clippy::assertions_on_constants)]
// The complexity is managable for now.
#![allow(clippy::too_many_lines)]
// Not a big issue.
#![allow(clippy::module_name_repetitions)]
// docs are WIP
#![allow(
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::module_inception
)]
//#![warn(missing_docs)]
//#![warn(clippy::missing_docs_in_private_items)]

//#![deny(dead_code)]

//! Rustc Codegen CLR - an experimental rustc backend compiling Rust for .NET. This project aims to bring the speed and memory efficency of Rust to .NET.
//!
//! # Explaing the project
//!  
//! This part of the documentation aims to help anyone interested in the project better understand the guiding principles behind it, and its architecture.
//! It is a bit more on the techincal side, but please feel free to ask me if anything is unclear.
//!
//! ## Guiding principles
//!
//! The project aims to keep it's codebase as simple as possible, at the cost of increased compile times. Compile times still should be on par with someting like
//! LLVM, due to the project not needing to perform as many complex optimzations.
//!
//! ### Functional
//!
//! The project heavily uses functional programing style. Each element of Rust MIR is handled by a simple pure function, which takes the MIR element,
//! and returns a single translated item, eg. a method, a list of CIL ops, or type definitions. All parameters passed to the functuion are immutable:
//! this heavily simplifies testing, and makes the backend far more predictable. It also makes recovering from errors very easy, since we don't have to deal
//! with potential changes to mutable structutes.
//! This does have its drawbacks(it makes allocating additional local variables harder than it needs to be), but its benefits outhgweight the issues it brings,
//! at least at this point in time.
//!
//! One notable exeception to this rule is the [`crate::type::tycache::TyCache`] - a structure used for caching type translations. Since it needs to perform some expensive work(eg. find `core::ptr::metadata::PtrComponents`)
//! upfront, reusing the `TyCache` for a whole codegen unit is needed. Thus, it is passed by a mutable reference. `TyCache` can be easily reset after a panic, ensuirng panic recovery is safe.
//!
//! ## Faithful to MIR
//!
//! The project first translates MIR to CIL in a very precise, but inefficient fasion. This is a deliberate choice - it recduces the chance of bugs, and enables easy checking of the resulting CIL.
//!
//! Since any given  MIR statement will always result in the same ops, and the ops from each statement are kept separate, any misformed piece of CIL byecode can be easily traced back to a
//! particular MIR statement.
//!
//! This way, it is far less likely that a piece of code will be miscompiled. It also helps with debuging, and allows us to achieve a very high-level translation of MIR.
//!
//! This intermediate, inefficent CIL can be optimized using the functions within the [`crate::opt`] module. Those optimzations are allowed to do things like reorder statements, remove/add locals, etc.
//! So, when debuging issues, it is recomeded the additional optimzations be turned off by seting the enviroment varaible `OPTIMIZE_CIL` to 0.
//!
//! ## Internal IR
//!
//! The project-internal IR(CIL trees) is defined in the module [`crate::cil_tree`]. Additional CIL-related data structures, such as call targets and field descriptors can be found in [`crate::cil`].
//! [`crate::cil_tree`] will also contain a brief overview of the CIL represenation used by the project.
//!
//! ## Type represenation
//!
//! All type-related data structures are defined in the module [`crate::type`]
//!
//! ## MIR handling
//!
//! Each MIR element is handled by a function defined in a module with the corresponding name. For example, MIR statements are handled by the function [`crate::statement::handle_statement`].
//!
//! # Where the compilation starts
//!
//! Almost everyting in this file is related to things specific to the rust compiler - reciving MIR from rustc, loading/saving intermediate data,
//! linking the final executable.
//! The compilation process really begins in [`crate::assembly::add_item`] - this is where an item - static, function, or inline assembly - gets turned into
//! its .NET representation. The [`crate::assembly::add_fn`] uses [`cilly::asm::Assembly::add_typedef`] to add all types needed by a method to the
//! assembly. `add_fn` gets the function name, signature, local varaiables and MIR. It uses `handle_statement` and `handle_terminator` turn MIR statements
//! and block terminators into CIL ops.
// TODO: Extend project desctiption.

// References to internal rustc crates.
extern crate rustc_abi;
extern crate rustc_ast;

extern crate rustc_codegen_ssa;
extern crate rustc_const_eval;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_metadata;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_symbol_mangling;
extern crate rustc_target;
extern crate rustc_ty_utils;

pub use rustc_codegen_clr_place::*;
// Modules
/// Code handling the creation of aggreate values (Arrays, enums,structs,tuples,etc.)
mod aggregate;
/// Representation of a .NET assembly
pub mod assembly;
/// Moudle containing defintion of basic blocks and method operating on them.
pub mod basic_block;
/// Code handling binary operations
mod binop;
mod call_info;
/// Code hansling rust `as` casts.
mod casts;
/// Runtime errors and utlity functions/macros related to them
mod codegen_error;
/// Test harnesses.
pub mod compile_test;
/// Implementation of compiletime features neccessary for interop.
mod comptime;
/// Method compilation context
mod fn_ctx;
/// Signature of a function (inputs)->output
pub mod function_sig;
/// Interop type handling.
mod interop;
pub mod native_pastrough;
/// Handles a MIR operand.
mod operand;
/// Converts righthandside of a MIR statement into CIL ops.
mod rvalue;
/// Code dealing with truning an individual MIR statement into CIL ops.
pub mod statement;
/// Converts a terminator of a basic block into CIL ops.
mod terminator;
/// Code related to types.
pub mod r#type;

/// Implementations of unary operations.
mod unop;
/// Contains small helper functions(debug assertions, functions used to get field names, etc), which are frequently used, but are not specific to a part of the coodegen.
mod utilis;

pub mod config;
mod unsize;
// rustc functions used here.
use cilly::{
    Assembly,
    {cilnode::MethodKind, MethodRef},
};
use rustc_codegen_clr_ctx::MethodCompileCtx;
use rustc_codegen_ssa::{
    back::archive::{ArArchiveBuilder, ArchiveBuilder, ArchiveBuilderBuilder},
    traits::CodegenBackend,
    CompiledModule, CompiledModules, CrateInfo, ModuleKind,
};

use rustc_metadata::EncodedMetadata;
use rustc_middle::{
    dep_graph::WorkProductMap,
    ty::TyCtxt,
};
use rustc_session::{
    config::{OutputFilenames, OutputType},
    Session,
};

use std::{any::Any, path::Path};
/// Immutable string - used to save a bit of memory on storage.
pub type IString = cilly::IString;
/// Immutable string - used to save a bit of memory on storage.
pub type AString = std::sync::Arc<Box<str>>;

/// An instance of the codegen.
struct MyBackend;
impl CodegenBackend for MyBackend {
    fn name(&self)->&'static str{
        "cg_clr"
    }
    fn target_cpu(&self, sess: &Session) -> String {
        sess.target.cpu.to_string()
    }
    /// Compiles a crate, and returns its in-memory representaion as a .NET assembly.
    fn codegen_crate<'a>(
        &self,
        tcx: TyCtxt<'_>,

    ) -> Box<dyn Any> {
        let cgus = tcx.collect_and_partition_mono_items(());

        let mut asm = Assembly::default();
     
        let _ = cilly::utilis::get_environ(&mut asm);

        for cgu in cgus.codegen_units {
            //println!("codegen {} has {} items.", cgu.name(), cgu.items().len());
            for (item, _data) in cgu.items() {
                assembly::add_item(&mut asm, *item, tcx).expect("Could not add function");
            }
        }

        if let Some((entrypoint_did, kind)) = tcx.entry_fn(()) {
            let penv = rustc_middle::ty::TypingEnv::fully_monomorphized();
            let entrypoint = rustc_middle::ty::Instance::try_resolve(
                tcx,
                penv,
                entrypoint_did,
                rustc_middle::ty::List::empty(),
            )
            .expect("Could not resolve entrypoint!")
            .expect("Could not resolve entrypoint!");
            let mut ctx = MethodCompileCtx::new(tcx, None, entrypoint, &mut asm);
            let sig = function_sig::sig_from_instance_(entrypoint, &mut ctx)
                .expect("Could not get the signature of the entrypoint.");
            let symbol = tcx.symbol_name(entrypoint);
            let symbol = format!("{symbol:?}");
            // A `fn main() -> T where T: Termination` (`-> Result<_,_>` / `-> ExitCode`) has a
            // non-`Void` return and no args; `entrypoint::wrapper` only handles `() -> ()` and the
            // C-main ABI, so it would `panic!` (ICE). Mirror rustc's `create_entry_fn`: route through
            // `std::rt::lang_start::<T>`, which runs `main`, maps `T` to an exit code via
            // `Termination::report`, and returns it. Everything else keeps the direct wrapper.
            //
            // IMPORTANT: a plain zero-arg `fn main()` (`Void` return) is deliberately EXCLUDED here,
            // even though real rustc's `create_entry_fn` routes it through `lang_start` too (`()`
            // implements `Termination`). A prior change (56890ea) made that "fix" and it regressed
            // ~94% of the `::stable` test suite: `lang_start::<()>` internally calls
            // `lang_start_internal(main: &(dyn Fn() -> i32 + Sync + RefUnwindSafe), ...)` through an
            // indirect call on a trait-object-coerced closure, and that monomorphized instantiation
            // is only reliably discovered when `std` is rebuilt alongside the user crate
            // (`-Z build-std`) — NOT when compiling against the toolchain's precompiled sysroot `std`
            // (the default path, and what `compile_test.rs`/virtually all real users hit), which
            // fails at link/load time with "missing methiod ...lang_start_internal". That
            // cross-crate generic-instantiation gap is real but out of scope here; until it's fixed,
            // void mains instead go through `entrypoint::wrapper_catch_and_exit` in `src/lib.rs`
            // below, which reuses `catch_unwind`'s proven try/catch shape to get a clean panic exit
            // (101) without ever calling `lang_start`. Do NOT revert this exclusion without first
            // fixing `lang_start_internal`'s cross-crate reachability against a prebuilt sysroot.
            let needs_lang_start = sig.inputs().is_empty() && *sig.output() != cilly::Type::Void;
            let is_void_main = sig.inputs().is_empty() && *sig.output() == cilly::Type::Void;
            let cs = MethodRef::new(
                *asm.main_module(),
                asm.alloc_string(symbol),
                asm.alloc_sig(sig),
                MethodKind::Static,
                vec![].into(),
            );

            if needs_lang_start {
                let rustc_session::config::EntryFnType::Main { sigpipe } = kind;
                let main_ret_ty = entrypoint.ty(tcx, penv).fn_sig(tcx).output().skip_binder();
                let start_did = tcx
                    .require_lang_item(rustc_hir::lang_items::LangItem::Start, rustc_span::DUMMY_SP);
                let start_inst = rustc_middle::ty::Instance::expect_resolve(
                    tcx,
                    penv,
                    start_did,
                    tcx.mk_args(&[main_ret_ty.into()]),
                    rustc_span::DUMMY_SP,
                );
                let start_sig = {
                    let mut sctx = MethodCompileCtx::new(tcx, None, start_inst, &mut asm);
                    function_sig::sig_from_instance_(start_inst, &mut sctx)
                        .expect("Could not get the signature of lang_start.")
                };
                let start_symbol = format!("{:?}", tcx.symbol_name(start_inst));
                let lang_start = MethodRef::new(
                    *asm.main_module(),
                    asm.alloc_string(start_symbol),
                    asm.alloc_sig(start_sig),
                    MethodKind::Static,
                    vec![].into(),
                );
                cilly::entrypoint::wrapper_lang_start(cs, lang_start, sigpipe, &mut asm);
            } else if is_void_main {
                // Plain `fn main()`: see the `needs_lang_start` comment above for why this does
                // NOT go through `lang_start`. Reuses `catch_unwind`'s CIL shape for a clean,
                // exit-101 panic path instead of an unhandled-exception crash.
                cilly::entrypoint::wrapper_catch_and_exit(cs, &mut asm);
            } else {
                cilly::entrypoint::wrapper(cs, &mut asm);
            }
        }

        let ffi_compile_timer = tcx
            .prof
            .generic_activity("insert .NET FFI functions/types");
        //builtin::insert_ffi_functions(&mut asm, tcx);
        drop(ffi_compile_timer);
        let name: IString = cgus
            .codegen_units
            .iter()
            .next()
            .unwrap()
            .name()
            .to_string()
            .into();

        // `CrateInfo` is now constructed by the driver (it calls `target_cpu` to build it) and
        // passed back into `join_codegen`/`link`, so the codegen no longer bundles it here.
        Box::new((name, asm))
    }

    fn target_config(&self, sess: &Session) -> rustc_codegen_ssa::TargetConfig {
        use rustc_span::sym;
        // FIXME return the actually used target features. this is necessary for #[cfg(target_feature)]
        use rustc_target::spec::{Arch, Os};
        let target_features = if sess.target.arch == Arch::X86_64 && sess.target.os != Os::None {
            // x86_64 mandates SSE2 support and rustc requires the x87 feature to be enabled
            vec![

                sym::sse,
                //sym::sse2,
                rustc_span::Symbol::intern("x87"),
            ]
        } else if sess.target.arch == Arch::AArch64 {
            match sess.target.os {
                Os::None => vec![],
                // On macOS the aes, sha2 and sha3 features are enabled by default and ring
                // fails to compile on macOS when they are not present.
                Os::MacOs => vec![sym::neon, sym::aes, sym::sha2, sym::sha3],
                // AArch64 mandates Neon support
                _ => vec![sym::neon],
            }
        } else {
            vec![]
        };
        // FIXME do `unstable_target_features` properly
        let unstable_target_features = target_features.clone();

        rustc_codegen_ssa::TargetConfig {
            target_features,
            unstable_target_features,
            // Cranelift does not yet support f16 or f128
            has_reliable_f16: false,
            has_reliable_f16_math: false,
            has_reliable_f128: false,
            has_reliable_f128_math: false,
        }
    }
    /// Saves an in-memory assemably to codegen specific IR in a .bc file.
    fn join_codegen(
        &self,
        ongoing_codegen: Box<dyn Any>,
        _sess: &Session,
        outputs: &OutputFilenames,
        _crate_info: &CrateInfo,
    ) -> (CompiledModules, WorkProductMap) {
        // Debug side-channel: rustc swallows codegen-backend ICE messages (only "the compiler
        // unexpectedly panicked" survives), and `cargo dotnet` buffers stderr. Mirror every panic's
        // location+message to /tmp/rcl_ice.txt so a swallowed codegen panic is recoverable. Chains
        // the previous (rustc ICE) hook so normal diagnostics are unaffected. Gated on RCL_ICE_LOG.
        if std::env::var_os("RCL_ICE_LOG").is_some() {
            use std::sync::Once;
            static ICE_HOOK: Once = Once::new();
            ICE_HOOK.call_once(|| {
                let prev = std::panic::take_hook();
                std::panic::set_hook(Box::new(move |info| {
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/tmp/rcl_ice.txt")
                    {
                        use std::io::Write;
                        let _ = writeln!(f, "=== ICE: {info}");
                    }
                    prev(info);
                }));
            });
        }
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            use std::io::Write;
            let (_asm_name, asm) = *ongoing_codegen
                .downcast::<(IString, Assembly)>()
                .expect("in join_codegen: ongoing_codegen is not an Assembly");
            let asm_name = "";
            let serialized_asm_path =
                outputs.temp_path_for_cgu(OutputType::Bitcode, asm_name);
            //std::fs::create_dir_all(&serialized_asm_path).expect("Could not create the directory temporary files are supposed to be in.");

            let mut asm_out = std::fs::File::create(&serialized_asm_path).expect(
                "Could not create the temporary files necessary for building the assembly!",
            );
            let mut prepared = asm.prepared();
            prepared.opt(&mut prepared.fuel_from_env());
            // Phase P1 type gate: in fatal mode (ALLOW_MISCOMPILATIONS=0) this aborts the build on
            // the first ill-typed method; in the default advisory mode it returns the violation
            // count, which we intentionally drop here (per-method warnings are already emitted).
            let _typecheck_violations = prepared.typecheck();
            asm_out
                .write_all(
                    &postcard::to_stdvec(&prepared)
                        .expect("Could not serialize the tmp assembly file!"),
                )
                .expect("Could not save the tmp assembly file!");
            let modules = vec![CompiledModule {
                name: asm_name.into(),
                kind: ModuleKind::Regular,
                object: Some(serialized_asm_path),
                bytecode: None,
                dwarf_object: None,
                llvm_ir: None,
                assembly: None,
                links_from_incr_cache: Vec::new(),
                global_asm_object: None,
            }];
            // `CodegenResults` was split into `CompiledModules` + `CrateInfo`; the driver now owns
            // the `CrateInfo` and re-supplies it to `link`, so we only build `CompiledModules` here.
            let compiled_modules = CompiledModules {
                modules,
                allocator_module: None,
            };
            (compiled_modules, WorkProductMap::default())
        }))
        .expect("Could not join_codegen")
    }
    /// Collects all the files emmited by the codegen for a specific crate, and turns them into a .rlib file containg the serialized assembly IR and metadata.
    fn link(
        &self,
        sess: &Session,
        compiled_modules: CompiledModules,
        crate_info: CrateInfo,
        metadata: EncodedMetadata,
        outputs: &OutputFilenames,
    ) {
        use rustc_codegen_ssa::back::link::link_binary;
        link_binary(
            sess,
            &RlibArchiveBuilder,
            compiled_modules,
            crate_info,
            metadata,
            outputs,
            self.name(),
        );
    }
}
// Inspired by cranelifts glue code. Is responsible for turing the files produced by teh backend into
struct RlibArchiveBuilder;
impl ArchiveBuilderBuilder for RlibArchiveBuilder {
    fn new_archive_builder<'a>(&self, sess: &'a Session) -> Box<dyn ArchiveBuilder + 'a> {
        Box::new(ArArchiveBuilder::new(
            sess,
            &rustc_codegen_ssa::back::archive::DEFAULT_OBJECT_READER,
        ))
    }
    fn create_dll_import_lib(
        &self,
        _sess: &Session,
        _lib_name: &str,
        _dll_imports: std::vec::Vec<rustc_codegen_ssa::back::archive::ImportLibraryItem>,
        _tmpdir: &Path,
    ) {
        unimplemented!("creating dll imports is not supported");
    }
}
#[no_mangle]
/// Entrypoint of the codegen. This function starts the backend up, and returns a reference to it to rustc.
pub extern "Rust" fn __rustc_codegen_backend() -> Box<dyn CodegenBackend> {
    std::alloc::set_alloc_error_hook(custom_alloc_error_hook);
    Box::new(MyBackend)
}
pub use cilly::{DATA_PTR, ENUM_TAG, METADATA};
use std::alloc::Layout;

pub fn custom_alloc_error_hook(layout: Layout) {
    panic!("memory allocation of {} bytes failed", layout.size());
}

// Retained as a generic MIR→cilly binop mapper. No longer used since `Assert` overflow lowering
// switched from the `assert_<op>` surrogate to the native `panic_*_overflow` lang items.
#[allow(dead_code)]
fn map_binop(op: &rustc_middle::mir::BinOp) -> cilly::BinOp {
    use rustc_middle::mir::BinOp::*;
    match op {
        Add | AddUnchecked | AddWithOverflow => cilly::BinOp::Add,
        Sub | SubUnchecked | SubWithOverflow => cilly::BinOp::Sub,
        Mul | MulUnchecked | MulWithOverflow => cilly::BinOp::Mul,
        Div => cilly::BinOp::Div,
        Rem => cilly::BinOp::Rem,
        BitXor => cilly::BinOp::XOr,
        BitOr => cilly::BinOp::Or,
        BitAnd => cilly::BinOp::And,
        Shl | ShlUnchecked => cilly::BinOp::Shl,
        Shr | ShrUnchecked => cilly::BinOp::Shr,
        _ => todo!(),
    }
}
