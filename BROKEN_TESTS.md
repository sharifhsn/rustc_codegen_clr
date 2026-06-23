# How to find the source code of a broken test?
1. Download the rust compiler source
`git clone https://github.com/rust-lang/rust.git --depth 1000`
2. Go into the library directory
3. Serach for the name of your test `grep -r "broken_test"`
# How to minimze a broken test?
1. After finding a broken test, extract it into a spearte crate.
2. Build the crate into native Rust using `cargo test` and check that it behaves as expcted. 
3. IMPORTANT! Clean the build cache using `cargo clean`. NEVER skip this step, beacause it can lead to suprising results which may hide the real issue.
4. Build the crate using `rustc_codegen_clr` and observe the incorrect results. 
5. Make a small change by removing \ simplifing some code in your example.
6. Clean the build cache using `cargo clean` again before rebuilding to prevent issues. DO NOT SKIP THS STEP.
7. Repeat steps 2-7 untill the test program can no longer be simplified.
8. Create an issue with your simplified broken test.
# List of broken core test:
## Did not compleate:
```
atomic::atomic_access_bool
atomic::bool_
atomic::int_max
atomic::int_min
atomic::int_nand
atomic::int_xor
atomic::ptr_bitops
atomic::uint_max
atomic::uint_min
atomic::uint_nand
atomic::uint_xor
cell::refcell_ref_coercion
future::test_join
hash::test_writer_hasher
iter::adapters::step_by::test_iterator_step_by_nth_try_fold
iter::range::test_range_advance_by
iter::test_monad_laws_left_identity
manually_drop::smoke
num::flt2dec::random::shortest_f32_exhaustive_equivalence_test
num::flt2dec::random::shortest_f64_hard_random_equivalence_test
num::i128::tests::test_saturating_abs
num::i128::tests::test_saturating_neg
num::int_log::checked_ilog
ptr::ptr_metadata
ptr::test_ptr_metadata_in_const
ptr::test_variadic_fnptr
result::result_try_trait_v2_branch
simd::testing
slice::select_nth_unstable
slice::take_in_bounds_max_range_from
slice::take_in_bounds_max_range_to
slice::take_mut_in_bounds_max_range_from
slice::take_mut_in_bounds_max_range_to
slice::take_mut_oob_max_range_to_inclusive
slice::take_oob_max_range_to_inclusive
```
## Failed
```
num::i8::tests::test_pow
iter::adapters::flat_map::test_flat_map_try_folds
num::bignum::test_add_small_overflow
num::i16::tests::test_checked_next_multiple_of
num::bignum::test_mul_small_overflow
mem::uninit_fill_clone_panic_drop
num::i128::tests::test_lots_of_isqrt
iter::adapters::peekable::test_peek_try_folds
num::bignum::test_mul_digits_overflow_1
num::i8::tests::test_from_str_radix
net::ip_addr::ipv6_properties
num::test_int_from_str_overflow
iter::traits::double_ended::test_rev_try_folds
ascii::test_is_ascii_align_size_thoroughly
num::i16::tests::test_pow
net::ip_addr::ipv6_addr_to_string
iter::adapters::flatten::test_flatten_try_folds
num::bignum::test_get_bit_out_of_range
slice::take_last_nonempty
num::bignum::test_add_overflow_1
iter::adapters::skip::test_skip_try_folds
num::i16::tests::test_from_str
net::socket_addr::ipv6_socket_addr_to_string
iter::adapters::map::test_map_try_folds
cell::refcell_unsized
num::f32::min
iter::adapters::cloned::test_cloned_try_folds
num::bignum::test_from_u64_overflow
iter::adapters::flatten::test_flatten_one_shot
slice::take_first_nonempty
num::i16::tests::test_from_str_radix
num::f32::max
option::as_slice
num::u16::tests::test_rotate
iter::adapters::step_by::test_iterator_step_by_nth_try_rfold
iter::range::test_range_inclusive_folds
iter::adapters::take_while::test_take_while_folds
iter::adapters::take::test_take_try_folds
num::u16::tests::test_leading_trailing_ones
iter::adapters::skip_while::test_skip_while_try_fold
slice::take_first_mut_nonempty
num::f64::max
net::socket_addr::socket_v6_to_str
num::i16::tests::test_rotate
num::i8::tests::test_checked_next_multiple_of
num::bignum::test_mul_digits_overflow_2
num::f64::min
num::i8::tests::test_from_str
slice::take_last_mut_nonempty
iter::adapters::filter_map::test_filter_map_try_folds
ptr::from_raw_parts
num::u8::tests::test_leading_trailing_ones
num::i16::tests::test_leading_trailing_ones
num::i8::tests::test_leading_trailing_ones
iter::adapters::flatten::test_flatten_one_shot_rev
num::bignum::test_mul_pow5_overflow_2
num::bignum::test_add_overflow_2
num::u128::tests::test_pow
result::result_const
iter::adapters::filter::test_filter_try_folds
num::bignum::test_mul_pow2_overflow_2
num::i128::tests::test_pow
```
# List of broken alloc tests:
## Did not compleate:
```
arc::make_mut_unsized
arc::shared_from_iter_normal
arc::shared_from_iter_trustedlen_no_fuse
arc::shared_from_iter_trustedlen_normal
arc::shared_from_iter_trustedlen_panic
arc::slice
arc::trait_object
arc::uninhabited
autotraits::test_binary_heap
autotraits::test_btree_map
autotraits::test_btree_set
autotraits::test_linked_list
autotraits::test_vec_deque
borrow::test_from_cow_c_str
borrow::test_from_cow_os_str
borrow::test_from_cow_path
borrow::test_from_cow_slice
borrow::test_from_cow_str
heap::alloc_system_overaligned_request
rc::shared_from_iter_normal
rc::shared_from_iter_trustedlen_no_fuse
rc::shared_from_iter_trustedlen_normal
rc::shared_from_iter_trustedlen_panic
rc::slice
rc::trait_object
rc::uninhabited
slice::subslice_patterns
slice::test_split_last
string::test_try_reserve
task::test_local_waker_will_wake_clone
task::test_waker_will_wake_clone
thin_box::align1zst
thin_box::align2zst
thin_box::align64_size_not_pow2
thin_box::align64big
thin_box::align64med
thin_box::align64small
thin_box::align64zst
vec::test_try_reserve
vec_deque::test_try_reserve
```
## Failed:
```
slice::test_split_first_mut
vec::vec_macro_repeating_null_raw_fat_pointer
slice::test_split_first
vec::extract_if_unconsumed_panic
vec::extract_if_consumed_panic
vec_deque::test_try_rfold_moves_iter
vec_deque::test_try_fold_moves_iter
str::const_str_ptr
vec::test_collect_after_iterator_clone
```

