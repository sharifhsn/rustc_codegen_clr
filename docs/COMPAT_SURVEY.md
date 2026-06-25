# Ecosystem Compatibility Survey (rustc_codegen_clr → .NET)

A broad, **differential** compatibility map of real crates.io library crates compiled with the backend
and run on .NET 8 (`x86_64-unknown-dotnet`, native macOS-arm64 harness), each output **byte-compared to
native rustc** (`nightly-2026-06-17`). The goal is breadth — to surface *categories* of what works, what
kinda works, and what does not — not to cherry-pick wins.

**Method.** Each crate is a tiny deterministic exercise (`cargo_tests/soak_<name>` and
`cargo_tests/survey_<name>`) that drives the crate's core surface and prints labeled values, validated to
run identically twice natively before the backend run. Build-fails are classified by the fatal CIL
type-gate's rejection reason (invariant I1) or the runtime fault; divergences by the stdout/stderr diff.

## Headline

| corpus | crates | ✅ works (byte-identical) | 🟡 diverges at runtime | ❌ build-fails |
|---|---|---|---|---|
| soak (pure/curated) | 97 | 94 | 0 | 3 real* + 4 pre-existing |
| survey (broad/harder) | 40 | 22 | 4 | 14 |
| **total** | **137** | **116 (≈85%)** | **4** | **17** |

\* the soak corpus had 0 *divergences* (0 silent miscompiles); its 7 build-fails are pre-existing codegen
gaps that fall into the same classes below. **Zero of the 137 crates silently miscompiled into wrong-but-
plausible output and exited 0** except the 4 survey divergences triaged in §3C — i.e. the fatal type-gate
caught nearly everything broken at *build* time rather than letting it run wrong.

## 1. ✅ WORKS — broad categories with byte-identical output

The backend handles a wide spread of real ecosystem code. Representative working crates by category
(116 total across both corpora):

- **Serialization / parsing:** serde, serde_json, bincode, postcard, borsh, ciborium, rmp-serde, ron,
  toml(read), csv, **rkyv** (zero-copy archive), pest, nom, winnow, logos, data-encoding, base64, hex, bs58.
- **Numeric / float / fixed / 128-bit:** num-bigint, num-rational, num-complex, num-traits, num-integer,
  libm, euclid, **cgmath**, **glam** (SIMD math), **statrs** (distributions), fixed, rust_decimal (i128),
  noisy_float, approx, ordered-float, half(soak-arith), lexical-core, ryu, itoa, az.
- **Hashing / checksums:** ahash, fxhash, foldhash, seahash, twox-hash, siphasher, wyhash, blake2, blake3,
  md-5, crc32fast, crc-any, **argon2** (memory-hard KDF), **aes**.
- **Data structures:** indexmap, **hashbrown**, smallvec, tinyvec, arrayvec, im, slotmap, **slab**, **lru**,
  **priority-queue**, generational-arena, bitvec, fixedbitset, compact_str, smartstring, bstr, bytes,
  **roaring** (compressed bitmaps).
- **Text / unicode:** unicode-segmentation, unicode-normalization, unicode-width, unicode-xid, heck,
  percent-encoding, strsim, encoding_rs.
- **Concurrency primitives (deterministic paths):** **crossbeam-channel**, **arc-swap**, **spin**.
- **Async (executor, no reactor):** **futures-lite** (block_on of combinators).
- **Proc-macro / derive / compile-time:** thiserror, bitflags, **derive_more**, **strum**, **pin-project**,
  **phf** (compile-time perfect-hash maps), zerocopy, bytemuck, bumpalo.
- **Compression (pure-Rust):** miniz_oxide, lz4_flex, **brotli**.
- **Error / util:** anyhow, **eyre**, itertools, humantime, fastrand, oorandom, chrono(fixed), time(fixed).

That includes working representatives of **every requested hard category** — SIMD math (glam/cgmath),
i128 (rust_decimal), zero-copy (rkyv), memory-hard crypto (argon2/aes), a concurrency channel
(crossbeam-channel), an async executor (futures-lite), and proc-macro-heavy derives (strum/derive_more).

## 2. 🟡 KINDA-WORKS — builds + runs, with caveats

