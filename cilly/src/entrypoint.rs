use std::num::NonZeroU8;

use crate::{
    Access, BasicBlock, CILNode, CILRoot, ClassRef, Const, MethodDef, MethodDefIdx, MethodImpl,
    Type,
    cilnode::ExtendKind,
    cilroot::BranchCond,
    {Assembly, Int, MethodRef, cilnode::MethodKind},
};

/// Entry wrapper for a `fn main() -> T where T: Termination` (e.g. `-> Result<_, _>` / `-> ExitCode`).
///
/// rustc never runs such a `main` as the process entry directly; its platform `main` shim calls
/// `std::rt::lang_start::<T>(user_main, argc, argv, sigpipe)`, which runs `user_main`, converts the
/// returned `T` to an exit code via `Termination::report` (printing `Error: <e>` to stderr on `Err`),
/// and returns that code. We mirror that exactly (see rustc_codegen_ssa `create_entry_fn`): load a
/// function pointer to the user `main`, call the already-monomorphized `lang_start`, and propagate
/// its returned code via `System.Environment.Exit` — matching the C `main` returning the code to the
/// OS (and side-stepping the single-file apphost dropping a plain managed exit code).
///
/// Previously this whole signature class fell through to `wrapper`'s `panic!`, ICE-ing the backend
/// on the very common `fn main() -> Result<_, _>` / `-> ExitCode` idiom.
pub fn wrapper_lang_start(
    user_main: MethodRef,
    lang_start: MethodRef,
    sigpipe: u8,
    asm: &mut Assembly,
) -> MethodDefIdx {
    let main_module = asm.main_module();
    let entrypoint_name = asm.alloc_string("entrypoint");
    // Force `user_init`/`static_init` to exist (see `wrapper`).
    asm.user_init();

    // lang_start's CIL params are `[fn() -> T, isize, *const *const u8, u8]`; we need the exact argv
    // pointer type to materialize the null `argv`.
    let ls_inputs: Vec<Type> = asm[lang_start.sig()].inputs().to_vec();
    let argv_ty = ls_inputs[2];

    let user_main = asm.alloc_methodref(user_main);
    let lang_start = asm.alloc_methodref(lang_start);

    let tcctor = MethodRef::new(
        *asm.main_module(),
        asm.alloc_string(".tcctor"),
        asm.sig([], Type::Void),
        MethodKind::Static,
        vec![].into(),
    );
    let tcctor = asm.alloc_methodref(tcctor);
    let static_init = MethodRef::new(
        *asm.main_module(),
        asm.alloc_string("static_init"),
        asm.sig([], Type::Void),
        MethodKind::Static,
        vec![].into(),
    );
    let static_init = asm.alloc_methodref(static_init);

    // lang_start(ldftn user_main, 0isize, (argv*)null, sigpipe) -> isize
    let main_ptr = asm.ld_ftn(user_main);
    let argc = asm.alloc_node(Const::ISize(0));
    // Null `argv` of the exact `*const *const u8` parameter type. `argv_ty` is `Type::Ptr(pointee)`;
    // `PtrCast(_, Ptr(pointee))` yields `*pointee == argv_ty` (mirrors `wrapper`'s C-main argv).
    let argv = asm.alloc_node(Const::ISize(0));
    let argv = match argv_ty {
        Type::Ptr(pointee) => asm.alloc_node(CILNode::PtrCast(
            argv,
            Box::new(crate::cilnode::PtrCastRes::Ptr(pointee)),
        )),
        _ => argv,
    };
    let sigpipe = asm.alloc_node(Const::U8(sigpipe));
    let code = CILNode::call(lang_start, [main_ptr, argc, argv, sigpipe]);
    let code = asm.alloc_node(code);
    // System.Environment.Exit((int)code) — propagate main's exit code to the OS.
    let env = ClassRef::enviroment(asm);
    let exit_name = asm.alloc_string("Exit");
    let exit =
        asm.class_ref(env)
            .clone()
            .static_mref(&[Type::Int(Int::I32)], Type::Void, exit_name, asm);
    let code = asm.int_cast(code, Int::I32, ExtendKind::SignExtend);
    let exit_call = asm.alloc_root(CILRoot::call(exit, [code]));

    let sig = asm.sig([], Type::Void);
    let blocks = vec![BasicBlock::new(
        vec![
            asm.alloc_root(CILRoot::call(tcctor, [])),
            asm.alloc_root(CILRoot::call(static_init, [])),
            exit_call,
            // Unreachable: Environment.Exit never returns, but keeps the block well-formed.
            asm.alloc_root(CILRoot::VoidRet),
        ],
        0,
        None,
    )];
    let method = MethodDef::new(
        Access::Extern,
        main_module,
        entrypoint_name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks,
            locals: vec![],
        },
        vec![],
    );
    asm.new_method(method)
}

