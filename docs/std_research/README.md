# Getting `std` working on rustc_codegen_clr — research & roadmap

A birds-eye study of what it takes to run the **real** Rust standard library on this
backend (the gateway blocker found by the [differential validator](../../feasibility/validate/)):
real-std projects can't compile today, and the ~207 passing unit tests are mostly
`#![no_std]` programs that hand-reimplement std, so they don't prove std works.

Three documents:

1. **[lessons_from_other_backends.md](lessons_from_other_backends.md)** — how
   `rustc_codegen_cranelift` and `rustc_codegen_gcc` got std working, the patterns to
   copy and the footguns. (Both cloned locally at `~/Code/rustc_codegen_{cranelift,gcc}`.)
2. **[current_std_state.md](current_std_state.md)** — where cg_clr actually stands: the
   "surrogate std" architecture, what works vs what regressed in the nightly jump, and a
   std-subsystem status map.
3. **[std_roadmap.md](std_roadmap.md)** — the complexity/LOC assessment **framework**, the
   architecture decision (surrogate-libc vs a proper .NET target + `std::sys` PAL), the
   structure to build, and a phased plan with milestones.
4. **[h2_design.md](h2_design.md)** — the **full design for H2**: the real, shippable `dotnet`
   target + managed `std::sys` PAL that *replaces* the surrogate. Layer cake (target spec /
   PAL / runtime lib / interop ABI / codegen), the PAL module→.NET-API map, the interop-ABI and
   GC-boundary design, build/ship mechanism, phased plan, and risk register. **This is the
   build target** — the surrogate is the thing we delete, not extend.

## The one-paragraph version

Cranelift and GCC get *full* std with ~5 tiny patches because they target **real OS triples
and reuse the existing `std::sys` + libc**. cg_clr **cannot** — it runs on the managed CLR
(no libc, no native unwinding/TLS/linker), so its true analogue is `wasm32`/SGX/UEFI:
targets that ship a **bespoke `std::sys` platform layer**. *But* the immediate breakage is
not architectural — std was ~95% working before the 8-month nightly jump, and the current
failures (`FieldOwnerMismatch`, `CallArgTypeWrong`) are **port regressions** (type-identity
and fat-pointer drift). So the work splits cleanly into two horizons: **(1) un-rot the
surrogate std** (bounded, weeks) and **(2) build a proper .NET std PAL** (the real
architecture, months). See the roadmap for the framework that quantifies both.