# Recently fixed (codegen corrections)

## `Assert`-terminator panic family — now matches native (fixed)

The `Assert` terminator (bounds check, division/remainder by zero, arithmetic
overflow, negate overflow, null/misaligned-pointer deref, invalid-enum
construction, coroutine-resume) used to lower through surrogate `assert_*`
builtins that **discarded the operands** and, on failure, called an **unbodied
`abort`** method. At runtime the program crashed with
`System.Exception: missing methiod abort` instead of producing the correct,
catchable Rust panic. For `BoundsCheck` this also meant the message never
contained the `len`/`index`.

Fix (`src/terminator/mod.rs`, `handle_terminator`'s `Assert` arm + new
`call_panic_lang_item` helper): lower each kind to the **exact** panic lang item
the native rustc backend uses — branch to the success block on the no-panic
condition, otherwise call e.g. `panic_bounds_check(index, len)` /
`panic_div_zero()` / `panic_add_overflow()` (all `#[track_caller]`, so the
implicit caller `Location` is materialized at the call site). Panic messages and
unwinding now match native exactly. Gate stays 426/12.

Repros (known-answer, verified backend-output == native Rust):
`cargo_tests/swap_panics`, `cargo_tests/panic_msgs`.

Now-passing entries moved out of the lists above:

- `slice::swap_panics::index_a_equals_len`
- `slice::swap_panics::index_b_equals_len`
- `slice::swap_panics::index_a_greater_than_len`
- `slice::swap_panics::index_b_greater_than_len`
- `vec::test_index_out_of_bounds`