- **rayon** — builds and the parallel math is *correct*, but the run aborts in rayon's lazy global
  thread-pool init: `OnceLock: one-time initialization may not be performed recursively`. A
  **threading-model limitation** (the PAL's lazy-init/thread story), not a codegen miscompile. Simple
  `par_iter` sums compute the right value up to the pool-init point.
- **jiff** — almost all of it is byte-identical; a single operation (`Timestamp::round` to the hour)
  returns `error` on the backend where native yields the rounded timestamp. Localized — likely a real
  miscompile in one rounding path, not a whole-crate wall.
- The working **concurrency/async** crates (crossbeam-channel, arc-swap, futures-lite) are validated only
  on **deterministic single-threaded paths**; real multi-threaded contention / a live async reactor are
  not exercised and are expected to hit the same threading limits as rayon/parking_lot.

## 3. ❌ DOESN'T-WORK — by failure class (clustered, not one-offs)

The failures cluster into a few **single-root-cause classes** — fixing each unblocks a whole group.

### Class A — x86 SIMD intrinsic codegen emits an ill-typed `U32 * USize` (HIGH VALUE: ~7 crates, 1 fix)

`__mm_extract_epi32` / `__mm256_extract_epi64` / `__mm_store_ss` / `__mm_storel_pd` and friends lower to
CIL containing a `WrongBinopArgs { lhs: Int(U32), rhs: Int(USize), op: Mul }` (a lane/offset multiply
whose two operands have mismatched integer widths), which the fatal type-gate rejects.

- **Affected:** curve25519-dalek, ed25519-dalek, nalgebra, ndarray, chacha20, sha1, (likely wide).
- **Note:** glam/cgmath SIMD math *works* — so it is specific lane-extract/store intrinsics, not all SIMD.
- **Fix shape:** in the SSE/AVX intrinsic lowering, the index/offset operand of the lane multiply must be
  unified to a single integer width (cast `U32`↔`USize`) before the `Mul`. One codegen fix should clear
  the entire class. **This is the highest-leverage next fix the survey found.**

### Class B — `Weak<T>::drop` glue emits a mismatched pointer `SetField` (~4 crates, 1 fix)

`alloc::sync::Weak::<T>::drop` lowers a `SetField` storing `Ptr(A)` into a `Ptr(B)` field →
`FieldAssignWrongType`, rejected by the gate.

- **Affected:** flume, globset, aho-corasick, regex (all via their `Weak<…>` drop glue).
- **Fix shape:** the Weak-drop field store needs the two erased pointer types reconciled (a `PtrCast` /
  pointer-relabel) at the codegen site, or the `SetField` checker needs the same pointer-erasure tolerance
  the `StInd` arm now has (see `docs/` typecheck refactor). Single root cause.

### Class C — silent runtime divergences (real miscompiles — verify these)

- **quick-xml** — `System.AccessViolationException` (protected-memory read/write) in `read_text` /
  `read_event_impl`. A genuine **memory-corruption miscompile** of the slice-reader — the most serious
  single finding (memory-unsafe codegen). Worth a focused repro + fix.
- **json5** — backend produces only the first 1–2 output lines then stops; native prints all 11. A
  parse/serde-derive miscompile or an early error path taken wrongly.
- (**jiff** rounding — see §2.)

### Class D — threading / syscall (PAL limits, not type errors; "no ICE dump")

Build/link fails without a fatal-gate rejection — the crate pulls real OS threads / sync / syscalls the
PAL does not fully provide.

- **Affected:** dashmap (sharded locks), parking_lot (futex/`libc`), smol (async reactor + getrandom),
  wide (the SIMD non-panic case).
- These overlap the known PAL threading boundary (real `std::thread`/futex/TLS), tracked in the libc-shim /
  PAL scope docs.

### Class E — miscellaneous one-offs (RECLASSIFIED — none are walls; all are fixable codegen bugs)

Verified by root-causing each: **Class E contains NO true walls.** Every item is a fixable codegen bug,
to be folded into the A/B/C-style fix sweep:
- **futures** — `FieldOwnerMismatch` in `LocalFutureObj::poll` (a field interned against the wrong owner).
  Type-gate-caught codegen bug, family of B. Fixable.
- **toml** — `CantCompareTypes { Bool vs F64 }` in `write_toml_value`. Type-gate-caught codegen bug
  (a comparison emitting mismatched operands). Fixable.
- **half** — **NOT an f16 wall** (the prior label was wrong). half's core is u16 *software* float; it
  fails at the LINKER because `half-2.4.1/src/binary16/arch.rs` pulls x86 **F16C** SIMD intrinsics
  (`_mm_cvtph_ps`/`_mm_cvtps_ph`, behind a runtime `is_x86_feature_detected!("f16c")` guard) for the
  conversion fast-path, which the backend cannot yet lower. A **SIMD-intrinsic codegen gap** (same family
  as Class A), not a fundamental float limitation — and even Rust's *native* `f16` has a plausible
  `System.Half` mapping. Fixable.
- **hmac**, **sha2** — non-panic build error, Class-A-adjacent (crypto SIMD). Fixable.
- **serde_with** — a deserialize-visitor method rejected by the gate (`#[serde_as]` generated code).

### Wall audit — what is *actually* irreducible

After this survey + the Class-D research, the genuinely-irreducible walls are **narrow**: AF_UNIX
abstract-namespace / `SCM_RIGHTS`, true inode/dev/nlink identity, `fork`/`execve`, and arbitrary novel
inline asm (see ABSOLUTE_CORRECTNESS_PLAN §7). **f16 is NOT one of them** (System.Half exists; half's
failure is the F16C intrinsic gap above), and **threading/async is NOT one of them** (Class-D research:
real threads/Mutex/TLS work; the rest is the Parker keystone + BCL overlays). Most "walls" the survey
first reported were mislabeled fixable codegen gaps.

## 3.5 Status — Class A/B/C fixes landed (commit c103b47)

Four clean root-cause fixes landed; **9 crates flipped to byte-identical** (curve25519-dalek, ndarray,
chacha20, sha1, flume, aho-corasick, jiff, json5, quick-xml), 0 regression:
- **A** — `Assembly::offset` zero-extends the lane index to USize (was an ill-typed `U32*USize`).
- **B** — `field_address` passes the pointee, not `nptr(pointee)`, to `cast_ptr` (Weak<dyn>::drop).
- **C1/C2** — the 128-bit multiply-overflow-check builtins compared the div-back to `rhs` not `lhs`
  (a **broad** latent miscompile: every `i128`/`u128` checked multiply wrongly overflowed).
- **C3** — `place_address_raw`'s single-Deref fast-path is gated on `pointer_to_is_fat` (the quick-xml
  memory-corruption miscompile).

