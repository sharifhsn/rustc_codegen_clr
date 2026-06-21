#!/usr/bin/env bash
# feasibility/rcc-rustc-wrapper.sh — crate-scoped cfg injection for the Cap-2
# libc-shim capstone. Cargo invokes `$RUSTC_WRAPPER <rustc> <args...>`.
#
# THE PROBLEM: unmodified upstream `mio` selects its readiness backend by
# `target_os` — only `{android, illumos, linux, redox}` get `selector/epoll.rs`
# + `waker/eventfd.rs`. And `libc` only exposes `epoll_*`/`sockaddr_*`/`EPOLL*`
# under its `target_os="linux"` (gnu) module. Our target is `os="dotnet"` (no
# target_family until the Cap-2 `families=["unix"]` flip, and never linux), so
# without help neither crate picks an epoll path and mio fails to compile.
#
# THE FIX (surgical, NOT global): for the `mio` and `libc` CRATES ONLY, ADD
# `--cfg target_os="linux" --cfg target_env="gnu" --cfg unix` to the rustc
# invocation. `target_os`/`target_env` are MULTI-VALUED, so this layers linux/gnu
# ALONGSIDE the spec's dotnet/"" — mio then matches its linux epoll selector +
# eventfd waker, and libc compiles its real linux/gnu module (epoll_create1/ctl/
# wait, eventfd, epoll_event, sockaddr_in/in6/un, EPOLL*/EFD*/AF_*/SOCK_*/SO_*).
# Every OTHER crate (std, core, alloc, the user crate) is passed through
# UNCHANGED, so it stays pristine `os="dotnet"`. The POSIX shim
# (cilly/src/ir/builtins/posix*.rs) provides the BODIES for those libc decls;
# this wrapper just unlocks the DECLS + mio's selector choice.
#
# GUARD: only inject when `--target` is in argv. cargo compiles each crate's
# build script for the HOST first (no --target); that host compile of mio/libc
# must NOT get the cross cfgs.
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

if [ "$has_target" = 1 ] && { [ "$crate_name" = "mio" ] || [ "$crate_name" = "libc" ]; }; then
  exec "$rustc" "$@" \
    --cfg unix \
    --cfg 'target_os="linux"' \
    --cfg 'target_env="gnu"'
fi

exec "$rustc" "$@"