/// Entry wrapper for a plain `fn main()` (zero args, `Void` return).
///
/// Real rustc routes even a plain `fn main()` through `std::rt::lang_start::<()>`
/// (`()` implements `Termination`), which internally calls
/// `lang_start_internal(main: &(dyn Fn() -> i32 + Sync + RefUnwindSafe), ...)` — an indirect call
/// through a trait-object-coerced closure. That monomorphized instantiation of
/// `lang_start_internal` is reliably discovered when `std` is rebuilt alongside the user crate
/// (`-Z build-std`), but is NOT reliably emitted when compiling against the toolchain's
/// precompiled sysroot `std` (the default/common path) — a real, latent cross-crate
/// generic-instantiation gap, tracked separately, that is NOT fixed here. Routing plain `()`
/// mains through `lang_start` (as a prior change briefly did) therefore ICEs with a "missing
/// method ... lang_start_internal" against a normal sysroot build.
///
/// So for the `Void`-returning case we deliberately do NOT call `lang_start` at all. Instead we
/// reuse the exact proven CIL `try`/`catch` shape that `std::panic::catch_unwind`'s own
/// `catch_unwind` builtin uses (see `insert_catch_unwind` in `cilly::ir::builtins`): call `main`
/// directly in a protected region; on a caught exception, check it `IsInst RustException` —
/// rethrow anything else unchanged (so a genuine backend miscompile/AV still surfaces visibly,
/// exactly like `catch_unwind` does) — and on a real Rust panic, simply exit with code `101`
/// (matching native's panic exit code) via `System.Environment.Exit`, without printing anything
/// else: the "thread '<name>' panicked at ..." message was already printed by std's own
/// `panic_fmt`/default hook before the exception was thrown, so nothing further is needed here.
///
/// One documented trade-off versus the (currently broken) `lang_start` path: because we never
/// call `lang_start`/`lang_start_internal`, the main OS thread is never renamed to `"main"` by
/// std's runtime init, so panic messages may show the default `<unnamed>` thread name instead.
/// This is intentional and strictly better than the alternative (an unhandled-exception crash).
pub fn wrapper_catch_and_exit(entrypoint: MethodRef, asm: &mut Assembly) -> MethodDefIdx {
    let main_module = asm.main_module();
    let entrypoint_name = asm.alloc_string("entrypoint");
    // Force `user_init`/`static_init` to exist (see `wrapper`).
    asm.user_init();

    let entrypoint = asm.alloc_methodref(entrypoint);

    let tcctor = MethodRef::new(
        *asm.main_module(),
        asm.alloc_string(".tcctor"),
        asm.sig([], Type::Void),
        MethodKind::Static,
        vec![].into(),
    );
    let tcctor = asm.alloc_methodref(tcctor);
    let static_init = MethodRef::new(
        *asm.main_module(),
        asm.alloc_string("static_init"),
        asm.sig([], Type::Void),
        MethodKind::Static,
        vec![].into(),
    );
    let static_init = asm.alloc_methodref(static_init);

    let tcctor_call = asm.alloc_root(CILRoot::call(tcctor, []));
    let static_init_call = asm.alloc_root(CILRoot::call(static_init, []));
    let call_main = asm.alloc_root(CILRoot::call(entrypoint, []));
    let exit_try_success = asm.alloc_root(CILRoot::ExitSpecialRegion {
        target: 2,
        source: 0,
    });

    // Handler: check the caught exception is a `RustException`; if not, rethrow unchanged.
    let ldloc_0 = asm.alloc_node(CILNode::LdLoc(0));
    let get_exception = asm.alloc_node(CILNode::GetException);
    let set_exception = asm.alloc_root(CILRoot::StLoc(0, get_exception));
    let exception = Type::ClassRef(ClassRef::exception(asm));
    let exception = asm.alloc_type(exception);
    let rust_exception = asm.alloc_string("RustException");
    let rust_exception = asm.alloc_class_ref(ClassRef::new(rust_exception, None, false, [].into()));
    let rust_exception_tpe = Type::ClassRef(rust_exception);
    let rust_exception_tpe = asm.alloc_type(rust_exception_tpe);
    let check_exception_tpe = asm.alloc_node(CILNode::IsInst(ldloc_0, rust_exception_tpe));
    let rethrow_if_wrong_exception = asm.alloc_root(CILRoot::Branch(Box::new((
        0,
        4,
        Some(BranchCond::False(check_exception_tpe)),
    ))));
    // It IS a genuine Rust panic: the clean "thread panicked at ..." message was already printed
    // by std before the throw, so just exit(101), matching native's panic exit code.
    let env = ClassRef::enviroment(asm);
    let exit_name = asm.alloc_string("Exit");
    let exit =
        asm.class_ref(env)
            .clone()
            .static_mref(&[Type::Int(Int::I32)], Type::Void, exit_name, asm);
    let exit_code = asm.alloc_node(Const::I32(101));
    let exit_call = asm.alloc_root(CILRoot::call(exit, [exit_code]));
    let exit_try_failure = asm.alloc_root(CILRoot::ExitSpecialRegion {
        target: 3,
        source: 0,
    });
    let rethrow = asm.alloc_root(CILRoot::ReThrow);

    let sig = asm.sig([], Type::Void);
    let ret_ok = asm.alloc_root(CILRoot::VoidRet);
    let ret_caught = asm.alloc_root(CILRoot::VoidRet);
    let blocks = vec![
        BasicBlock::new(
            vec![tcctor_call, static_init_call, call_main, exit_try_success],
            0,
            Some(vec![
                BasicBlock::new(
                    vec![
                        set_exception,
                        rethrow_if_wrong_exception,
                        exit_call,
                        exit_try_failure,
                    ],
                    1,
                    None,
                ),
                BasicBlock::new(vec![rethrow], 4, None),
            ]),
        ),
        BasicBlock::new(vec![ret_ok], 2, None),
        BasicBlock::new(vec![ret_caught], 3, None),
    ];
    let method = MethodDef::new(
        Access::Extern,
        main_module,
        entrypoint_name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks,
            locals: vec![(Some(asm.alloc_string("exception")), exception)],
        },
        vec![],
    );

    asm.new_method(method)
}