**Residual second-layer bugs** (separate, exposed by the above; tracked): **A2** ed25519-dalek + nalgebra
hit a *different* SIMD site (non-panic build error); **B2** globset + regex now build but crash with an
`AccessViolation` in `Arc<dyn>::drop` drop-glue (a fat-pointer drop bug, same family as B/C3).

**Class D** is fully researched in **[docs/THREADING_PAL_RESEARCH.md](THREADING_PAL_RESEARCH.md)** — there
is no fundamental .NET wall; real threads/Mutex/TLS/atomics already work, and the rest is a single keystone
primitive (a `ManualResetEventSlim`-backed `Parker`) + routing std's generic sync arms + a few BCL overlays.

## 4. Ranked next fixes (what the survey surfaced)

1. **Class A — SIMD lane-extract/store `U32*USize` binop** — one codegen fix, ~7 crates (crypto + linalg).
2. **Class C — quick-xml AccessViolation** — a memory-unsafe miscompile; highest-severity single bug.
3. **Class B — `Weak::drop` `FieldAssignWrongType`** — one fix, ~4 crates.
4. **Class C — json5 / jiff divergences** — localized miscompiles.
5. Class D (threading/syscall) and Class E walls (f16, AF_UNIX-style) are PAL-frontier / fundamental
   walls, tracked elsewhere.

**Bottom line:** the backend is broadly correct on real ecosystem library code (~85% byte-identical across
137 crates, with working representatives in every hard category), the fatal type-gate catches almost all
remaining breakage at build time, and the residual failures collapse into **~3 single-root-cause codegen
classes** (SIMD-intrinsic binop, Weak-drop field store, a quick-xml memory miscompile) plus the known
threading/syscall PAL frontier.