## Verified STALE (per-feature codegen is already correct)

These BROKEN_TESTS.md entries do **not** reproduce as codegen bugs: minimized
standalone repros (native-first, then through the backend, `cargo clean` between)
produce byte-identical output to native Rust. They almost certainly fail in the
real fork test-binary for a test-binary-wide reason (an unrelated test in the
same module aborting, or test-source bit-rot), not the listed lowering. Left in
the lists above pending a real fork-suite re-validation, but flagged here.

- IPv6 Display/formatting: `net::ip_addr::ipv6_addr_to_string`,
  `net::ip_addr::ipv6_properties`, `net::socket_addr::ipv6_socket_addr_to_string`,
  `net::socket_addr::socket_v6_to_str` — repro `cargo_tests/ipv6_fmt`,
  `cargo_tests/ipv6_props`.
- Iterator try_fold/try_rfold/ControlFlow family
  (`iter::adapters::*::test_*_try_folds`, `step_by` nth_try_fold/rfold,
  `double_ended::test_rev_try_folds`, …) — repro `cargo_tests/iter_tryfold`.
- i128 saturating: `num::i128::tests::test_saturating_abs`,
  `test_saturating_neg` (already green via WF-A `e3cdd4b`) — repro
  `cargo_tests/i128_sat`.
- Sub-word `leading_ones`/`trailing_ones` masking
  (`num::{u8,i8,u16,i16}::tests::test_leading_trailing_ones`) — repro
  `cargo_tests/lead_trail_ones`. The suspected sign-agnostic-stack hazard does
  **not** materialize; all sub-word leading/trailing bit ops match native.

# P2-S1 (behavioral-equivalence) — differential census + fixes

First slice of P2 (invariant I2). A differential oracle (native rustc vs the
`CARGO_DOTNET_BACKEND=native` backend, byte-for-byte stdout/exit) was run over
targeted edge probes (float min/max + NaN/signed-zero, i128/u128 pow/isqrt/bit
ops, integer overflow/from_str/rotate, iterator try_fold/flatten/step_by,
slices/closures, float formatting incl. ULP-sensitive shortest-repr) plus the
CI-skipped fuzzer cases. The canonical Docker `::stable` gate stayed
**428 passed / 12 failed (baseline, zero regressions, zero fatal aborts)** under
the now-FATAL typechecker throughout.

## REAL codegen bugs — FIXED (each verified byte-identical vs native)

### `System.Double`/`System.Single` referenced as `class`, not `valuetype`

`ClassRef::double`/`single` (`cilly/src/ir/class.rs`) were constructed with
`is_valuetype = false`, so the IL exporter emitted `class [System.Runtime]System.Double`
instead of `valuetype …`. Any method whose **declaring type** is `System.Double`/
`System.Single` (`f64::min`/`max`, `mul_add`, `powi`, `Floor`/rounding used by
`{:.N}` formatting, `Abs`, `CopySign`, `MinNumber`/`MaxNumber`, `FusedMultiplyAdd`,
the IEEE `minimum`/`maximum`) made the CLR reject the type-load the instant that
method JITted: `System.TypeLoadException: Could not load type 'System.Double' …
value type mismatch`. Plain f64/f32 arithmetic and `{}` Display of simple values
were unaffected (they never name `System.Double` as a declaring type), which is
why it hid. Fix: `is_valuetype = true` (these ARE .NET value types, exactly like
`Int128`/`UInt128`). Regression crate: `cargo_tests/float_class_methods`.
Also fixed a copy-paste slip in `Float::F64`'s `is_nan` (`cilly/src/ir/tpe/float.rs`)
that called `System.Single::IsNaN` on an f64 arg.

### `u128`/`i128` `leading_zeros`/`trailing_zeros` returned garbage