/// Creates a wrapper method around entypoint represented by `Interned<MethodRef>`
pub fn wrapper(entrypoint: MethodRef, asm: &mut Assembly) -> MethodDefIdx {
    let uint8_ptr = asm.nptr(Type::Int(Int::U8));
    let uint8_ptr_idx = asm.alloc_type(uint8_ptr);
    let uint8_ptr_ptr = asm.nptr(uint8_ptr);

    let entry_sig = asm[entrypoint.sig()].clone();
    let main_module = asm.main_module();
    let entrypoint_name = asm.alloc_string("entrypoint");
    let entrypoint = asm.alloc_methodref(entrypoint);
    // TODO: check if user_init is used, and only call that method in wrapper if so.
    // This is just a hack that forces user_init to be always present, even when unneded.
    asm.user_init();
    if entry_sig.inputs() == [Type::Int(Int::ISize), uint8_ptr_ptr]
        && entry_sig.output() == &Type::Int(Int::ISize)
    {
        let string = asm.alloc_type(Type::PlatformString);
        let sig = asm.sig(
            [Type::PlatformArray {
                elem: string,
                dims: NonZeroU8::new(1).unwrap(),
            }],
            Type::Void,
        );
        let tcctor = MethodRef::new(
            *asm.main_module(),
            asm.alloc_string(".tcctor"),
            asm.sig([], Type::Void),
            MethodKind::Static,
            vec![].into(),
        );
        let tcctor = asm.alloc_methodref(tcctor);
        let static_init = MethodRef::new(
            *asm.main_module(),
            asm.alloc_string("static_init"),
            asm.sig([], Type::Void),
            MethodKind::Static,
            vec![].into(),
        );

        let static_init = asm.alloc_methodref(static_init);
        let argv = asm.alloc_node(Const::ISize(0_i64));
        let argv = asm.alloc_node(CILNode::PtrCast(
            argv,
            Box::new(crate::cilnode::PtrCastRes::Ptr(uint8_ptr_idx)),
        ));
        let args = [asm.alloc_node(Const::ISize(0_i64)), argv];

        let call_main = CILNode::call(entrypoint, args);
        let call_main = asm.alloc_node(call_main);
        let blocks = vec![BasicBlock::new(
            vec![
                asm.alloc_root(CILRoot::call(tcctor, [])),
                asm.alloc_root(CILRoot::call(static_init, [])),
                asm.alloc_root(CILRoot::Pop(call_main)),
                asm.alloc_root(CILRoot::VoidRet),
            ],
            2,
            None,
        )];
        let mimpl = MethodImpl::MethodBody {
            blocks,
            locals: vec![],
        };
        let method = MethodDef::new(
            Access::Extern,
            main_module,
            entrypoint_name,
            sig,
            MethodKind::Static,
            mimpl,
            vec![Some(asm.alloc_string("args"))],
        );

        asm.new_method(method)
    } else if entry_sig.inputs().is_empty() && entry_sig.output() == &Type::Void {
        let sig = asm.sig([], Type::Void);
        let tcctor = MethodRef::new(
            *asm.main_module(),
            asm.alloc_string(".tcctor"),
            asm.sig([], Type::Void),
            MethodKind::Static,
            vec![].into(),
        );
        let tcctor = asm.alloc_methodref(tcctor);
        let static_init = MethodRef::new(
            *asm.main_module(),
            asm.alloc_string("static_init"),
            asm.sig([], Type::Void),
            MethodKind::Static,
            vec![].into(),
        );
        let static_init = asm.alloc_methodref(static_init);
        let blocks = vec![BasicBlock::new(
            vec![
                asm.alloc_root(CILRoot::call(tcctor, [])),
                asm.alloc_root(CILRoot::call(static_init, [])),
                asm.alloc_root(CILRoot::call(entrypoint, [])),
                //CILRoot::debug(&format!("Preparing to execute the main program.")).into(),
                asm.alloc_root(CILRoot::VoidRet),
            ],
            0,
            None,
        )];
        let method = MethodDef::new(
            Access::Extern,
            main_module,
            entrypoint_name,
            sig,
            MethodKind::Static,
            crate::MethodImpl::MethodBody {
                blocks,
                locals: vec![],
            },
            vec![],
        );

        asm.new_method(method)
    } else {
        panic!("Unsuported entrypoint wrapper signature! entrypoint:{entrypoint:?}");
    }
}
