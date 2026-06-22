#!/usr/bin/env bash
# feasibility/rcc-rustc-wrapper.sh — crate-scoped cfg injection for the Cap-2.5
# libc-shim capstone. Cargo invokes `$RUSTC_WRAPPER <rustc> <args...>`.
#
# THE PROBLEM: unmodified upstream `mio` selects its readiness backend by
# `target_os` — only `{android, illumos, linux, redox}` get `selector/epoll.rs`,
# and its whole unix `sys`/`io_source` surface is gated on `cfg(unix)`. Our target
# is `os="dotnet"` (no target_family, never linux), so without help mio picks no
# epoll path and fails to compile.
#
# THE FIX (surgical, NOT global): for the `mio` CRATE ONLY, ADD
# `--cfg unix --cfg target_os="linux"` to the rustc invocation. `target_os` is
# MULTI-VALUED, so this layers linux ALONGSIDE the spec's dotnet — mio then
# matches its `#[cfg(unix)]` sys arm + `selector/epoll.rs` and drives its epoll
# path through `libc::epoll_*`/`socket`/... . Every OTHER crate (std, core, alloc,
# libc, the user crate) is passed through UNCHANGED, so it stays pristine
# `os="dotnet"`.
#
# WHY NOT libc: forcing libc's real linux module while `target_os="dotnet"` is
# ALSO active makes libc 0.2's `new/` module tree inconsistent — the gnu-gated
# `pub use net::route::*` + the `prelude!()` base-type imports (`c_int`/...) fail
# under a multi-valued `target_os` (verified: E0433/E0432). So libc stays on its
# `target_os="dotnet"` arm for EVERY build; that single dotnet arm
# (`dotnet_pal/libc/dotnet.rs`) is the libc face for BOTH std::os::fd AND mio,
# declaring the epoll/socket/sockaddr surface mio imports. The POSIX shim
# (cilly/src/ir/builtins/posix*.rs) provides the BODIES by bare C-ABI symbol name;
# this wrapper just unlocks mio's selector + cfg(unix) arm choice.
#
# GUARD: only inject when `--target` is in argv. cargo compiles each crate's
# build script for the HOST first (no --target); that host compile of mio must
# NOT get the cross cfgs.
#
# Wired by `export RUSTC_WRAPPER=/work/feasibility/rcc-rustc-wrapper.sh` in
# dev.sh's pal-build env. Idempotent / stateless.

# $1 is the real rustc; the rest are its args.
rustc="$1"
shift

crate_name=""
has_target=0
prev=""
for arg in "$@"; do
  case "$arg" in
    --crate-name=*) crate_name="${arg#--crate-name=}" ;;
    --target|--target=*) has_target=1 ;;
  esac
  # space-separated form: `--crate-name foo`
  if [ "$prev" = "--crate-name" ]; then crate_name="$arg"; fi
  if [ "$prev" = "--target" ]; then has_target=1; fi
  prev="$arg"
done

if [ "$has_target" = 1 ] && [ "$crate_name" = "mio" ]; then
  # `unix`/`target_os` are built-in cfgs normally controlled only by --target;
  # this nightly's `explicit_builtin_cfgs_in_flags` lint denies setting them via
  # --cfg. We deliberately layer them (mio-only) without the global target-family
  # flip, so downgrade that lint to allow for THIS crate's compile.
  exec "$rustc" "$@" \
    -A explicit_builtin_cfgs_in_flags \
    --cfg unix \
    --cfg 'target_os="linux"'
fi

exec "$rustc" "$@"