`ctlz`/`cttz` (`src/terminator/intrinsics/ints.rs`) for 128-bit ints called
`System.{U,}Int128::{Leading,Trailing}ZeroCount` (which return a 128-bit value)
and then narrowed to `u32` with a raw `conv.u4` (`ctx.int_cast`). `conv.u4` is
invalid IL applied to a `System.UInt128`/`Int128` **struct** operand — the runtime
did not truncate, it read garbage (`1u128.leading_zeros()` → `2386363928`, not
127). The `ctpop`/`PopCount` arm already did this right via `op_Explicit`
(`crate::casts::int_to_int`); ctlz/cttz now route the same way. Regression crate:
`cargo_tests/wideint_ctlz`.

### Sub-word atomic CAS/swap emitted invalid IL (`InvalidProgramException`)

The `.NET 8` emulation builtins `atomic_cmpxchng{8,16}_correct` /
`atomic_xchng{8,16}_correct` (`cilly/src/ir/builtins/atomics.rs`) emulate an
8/16-bit atomic compare-exchange/swap with a masked 32-bit
`Interlocked.CompareExchange(int32&, int32, int32)` loop. They computed the
containing-word **address** into local 0, but declared local 0 as a plain
`int32` (not a pointer). The IL then `ldloc.0`-ed an `int32` value where the
call's first parameter requires a managed byref `int32&` — which the JIT rejects
the instant the helper runs:

```
System.InvalidProgramException: Common Language Runtime detected an invalid program.
   at MainModule.atomic_cmpxchng8_correct(Byte& , Byte , Byte )
```

This crashed **any** program performing an `AtomicU8`/`AtomicI8`/`AtomicU16`/
`AtomicI16`/`AtomicBool` `compare_exchange`/`swap` — and, transitively,
`std::panic::catch_unwind` (the panic-count machinery does a
`compare_exchange` on a static `Atomic<u8>`), so even bounds-check-then-catch
programs died. Found via the differential oracle on a `catch_unwind` probe.

Fix: declare local 0 as `Type::Ptr(i32)` in both `emulate_subword_cmp_xchng`
and `emulate_subword_xchng`, so `ldloc.0` yields a pointer the runtime accepts
for the `int32&` parameter. Verified byte-identical vs native (`AtomicU8`/`I8`/
`U16`/`I16`/`Bool` CAS+swap, signed sub-words, and the `catch_unwind` path).
Regression crate: `cargo_tests/cd_subword_atomics`. Gate stays 428/12.

## FUNDAMENTAL / hard walls — classified, NOT faked

### C variadic FFI (`printf(fmt, …)`) in .NET mode — the fuzz47/86/87/96 family

All four CI-skipped fuzzer cases (`test/fuzz/fuzz{47,86,87,96}.rs`) are large
auto-generated `custom_mir` programs whose *only* observable output goes through
`extern "C" { fn printf(fmt, …) }`. Minimal repro (`printf(c"%i", 42)` prints a
garbage address instead of `42`; `%s` reads garbage): the IL exporter has **no C
`vararg` calling-convention support** (`grep -i vararg cilly/src/ir/il_exporter`
finds none). Variadic args are pushed as an ordinary `call`, so the libc-shim
`printf` reads the wrong stack slots. CoreCLR's IL `vararg`/`__arglist` support
does not bridge cleanly to a native-cdecl variadic callee, so this is a genuine
.NET-ABI wall, not a quick fix. **The computation underneath is almost certainly
correct** — only the variadic *display* is wrong, so the divergence is entirely in
the printf marshalling. (C-output mode is unaffected: it emits literal C `printf`.)
Classification: FUNDAMENTAL-hard; would need a `vararg` calling-convention in the
exporter + a per-call-site marshalling shim in the linker.

## Float-formatting ULP — NOT a divergence (the flt2dec concern does not materialize)

