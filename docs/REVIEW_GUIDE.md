# Technical review guide

This fork is an independently maintained hardening of FractalFir's original
`rustc_codegen_clr` research backend. It preserves the upstream history and treats the original
compiler as a behavioral reference; it does not claim to represent FractalFir's current roadmap.

The shortest useful review is deliberately bounded. Pick one of these paths rather than reading
the whole repository:

1. **Compiler invariants (about ten minutes).** Start with [`ARCHITECTURE.md`](ARCHITECTURE.md),
   then inspect the fatal verifier and transactional linker boundaries under `cilly/src/ir/`.
2. **One runnable interop example.** Follow [`QUICKSTART_INTEROP.md`](QUICKSTART_INTEROP.md) and
   build the typed C# consumer in `cargo_tests/cd_typed_dto`.
3. **One semantic boundary.** Review managed-reference storage (`src/managed_storage.rs`), ABI
   planning (`src/abi.rs`), place projections (`src/place/projection.rs`), or deterministic
   assembly linking (`cilly/src/ir/asm_link.rs`) in isolation.

## Invariants this fork tries to make explicit

- Unsupported MIR, ABI, target-layout, managed-storage, linker, or PE-emission cases fail before a
  partial artifact is accepted.
- The fatal CIL verifier runs at export boundaries; optimization is not allowed to erase observable
  exceptions, type initialization, argument evaluation, or cleanup effects.
- Class, method, static-field, and runtime-service reconciliation is read-only before commit.
  Expected conflicts return structured errors and preserve both input assemblies.
- Rust ABI decisions come from rustc's `FnAbi`; place and unsizing decisions come from rustc layout,
  not guessed offsets or argument counts.
- Naked CLR references cannot be persisted in Rust byte storage. Long-lived managed values use
  explicit rooted/tokenized representations with audited escape boundaries.
- Public SDK support is intentionally narrow: the pinned nightly, .NET 10, Linux x64, macOS Apple
  Silicon, and Windows x64. Other targets are rejected instead of silently inheriting host layout.
- Release inputs, installed homes, private sysroots, NuGet closures, and generated helpers are
  content-bound and transactionally published; acceptance evidence names and hashes its artifacts.

## Reproduce the main evidence

Use the pinned toolchain from the repository root:

```bash
cargo check --workspace --locked
cargo test -p cilly --locked
cargo check -p rustc_codegen_clr --all-targets --locked
cargo test -p rustc_codegen_clr --lib managed_references_are_rejected_from_rust_byte_storage -- --nocapture
cargo test --manifest-path tools/cargo-dotnet/Cargo.toml --locked -- --test-threads=1
cargo fmt --all -- --check
git diff --check
```

Product-shaped tests build the standard library through this backend and then execute a managed or
C# oracle. The closest focused entry points are documented in `src/compile_test.rs`,
`feasibility/e2e_matrix.sh`, and the acceptance scripts under `feasibility/`.

The package-wide `cargo test -p rustc_codegen_clr` command also includes the historical
direct-rustc/host-sysroot fixture corpus. That corpus does not supply the product build-std closure;
with retained missing-method verification now fatal, many of those fixtures stop on absent `core`
bodies before reaching their old oracle, and some legacy fuzz/run cases still expose behavioral
mismatches. It is not represented as a green release gate. Do not ignore those failures or weaken
the verifier: use the exact hardened compiler matrix above plus product-shaped build-std acceptance,
and treat rehabilitating the historical corpus as separate compatibility work.

## What this does not claim

This remains compiler research. It does not promise complete Rust semantics, a stable compiler ABI,
production readiness, or support outside the documented profile. See
[`TRANSLATION_STATUS.md`](TRANSLATION_STATUS.md), [`STATE_OF_THE_PROJECT.md`](STATE_OF_THE_PROJECT.md),
and [`RELEASE_NOTES_0.0.2.md`](RELEASE_NOTES_0.0.2.md) for the current boundaries.
