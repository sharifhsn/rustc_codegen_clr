//! Documentation marker for the `panic_unwind` flavour used by `os = "dotnet"`.
//!
//! .NET unwinds via its own managed-exception model, not DWARF/SEH. For
//! `panic=unwind` we reuse the GCC flavour *only for its entry point*:
//! `imp::panic` (in `gcc.rs`) calls `_Unwind_RaiseException`, which the cilly
//! linker overrides to construct a `RustException` and `throw` it (see
//! `cilly/src/ir/builtins/unwind.rs`). The managed try/catch installed by
//! `catch_unwind` (linker's `insert_catch_unwind`) then catches it. The DWARF
//! personality that the GCC flavour would normally rely on is never invoked —
//! .NET's EH runs the handlers — so the matching `eh_personality` lang item is
//! supplied as a trivial aborting stub from `std::sys::personality` (the
//! no-DWARF target pattern, like wasm/msvc).
//!
//! `feasibility/dev.sh pal-build` injects the `target_os = "dotnet"` arm into
//! rust-src's `library/panic_unwind/src/lib.rs` as `#[path = "gcc.rs"] mod imp;`
//! — identical to the upstream gcc arm. gcc.rs is included directly as `imp`
//! (rather than nested under this file) so its `super::__rust_drop_panic` /
//! `super::__rust_foreign_exception` references resolve to the crate root.
//! This file itself is copied into rust-src purely as an in-tree breadcrumb;
//! nothing imports it.