Contrary to the long-standing suspicion, the backend's float formatting is
byte-identical to native across `{}`, `{:?}`, `{:e}`, `{:.N}`, and the
shortest-round-trip path on ULP-sensitive values (`0.1+0.2`, `1.0/3.0`,
`f64::MAX`/`MIN_POSITIVE`, subnormals, `9007199254740993.0`, `355.0/113.0`, etc.)
once the `System.Double` value-type fix above is applied — those cases were
*crashing* on the TypeLoadException (the `{:.N}`/`{:e}` paths call `System.Double`
rounding methods), never mis-rounding. `fmt::float::test_format_f64`/`_f32` and the
`flt2dec` exact/shortest *equivalence* tests are already in the recorded passing
list (`bin/success_coretests.txt`); only the two `shortest_*_{exhaustive,hard_random}`
remain (million-iteration stress — timeout/throughput, not a last-digit ULP defect).
No flt2dec codegen change is warranted.

> RESOLVED (P2-S2, was mis-classified as a native-harness artifact): the
> `{:?}`-float crash was a REAL portable codegen defect, not a build-std/sysroot
> quirk. The final IL contained BOTH `class` and `valuetype` references to
> `System.Double` in one assembly: P2-S1 fixed `ClassRef::double`/`single` to be
> value types, but the `Abs`/`CopySign`/`FusedMultiplyAdd` refs (reached by
> `f64::abs` inside `core::fmt::float::GeneralFormat::already_rounded_value_should_use_exponential`,
> which every `{:?}` of a float calls) still rendered as
> `class [System.Runtime]System.Double` → `TypeLoadException: ... value type mismatch`
> the instant such a method was JITted. So ANY Debug-format of a float — including
> derived `#[derive(Debug)]` on any struct/enum/tuple/`Vec`/`Option` holding a float —
> crashed; plain `{}` (Display) was unaffected, which is why it hid behind the green
> gate. Fix: `cilly` `il_exporter::class_ref` now normalizes the known BCL primitive
> value types (`System.Double`/`Single`/`Half`/`Int128`/`UInt128`) to the `valuetype`
> prefix at the rendering boundary regardless of which path interned the ClassRef
> (these CoreLib names are unconditionally value types — safe). Verified byte-identical
> vs native by `cargo_tests/float_debug_fmt`; the Docker `::stable` gate stays
> 428/12 (zero regressions).

## REAL — panic/`#[track_caller]`/exit-code fidelity cluster (program logic correct; diagnostics + exit code diverge)

P2-S2 re-measured this cluster precisely with the stderr-aware oracle. The earlier
"note goes to stdout" classification is **superseded**: the panic note now lands on
**stderr** in both native and backend (the stream routing is correct). The surviving,
distinct divergences are:

1. **`#[track_caller]` caller-location is wrong (the load-bearing one).** Every panic —
   and any plain `core::panic::Location::caller()` — reports the std-internal site
   `<WORKSPACE>/src/panic/location.rs:181:9` (the body of `Location::caller`) instead of
   the user call site (`src/main.rs:<line>:<col>`). Confirmed even for a user-level
   `#[track_caller] fn` calling `Location::caller()` (`e_track_caller` probe:
   backend prints `location.rs:181`, native prints `main.rs:6`). Root cause: the
   caller-location intrinsic / track_caller implicit-arg threading does not propagate
   the caller's location through the chain on this backend; reading `LdArg(arg_count)`
   in the `requires_caller_location` branch does not yield the caller's `&Location`.
   This is REAL (observable, in the panic note + `Location::caller()` return value) and
   needs proper `#[track_caller]` implicit-arg plumbing (mirroring rustc's
   `FunctionCx::get_caller_location` / `Body::caller_location_span`). An attempt at this
   plumbing existed in the tree but did not deliver (still mislocates) and was reverted.

2. **Thread name/id cosmetic:** backend prints `thread '<unnamed>' (1)`; native prints
   `thread 'main' (<tid>)`. The main thread is unnamed on the dotnet PAL and the id is a
   synthetic constant. Cosmetic; native tid is itself non-deterministic.

3. **Uncaught-panic / `process::exit(N)` exit code lost by the APPHOST (harness, not codegen).**
   An uncaught panic exits 0 (native: 101); `std::process::exit(7)` via the apphost exits 0
   (native: 7). DECISIVE TEST: running the produced `.dll` directly with `dotnet <dll>`
   yields the **correct** exit code (7), and the IL correctly emits
   `System.Environment::Exit(int32)`. So the `process::exit` → `Environment.Exit` codegen/PAL
   mapping is CORRECT; the residual loss is in the cargo-dotnet **native apphost launcher**
   not propagating the managed exit code. Fix belongs in the `cargo dotnet run` path
   (invoke via `dotnet <dll>` or fix apphost), not the backend.

