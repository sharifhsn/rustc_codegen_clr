//! The throw side of the panic ↔ managed-exception bridge.
//!
//! On Unix (this project's target) native↔managed exception crossing is unsupported by design, so a
//! Rust panic is mapped to a **managed** exception (`RustException`) caught entirely within managed
//! frames — never across a P/Invoke boundary. The *catch* side lives in
//! `super::insert_catch_unwind` / [`super::insert_exception`]: it wraps the protected call in a CIL
//! `try`/`catch`, filters on `IsInst RustException`, and reads the exception's `usize data_pointer`
//! field back out to hand to the catch closure.
//!
//! The *throw* side is here. The Rust `panic_unwind` runtime (gcc flavour) ultimately calls
//! `_Unwind_RaiseException(exception: *mut _Unwind_Exception)` to start unwinding. We override that
//! libgcc symbol so that, instead of running the DWARF unwinder, it constructs a `RustException`
//! carrying the `*mut _Unwind_Exception` pointer (as a `usize`) and `throw`s it. The catch side reads
//! that exact pointer back out and the Rust side decodes it via `__rust_panic_cleanup`, so the panic
//! payload round-trips unchanged.

use crate::{
    BasicBlock, CILNode, CILRoot, ClassRef, Const, Int, MethodImpl, MethodRef, Type,
    asm::{MissingMethodPatcher, RuntimeService},
    cilnode::{IsPure, MethodKind, PtrCastRes},
};

use super::super::Assembly;

/// Registers the managed fallback for libunwind's symbol-address helper.
///
/// `std::backtrace` uses `_Unwind_FindEnclosingFunction` only to turn a frame program counter into
/// a symbol address. Managed CIL has no native DWARF unwind table to query. Returning the original
/// pointer matches Rust's own fallback on targets where the native helper is unavailable or
/// unreliable, while preserving backtrace capture instead of returning null or terminating.
pub fn find_enclosing_function(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    patcher.insert_runtime_service(
        asm,
        RuntimeService::UnwindFindEnclosingFunction,
        Box::new(|_, asm| {
            let pc = asm.alloc_node(CILNode::LdArg(0));
            let ret = asm.alloc_root(CILRoot::Ret(pc));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        }),
    );
}

/// Registers the managed fallback for libunwind's canonical-frame-address accessor.
///
/// The only pinned-`std` caller uses this to copy the stack pointer out of a native
/// `_Unwind_Context`. The managed `_Unwind_Backtrace` capability never constructs or exposes such
/// a context, so zero is the only truthful value: no native stack pointer is available.
pub fn get_cfa(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    patcher.insert_runtime_service(
        asm,
        RuntimeService::UnwindGetCfa,
        Box::new(|_, asm| {
            let unavailable = asm.alloc_node(Const::USize(0));
            let ret = asm.alloc_root(CILRoot::Ret(unavailable));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        }),
    );
}

/// Registers the managed fallback for libunwind's native frame walker.
///
/// Managed assemblies have no DWARF unwind table and therefore expose no native frames to the
/// callback. Returning `_URC_END_OF_STACK` (the pinned ABI value `5`) reports that empty walk
/// deterministically instead of returning an uninitialized local.
pub fn backtrace_end_of_stack(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    patcher.insert_runtime_service(
        asm,
        RuntimeService::UnwindBacktrace,
        Box::new(|mref, asm| {
            let output = *asm[asm[mref].sig()].output();
            let output = asm.alloc_type(output);
            let i32_type = asm.alloc_type(Type::Int(Int::I32));
            let output_address = asm.alloc_node(CILNode::LdLocA(0));
            let output_address = asm.alloc_node(CILNode::PtrCast(
                output_address,
                Box::new(PtrCastRes::Ptr(i32_type)),
            ));
            let end_of_stack = asm.alloc_node(Const::I32(5));
            let initialize = asm.alloc_root(CILRoot::StInd(Box::new((
                output_address,
                end_of_stack,
                Type::Int(Int::I32),
                false,
            ))));
            let result = asm.alloc_node(CILNode::LdLoc(0));
            let ret = asm.alloc_root(CILRoot::Ret(result));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![initialize, ret], 0, None)],
                locals: vec![(None, output)],
            }
        }),
    );
}

/// Registers the .NET throw-bridge: overrides `_Unwind_RaiseException` to throw a `RustException`
/// wrapping its `*mut _Unwind_Exception` argument. Requires [`super::insert_exception`] (which defines
/// the `RustException` class + its `.ctor(usize)`) to have run first.
pub fn raise_exception(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("_Unwind_RaiseException");
    let generator = move |mref: crate::Interned<MethodRef>, asm: &mut Assembly| {
        // Reference the linker-defined `RustException` class (same-assembly, so `None` asm name —
        // matching how the catch side in `insert_catch_unwind` refers to it).
        let rust_exception_name = asm.alloc_string("RustException");
        let rust_exception =
            asm.alloc_class_ref(ClassRef::new(rust_exception_name, None, false, [].into()));
        // `RustException::.ctor(this, usize)` — must match the def in `insert_exception`.
        let ctor_name = asm.alloc_string(".ctor");
        let sig = asm.sig(
            [Type::ClassRef(rust_exception), Type::Int(Int::USize)],
            Type::Void,
        );
        let ctor = asm.alloc_methodref(MethodRef::new(
            rust_exception,
            ctor_name,
            sig,
            MethodKind::Constructor,
            [].into(),
        ));
        // arg0 is the `*mut _Unwind_Exception`; it flows straight into the `usize data_pointer` field
        // (native pointer ≡ native int in CIL). The catch side passes it back to the catch closure,
        // which decodes it with `__rust_panic_cleanup`.
        let exception_ptr = asm.alloc_node(CILNode::LdArg(0));
        let input_type = asm[asm[mref].sig()].inputs()[0];
        let exception_ptr = asm.adapt_call_value(exception_ptr, input_type, Type::Int(Int::USize));
        let exception = asm.call(ctor, &[exception_ptr], IsPure::NOT);
        let throw = asm.alloc_root(CILRoot::Throw(exception));
        // `throw` ends the path; `_Unwind_RaiseException` only "returns" on failure, which never
        // happens here, so no `ret` is needed.
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![throw], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
