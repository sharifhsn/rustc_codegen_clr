# feasibility/_cargo_dotnet_core.sh — the crate-AGNOSTIC pipeline CORE.
#
# This is the SINGLE SOURCE OF TRUTH for the Rust->.NET build pipeline (PAL inject
# into rust-src + backend RUSTFLAGS + dotnet_overlays auto-apply + libc registry
# patch + build-std + optional run). It runs INSIDE the rcc-dev container, streamed
# on stdin by an execution driver (the docker driver in feasibility/cargo-dotnet,
# or `dev.sh pal-build` which delegates to it).
#
# Contract (set by the driver):
#   cwd          = the crate to build. The docker driver sets this to either
#                  /work/<relpath> (IN-REPO crates: built within the /work mount so
#                  sibling RELATIVE path-deps like ../getrandom_dotnet resolve) or
#                  /project (EXTERNAL crates: a SECOND bind mount, -w /project, with
#                  absolute path-deps + extra read-only sibling mounts — the J4
#                  contract). The CORE body is fully cwd-relative, so it is agnostic.
#   /work        = the repo         (backend dylib in rcc-target volume, overlays, spec)
#   CD_REL=1|0   release (default 1) | debug
#   CD_RUN=1|0   build only (0) | build + run the produced apphost (1)
#   CD_CLEAN=1|0 `cargo clean` first (rebuilds std; bulletproof)
#   CD_VERBOSE=1|0  unfiltered build log (default 0 = filtered like dev.sh)
#   "$@"         = program args, forwarded to the .NET exe on `run`
#
# Exit code: on `run`, the program's exit code; otherwise the build exit code.
#
# Body below is the dev.sh pal-build in-container heredoc, generalized: the
# hardcoded `cd /work/cargo_tests/$DEV_CRATE` is DROPPED (cwd is set by the driver -w),
# DEV_RUN -> CD_RUN, profile/dir keyed on CD_REL, the binary name is read from
# cargo (no dir-basename assumption, no jq/python3 — neither is in the image), and
# the exit code is propagated. The PAL-inject + apply_overlays + libc-patch blocks
# are byte-identical to dev.sh (crate-independent).
set -e
# ---------------------------------------------------------------------------
# HOST PARAMETERIZATION (docker vs native). Everything below is byte-identical
# between the two backends EXCEPT a handful of host-specific facts, supplied by
# these env vars. The docker driver leaves them UNSET, so the DEFAULTS reproduce
# the original /work + /root layout (Linux container) EXACTLY — no behaviour
# change for docker/Linux users. The native driver (CARGO_DOTNET_BACKEND=native
# in feasibility/cargo-dotnet) exports host equivalents before streaming this in.
#
#   CD_REPO        repo root that holds dotnet_pal/, dotnet_overlays/, the target
#                  spec, and target/release/<backend dylib>+linker. Docker: /work.
#   CD_BACKEND_DYLIB  absolute path to the codegen backend cdylib. Docker:
#                  /work/target/release/librustc_codegen_clr.so. The host
#                  extension differs (.so linux / .dylib macOS / .dll windows).
#   CD_LINKER      absolute path to the cilly linker binary. Docker:
#                  /work/target/release/linker.
#   CD_TARGET_SPEC absolute path to x86_64-unknown-dotnet.json. Docker:
#                  /work/x86_64-unknown-dotnet.json.
#   CD_REGISTRY_SRC  cargo registry src root scanned for the libc-0.2 copy
#                  build-std extracts. Docker: /root/.cargo/registry/src. Native:
#                  $CARGO_HOME/registry/src (or ~/.cargo/registry/src).
#   CD_LASTBUILD_LOG  where the filtered build log is tee'd. Docker:
#                  /work/feasibility/_lastbuild.log.
CD_REPO="${CD_REPO:-/work}"
CD_BACKEND_DYLIB="${CD_BACKEND_DYLIB:-$CD_REPO/target/release/librustc_codegen_clr.so}"
CD_LINKER="${CD_LINKER:-$CD_REPO/target/release/linker}"
CD_TARGET_SPEC="${CD_TARGET_SPEC:-$CD_REPO/x86_64-unknown-dotnet.json}"
CD_REGISTRY_SRC="${CD_REGISTRY_SRC:-/root/.cargo/registry/src}"
CD_LASTBUILD_LOG="${CD_LASTBUILD_LOG:-$CD_REPO/feasibility/_lastbuild.log}"
#   CD_EXE_EXT     host executable suffix for the produced .NET apphost. Docker/
#                  Linux/macOS: "" (empty). Windows: ".exe". ONLY the bin-fallback
#                  path (cargo's JSON 'executable' field is already host-correct,
#                  carrying .exe on Windows) consults this; default "" keeps the
#                  docker/Linux behaviour byte-identical.
CD_EXE_EXT="${CD_EXE_EXT:-}"

# Portable in-place sed: GNU sed wants `sed -i 's/…/…/' f`; BSD/macOS sed wants
# `sed -i '' 's/…/…/' f`. The PAL-injection seds below use BSD-incompatible
# `sed -i` GNU syntax; route them all through this so the SAME script edits
# rust-src correctly on Linux (docker) AND macOS (native). Detection is a cheap
# one-shot probe of the local sed (GNU `--version` succeeds; BSD has no such flag).
if sed --version >/dev/null 2>&1; then
  sed_i() { sed -i "$@"; }            # GNU sed (Linux container / GNU coreutils)
else
  sed_i() { local e="$1"; shift; sed -i '' "$e" "$@"; }   # BSD sed (macOS native)
fi

SRC="$(rustc --print sysroot)/lib/rustlib/src/rust/library/std/src/sys"
PAL="$CD_REPO/dotnet_pal/sys"
[ -d "$PAL" ] || { echo "!! no $PAL"; exit 1; }
echo "==> injecting dotnet PAL into rust-src ($SRC)"
# Each --rm container starts from a pristine rust-src baked into the image, so we
# always inject onto a clean base. Mirror every file under dotnet_pal/sys/** to $SRC/**.
( cd "$PAL" && find . -type f ) | while read -r f; do
  mkdir -p "$SRC/$(dirname "$f")"
  cp "$PAL/$f" "$SRC/$f"
