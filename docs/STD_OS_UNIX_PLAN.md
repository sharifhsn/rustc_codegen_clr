# Full `std::os::unix` on .NET — the `target-family=["unix"]` plan

**Status:** read-only analysis + ordered implementation plan. No code changed; no build run.
**Scope of THIS doc:** the whole multi-package effort, with **Package A** (make `std` COMPILE under
the global `target-family=["unix"]` flip) specified to file:line. Packages B and C are outlined.

**Source of truth:** live std read at
`~/.rustup/toolchains/nightly-2026-05-13/.../library/std/src` (local). The container pins
`nightly-2026-06-17`; every cascade cited here is years-stable and identical across that ~1-month gap,
but **re-verify the cfg_select! ordinals before injecting** (the dev.sh helpers are ordinal- or
anchor-based — see §A.2). All `file:line` are into that `std/src` unless tagged `[pal]` (= repo
`dotnet_pal/`), `[posix]` (= `cilly/src/ir/builtins/`), `[spec]` (= `x86_64-unknown-dotnet.json`),
or `[dev]` (= `feasibility/dev.sh`).

**Companion docs (do not duplicate; this is the umbrella):**
- `feasibility/PACKAGE_A_OS_UNIX_PLAN.md` — the `os::unix` *public surface* spec (= Package B here).
- `docs/std_research/PKGA_CLUSTER_os_str_ffi.md` — the os_str/ffi cluster (verification-only).

---

## 1. Executive summary

### 1.1 What the flip is, and why it cascades

Today `[spec]:21` declares `"os": "dotnet"` with **no `target-family` key** — that is the Cap-2.5
state. The flip = add `"target-family": ["unix"]` to the spec. That turns on, **globally**, both the
`cfg(unix)` and `cfg(target_family="unix")` predicates. Two break-classes follow:

1. **std's own per-family cascades switch arms.** ~30 `sys::*` dispatchers select on
   `target_family="unix"` (or bare `unix`). Pre-flip dotnet falls into their `_`/unsupported arm;
   post-flip the `unix` arm wins **unless** a `target_os="dotnet"` arm-0 is injected ahead of it.
   `[dev]:186 inject_arm` already does this for ~20 of them (the os=dotnet PAL was built this way).
   The flip widens exposure to the cascades that **lack** an arm-0 today.

2. **`os::unix` itself activates.** `os/mod.rs:84` gates `pub mod unix;` on `any(unix, doc)`. Pre-flip
   `unix` is false ⇒ os::unix is absent (this is exactly how Cap-2 worked: by NOT compiling it).
   Post-flip the entire `os::unix` public surface compiles — that is **Package B**.

Package A is **case (1) only**: close every newly-activated std-internal cascade so `std` *compiles*.
It is the make-or-break spike: does the `cfg(unix)` cascade close at all? Runtime correctness of any
stub is explicitly deferred.

### 1.2 The two mechanisms, and the one that is fiddly

- `cfg_select! { … }` blocks → `[dev] inject_arm`/`inject_arm_anchor` prepend a `target_os="dotnet"`
  arm-0. Because `target_os="dotnet"` is true for *only* our target, arm-0 always wins. **This is the
  universal defense and it already exists.** Most of Package A is "extend the dotnet PAL module the
  arm-0 points at" — pure Rust, no injection-machinery change.
