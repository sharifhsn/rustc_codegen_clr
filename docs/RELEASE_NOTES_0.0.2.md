# rust-dotnet 0.0.2

This prerelease keeps the .NET 10 and host support boundary from 0.0.1: Linux x64, macOS Apple
Silicon, and Windows x64 on the pinned Rust nightly.

## Highlights

- Safe Rust-to-Rust native contracts now generate narrow C ABI shims for scalars, strings, slices,
  and owned scalar buffers. Contract signatures are embedded in exported symbol fingerprints so a
  mismatched importer fails to resolve instead of silently calling an incompatible ABI.
- Release and CI dependency resolution is locked, release builds are frozen after an explicit
  locked fetch, and third-party workflow actions are pinned to immutable commits.
- Installed SDK integrity rejects injected files, directories, symlinks, content changes, and
  executable-mode drift. Mutable Cargo, NuGet, lock, and private-sysroot state lives outside the
  immutable SDK home.
- SDK activation is a durable two-object transaction across the immutable home and `cargo-dotnet`
  executable. Crash recovery, shared invocation leases, no-follow publication, and exact inventory
  validation prevent mixed-version or path-rebinding installs.
- Private sysroots, generated interop helpers, NuGet asset closures, and bindgen output use
  content-addressed caches with typed receipts, per-key build locks, shared consumption leases,
  integrity invalidation, and bounded root-confined garbage collection.
- NuGet IDs, versions, RIDs, local asset paths, and forced cache deletion are checked against their
  filesystem boundaries. Package filenames are derived only from validated components, and packages
  are structurally validated before transactional publication.
- Capability evidence now binds every required artifact name to a verified path and SHA-256 receipt.
- Backend rustc flags use Cargo's encoded argument channel, including checkouts and SDK paths that
  contain spaces, and host detection rejects architectures outside the published support matrix.
- Compiler lowering is fail closed: unsupported MIR, global assembly, invalid managed-reference
  storage, unsupported target layouts, and unsupported indirect C-variadic calls stop compilation
  with structured diagnostics instead of emitting stubs or verifier-invalid CIL.
- One `AbiPlan` now drives definitions, direct and indirect calls, closures, RustCall tuple slots,
  ZST arguments, and `#[track_caller]`; one projection plan drives place address/read/write layout;
  layout-derived unsizing supports non-first custom smart-pointer fields without byte overlays.
- The CIL optimizer no longer performs untyped call inlining, deletes only total pure expressions,
  preserves exception/type-initialization behavior, and canonicalizes CFG reachability before DCE.
  Export verification is fatal before and after final runtime-service resolution.
- Deterministic semantic identities, ordering, DCE metadata closure, and PE lookup indexes improve
  reproducibility and linker scalability. Expected assembly conflicts are preflighted before parent
  mutation, with persistent indexes avoiding cumulative destination rescans. The serialized IR is
  schema 11 (`CILLYAR11`); schema-10 and schema-9 artifacts are rejected before positional
  decoding. The .NET 10 atomic matrix covers every signed/unsigned integer RMW width.
- Missing runtime services remain fatal unless an exact capability is registered. The managed
  backtrace fallback recognizes only four pinned libunwind ABIs: it preserves the program counter
  for `_Unwind_FindEnclosingFunction`, reports no native instruction or stack pointer for
  `_Unwind_GetIP` and `_Unwind_GetCFA`, and terminates `_Unwind_Backtrace` with
  `_URC_END_OF_STACK`. The exact pinned `llvm.x86.xgetbv(u32) -> i64` service reports zero XCR0
  capability so managed `std_detect` remains conservative. Neighboring libunwind and x86 symbols
  are not synthesized.
- The panic-message acceptance forces `overflow-checks=true` through build-std and compares native
  Rust with managed debug and release output for bounds, division/remainder-by-zero, and arithmetic
  overflow paths.

This is still an experimental compiler preview. It does not promise a stable compiler ABI or full
Rust/.NET semantic parity.
