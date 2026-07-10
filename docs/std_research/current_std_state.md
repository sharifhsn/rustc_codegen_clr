# cg_clr's current `std` state

## The architecture today: "surrogate std"

cg_clr has **no .NET-native std and no functioning .NET target triple**. It compiles std
*as if for `x86_64-unknown-linux-gnu`* and **P/Invokes libc at runtime** (confirmed in
`cargo_tests/build_std/.cargo/config.toml`; stated in `docs/fractalfir_articles/v0_1_3.md`).
A stub target spec exists (`clr64-unknown-mono.json`) but isn't the std path. The known
pain points of mimicking Linux-on-.NET (`target.md`, articles):

- **errno** — any extern call can clobber it even on success → every P/Invoke needs errno metadata.
- **environ / `set_env`** — emulated; copies desync, so `set_env` "doesn't always work".
- **argv** — read from statics filled by a GNU `.init_array` initializer pre-`main`; emulated by
  calling `really_init` from a .NET static constructor (`.cctor`) on `RustModule` (`v0_1_2.md`).
- **TLS destructors, thread names** — more POSIX APIs to emulate.
- **threads** — emulated pthreads inside the CLR, not native .NET threads (`v0_1_2.md`).
- **8/16-bit atomics** — unsupported pre-.NET 9; lock-emulated non-compliantly (`v0_2_0.md`).
- **fork** — libc fork / malloc-after-fork is UB on .NET.

This is **Linux-x86_64-only and brittle by construction** — the surrogate only works on the one
OS it mimics, and each extern fn needs per-OS signature + errno data.

## Works vs broken **now**

- **core works** (~96% per the README table; the `#![no_std]` test corpus passes). But note the
  ~207 passing tests **hand-reimplement std** (`test/alloc/abox.rs` rolls its own
  `Box`/`Layout`/`Alloc`; `test/std/*` call raw `pthread_*`/atomics) and print via `.NET`
  interop stubs — **they are not evidence that real `alloc`/`std` compiles.**
- **real `alloc`/`std` build-std is broken** (`cargo build -Zbuild-std`):
  - `FieldOwnerMismatch` compiling `Rc`/`Arc`/`Cell` (`RcInner`, `ArcInner`, `Atomic<usize>`).
  - `CallArgTypeWrong { got: FatPtru8, expected: FatPtr<FatPtru8>, …String…push_str_slice }`.

### Root-cause hypothesis: nightly-port regressions, not fundamental limits

std was ~95% functional before the 8-month nightly jump we just ported across, so these are
almost certainly **bit-rot from the port**, in two known-fragile areas:

- **`FieldOwnerMismatch` = type-identity drift.** It fires in `cilly/src/v2/typecheck.rs`
  only when the pointee's class at a field-access site ≠ the field's declared owner class.
  Class *identity* is the demangled symbol name (incl. generic hash) from
  `rustc_symbol_mangling::symbol_name_for_instance_in_crate` via `adt_name`
  (`src/type/utilis.rs`), feeding the owner in `src/type/adt.rs`.
  If the nightly renamed std internals (historically `RcBox`→`RcInner`; `ArcInner`; the
  `Atomic<T>` wrapper) or changed mangling/generic-hash inputs, the access-site class and the
  field-owner class resolve to *different* `Interned<ClassRef>` for the same logical type — a
  one-digit hash difference is enough to reject the access. **Identity mismatch, not a modelling gap.**
- **`CallArgTypeWrong` = fat-pointer-nesting drift.** Fat pointers are built in
  `src/type/mod.rs` (`fat_ptr_to`, a 16-byte `{DATA_PTR, METADATA}`) and
  consumed in `src/unsize.rs`/`src/aggregate.rs`. A by-ref/DST-metadata decision is adding or
  dropping one indirection level versus the nightly's new monomorphized `String::push_str`
  signature. This is the **same bug family** as the v0_1_3 `FatPtrg` foreign-type fix (a wrong
  `is_sized` decision producing an invalid extra fat pointer), which was a few-line fix.

## std-subsystem status map

| Component | Status | Evidence |
|---|---|---|
| core | ✅ works (~96%) | README; no_std tests |
| alloc: Box/Vec | ⚠️ worked historically; **build-std broken now** | README (616 alloc pass); build-std errors |
| alloc: String | 🔴 broken now | `CallArgTypeWrong …push_str_slice` |
| alloc: Rc/Arc | 🔴 broken now | `FieldOwnerMismatch`; `BROKEN_TESTS.md` Arc/Rc entries |
| alloc: Cell/Atomic | 🔴 broken now | `FieldOwnerMismatch` on `Atomic<usize>` |
| collections (BTree/Hash/VecDeque) | ⚠️ untested directly | `BROKEN_TESTS.md` lists them "Did Not Complete" |
| fmt | ⚠️ fixed in v0_1_3, now suspect | same bug family resurfacing |
| panic/unwind | ✅ libunwind path → .NET exceptions | `v0_2_2.md`; `catch` tests |
| thread/sync | ⚠️ emulated pthreads/futex | `test/std/*` hand-roll; `std::thread` unused |
| io/fs/net/process/env | ⚠️ libc P/Invoke, largely untested | no real tests; `target.md`; `BROKEN_TESTS.md` ipv6/socket |
| os / `sys` PAL | 🟥 surrogate libc only, no .NET-native layer | `target.md` |

## The architectural fork

- **Continue surrogate-libc:** works *today* for one OS, no upstream needed — but inherits every
  pain point above (errno, set_env desync, fork UB, lock-emulated sub-word atomics, per-OS libc
  metadata), is Linux-only, and stays brittle to nightly drift.
- **Proper `.NET` target + native `std::sys` PAL** (`target.md` draft): a real target triple
  (`dotnet-*`), a `std::sys` layer on .NET APIs (managed threads, `Interlocked`, .NET env/args/TLS),
  upstreaming (designated maintainer, license audit, std `sys` patches). Upside: genuinely
  cross-platform (one assembly, any CLR OS), correct semantics, native threads/TLS/atomics, no
  argv/fork hacks. Cost: upstream cooperation + a sustained platform-layer effort.

## Concrete near-term blockers (Horizon 1)

1. Reconcile **class identity** so a pointee's class matches its field's owner — handle the
   renamed std internals in `adt_name`/symbol-mangling (`utilis.rs:242`). Clears `FieldOwnerMismatch`.
2. Fix **fat-pointer nesting** in the call ABI for `String::push_str` & friends
   (`type.rs:172-184` / `unsize.rs` / `aggregate.rs`) — same family as the v0_1_3 fix.
3. Finish porting **new nightly type kinds** (`pattern_type` already landed; verify `Variants::Multiple`).
4. Re-validate with `cargo_tests/build_alloc`, `build_std`, and the differential validator on a
   real-std program — confirm the regression is *cleared*, not masked.