- **Non-`cfg_select!` sites** are the fiddly ones: bare `#[cfg(target_family="unix")] pub use …;`
  re-exports and the `os/unix/mod.rs:39 mod platform` per-line `#[cfg]` list. `inject_arm` cannot
  touch these; they need either a PAL symbol to exist (so the re-export resolves) or a new
  line-insert helper. There are exactly **3** of these (§A.3.B getppid, §A.3.C errno re-exports — both
  resolved by adding symbols, not by injection — and §B's `mod platform`, which is Package B).

### 1.3 Refined workflow-unit estimate (yardstick: libc shim ≈ 4 wu)

The libc/POSIX shim (tasks #53-56: fd-table + errno + ~20 POSIX C-ABI symbols + epoll + the pal_libc
floor proof) is the **~4-workflow-unit** baseline. Grounded against the *actual* dotnet PAL surface
(read this session), Package A — **compile only, NOT the os::unix public surface** — is materially
smaller than that baseline, because every cascade it touches already has an arm-0 PAL module and the
new symbols are thin wrappers over machinery that already exists (the errno cell, FileAttr, getpid):

| Package | what | wu |
|---|---|---|
| **A** (this doc §A) — std compiles under the flip, *excluding* os::unix surface | errno wrappers, getppid stub, exit override, with_native_path + re-export widen, paths arm, the post-flip sys-tree audit | **~1.5–2.5** |
| **B** (`feasibility/PACKAGE_A_OS_UNIX_PLAN.md`) — the os::unix public surface compiles | os/dotnet platform tree + MetadataExt, fs exts, net AF_UNIX compile-stubs, process/thread exts | **~3.5–4.5** |
| **C** — flip the spec + broad re-verification + probe + gate + commit | the actual `[spec]` edit, build-std, triage the global cascade, regression | **~1.0–2.0** |

A and B share the `os::unix` enablement, so in practice A+B are executed together once the flip is on.
**The honest number the owner should budget for "std + full os::unix compiles under the flip" is
~6–9 wu**, front-loaded with risk in C's global-cascade triage (the thing that sank Cap-2). Package A
*alone* (compile std, os::unix still narrowed) is the cheap, high-signal first move at ~1.5–2.5 wu.

### 1.4 The headline

**The flip is viable and most of it is mechanical.** Every genuinely-impossible POSIX primitive
(fork/execve, abstract-namespace unix sockets, SCM_RIGHTS fd-passing, ucred, true inode identity,
mmap MAP_FIXED) is either behind a linux/bsd `cfg` that **drops** for `target_os="dotnet"` (so it
never compiles) or compiles cleanly as an `Err(Unsupported)`/synthetic-0 stub. **Nothing in the
compile-scope is a hard wall.** The one real risk is breadth, not depth: `sys/pal/mod.rs:7`'s bare
`unix =>` arm and the scattered bare `#[cfg(target_family="unix")]` re-exports must each be covered,
and the global build-std is the first time they are all exercised at once.

---

## 2. The complete flip-sensitive cascade census

Every `target_family="unix"` / bare `unix` site in the std `sys` tree (exhaustive grep this session),
classified as **COVERED** (arm-0 dotnet already injected by `[dev]`, so the flip's unix arm loses) or
**NEW BREAK** (no arm-0 / not a cfg_select ⇒ Package A must act). os::unix-surface refs are Package B.

| site | shape | status | note |
|---|---|---|---|
| `sys/pal/mod.rs:7` `unix =>` | cfg_select | **COVERED** | `[dev]:224` injects `mod dotnet; pub use self::dotnet::*` arm-0 → wins over bare `unix`. **VERIFY post-flip (highest risk).** |
| `sys/fs/mod.rs:9` `any(family=unix, wasi)` | cfg_select | **COVERED arm, NEW symbols** | `[dev]:279` injects arm-0. But the **free `with_native_path`** at `:55` is `cfg(not(any(family=unix,…)))` ⇒ DROPS post-flip; arm-0 must now supply its own `with_native_path` + `debug_assert_fd_is_open` + widen re-exports. See §A.3.A. |
| `sys/fs/mod.rs:124` `set_permissions_nofollow` | bare `cfg(all(unix,…))` | **NEW BREAK** | activates; needs `os::unix::fs::OpenOptionsExt::custom_flags` + `libc::O_NOFOLLOW`. §A.3.A. |
| `sys/paths/mod.rs:33` `family=unix =>` | cfg_select | **NEW BREAK** | no dotnet arm today (dotnet→`_` at `:50`). Post-flip selects unix (libc getcwd/chdir/getpwuid_r/current_exe). §A.3.D. |
| `sys/io/mod.rs:58` `pub use error::errno_location` | bare `cfg(family=unix,…)` | **NEW BREAK** | re-export activates; `error::errno_location` must EXIST in dotnet io/error. §A.3.C. |
| `sys/io/mod.rs:64` `pub use error::set_errno` | bare `cfg(family=unix,…)` | **NEW BREAK** | ditto `set_errno`. §A.3.C. |
| `sys/io/error/mod.rs:26` `family=unix =>` | cfg_select | **COVERED** | `[dev]:260` arm-0 → `imp=dotnet`. (Resolves §A.3.C's `error::` path once dotnet.rs gains the two fns.) |
| `sys/io/mod.rs:7,28` io_slice/is_terminal | cfg_select | **COVERED** | iovec is family-agnostic core; is_terminal arm-0 `[dev]:338`. |
| `sys/exit.rs:116` `any(family=unix, wasi) => libc::exit` | in-fn cfg_select | **NEW BREAK** | pre-flip dotnet→`_ => abort()` at `:138`; post-flip→`libc::exit` (host P/Invoke). §A.3.E. |
| `sys/process/mod.rs:2` `family=unix =>` | cfg_select | **COVERED** | `[dev]:315` arm-0 `mod dotnet; use dotnet as imp;`. |
| `sys/process/mod.rs:33` `pub use imp::getppid` | bare `cfg(family=unix)` | **NEW BREAK** | re-export activates; dotnet `imp` (process_unsupported) has no `getppid`. §A.3.B. |
| `sys/pipe/mod.rs:4` `unix =>` | cfg_select | **COVERED** | `[dev]:317` arm-0. |
| `sys/thread/mod.rs:51` `any(family=unix, wasi)` | cfg_select | **COVERED** | `[dev]:272` arm-0. |
| `sys/stdio/mod.rs:4` `any(family=unix,…)` | cfg_select | **COVERED** | `[dev]:226` arm-0. |
| `sys/args/mod.rs:5,18` | bare `cfg` `mod common` + cfg_select | **COVERED** | arm-0 `[dev]:227`; `mod common` (`args/common.rs`) is pure core/alloc (no libc) — benign extra compile. |
| `sys/env/mod.rs:6,18` | bare `cfg` `mod common` + cfg_select | **COVERED** | arm-0 `[dev]:228`; `env/common.rs` pure core/alloc — benign. |
| `sys/net/connection/mod.rs:3` | cfg_select | **COVERED** | `[dev]:296` arm-0. |
| `sys/net/connection/socket/mod.rs:25` | cfg_select | n/a | dotnet uses the `connection/dotnet.rs` arm, never enters `socket/`. |
| `sys/net/hostname/mod.rs:2` | cfg_select | **COVERED** | `[dev]:332` arm-0. |
| `sys/time/mod.rs:23` | cfg_select | **COVERED** | `[dev]:266` arm-0. |
| `sys/alloc/mod.rs:73` | bare `cfg` (additive) | **COVERED** | `[dev]:225` `mod dotnet;` provides the allocator; the unix `cfg` block is additive default-impl code, dotnet shadows. VERIFY. |
| `sys/sync/{mutex,rwlock,condvar,once,thread_parking}/mod.rs` | cfg_select | **COVERED** | `[dev]:325-329` arm-0 each. |
| `sys/thread_local/mod.rs:149` | cfg_select (4 blocks) | **COVERED** | `[dev]:248,254` storage+guard arms (anchor-based). |
| `sys/personality/mod.rs:36` `all(family=unix,…)` | cfg_select | **COVERED** | `[dev]:454` injects the aborting-stub eh_personality (Phase 3). VERIFY the family arm doesn't pull a 2nd personality. |
| `sys/fd/mod.rs:6` `any(family=unix, wasi)` | cfg_select | **COVERED** | `[dev]:312` arm-0 (the fd-table FileDesc). |
| `sys/backtrace.rs:200-203` `cfg(unix)` → `os::unix::prelude` | bare `cfg` | **NEW BREAK (resolved by B)** | forces `os::unix::ffi::OsStrExt` to exist — comes free once os::unix compiles (Package B / os_str cluster). |
| `os/mod.rs:84` `pub mod unix` | bare `cfg(any(unix,doc))` | **NEW BREAK (= Package B)** | activates the whole os::unix tree. |
| `os/fd/raw.rs:22` `cfg(unix) use os::unix::io::OwnedFd` | bare `cfg` | **NEW BREAK (resolved by B)** | resolves once os::unix::io compiles (Cap-1 fd onion). |

**Cascade count for the structured field:** **8 distinct std cfg(unix)/family cascades or refs that
newly need a dotnet arm** under Package A's compile-scope (excluding the os::unix public-surface
internals, which are Package B): `sys/fs` (with_native_path + re-exports + set_permissions_nofollow),
`sys/paths`, `sys/io` errno_location, `sys/io` set_errno, `sys/exit`, `sys/process` getppid,
`sys/backtrace`, `os/mod.rs:84 pub mod unix`. (Counting the two errno re-exports as one io cluster
gives 7; the leaner "cascades needing a *new dotnet arm*" count — fs, paths, io-errno, exit, process,
pal-verify, os::unix-enable — is the same 7–8 band.)

---

## A. Package A — make `std` COMPILE under the flip (the ordered plan)

### A.1 Pre-flip invariant (why the arm-0s survive)

The whole defense rests on `[dev]:186 inject_arm` placing `target_os="dotnet"` as cfg_select arm-0,
and `target_os="dotnet"` being true for *only* our target. So when the flip turns on the `unix` arm,
arm-0 still wins. **This is already true for the ~20 COVERED cascades in §2** — they were built this
way for the os=dotnet PAL and need no change. Package A is the residue: the **NEW BREAK** rows.

### A.2 Re-verify before injecting (nightly-drift guard)

`inject_arm` is ordinal-based (nth cfg_select in the file); `inject_arm_anchor` keys on a fixed
string. Across the 05-13 → 06-17 gap re-check: `sys/fs/mod.rs:8` (1st cfg_select), `sys/io/mod.rs`
(is_terminal is the 2nd cfg_select, `[dev]:338` uses nth=1 today — **AUDIT**), `os/unix/process.rs:15`
(`_` arm present, no inject needed). Prefer anchor-based for any file whose cfg_select count can drift.

### A.3 The NEW-BREAK resolutions (ordered; each independently compile-checkable)

#### A.3.A — `sys/fs` (HIGH within Package A; the with_native_path drop is the subtle one)

The flip removes the free `with_native_path` fallback (`sys/fs/mod.rs:55` is
`cfg(not(any(family=unix,…)))` ⇒ DROPS for dotnet post-flip). The existing arm-0 (`[dev]:279`,
`mod dotnet; use dotnet as imp;`) currently relies on that free fn. Three sub-fixes, all in
`[pal] sys/fs/dotnet.rs` + the injected arm body:

1. **`with_native_path`** — the unix arm imports it as `run_path_with_cstr` (CStr model);
   dotnet has no CStr path model. Add to the dotnet arm the no-CStr form:
   `pub fn with_native_path<T>(path:&Path, f:&dyn Fn(&Path)->io::Result<T>)->io::Result<T> { f(path) }`
   (verbatim copy of the upstream `:55-58` body). Widen the injected arm to
   `mod dotnet; use dotnet as imp; use dotnet::with_native_path;`. **LEAKY-L6:** interior-NUL paths
   not rejected at the std boundary (surface as a System.IO exception instead of `InvalidInput`).
2. **`debug_assert_fd_is_open`** — `sys/fs/mod.rs:17` `pub(crate) use unix::debug_assert_fd_is_open`
   is inside the unix arm and is consumed by `os/fd/owned.rs:213`. Add a no-op
   `pub(crate) fn debug_assert_fd_is_open(_fd: RawFd) {}` to `[pal] sys/fs/dotnet.rs` and widen the
   arm re-export. **CLEAN.** (This protects the EXISTING-green os::fd from breaking.)
3. **re-export widen** — the unix arm re-exports `chown,fchown,lchown,mkfifo` (`:13`) and `chroot`
   (`:15`). `set_permissions_nofollow` (`:124`, now active) needs `os::unix::fs::OpenOptionsExt` +
   `libc::O_NOFOLLOW`. For COMPILE: add `chown/fchown/lchown/mkfifo/chroot` as `unsupported()` stubs
   to `[pal] sys/fs/dotnet.rs` and widen the arm to re-export them; patch the `set_permissions_nofollow`
   gate (`:123`) to exclude dotnet OR provide a dotnet `O_NOFOLLOW` const + a stored-ignored
   `custom_flags`. **MUST-STUB / LEAKY-L1.** (The fuller fs surface — read_at/write_at, FileType::is,
   MetadataExt st_* — is **Package B**; for Package-A compile-of-std-proper these are NOT reached
   until os::unix compiles.)

Effort: MED. ~0.6 wu. **Reuse:** `[pal] sys/fs/dotnet.rs` (461d38a) is the host; only additive.

#### A.3.B — `sys/process` getppid (LOW)

`sys/process/mod.rs:33` `#[cfg(target_family="unix")] pub use imp::getppid;` activates; dotnet `imp`
(= process_unsupported, re-exported at `[pal] sys/process/dotnet.rs:31`) has no `getppid`. Add a
shadow alongside the existing `getpid` shadow (`[pal] sys/process/dotnet.rs:43`):
`pub fn getppid()->u32 { 0 }` (or an optional `rcl_dotnet_getppid` BCL hook later). **LEAKY:** synthetic
0; CoreCLR has no portable parent pid. Not load-bearing (a pure re-export). ~0.1 wu.

#### A.3.C — `sys/io` errno_location / set_errno (LOW, CLEAN)

`sys/io/mod.rs:58` `pub use error::errno_location;` and `:64` `pub use error::set_errno;` are bare
`#[cfg(...family=unix...)]` re-exports that activate. The dotnet io/error arm-0 (`[dev]:260`,
`mod dotnet; pub use dotnet::*`) means `error::*` = whatever `[pal] sys/io/error/dotnet.rs` defines —
and today it has `errno()` (`:21`) but NOT `errno_location`/`set_errno`. The `__errno_location` extern
is **already declared** there (`[pal] sys/io/error/dotnet.rs:18`), so add two thin wrappers:
```
pub fn errno_location() -> *mut c_int { unsafe { __errno_location() } }
pub fn set_errno(e: i32)              { unsafe { *__errno_location() = e } }
```
**CLEAN:** the shim's thread-local errno cell ([posix] posix.rs:676 `__errno_location`) already backs
both. Zero new hooks. ~0.1 wu. (This is precisely a Cap-2 blocker line; it is trivial once you see the
extern is already present.)

#### A.3.D — `sys/paths` (MED)

`sys/paths/mod.rs:33` `target_family="unix" => { mod unix; use unix as imp; }` activates; dotnet
currently lands in `_` (`:50` unsupported). The unix arm pulls `libc::getcwd/chdir`, `getpwuid_r`,
`passwd`, `getuid`, `sysconf`, and the apple/bsd `current_exe` sysctl path (`sys/paths/unix.rs:5,8-11,
17-55,111-466`) — none mapped on dotnet. Inject a `target_os="dotnet"` arm-0 → new
`[pal] sys/paths/dotnet.rs` (mirror the hermit/wasi split-arm shape at `:2-9`):
- `getcwd`/`current_exe`/`chdir`/`temp_dir` → 4 new BCL hooks
  (`Directory.GetCurrentDirectory`/`Environment.ProcessPath`/`Directory.SetCurrentDirectory`/
  `Path.GetTempPath`) — **CLEAN**, all have direct managed equivalents.
- `split_paths`/`join_paths`/`SplitPaths`/`JoinPathsError` → copy verbatim from `sys/paths/unix.rs`
  (pure byte-split on `:`; no libc). **CLEAN.**
- `home_dir` → `HOME`-env-or-`None`. **LEAKY-L5:** drops the `getpwuid_r` passwd fallback.

Effort: MED (new PAL file + 4 hooks). ~0.5 wu. **Reuse:** `sys/path/unix.rs` (the path *parser*, not
`paths`) is already dotnet's path arm via the os_str `_` route and compiles clean unchanged.

#### A.3.E — `sys/exit` (LOW)

`sys/exit.rs:116-119` `any(family=unix, wasi) => libc::exit(code)` activates; pre-flip dotnet hits
`_ => crate::intrinsics::abort()` (`:138`). `libc::exit` resolves to a host-libc P/Invoke
(in `cilly` LIBC_FNS; no posix-shim override) — compiles + runs on the Linux test host but is a LEAK
on a shipped non-Linux .NET host. Two options:
- **(preferred, honest+shippable)** add an `rcl_dotnet_exit` shim override → `System.Environment.Exit(code)`.
- **(cheapest, compile-only)** `[dev]` inject a `target_os="dotnet" => crate::intrinsics::abort()` arm
  into the in-fn `cfg_select!` at `:72` (closes the host-libc leak, abort-not-clean-exit).

Effort: LOW. ~0.15 wu. **LEAKY** until `rcl_dotnet_exit` lands.

#### A.3.F — `os::unix` enablement is shared with Package B

`os/mod.rs:84 pub mod unix` activating is the gateway to Package B. For Package A *compile of std
proper*, the direct refs that force os::unix to at least exist — `sys/backtrace.rs:202`
(`os::unix::ffi::OsStrExt::from_bytes`, `:200-203`) and `os/fd/raw.rs:22`
(`os::unix::io::OwnedFd`) — are satisfied the instant os::unix's `io` + `ffi` submodules compile,
which is the os_str cluster (already-clean, `PKGA_CLUSTER_os_str_ffi.md`) + the Cap-1 fd onion. So
A and B share the single os::unix turn-on; in practice flip them together (§C). The remaining os::unix
internals (the `mod platform` keystone, fs/net/process/thread exts) are Package B.

### A.4 Ordered Package-A steps (STOP at the first that won't close, report)

1. **errno wrappers (§A.3.C)** — 2 fns into `[pal] sys/io/error/dotnet.rs`. CLEAN, no hooks.
2. **getppid stub (§A.3.B)** — 1 fn into `[pal] sys/process/dotnet.rs`.
3. **exit override (§A.3.E)** — `rcl_dotnet_exit` shim or the abort arm-0.
4. **fs with_native_path + debug_assert + re-export widen + stubs (§A.3.A)** — `[pal] sys/fs/dotnet.rs`.
5. **paths arm (§A.3.D)** — new `[pal] sys/paths/dotnet.rs` + 4 BCL hooks + dev.sh inject_arm.
6. **THE FLIP + global triage (§C)** — add `"target-family":["unix"]` to `[spec]`, build-std, triage
   every new error against the §2 census; the os::unix surface (Package B) errors surface here too.

---

## B. Package B — the `os::unix` public surface compiles (outline)

Fully specified in **`feasibility/PACKAGE_A_OS_UNIX_PLAN.md`** (do not duplicate). Summary:

- **Keystone (HIGH):** `os/unix/mod.rs:39 mod platform` is a per-line `#[cfg(target_os=…)]` list with
  NO dotnet arm ⇒ empty post-flip ⇒ `os/unix/raw.rs:27-33` (pthread_t/blkcnt_t/… aliases) and
  `os/unix/fs.rs:10` (`platform::fs::MetadataExt`) fail to resolve. Fix: new `os/dotnet/{mod,raw,fs}.rs`
  (model on `os/darwin/`); inject `#[cfg(target_os="dotnet")] pub mod dotnet;` into `os/mod.rs` and the
  dotnet line into the `mod platform` list. **Needs a line-insert helper, not `inject_arm`** (it's a
  `#[cfg]` list, not a cfg_select). ~0.5 wu.
- **fs exts (HIGH):** `os/unix/fs.rs` needs `File::{read_at,write_at,read_buf_at,read_vectored_at,
  write_vectored_at}` (RandomAccess hooks), `OpenOptionsExt::{mode,custom_flags}`,
  `PermissionsExt::{mode,set_mode,from_mode}`, `FileType::is(mode)` + libc `S_IF*`,
  `DirEntryExt::{ino,file_name_os_str}`, `DirBuilderExt::set_mode`, and the platform `MetadataExt`
  st_* synthesized from `FileAttr`. ~1.2 wu.
- **net AF_UNIX (MED, compile-stub):** add `sockaddr_un` + `AF_UNIX=1` + `MSG_NOSIGNAL` + `MSG_PEEK`
  to `[pal] libc/dotnet.rs`; add `Socket::new`/`new_pair` (Err-Unsupported) to dotnet sys::net. The
  genuinely-impossible AF_UNIX pieces (abstract namespace, SCM_RIGHTS, ucred) are linux/bsd-cfg'd and
  DROP for dotnet — they never compile. ~0.4 wu. Real runtime (AddressFamily.Unix) deferred.
- **process/thread exts (MED/LOW):** CommandExt/ExitStatusExt resolve via process_unsupported (the
  `os/unix/process.rs:15` cfg_select has a `_` arm); thread needs `Thread::id/into_id`. ~0.45 wu.
- **ffi/raw (NONE/LOW):** os_str cluster is already clean; raw is satisfied by the os/dotnet foundation.

---

## C. Package C — flip + broad re-verification (outline)

1. **Edit `[spec]`:** add `"target-family": ["unix"]`. (Cap-2.5's RUSTC_WRAPPER scoping in `[dev]`
   becomes unnecessary for std; keep it only if still scoping mio/libc.)
2. **build-std** the workspace. **This is the first time the global cascade is exercised at once** —
   the Cap-2 revert happened here.
3. **Triage** every new error against the §2 census. Prime suspects for surprises:
   - `sys/pal/mod.rs:7` bare `unix =>` — confirm the `[dev]:224` arm-0 still wins and no
     `sys::pal::unix` submodule is pulled in by family.
   - `sys/alloc/mod.rs:73` + `sys/personality/mod.rs:36` additive unix blocks — confirm dotnet shadows.
   - any NEW bare `#[cfg(target_family="unix")]` re-export added by a nightly bump.
4. **Probe:** `cargo_tests/pal_os_unix` importing `std::os::unix::{fs::MetadataExt, net::UnixStream,
   process::CommandExt, thread::JoinHandleExt, io::AsRawFd}`, exercising the CLEAN paths (st_size,
   AsRawFd round-trip, OsStr::from_bytes on valid UTF-8).
5. **Regression gate:** `::stable` 416/22 + all `pal_*` probes + soak set, to prove the flip didn't
   regress the os=dotnet-only paths. Commit on green.

---

## D. The leaky-bits ledger (honest: leaky = compiles+runs-with-loss; impossible = stub/ENOSYS)

### D.1 LEAKY (documented loss, compiles + runs)

| id | piece | site | loss |
|---|---|---|---|
| L1 | OpenOptions `custom_flags(i32)` | os/unix/fs.rs:531 (B) | .NET FileStream has no raw-O_* passthrough → flags stored+silently ignored (incl. `set_permissions_nofollow`'s `O_NOFOLLOW`). |
| L2 | MetadataExt `st_size/atime/mtime/ctime/mode` | os/dotnet/fs.rs (B) | size + DateTimeOffset timestamps + dir/file mode bit are backfillable; perm bits synthesized (suid/sgid/sticky/per-class rwx lost). |
| L3 | FileTypeExt `is_block/char_device/fifo/socket` | os/unix/fs.rs:967 (B) | BCL models dir-vs-file only → all false. |
| L5 | `home_dir` | sys/paths/dotnet.rs (A.3.D) | drops `getpwuid_r` fallback → `HOME`-env-only. |
| L6 | no-CStr `with_native_path` | sys/fs/dotnet.rs (A.3.A) | interior-NUL paths surface as System.IO exception, not `InvalidInput`. |
| L7 | `getppid` | sys/process/dotnet.rs (A.3.B) | synthetic 0 (no portable parent pid on CoreCLR). |
| L8 | `is_terminal` | sys/io/is_terminal/dotnet.rs (covered) | always false (generic `<T>` signature can't see Console.IsRedirected). |
| L9 | `exit` (pre-`rcl_dotnet_exit`) | sys/exit.rs (A.3.E) | `libc::exit` is a host-libc P/Invoke: green on Linux host, LEAK on a shipped non-Linux .NET host. |
| L10 | os_str ↔ System.String boundary | [posix] dotnet.rs:905, posix_symbols.rs:937; [pal] fs/dotnet.rs path_bytes | non-UTF-8 path bytes mangled to U+FFFD at the BCL String boundary; ASCII + all valid-UTF-8 (100% of portable Rust) round-trip perfectly. RUNTIME-only, already-latent. |
| L11 | errno decode tail | [pal] io/error/dotnet.rs:33 | ~20 SocketError codes map; IOException/HResult tail collapses to EIO; EINTR never fires. (`errno_location`/`set_errno` THEMSELVES are CLEAN — real thread-local cell.) |
| L12 | net socketpair (AF_UNIX, runtime, deferred) | sys/net (B) | emulated via bound-listener+connect pair (no kernel socketpair). |

### D.2 IMPOSSIBLE (compiles as Unsupported/synthetic stub; fails or is meaningless at runtime)

| id | piece | why | resolution |
|---|---|---|---|
| I1 | true inode identity `st_ino/st_dev/st_rdev` | no managed file-id | → 0 (breaks same-file detection). |
| I2 | `st_uid/st_gid/st_nlink` | no POSIX ownership/link-count | → 0 / 1. |
| I3 | `chown/fchown/lchown/chroot/mkfifo` | no BCL ownership/chroot/named-pipe-node | → `Err(Unsupported)` stubs (must EXIST to satisfy re-exports). |
| I4 | raw open flags `O_NOFOLLOW/O_DIRECT/O_SYNC/O_TMPFILE/O_PATH` | FileStream can't express | ignored or Unsupported. |
| I5 | full POSIX mode bits on set_permissions/set_times | only FileAttributes.ReadOnly + SetLastWriteTime | readonly-bit only. |
| I6 | fork/execve/pre_exec/exec | no fork on CLR | process_unsupported stubs; pre_exec stored-never-run. |
| I7 | ExitStatusExt signal/core_dumped/stopped_signal | no POSIX wait-status on CLR | → None. |
| I8 | abstract-namespace unix sockets, SCM_RIGHTS/ancillary, ucred/SO_PEERCRED | no managed model | **NOT Package A/B blockers** — all linux/bsd-cfg'd, DROP for target_os=dotnet, never compile. |
| I9 | mmap MAP_FIXED, mprotect guard pages, raw signal delivery | no managed equivalent | not reached by the compile-scope; out of scope. |

**NOTE — NOT impossible (real follow-ups, currently stubbed):** `symlink/link/readlink`
(`File.CreateSymbolicLink`/`CreateHardLink` exist), AF_UNIX runtime (`AddressFamily.Unix` +
`UnixDomainSocketEndPoint`), fs timestamps (`FileInfo.LastWriteTimeUtc`). These compile as stubs in
A/B and upgrade to real impls behind a green compile.

---

## E. Go / no-go signals for the Package-A implement

**GREEN — "std compiles clean, proceed to B/C":**
- Steps A.4.1-3 (errno wrappers, getppid, exit) land with zero new errors — they're additive symbols
  over machinery that already exists (errno cell, process_unsupported, abort).
- A.4.4-5 (fs with_native_path + paths arm) close `sys::fs`/`sys::paths` with the arm-0 winning over
  the family arm (confirm by `cargo check` of std alone showing `imp = dotnet` selected).
- `sys/pal/mod.rs:7`'s arm-0 still wins after the flip (no `sys::pal::unix` in the error set).
- No bare `#[cfg(target_family="unix")]` re-export references a symbol the dotnet PAL lacks.

**AMBER — "narrow surprises, fixable in-flight":**
- A handful of additional bare `#[cfg(family=unix)]` re-exports surface (likely in `sys/pal` submodules
  or a nightly-added one). Each is fixed by adding the symbol to the matching dotnet PAL module —
  same pattern as A.3.C. Budget a few iterations.

**RED — "deeper wall, stop and revert the flip":**
- `sys/pal/mod.rs:7` arm-0 does NOT win and `mod unix` (the full libc/pthread unix PAL) is dragged in —
  the exact Cap-2 failure. If a `sys::pal` submodule selects by `target_family` (not `target_os`) and
  has no arm-0, it pulls the entire unix pal and the flip cannot close without re-architecting pal
  selection. **This is the single make-or-break check; run it FIRST after the flip** (`cargo check`
  std and grep the error set for `sys::pal::unix` / `pthread` / `libc::pthread_*`).
- (Runtime walls — fork/execve, inode identity, socketpair — are NOT red for COMPILE; they all build
  as stubs and are deferred by design.)

**Bottom line:** Package A is the cheap, high-signal spike. If the `sys/pal` arm-0 holds after the flip
(the RED check above), the rest is mechanical symbol-filling and the flip is viable.