done
# Insert the `target_os = "dotnet"` arm as the FIRST arm of each cascade's cfg_select!
# (dotnet is only true for our target, so order is irrelevant to correctness). Idempotent.
inject_arm() { # $1 = cascade file under $SRC ; $2 = arm body (one line) ; $3 = which cfg_select! block (1-based, default 1)
  local file="$SRC/$1"; local nth="${3:-1}"
  [ -f "$file" ] || return 0
  # Idempotency is per-block: a file with several cfg_select!s (thread_local/mod.rs)
  # gets a dotnet arm injected into each of blocks 1,2,3 across calls, so we key the
  # marker on the arm body, not just on the presence of `target_os = "dotnet"`.
  grep -qF "$2" "$file" && return 0
  awk -v arm="$2" -v nth="$nth" '
    /cfg_select! \{/ { blk++ }
    { print }
    /cfg_select! \{/ && blk==nth && !ins {
      print "    target_os = \"dotnet\" => {"
      print "        " arm
      print "    }"
      ins=1
    }' "$file" > "$file.__t" && mv "$file.__t" "$file"
}
# Anchor-based variant: insert the dotnet arm as the FIRST arm of the *next*
# `cfg_select! {` that appears at or after a line matching $2 (a fixed-string
# anchor). Use this for files whose cfg_select! count drifts across nightlies
# (e.g. thread_local/mod.rs, where a `destructors` cfg_select block was added,
# shifting every ordinal). Idempotent on the arm body. The arm body may be a
# multi-line block: pass it with literal newlines.
inject_arm_anchor() { # $1 = cascade file under $SRC ; $2 = fixed-string anchor ; $3 = arm body (may be multi-line)
  local file="$SRC/$1"
  [ -f "$file" ] || return 0
  grep -qF "$3" "$file" && return 0
  grep -qF "$2" "$file" || { echo "!! inject_arm_anchor: anchor '$2' not found in $1"; return 1; }
  awk -v anchor="$2" -v arm="$3" '
    index($0, anchor) { armed=1 }
    { print }
    armed && !ins && /cfg_select! \{/ {
      print "    target_os = \"dotnet\" => {"
      print arm
      print "    }"
      ins=1
    }' "$file" > "$file.__t" && mv "$file.__t" "$file"
}
[ -f "$PAL/pal/dotnet/mod.rs" ]      && inject_arm pal/mod.rs          'mod dotnet; pub use self::dotnet::*;'
[ -f "$PAL/alloc/dotnet.rs" ]        && inject_arm alloc/mod.rs        'mod dotnet;'
[ -f "$PAL/stdio/dotnet.rs" ]        && inject_arm stdio/mod.rs        'mod dotnet; pub use dotnet::*;'
[ -f "$PAL/args/dotnet.rs" ]         && inject_arm args/mod.rs         'mod dotnet; pub use dotnet::*;'
[ -f "$PAL/env/dotnet.rs" ]          && inject_arm env/mod.rs          'mod dotnet; pub use dotnet::*;'
[ -f "$PAL/random/dotnet.rs" ]       && inject_arm random/mod.rs       'mod dotnet; pub use dotnet::*;'
if [ -f "$PAL/thread_local/dotnet.rs" ]; then
  # thread_local/mod.rs has FOUR cfg_select!s, in source order:
  #   (1) the storage layer (top of file),
  #   (2) `pub(crate) mod destructors { cfg_select! { … } }`  <- added upstream;
  #       it is `#[cfg(all(target_thread_local, …))]`-gated, so it is compiled
  #       OUT for os=dotnet (not target_thread_local) and needs no arm,
  #   (3) `pub(crate) mod guard { cfg_select! { … } }`  -> supplies `enable`,
  #   (4) `pub(crate) mod key   { cfg_select! { … } }`  -> `_ => {}` (empty).
  # The (2) `destructors` block shifted every ordinal vs the older 3-block layout
  # this script was first written against, which is why the guard arm must be
  # anchored to `pub(crate) mod guard {` instead of injected by ordinal (the old
  # nth=2 landed in `destructors`, leaving `guard::enable` undefined — the very
  # E0425 we are fixing).
  #
  # Storage arm (block 1, still the first cfg_select): declares `mod dotnet` at
  # thread_local level and re-exports ITS STORAGE ITEMS ONLY (mirroring the
  # `no_threads` arm — a glob `pub use dotnet::*` instead leaks the PAL's own
  # `key`/`guard` items into thread_local scope and trips `hidden_glob_reexports`).
  inject_arm thread_local/mod.rs 'pub use dotnet::{EagerStorage, LazyStorage, thread_local_inner}; pub(crate) use dotnet::{LocalPointer, local_pointer}; mod dotnet;' 1
  # Guard arm: reach `enable` via `super::dotnet` (super of `guard` is
  # thread_local, where `mod dotnet` was declared above). `current.rs` calls
  # `crate::sys::thread_local::guard::enable()` from two sites; this is what they
  # resolve to. dotnet is modelled on `no_threads` (single managed thread), whose
  # guard `enable` is a leak-everything no-op.
  inject_arm_anchor thread_local/mod.rs 'pub(crate) mod guard {' '        pub(crate) use super::dotnet::enable;'
  # Key arm: WITH families unset the `_ => {}` empty key arm caught os=dotnet and
  # nothing imports from `sys::thread_local::key` (storage re-exports from `dotnet`
  # directly). Under the `target-family=["unix"]` flip, the key cascade's FIRST arm
  # — `all(not(apple), not(wasm), target_family="unix")` — now matches dotnet and
  # pulls `key/unix.rs` (libc::pthread_key_t/pthread_key_create/...). dotnet is the
  # no_threads (single managed thread) model — it needs NO pthread TLS keys — so
  # inject an EMPTY `target_os="dotnet" => {}` arm-0 (anchored on `pub(crate) mod
  # key {`, the 4th cfg_select!) so dotnet wins with the same empty body the `_`
  # arm gave pre-flip. CLEAN: no key consumers on this PAL.
  inject_arm_anchor thread_local/mod.rs 'pub(crate) mod key {' '        /* dotnet: no_threads model, no pthread TLS keys */'
fi
[ -f "$PAL/io/error/dotnet.rs" ]     && inject_arm io/error/mod.rs     'mod dotnet; pub use dotnet::*;'
# time/mod.rs uses the `mod X; use X as imp;` cascade shape (it re-exports
# `imp::{Instant, SystemTime, UNIX_EPOCH}` at the bottom), unlike the
# `pub use dotnet::*` arms above. The dotnet arm backs Instant/SystemTime with
# Stopwatch/DateTime via the rcl_dotnet_{instant_ticks,instant_freq,unix_ticks}
# hooks (see cilly/src/ir/builtins/dotnet.rs).
[ -f "$PAL/time/dotnet.rs" ]         && inject_arm time/mod.rs         'mod dotnet; use dotnet as imp;'
# thread/mod.rs is a plain `pub use dotnet::*` cascade (like the `_`/unsupported
# arm): the dotnet arm provides Thread{new,join} + sleep/yield_now/set_name/
# current_os_id/available_parallelism/DEFAULT_MIN_STACK_SIZE, backed by
# System.Threading.Thread / System.Environment via the rcl_dotnet_thread_* and
# rcl_dotnet_available_parallelism hooks (see cilly/src/ir/builtins/dotnet.rs).
[ -f "$PAL/thread/dotnet.rs" ]       && inject_arm thread/mod.rs       'mod dotnet; pub use dotnet::*;'
# fs/mod.rs uses the `mod X; use X as imp;` cascade shape (the `_` arm is
# `mod unsupported; use unsupported as imp;`), then re-exports
# `imp::{Dir, DirBuilder, DirEntry, File, FileAttr, FilePermissions, FileTimes,
# FileType, OpenOptions, ReadDir}`. The dotnet arm backs std::fs with System.IO
# (FileStream/File/Directory/FileInfo) via the rcl_dotnet_fs_* hooks (see
# cilly/src/ir/builtins/dotnet.rs). fs cascade is block 1 (default nth).
# PACKAGE A — the fs arm body is WIDENED for the target-family=unix flip. With
# families OFF, the dotnet arm only needs `mod dotnet; use dotnet as imp;` (the
# FREE `with_native_path` fallback at fs/mod.rs:55 supplies the path adaptor, and
# os/fd/owned.rs's debug_assert_fd_is_open call is `#[cfg(unix)]`-gated OFF). Post
# flip BOTH of those drop/activate, so the arm must also import the dotnet
# `with_native_path` (shadowing the now-dropped free fn) and re-export the unix
# cascade's `chown/fchown/lchown/mkfifo`, `chroot`, and (crate) `debug_assert_fd_is_open`
# — all defined in dotnet_pal/sys/fs/dotnet.rs (Package A stubs). These extra
# `use`/`pub use` lines are HARMLESS with families unset (the symbols simply
# exist and the re-exports are dead) and LOAD-BEARING under the flip.
[ -f "$PAL/fs/dotnet.rs" ]           && inject_arm fs/mod.rs           'mod dotnet; use dotnet as imp; #[cfg(target_family = "unix")] use dotnet::with_native_path; #[cfg(target_family = "unix")] pub use dotnet::{chown, fchown, lchown, mkfifo, chroot}; #[cfg(target_family = "unix")] pub(crate) use dotnet::debug_assert_fd_is_open;'
# PACKAGE A — sys/fs/mod.rs `set_permissions_nofollow`: the real impl is gated
# `#[cfg(all(unix, not(target_os="vxworks")))]` (active under the flip) and pulls
# `os::unix::fs::OpenOptionsExt::custom_flags` + `libc::O_NOFOLLOW` — neither of
# which the dotnet FileStream model can honour (L1/I4). The dotnet PAL has no raw
# O_* passthrough, so route dotnet to the `unimplemented!` arm instead: exclude
# dotnet from the real-impl gate and add it to the stub gate. Idempotent (guarded
# on the dotnet string). LEAKY-adjacent: set_permissions_nofollow is Unsupported
# on dotnet (matches I4 — raw open flags can't be expressed by FileStream).
FSMOD="$SRC/fs/mod.rs"
if [ -f "$FSMOD" ] && ! grep -q 'not(target_os = "dotnet")' "$FSMOD"; then
  echo "==> routing set_permissions_nofollow to the unimplemented arm for os=dotnet"
  # real-impl gate: all(unix, not(vxworks)) -> all(unix, not(vxworks), not(dotnet))
  sed_i 's/#\[cfg(all(unix, not(target_os = "vxworks")))\]/#[cfg(all(unix, not(target_os = "vxworks"), not(target_os = "dotnet")))]/' "$FSMOD"
  # stub gate: any(not(unix), vxworks) -> any(not(unix), vxworks, dotnet)
  sed_i 's/#\[cfg(any(not(unix), target_os = "vxworks"))\]/#[cfg(any(not(unix), target_os = "vxworks", target_os = "dotnet"))]/' "$FSMOD"
fi
# net is a TWO-subdir module: net/mod.rs just re-exports `connection::*` and
# `hostname::hostname`. The IMPL cascade lives in net/CONNECTION/mod.rs (a
# `mod X; pub use X::*` cfg_select! whose `_` arm is `mod unsupported; pub use
# unsupported::*`), so the dotnet arm goes THERE, not in net/mod.rs. The PAL file
# lives at dotnet_pal/sys/net/connection/dotnet.rs, so the mirror loop above
# copies it to $SRC/net/connection/dotnet.rs and the arm's `mod dotnet` resolves
# to it. The dotnet arm provides TcpStream/TcpListener/UdpSocket/LookupHost +
# lookup_host backed by System.Net.Sockets via the rcl_dotnet_net_* hooks (see
# cilly/src/ir/builtins/dotnet.rs). The always-compiled module-level
# each_addr/lookup_host_string fns in connection/mod.rs stay as-is (each_addr is
# dead-code-allowed for dotnet; lookup_host_string finds our `lookup_host`).
#
# NO hostname arm is injected: net/hostname/mod.rs has its own `_ =>` unsupported
# arm that already catches os=dotnet (returns Err(UNSUPPORTED_PLATFORM)), and
# `hostname()` is not exercised by the net probe.
[ -f "$PAL/net/connection/dotnet.rs" ] && inject_arm net/connection/mod.rs 'mod dotnet; pub use dotnet::*;'
# PACKAGE A — sys/paths/mod.rs: the `target-family="unix"` flip switches the
# cascade onto its `target_family="unix"` arm (`mod unix; use unix as imp;`)
# which pulls libc getcwd/chdir/getpwuid_r/getuid/sysconf + apple/bsd current_exe
# sysctl — none mapped on dotnet (pre-flip dotnet landed in `_`/unsupported). The
# dotnet arm-0 routes to dotnet_pal/sys/paths/dotnet.rs (REAL getcwd/current_exe/
# chdir/temp_dir via 4 new BCL hooks + pure byte split/join + HOME-only home_dir).
# Same `mod X; use X as imp;` shape as fs/time. paths/mod.rs cascade is block 1.
[ -f "$PAL/paths/dotnet.rs" ]        && inject_arm paths/mod.rs        'mod dotnet; use dotnet as imp;'
# ===========================================================================
# CAP-1 LIBC-SHIM FOUNDATION ARMS (LIBC_SHIM_SCOPE §4.2). These six dotnet std
# PAL cascade arms are injected as the FIRST cfg_select! arm of each module, so
# that when `families=["unix"]` is flipped at Cap-2 the unix/libc arm never wins.
# target_os="dotnet" is true ONLY for our target, so arm-0 injection cannot change
# any other target's selection — purely additive. With `families` UNSET today
# these modules already fall through to their `_`/no_threads/unsupported fallback,
# so each dotnet arm need only be AT LEAST as complete as the arm it shadows
# (most re-use the verbatim fallback source via `#[path]`). DORMANT-BUT-PRESENT
# with families unset; LOAD-BEARING at the Cap-2 flip. The fd-backed net Socket
# (net/connection/dotnet.rs, above) + sys::fd (below) are the exception: they are
# load-bearing NOW (the os/fd Socket onion + std::os::fd traits depend on them).
# ===========================================================================
# sys::fd — FileDesc(OwnedFd) over the fd-table; the intermediate type
# os/fd/net.rs needs (Socket(FileDesc)). fd/mod.rs `_ =>` arm is empty today.
[ -f "$PAL/fd/dotnet.rs" ]            && inject_arm fd/mod.rs            'mod dotnet; pub use dotnet::*;'
# sys::process — mirror unsupported + REAL getpid (Environment.ProcessId). Uses
# the `mod X; use X as imp;` cascade shape (like time/fs), NOT `pub use dotnet::*`.
[ -f "$PAL/process/dotnet.rs" ]       && inject_arm process/mod.rs       'mod dotnet; use dotnet as imp;'
# sys::pipe — PRESENT-but-Unsupported (System.IO.Pipes can't ride Socket.Poll).
[ -f "$PAL/pipe/dotnet.rs" ]          && inject_arm pipe/mod.rs          'mod dotnet; pub use dotnet::{Pipe, pipe};'
# sys::sync::{mutex,rwlock,condvar,once} + thread_parking — Cap-1 mirrors the
# no_threads/unsupported inner contracts (single-managed-thread correct; REAL
# System.Threading locks deferred to Cap-2 with the [ThreadStatic] TLS fix). Each
# of these mod.rs has exactly ONE cfg_select! (block 1), so nth=1 default is safe;
# the futex arm is first inside but keys on an explicit target_os list dotnet
# misses, so arm-0 dotnet injection wins. Do NOT point at pthread/queue (they
# depend on sys::pal::unix / pull thread parking).
[ -f "$PAL/sync/mutex/dotnet.rs" ]    && inject_arm sync/mutex/mod.rs    'mod dotnet; pub use dotnet::Mutex;'
[ -f "$PAL/sync/rwlock/dotnet.rs" ]   && inject_arm sync/rwlock/mod.rs   'mod dotnet; pub use dotnet::RwLock;'
[ -f "$PAL/sync/condvar/dotnet.rs" ]  && inject_arm sync/condvar/mod.rs  'mod dotnet; pub use dotnet::Condvar;'
[ -f "$PAL/sync/once/dotnet.rs" ]     && inject_arm sync/once/mod.rs     'mod dotnet; pub use dotnet::{Once, OnceState};'
[ -f "$PAL/sync/thread_parking/dotnet.rs" ] && inject_arm sync/thread_parking/mod.rs 'mod dotnet; pub use dotnet::Parker;'
# sys::net::hostname — REAL (Environment.MachineName via rcl_dotnet_hostname);
# replaces the current `_ => unsupported` catch.
[ -f "$PAL/net/hostname/dotnet.rs" ]  && inject_arm net/hostname/mod.rs  'mod dotnet; pub use dotnet::hostname;'
# sys::io is_terminal — a NESTED cfg_select! inside `mod is_terminal {` (the only
# cfg_select! in io/mod.rs -> nth=1). Generic is_terminal<T>(_)->false form
# (the isatty/AsFd form would break the Stdin/Stdout/File callers). If a future
# nightly adds another cfg_select! to io/mod.rs, switch to inject_arm_anchor on
# 'mod is_terminal {'.
[ -f "$PAL/io/is_terminal/dotnet.rs" ] && inject_arm io/mod.rs 'mod dotnet; pub use dotnet::*;' 1
# PACKAGE A — sys/exit.rs `exit(code)`: the `target-family="unix"` flip activates
# the `any(target_family="unix", target_os="wasi") => libc::exit(code)` arm of the
# in-fn cfg_select! (pre-flip dotnet hit the `_ => abort()` arm). `libc::exit` is
# NOT in the dotnet libc PAL face (close/read/socket/epoll only), so the unix arm
# would be E0425 under the flip. Inject a `target_os="dotnet"` arm-0 routing to
# `crate::intrinsics::abort()` (identical to the existing `_` fallback), which
# closes the would-be host-libc::exit leak with ZERO new symbols. exit.rs has TWO
# cfg_select!s: block 1 is the file-level `unique_thread_exit` cascade, block 2 is
# the in-fn one inside `pub fn exit`; we target block 2 (nth=2). LEAKY (L9):
# abort-not-clean-exit (exit code is dropped); an `rcl_dotnet_exit` ->
# Environment.Exit(code) hook is the honest upgrade. The doc-only marker file
# dotnet_pal/sys/exit_marker keeps this idempotent/guarded like the others.
inject_arm exit.rs 'let _ = code; crate::intrinsics::abort()' 2
# os/mod.rs gate widen: `pub mod fd` is gated `any(unix, hermit, trusty, wasi,
# motor, doc)` — os=dotnet is NOT in that list, so std::os::fd (OwnedFd/RawFd +
# os/fd/net.rs's Socket onion) is compiled OUT for dotnet today. Add dotnet to the
# gate so the fd-backed net Socket's std::os::fd traits become reachable (the
# pal_fd probe + the Cap-2 mio capstone need this). os=dotnet-only; additive.
# `libc` IS linked into dotnet std (Cargo dep gated on not(all(windows,msvc))), so
# owned.rs's `libc::close`/`fcntl` route through the POSIX shim, and `crate::sys::cvt`
# is provided by pal/dotnet/mod.rs (this repo). os/mod.rs is at $SRC/../os/mod.rs.
OSMOD="$SRC/../os/mod.rs"
if [ -f "$OSMOD" ] && ! grep -q 'target_os = "dotnet"' "$OSMOD"; then
  echo "==> widening os/mod.rs 'pub mod fd' gate to include os=dotnet"
  # The `pub mod fd` gate is a multi-line `#[cfg(any( ... ))]` ending at the line
  # before `pub mod fd;`. Find the nearest `#[cfg(any(` ABOVE `pub mod fd;` and
  # inject `    target_os = "dotnet",` immediately after it (first disjunct). The
  # scan keys on the unique `pub mod fd;` line, so it is robust to the disjunct
  # set drifting across nightlies. Idempotent (guarded on the dotnet string above).
  awk '
    { lines[NR]=$0 }
    END {
      # locate the `pub mod fd;` line.
      fdline=0;
      for (i=1;i<=NR;i++) if (lines[i] ~ /^pub mod fd;/) { fdline=i; break }
      # walk up to the opening `#[cfg(any(` of its gate.
      anyline=0;
      for (i=fdline-1;i>=1;i--) if (lines[i] ~ /#\[cfg\(any\($/) { anyline=i; break }
      for (i=1;i<=NR;i++) {
        print lines[i];
        if (i==anyline && anyline>0) print "    target_os = \"dotnet\",";
      }
    }' "$OSMOD" > "$OSMOD.__t" && mv "$OSMOD.__t" "$OSMOD"
fi
# B1 CONVERGENCE: the global `target-family=["unix"]` flip IS applied (committed in
# x86_64-unknown-dotnet.json), so std picks the dotnet PAL by os while cfg(unix) is
# global. The Cap-2.5 crate-scoped RUSTC_WRAPPER is GONE (no longer wired below):
# mio gets cfg(unix) from --target and libc from dep-resolution, and the few
# remaining mio backend-selection arms are baked into vendor/mio as
# `target_os="dotnet"` cfg arms. The wide std cfg(unix) cascades (sys::{fs,paths,io,
# process}/os::unix) are all covered by the dotnet PAL arm-0 injections above
# (Packages A/B). See docs/LIBC_SHIM_SCOPE.md §4.5.
# os/fd/{owned,raw}.rs File/Pipe fd-impl gating. Enabling os::fd for dotnet pulls in
# owned.rs's + raw.rs's `impl As/From/IntoRawFd`/`AsFd`/`From<…>` impls for fs::File
# and io::Pipe{Reader,Writer}, which require the dotnet `sys::fs::File` (System.IO
# FileStream, GCHandle-backed) and `sys::pipe::Pipe` (the `!` unsupported) to be
# fd-backed — they are NOT (Cap-2: fd-backing fs/pipe is a separate, large surface;
# in raw.rs `OwnedFd` is also not even imported for os=dotnet). These impls are
# already `#[cfg(not(target_os = "trusty"))]` (trusty has os::fd but is not fd-backed
# for File/Pipe either — the exact precedent). Mirror it: add `not(target_os =
# "dotnet")` to the File/Pipe impl gates ONLY, leaving the `crate::net::{TcpStream,
# TcpListener,UdpSocket}` impls (which DO have the fd-backed Socket onion) + os/fd/net.rs
# ENABLED for dotnet. Idempotent (guarded per-file on the dotnet string).
for OFD in "$SRC/../os/fd/owned.rs" "$SRC/../os/fd/raw.rs"; do
  if [ -f "$OFD" ] && ! grep -q 'not(target_os = "dotnet")' "$OFD"; then
    echo "==> deferring File/Pipe fd-impls for dotnet in $(basename "$OFD") (Cap-2; fs/pipe not fd-backed yet)"
    # For each `#[cfg(not(target_os = "trusty"))]` whose NEXT line is an impl
    # referencing fs::File / io::PipeReader / io::PipeWriter, widen the cfg to
    # also exclude dotnet. Keys on the impl target on the following line.
    awk '
      {
        if (prevline ~ /#\[cfg\(not\(target_os = "trusty"\)\)\]/ &&
            ($0 ~ /for fs::File/ || $0 ~ /<fs::File>/ ||
             $0 ~ /for io::Pipe/ || $0 ~ /<io::Pipe(Reader|Writer)>/)) {
          sub(/#\[cfg\(not\(target_os = "trusty"\)\)\]/,
              "#[cfg(all(not(target_os = \"trusty\"), not(target_os = \"dotnet\")))]", prevline)
        }
        if (NR>1) print prevline
        prevline=$0
      }
      END { if (NR>0) print prevline }
    ' "$OFD" > "$OFD.__t" && mv "$OFD.__t" "$OFD"
  fi
done
# PACKAGE A — os/unix/io/mod.rs StdioExt `null_fd()`. The above deferral keeps
# `From<fs::File> for OwnedFd` OFF for dotnet (the dotnet sys::fs::File is a
# managed FileStream GCHandle, NOT fd-backed — Cap-2). But the flip activates
# `os::unix::io` whose `null_fd()` does `Ok(null_dev.into())` on a `crate::fs::File`,
# requiring exactly that `From` — E0277 under the flip. `StdioExt` (the
# stdio-fd-swap UNSTABLE feature) is genuinely unsupported on a non-fd-backed fs
# PAL (you cannot hand a `/dev/null` File's "fd" to `dup2`), so neutralise
# `null_fd()` to `Err(UNSUPPORTED_PLATFORM)` for the dotnet rust-src (this rust-src
# is only ever built for target_os=dotnet). MUST-STUB (the matching
# `replace_stdio_fd` already has a `_ => UNSUPPORTED_PLATFORM` arm dotnet falls
# into, and `dup2` exists in the libc face). Idempotent (guarded on the marker).
IOMOD="$SRC/../os/unix/io/mod.rs"
if [ -f "$IOMOD" ] && ! grep -q 'dotnet: StdioExt null_fd unsupported' "$IOMOD"; then
  echo "==> neutralising os::unix::io StdioExt null_fd() for dotnet (fs not fd-backed)"
  perl -0pi -e 's/let null_dev = crate::fs::OpenOptions::new\(\)\.read\(true\)\.write\(true\)\.open\("\/dev\/null"\)\?;\s*\n\s*Ok\(null_dev\.into\(\)\)/\/\/ dotnet: StdioExt null_fd unsupported (fs::File not fd-backed)\n    Err(io::Error::UNSUPPORTED_PLATFORM)/s' "$IOMOD"
fi
# ===========================================================================
# PACKAGE A/B — the os::unix `platform` keystone. The `target-family=["unix"]`
# flip activates `os/mod.rs:84 pub mod unix;` globally, which makes
# `os/unix/mod.rs`'s `mod platform { ... }` per-target list resolve `platform::raw`
# (os/unix/raw.rs pthread_t/blkcnt_t/... aliases) and `platform::fs::MetadataExt`
# (the cross-unix st_* delegate). That list has NO dotnet arm by default, so it is
# empty for dotnet and those refs fail (E0432 "could not find raw/fs in platform").
# Mirror the in-repo os/dotnet tree (dotnet_pal/os/dotnet/{mod,raw,fs}.rs, modelled
# on os/darwin) into rust-src, then (1) declare `pub mod dotnet` in os/mod.rs and
# (2) add the dotnet line to the `mod platform` list in os/unix/mod.rs. Both are
# `#[cfg]` lists, NOT cfg_select!, so they need line-inserts (not inject_arm).
# os=dotnet-only; idempotent (guarded on the dotnet string).
OSDIR="$SRC/../os"
OSPAL="$CD_REPO/dotnet_pal/os"
if [ -d "$OSPAL/dotnet" ] && [ -d "$OSDIR" ]; then
  echo "==> mirroring os/dotnet platform tree + wiring os::unix platform list"
  mkdir -p "$OSDIR/dotnet"
  ( cd "$OSPAL/dotnet" && find . -type f ) | while read -r f; do
    mkdir -p "$OSDIR/dotnet/$(dirname "$f")"
    cp "$OSPAL/dotnet/$f" "$OSDIR/dotnet/$f"
  done
  # (1) os/mod.rs: declare `pub mod dotnet` (model on the darwin/linux decls).
  if ! grep -q 'pub mod dotnet;' "$OSDIR/mod.rs"; then
    # Insert right before the first `#[cfg(target_os = "aix")]` per-target block.
    awk '!ins && /#\[cfg\(target_os = "aix"\)\]/ {
           print "#[cfg(target_os = \"dotnet\")]";
           print "pub mod dotnet;";
           ins=1
         } { print }' "$OSDIR/mod.rs" > "$OSDIR/mod.rs.__t" && mv "$OSDIR/mod.rs.__t" "$OSDIR/mod.rs"
  fi
  # (2) os/unix/mod.rs: add the dotnet arm to the `mod platform { ... }` list.
  if ! grep -q 'crate::os::dotnet' "$OSDIR/unix/mod.rs"; then
    awk '!ins && /#\[cfg\(target_os = "aix"\)\]/ {
           print "    #[cfg(target_os = \"dotnet\")]";
           print "    pub use crate::os::dotnet::*;";
           ins=1
         } { print }' "$OSDIR/unix/mod.rs" > "$OSDIR/unix/mod.rs.__t" && mv "$OSDIR/unix/mod.rs.__t" "$OSDIR/unix/mod.rs"
  fi
fi
# DOTNET PAL ARM (mio): expose the socket's opaque GCHandle on the PUBLIC
# `std::net::{TcpStream,TcpListener,UdpSocket}` wrappers so the vendored mio
# dotnet arm can key its readiness Selector by it. The handle lives on the inner
# `sys` type (`dotnet_pal/sys/net/connection/dotnet.rs::dotnet_raw_handle`), but
# `std::net::TcpStream(net_imp::TcpStream)`'s inner is only reachable via the
# crate-private `AsInner` trait — not visible to mio. So we forward it with a
# public inherent method `dotnet_raw_handle(&self) -> *mut u8 { self.0.dotnet_raw_handle() }`
# injected into each `impl <Type> {` block. os=dotnet-only (these net.rs files are
# NOT mirrored from dotnet_pal — they are the upstream std wrappers, patched in
# place only inside the os=dotnet build), so ::stable / the surrogate are untouched.
# inject_method anchors on the FIRST `impl <Type> {` line in $1. The injected
# method is `#[stable]`, NOT `#[unstable]`: an unstable inherent method is invisible
# to the consumer (mio) without a matching `#![feature(...)]`, so resolution would
# fall back to mio's own `DotnetRawHandle` trait method and recurse. Marking it
# stable makes the inherent method visible (and thus shadow the trait) without
# forcing a feature gate into mio. Cosmetic-only on our patched os=dotnet std.
inject_method() { # $1 = file under $SRC ; $2 = exact `impl X {` anchor line ; $3 = unique marker comment
  local file="$SRC/$1"
  [ -f "$file" ] || { echo "!! inject_method: no $1"; return 1; }
  grep -qF "$3" "$file" && return 0
  grep -qF "$2" "$file" || { echo "!! inject_method: anchor '$2' not in $1"; return 1; }
  awk -v anchor="$2" -v marker="$3" '
    { print }
    !ins && index($0, anchor) {
      print "    " marker
      print "    #[cfg(target_os = \"dotnet\")]"
      print "    #[stable(feature = \"rust1\", since = \"1.0.0\")]"
      print "    #[allow(missing_docs)]"
      print "    pub fn dotnet_raw_handle(&self) -> *mut u8 { self.0.dotnet_raw_handle() }"
      ins=1
    }' "$file" > "$file.__t" && mv "$file.__t" "$file"
}
# NOTE: $SRC is `…/std/src/sys`, so the public net wrappers are at `../net/*.rs`.
inject_method "../net/tcp.rs" 'impl TcpStream {'   '// DOTNET PAL ARM: mio handle accessor (TcpStream)'
inject_method "../net/tcp.rs" 'impl TcpListener {' '// DOTNET PAL ARM: mio handle accessor (TcpListener)'
inject_method "../net/udp.rs" 'impl UdpSocket {'   '// DOTNET PAL ARM: mio handle accessor (UdpSocket)'
# personality/mod.rs holds the `eh_personality` lang item. With panic=unwind,
# rustc's front-end requires that lang item to EXIST (the missing-eh_personality
# weak-lang-item check that emits "unwinding panics are not supported without
# std"). os=dotnet has no DWARF/SEH unwinder — .NET's own managed EH runs the
# handlers — so, exactly like the wasm/msvc/motor arm right above the gcc arm in
# this same cfg_select!, we supply a trivial aborting STUB personality that
# satisfies the lang item but is never actually called at runtime. (block 1 is
# the only cfg_select! in this file, under `#[cfg(not(any(test, doctest)))]`.)
inject_arm personality/mod.rs '#[lang = "eh_personality"] fn rust_eh_personality() { core::intrinsics::abort() }'
# Teach std's build.rs that os=dotnet is a *supported* platform, otherwise std
# marks itself `restricted_std` (E0658 on use + "unwinding panics are not
# supported without std"). The allow-list is the long `if target_os == "linux"
# || ... {` block; inject our os as the first disjunct. Idempotent.
BUILD_RS="$SRC/../../build.rs"
if [ -f "$BUILD_RS" ] && ! grep -q 'target_os == "dotnet"' "$BUILD_RS"; then
  echo "==> teaching std/build.rs that os=dotnet is supported (un-restricted_std)"
  # Inject `target_os == "dotnet" ||` just before the first `target_os == "linux"`
  # disjunct of the supported-platform allow-list. awk (no perl dependency).
  awk '!ins && /target_os == "linux"/ {sub(/target_os == "linux"/, "target_os == \"dotnet\"\n        || target_os == \"linux\""); ins=1} {print}' "$BUILD_RS" > "$BUILD_RS.__t" && mv "$BUILD_RS.__t" "$BUILD_RS"
fi
# panic_unwind is a SEPARATE crate (library/panic_unwind/src), not under std/src/sys,
# so it is outside the sys mirror loop above. Its lib.rs picks the unwind FLAVOUR
# (gcc/seh/dummy/…) from a cfg_select!; os=dotnet (no target-family) falls through to
# the aborting `dummy.rs` arm, so even with `build-std=…,panic_unwind` no real unwind
# runtime is selected. Inject a `target_os = "dotnet"` arm that routes to the GCC
# flavour: gcc's `imp::panic` calls `_Unwind_RaiseException`, which the cilly linker
# overrides into a managed `RustException` throw (the WF-6 throw-bridge). The DWARF
# personality gcc would otherwise need is never invoked — .NET EH runs the handlers —
# and the matching `eh_personality` lang item is the dotnet stub added to
# std/sys/personality above. The arm is literally the gcc arm (`#[path = "gcc.rs"] mod imp;`):
# gcc.rs's `super::__rust_drop_panic`/`__rust_foreign_exception` refs must resolve to the crate
# root, so gcc.rs is included directly as `imp` (no extra module nesting).
# dotnet_pal/panic_unwind/dotnet.rs is a doc-only marker for this arm.
PUSRC="$SRC/../../../panic_unwind/src"      # library/panic_unwind/src
PUPAL="$CD_REPO/dotnet_pal/panic_unwind"
if [ -d "$PUPAL" ] && [ -f "$PUSRC/lib.rs" ]; then
  echo "==> injecting dotnet panic_unwind arm ($PUSRC)"
  cp "$PUPAL/dotnet.rs" "$PUSRC/dotnet.rs"   # doc-only marker for the arm
  # Add the dotnet arm as the FIRST arm of the FLAVOUR cfg_select! (the first, and
  # only, cfg_select! in panic_unwind/lib.rs). `&& !ins` keeps it to the first block
  # so a later cfg_select! (should upstream add one) is never touched. Idempotent.
  if ! grep -qF 'target_os = "dotnet" =>' "$PUSRC/lib.rs"; then
    awk '
      /cfg_select! \{/ && !ins {
        print
        print "    target_os = \"dotnet\" => {"
        print "        #[path = \"gcc.rs\"]"
        print "        mod imp;"
        print "    }"
        ins=1
        next
      }
      { print }' "$PUSRC/lib.rs" > "$PUSRC/lib.rs.__t" && mv "$PUSRC/lib.rs.__t" "$PUSRC/lib.rs"
  fi
fi
# The `unwind` crate (library/unwind/src) is what panic_unwind's gcc.rs imports as `uw` for the
# `_Unwind_*` type/fn DECLARATIONS. Its own flavour cfg_select! also falls through to an empty
# `_ => {}` arm for os=dotnet (no unix/windows/wasm family), leaving `uw::_Unwind_Exception` etc.
# undefined -> E0425/E0422 in gcc.rs. Route the dotnet arm to `libunwind` (pure declarations: the
# `_Unwind_*` types/consts + `extern "C"` blocks). Its `#[link(name = "unwind")]` is inert here —
# the cilly linker overrides `_Unwind_RaiseException`/`_DeleteException`/`_Backtrace` as builtins,
# so the native libunwind is never actually linked into the .NET CIL output. `unwinder_private_data_size`
# resolves to 2 (target_arch = x86_64, not windows), matching the surrogate llvm-target.
UWSRC="$SRC/../../../unwind/src"            # library/unwind/src
if [ -f "$UWSRC/lib.rs" ]; then
  echo "==> injecting dotnet unwind arm ($UWSRC)"
  if ! grep -qF 'target_os = "dotnet" =>' "$UWSRC/lib.rs"; then
    awk '
      /cfg_select! \{/ && !ins {
        print
        print "    target_os = \"dotnet\" => {"
        print "        mod libunwind;"
        print "        pub use libunwind::*;"
        print "    }"
        ins=1
        next
      }
      { print }' "$UWSRC/lib.rs" > "$UWSRC/lib.rs.__t" && mv "$UWSRC/lib.rs.__t" "$UWSRC/lib.rs"
  fi
fi
# The `libc` crate (vendor/libc-0.2.*) is linked into dotnet std (its Cargo dep is
# gated on not(all(windows, msvc)), which includes dotnet), and std's std::os::fd
# files (os/fd/raw.rs, owned.rs) reference a small fixed set of `libc::` symbols
# (close/fcntl/STD*_FILENO/F_DUPFD*). But libc 0.2 has NO module for
# target_os="dotnet": its top-level cfg_if! falls through to an empty `else {}`.
# So with os::fd enabled for dotnet (the unified fd-backed net Socket capstone),
# those `libc::` refs are E0425. Inject a minimal dotnet libc module (extern "C"
# decls the cilly POSIX shim resolves + the consts) into that empty else block.
# os=dotnet-only (the else only fires for unsupported OSes). The PAL file lives at
# dotnet_pal/libc/dotnet.rs. Idempotent (guarded on the dotnet string).
LIBC_PAL="$CD_REPO/dotnet_pal/libc"
# build-std resolves libc from the cargo REGISTRY copy (…/.cargo/registry/src/…/
# libc-0.2.*), NOT the rust-src vendor tree — and the registry copy only exists
# AFTER a build extracts it, so it may not be present on the first invocation
# (handled by the cargo-build-time patch step further down). Patch EVERY libc
# lib.rs we can find (rust-src vendor + registry) so whichever one build-std picks
# is covered. Idempotent (guarded on the dotnet string).
inject_libc() { # $1 = libc src dir
  local d="$1"
  [ -f "$d/lib.rs" ] || return 0
  [ -f "$LIBC_PAL/dotnet.rs" ] || return 0
  cp "$LIBC_PAL/dotnet.rs" "$d/dotnet.rs"
  # PACKAGE A — under the `target-family=["unix"]` flip, `cfg(unix)` is now TRUE
  # for os=dotnet, so libc 0.2 stops falling into its empty `else{}` and instead
  # selects its REAL unix module tree (lib.rs `else if #[cfg(unix)]` -> `mod unix`,
  # plus new/common/posix's `unistd`/`pthread`). That collides with the appended
  # dotnet arm: both glob-export `c_int`/`c_long`/... (263× E0659) and `unistd`
  # re-exports a module not wired for this config (1× E0432). The dotnet arm is
  # the SINGLE intended libc face (LIBC_SHIM_SCOPE / cap2-outcome), so SUPPRESS
  # libc's own unix/posix arms for os=dotnet by excluding dotnet from their cfgs;
  # libc then falls back to the empty `else{}` and the dotnet arm is sole. These
  # three sed patches are the make-or-break AMBER fix and mirror the existing
  # `not(target_os="dotnet")` exclusions in os/fd. Idempotent (the patterns no
  # longer match once rewritten). os=dotnet-only effect (no other target's libc
  # cfg matches the BARE `unix`/`target_family="unix"` predicates we narrow here).
  # 1) lib.rs top-level: `else if #[cfg(unix)]` (the unix module selector).
  sed_i 's/} else if #\[cfg(unix)\] {/} else if #[cfg(all(unix, not(target_os = "dotnet")))] {/' "$d/lib.rs"
  # 2) new/mod.rs per-family headers: `cfg(all(target_family="unix", not(qurt)))`.
  if [ -f "$d/new/mod.rs" ]; then
    sed_i 's/if #\[cfg(all(target_family = "unix", not(target_os = "qurt")))\] {/if #[cfg(all(target_family = "unix", not(target_os = "qurt"), not(target_os = "dotnet")))] {/' "$d/new/mod.rs"
  fi
  # 3) new/common/mod.rs: `#[cfg(target_family = "unix")] pub(crate) mod posix;`.
  if [ -f "$d/new/common/mod.rs" ]; then
    sed_i 's/#\[cfg(target_family = "unix")\]/#[cfg(all(target_family = "unix", not(target_os = "dotnet")))]/' "$d/new/common/mod.rs"
  fi
  grep -qF 'mod dotnet;' "$d/lib.rs" && return 0
  # Declare the dotnet module at the libc crate ROOT (outside the big arch/os
  # cfg_if!), cfg-gated on os=dotnet. Appending after the cfg_if avoids any
  # macro-hygiene quirk of declaring `mod` inside cfg_if's `else` body; the glob
  # re-export then makes `libc::{close, read, c_int, …}` resolve for dotnet.
  #
  # B1: libc is the SINGLE dotnet libc face for BOTH std::os::fd AND mio. Under
  # the `target-family=["unix"]` flip, cfg(unix) is true so libc 0.2 would pick
  # its REAL unix module tree, which collides with the appended dotnet arm (263×
  # E0659 glob dupes + an unwired `unistd` re-export). So libc's own unix/posix
  # arms are SUPPRESSED for os=dotnet (the three flip-suppression seds above), and
  # the dotnet arm stays ON for EVERY libc build, declaring the full epoll/socket/
  # sockaddr surface mio imports (dotnet_pal/libc/dotnet.rs); the POSIX shim
  # resolves the bodies by bare C-ABI name. Gate is plain target_os="dotnet".
  {
    echo ''
    echo '// DOTNET PAL: the single libc face for os=dotnet (see dotnet_pal/libc/dotnet.rs).'
    echo '// libc 0.2 has no module for target_os="dotnet" (its top-level cfg_if! falls'
    echo '// through to an empty else{}); std::os::fd references libc::{close,fcntl,...} and'
    echo '// (Cap-2.5) near-unmodified mio references libc::{epoll_*,socket,sockaddr_*,...}.'
    echo '// One dotnet arm serves both; the mio-scoped wrapper re-cfgs ONLY mio, not libc.'
    echo '#[cfg(target_os = "dotnet")]'
    echo 'mod dotnet;'
    echo '#[cfg(target_os = "dotnet")]'
    echo 'pub use crate::dotnet::*;'
  } >> "$d/lib.rs"
  echo "==> injected dotnet libc module ($d)"
}
# Patch any libc copies present now (rust-src vendor + already-extracted registry).
for d in $(find "$SRC/../../.." "$CD_REGISTRY_SRC" -path '*libc-0.2*/src/lib.rs' 2>/dev/null); do
  inject_libc "$(dirname "$d")"
done

# ===========================================================================
# CRATE-AGNOSTIC BUILD + RUN (the generalization of dev.sh pal-build).
# cwd is the project dir (/project via the docker -w, or the crate's real host
# path under the native driver). NO `cd $CD_REPO/cargo_tests/$DEV_CRATE` — the
# crate is supplied by cwd.
# ===========================================================================
[ -f Cargo.toml ] || { echo "!! no Cargo.toml in $(pwd) — not a crate dir"; exit 2; }
PROFILE=release; CARGOFLAGS=(--release)
if [ "${CD_REL:-1}" = 0 ]; then PROFILE=debug; CARGOFLAGS=(); fi
echo "==> cargo dotnet: building $(pwd) (profile=$PROFILE)"
if [ "${CD_CLEAN:-0}" = 1 ]; then echo "==> cargo clean (full, bulletproof)"; cargo clean; fi
# `--cfg getrandom_backend="custom"` selects getrandom 0.3/0.4's custom backend
# (our os="dotnet" target has no built-in getrandom arm). Harmless for crates that
# don't depend on getrandom (the cfg is simply unused). See dev.sh pal-build.
export RUSTFLAGS="-Z codegen-backend=$CD_BACKEND_DYLIB -C linker=$CD_LINKER -C link-args=--cargo-support --cfg getrandom_backend=\"custom\""
set +e
# PHASE G — CENTRAL OVERLAY REGISTRY auto-apply. Three load-bearing crates (mio,
# socket2, tokio) need a small, marked source overlay to build/run on os=dotnet
# that NO cfg flip can supply. The overlays live ONCE under /work/dotnet_overlays/
# {mio,socket2,tokio} (REGISTRY.toml lists name/version/dir). We auto-redirect any
# present in this project's dep graph via a generated `.cargo/config.toml`
# `paths = [...]` override — the user's tracked Cargo.toml is NEVER touched, and
# `paths` is graph-wide by crate NAME so it covers a TRANSITIVE dep (mio under
# tokio). A `paths` entry whose crate isn't in the graph (or whose version doesn't
# satisfy the locked requirement) is silently ignored by cargo, so emitting all
# overlays is safe; we ALSO parse the lock afterwards and warn loudly on a
# name-match/version-mismatch (the "overlay silently not applied" footgun).
apply_overlays() { # cwd = the project dir; reads $CD_REPO/dotnet_overlays/REGISTRY.toml
  local reg="$CD_REPO/dotnet_overlays/REGISTRY.toml"
  [ -f "$reg" ] || { echo "==> no overlay registry at $reg (skipping auto-apply)"; return 0; }
  # Parse REGISTRY.toml [[overlay]] blocks into parallel name/version/dir arrays.
  local names vers dirs
  names=$(awk -F'"' '/^name *=/{print $2}' "$reg")
  vers=$(awk -F'"' '/^version *=/{print $2}' "$reg")
  dirs=$(awk -F'"' '/^dir *=/{print $2}' "$reg")
  # Build the paths list (one dir per overlay; cargo ignores entries not in graph).
  local paths_lines="" d
  while IFS= read -r d; do
    [ -n "$d" ] || continue
    [ -d "$CD_REPO/dotnet_overlays/$d" ] || { echo "!! overlay dir missing: $CD_REPO/dotnet_overlays/$d"; continue; }
    paths_lines="$paths_lines    \"$CD_REPO/dotnet_overlays/$d\",
"
  done <<< "$dirs"
  # Regenerate .cargo/config.toml FROM SCRATCH (idempotent): preserve the dotnet
  # target + build-std, then append the top-level `paths` override. Paths are
  # resolved relative to the dir containing .cargo/, so absolute $CD_REPO paths.
  mkdir -p .cargo
  # NOTE: `paths` is a TOP-LEVEL config key (a local-source override), NOT an
  # `[unstable]` or `[build]` sub-key — it MUST be emitted before any table header
  # or cargo silently ignores it (`unused config key`), the override no-ops, and
  # the crate resolves unpatched from the registry. Emit it first.
  {
    echo '# GENERATED by feasibility/cargo-dotnet (apply_overlays) — do not hand-edit.'
    echo '# (Phase G: the dotnet_overlays paths-override; Phase D: cargo dotnet.)'
    echo 'paths = ['
    printf '%s' "$paths_lines"
    echo ']'
    echo '[build]'
    echo "target = \"$CD_TARGET_SPEC\""
    echo '[unstable]'
    echo 'build-std = ["core", "alloc", "std", "panic_unwind"]'
  } > .cargo/config.toml
  echo "==> regenerated .cargo/config.toml with dotnet_overlays paths override"
  # Ensure a Cargo.lock exists so cargo pins versions (and so we can verify them).
  # The `paths` override participates in resolution, so the lock pins the overlay
  # versions. Run generate-lockfile only if absent (keeps an existing lock stable).
  [ -f Cargo.lock ] || cargo -Zjson-target-spec generate-lockfile >/dev/null 2>&1 || true
  # Verify each overlay's declared version against the locked version; warn loudly
  # on a mismatch (cargo would silently ignore the override -> miscompile risk).
  if [ -f Cargo.lock ]; then
    local n v locked
    paste <(echo "$names") <(echo "$vers") | while IFS=$'\t' read -r n v; do
      [ -n "$n" ] || continue
      locked=$(awk -v want="$n" '
        $1=="name" && $3=="\""want"\""{f=1; next}
        f && $1=="version"{gsub(/"/,"",$3); print $3; f=0}
        $1=="name"{f=0}' Cargo.lock | head -1)
      if [ -z "$locked" ]; then
        : # overlay crate not in this project's graph — fine, paths entry is inert.
      elif [ "$locked" = "$v" ]; then
        echo "==> overlay APPLIED: $n $v (locked $locked) -> $CD_REPO/dotnet_overlays/$n"
      else
        echo "!! OVERLAY VERSION MISMATCH: $n overlay=$v but Cargo.lock pins $locked"
        echo "!! cargo will IGNORE the paths override for $n (footgun: overlay NOT applied)."
        echo "!! Fix: refresh dotnet_overlays/$n to $locked + bump REGISTRY.toml (see README)."
      fi
    done
  fi
}
apply_overlays
# build-std resolves libc from the cargo REGISTRY (not the rust-src vendor tree),
# which is extracted on first download. `cargo fetch` materialises the registry
# sources WITHOUT compiling, so we can patch the registry libc copy before it is
# compiled. (Without this, the first compile of an unpatched registry libc fails
# on the std::os::fd `libc::` refs.) Idempotent — inject_libc is a no-op on copies
# already carrying the dotnet module.
cargo -Zjson-target-spec fetch >/dev/null 2>&1 || true
for d in $(find "$CD_REGISTRY_SRC" -path '*libc-0.2*/src/lib.rs' 2>/dev/null); do
  inject_libc "$(dirname "$d")"
done
# Build. Filter the log like dev.sh unless CD_VERBOSE=1 (errors always shown).
if [ "${CD_VERBOSE:-0}" = 1 ]; then
  cargo -Zjson-target-spec build "${CARGOFLAGS[@]}" 2>&1 | tee "$CD_LASTBUILD_LOG"
  rc=${PIPESTATUS[0]}
else
  cargo -Zjson-target-spec build "${CARGOFLAGS[@]}" 2>&1 | tee "$CD_LASTBUILD_LOG" \
    | grep -vE 'discirminant' \
    | grep -E '^error|error\[|could not compile|warning: unused|Compiling (std|core|alloc) |Finished' | head -60
  rc=${PIPESTATUS[0]}
fi
echo "== build exit: $rc =="
# Derive the produced artifact path(s) from cargo's JSON message stream — the most
# reliable source, already produced by the build. No jq/python3 (neither is in the
# rcc-dev image). Two artifact kinds:
#   * an EXECUTABLE (a bin crate)        -> the `"executable":"…"` field. Run it.
#   * a LIBRARY (cdylib/dylib/staticlib) -> a compiler-artifact whose target
#     crate_types includes cdylib/dylib/staticlib; its `"filenames":[…]` lists the
#     produced `.so`. The dotnet target has dynamic-linking:true, so a
#     crate-type=["cdylib"] makes cargo pass `-o …/lib<crate>.so` and the cilly
#     linker (is_lib keys on the .so) writes a referenceable .NET PE there. C# has
#     no entrypoint to run, so we copy that .so to `<crate>.dll` beside it for a
#     direct `<Reference>`/<HintPath> and DON'T try to run it.
# We capture the JSON stream ONCE (rebuilding twice is wasteful and was the old
# shape's only inefficiency) and extract both kinds from it.
out=""        # executable apphost path (bin crate)
libout=""     # produced library .so path (cdylib/dylib/staticlib crate)
libdll=""     # the <crate>.dll copy beside the .so (C#-referenceable)
if [ "$rc" = 0 ]; then
  jsonlog=$(cargo -Zjson-target-spec build "${CARGOFLAGS[@]}" --message-format=json 2>/dev/null)
  # (1) executable apphost: LAST non-null "executable":"…" field.
  out=$(printf '%s\n' "$jsonlog" \
        | awk 'match($0, /"executable":"[^"]+"/){ s=substr($0,RSTART+14,RLENGTH-15); if (s!="null" && s!="") last=s } END{ if(last!="") print last }')
  # awk above strips the leading `"executable":"` (14 chars) and trailing `"`.
  if { [ -z "$out" ] || [ ! -f "$out" ]; }; then
    # No executable -> maybe a bin crate cargo metadata can name, else a LIBRARY.
    # (2) library .so: a compiler-artifact JSON line whose target lists a
    # cdylib/dylib/staticlib crate-type; pull a `.so`/`.dll`/`.dylib` from its
    # "filenames" array. One awk pass per line (the whole artifact JSON is on one
    # line in the cargo stream), keying on the crate_types presence.
    libout=$(printf '%s\n' "$jsonlog" \
      | awk '
          /"reason":"compiler-artifact"/ &&
          (/"cdylib"/ || /"dylib"/ || /"staticlib"/) {
            # extract the first "filenames":["…"] entry that looks like a shared obj
            if (match($0, /"filenames":\[[^]]*\]/)) {
              fns=substr($0,RSTART,RLENGTH)
              n=split(fns, parts, "\"")
              for (i=1;i<=n;i++) if (parts[i] ~ /\.(so|dll|dylib)$/) last=parts[i]
            }
          }
          END{ if(last!="") print last }')
    if [ -z "$out" ] || [ ! -f "$out" ]; then out=""; fi
    if [ -n "$libout" ] && [ -f "$libout" ]; then
      # The .so PE's real .NET assembly identity is `<crate>` regardless of the .so
      # filename, so the .dll is a pure FILE COPY (never a transmute). Derive the
      # crate name from the lib<crate>.so stem (strip dir, the cargo `lib` prefix,
      # and the extension) and copy beside the .so so a C# <Reference>/<HintPath>
      # (HintPath=<crate>.dll) resolves it. cp -f overwrites a stale copy.
      libstem=$(basename "$libout"); libstem="${libstem%.*}"; libstem="${libstem#lib}"
      libdll="$(dirname "$libout")/${libstem}.dll"
      cp -f "$libout" "$libdll"
      echo "== lib PE: $libout -> $libdll (assembly '$libstem') =="
    fi
    # Bin fallback ONLY if neither an executable nor a library was found (an
    # arbitrary bin crate whose JSON 'executable' field cargo left null).
    if [ -z "$out" ] && [ -z "$libout" ]; then
      bin=$(cargo metadata --no-deps --format-version 1 2>/dev/null \
            | tr ',' '\n' | awk -F'"' '/"kind":\["bin"\]/{want=1} want && /"name":/{print $4; exit}')
      [ -z "$bin" ] && bin="$(basename "$(pwd)")"
      # Probe both the bare name (Linux/macOS apphost) and the host exe suffix
      # ($CD_EXE_EXT = ".exe" on Windows; "" elsewhere, so these collapse to the
      # bare-name probes and the docker/Linux behaviour is unchanged).
      out=""
      for cand in \
        "target/x86_64-unknown-dotnet/$PROFILE/$bin$CD_EXE_EXT" \
        "target/x86_64-unknown-dotnet/$PROFILE/$bin" \
        "target/dotnet/$PROFILE/$bin$CD_EXE_EXT" \
        "target/dotnet/$PROFILE/$bin"; do
        if [ -n "$cand" ] && [ -f "$cand" ]; then out="$cand"; break; fi
      done
    fi
  fi
fi
if [ "$rc" != 0 ]; then exit "$rc"; fi
if [ "${CD_RUN:-0}" = 1 ]; then
  if [ -z "$out" ] && [ -n "$libdll" ]; then
    # A library has no entrypoint — refuse to run it, but cleanly (exit 0): the
    # build SUCCEEDED, the user just asked to run a non-runnable artifact.
    echo "cargo dotnet run: '$libstem' is a LIBRARY (no entrypoint) — reference $(basename "$libdll") from a C# project (see docs/INTEROP_CSHARP.md)"
    exit 0
  fi
  [ -n "$out" ] && [ -f "$out" ] || { echo "!! cargo dotnet run: no runnable apphost found (looked for an executable artifact)"; exit 3; }
  echo "== RUN $out =="
  "$out" "$@"
  prc=$?
  echo "run exit: $prc"
  exit "$prc"
fi
if [ -n "$libdll" ]; then
  echo "== built lib: $libout (referenceable as $(basename "$libdll")) =="
else
  echo "== built: ${out:-<no bin artifact>} =="
fi
exit 0
