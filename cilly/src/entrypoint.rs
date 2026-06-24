use std::num::NonZeroU8;

use crate::{
    cilnode::ExtendKind, Access, BasicBlock, CILNode, CILRoot, ClassRef, Const, MethodDef,
    MethodDefIdx, MethodImpl, Type, {cilnode::MethodKind, Assembly, Int, MethodRef},
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
        Type::Ptr(pointee) => {
            asm.alloc_node(CILNode::PtrCast(argv, Box::new(crate::cilnode::PtrCastRes::Ptr(pointee))))
        }
        _ => argv,
    };
    let sigpipe = asm.alloc_node(Const::U8(sigpipe));
    let code = CILNode::call(lang_start, [main_ptr, argc, argv, sigpipe]);
    let code = asm.alloc_node(code);
    // System.Environment.Exit((int)code) — propagate main's exit code to the OS.
    let env = ClassRef::enviroment(asm);
    let exit_name = asm.alloc_string("Exit");
    let exit = asm
        .class_ref(env)
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