Classification: program logic (catch/`Err`/branch) is correct; the residuals are a
diagnostics-fidelity defect (#1, real codegen — track_caller), a cosmetic (#2), and a
harness exit-code-propagation defect (#3, not codegen). Deferred to a dedicated
panic/track_caller-fidelity slice.

## NEW real divergences found in P2-S2 (ranked, beyond the S1-fixed 3)

1. **FIXED — float `{:?}` Debug + `abs`/`copysign`/`mul_add` crash** (`class`-vs-`valuetype`
   `System.Double`/`Single`; see the RESOLVED note above). High leverage: hit every
   Debug-format of a float. Regression: `cargo_tests/float_debug_fmt`. Gate 428/12 green.

2. **OPEN, real codegen ICE — `fn main() -> Result<_,_>` (Termination trait).** A `main`
   returning `Result` (even the trivial `fn main() -> Result<(), String> { Ok(()) }`)
   makes the backend ICE ("the compiler unexpectedly panicked", build exit 101) — native
   compiles+runs fine. Loud failure (I3-ok) but an I2 gap: `Termination`-returning `main`
   is unsupported and crashes codegen. Tractable: implement the `Termination` lang-item
   lowering for the `main` shim. Repro: `/tmp/probes/e_result_main`, `e_result_unit`.

3. **OPEN, real — track_caller location** (item 1 of the panic cluster above).

4. **OPEN, harness — apphost exit code** (item 3 of the panic cluster above; not codegen).

## Verified clean NOW (census — match native byte-for-byte)

Edge probes (`/tmp/probes/*`, oracle `/tmp/diff_oracle.sh`):
`f32`/`f64` `min`/`max` + NaN + signed-zero; `sqrt`/`mul_add`/`powi`/`powf`/`floor`/
`ceil`/`trunc`/`round`/`abs`/`copysign`/`sin`/`cos`; `is_nan`; `u128`/`i128`
`pow`/`isqrt`/`leading_zeros`/`trailing_zeros`/`count_ones`/wrapping arith/`checked_*`/
div/rem/`from_str`; `u8`/`u16` rotate/`reverse_bits`/`swap_bytes`, sub-word
`leading_ones`/`trailing_ones`, `from_str(_radix)` overflow, `checked_pow`,
`ilog`/`ilog2`; integer overflow (`overflowing_*`/`checked_neg`/`wrapping_*`);
**sub-word atomics** (`AtomicU8`/`I8`/`U16`/`I16`/`Bool` `compare_exchange`/`swap`/
`fetch_{and,or,xor,nand,max,min,add}`); iterator `try_fold`/`try_rfold`/`take_while`/
`flat_map`/`flatten`/`step_by`/`filter`/`skip_while`/`rfold`/`peekable`; slice
`split_first(_mut)`/`split_last`/`split_at`/`windows`/`chunks`; ptr metadata
(`slice_from_raw_parts`/`size_of_val` for `[u8]`/`str`, `dyn Debug`, `Box<dyn>`);
int→float casts incl. huge `u128`/`i128 as f32/f64`; IPv6/`SocketAddrV6` Display;
move-closures; float formatting (all forms, incl. ULP-sensitive).

Crate corpus (`cargo_tests/soak_*`, byte-for-byte via the oracle, post-fix):
`libm`, `euclid`, `approx`, `lexical-core`, `itertools`, `bincode`, `indexmap`,
`blake3`, `chrono`, `arrayvec`, `bytemuck`, `byteorder`, `ahash`, `base64`, `hex`,
`crc32fast`, `fastrand`, `compact_str`, `bstr`, `data-encoding` — 20/20 MATCH.
(`soak_half` = the `half` crate's `f16`/`bf16`; fails to link on the f16/f128 wall
the CI already `--skip f16`s; out of scope for this slice.)
