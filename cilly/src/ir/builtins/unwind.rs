//! The throw side of the panic ↔ managed-exception bridge.
//!
//! On Unix (this project's target) native↔managed exception crossing is unsupported by design, so a
//! Rust panic is mapped to a **managed** exception (`RustException`) caught entirely within managed
//! frames — never across a P/Invoke boundary. The *catch* side lives in
//! [`super::insert_catch_unwind`] / [`super::insert_exception`]: it wraps the protected call in a CIL
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
    BasicBlock, CILNode, CILRoot, ClassRef, Int, MethodImpl, MethodRef, Type,
    asm::MissingMethodPatcher,
    cilnode::{IsPure, MethodKind},
};

use super::super::Assembly;

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
