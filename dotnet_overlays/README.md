# dotnet_overlays — the central crate-overlay registry for the .NET target

This directory is the **single source of truth** for the small per-crate source
overlays a few popular crates need to compile/run on the `x86_64-unknown-dotnet`
target. The build framework (the `cargo dotnet` one-command DX —
`feasibility/cargo-dotnet`, shared with `feasibility/dev.sh pal-build` via the
common pipeline core `feasibility/_cargo_dotnet_core.sh`) **auto-applies** these
overlays from here — a downstream user adds **nothing** to their own `Cargo.toml`.

## Why this exists (the honest constraint)

`cfg(unix)` crates already work **unpatched** on the .NET target: the spec carries
`target-family = ["unix"]`, so `cfg(unix)`/`cfg(target_family="unix")` are true and
those crates pick their existing unix arms straight onto the .NET PAL.

But three load-bearing crates need a *source* edit that no cfg flip can supply,
and the three edits are **heterogeneous** — there is no single trick:

| crate | overlay shape | why a cfg flip can't do it |
|---|---|---|
| `mio` | ~22 `#[cfg(target_os="dotnet")]` arm lines across 5 files + 1 new `waker/dotnet.rs` | mio selects its readiness backend/waker by `target_os`, and has no `dotnet` concept; the dotnet waker is a new file |
| `socket2` | 1 crate-INTERNAL `type IovLen = libc::size_t` alias | a private type alias inside the crate — nothing external can inject it |
| `tokio` | 2 lines (a `cfg_net_unix!` gate + a `memchr` fallback arm) | excludes dotnet from the AF_UNIX/`unix::pipe` surface and from the libc memchr arm |

So the generalization is a **bundled overlay registry the framework auto-applies**,
not a flag. Each overlay is stored ONCE here and serves every consuming project
(direct or transitive — e.g. `mio` is pulled in by `tokio`).

## Layout

```
dotnet_overlays/
  REGISTRY.toml   # data-driven manifest the auto-apply step reads (name/version/dir)
  README.md       # this file
  mio/            # full crate: upstream mio 1.2.1 + the marked dotnet arms
  socket2/        # full crate: upstream socket2 0.6.4 + the 1 marked line
  tokio/          # full crate: upstream tokio 1.52.3 + the 2 marked lines
```

Each overlay is a **full crate directory** (upstream byte-identical except the
`// DOTNET PAL`-marked lines), NOT a diff/patch file. This makes auto-apply a
trivial `paths` pointer with no extract/patch/3-way-merge machinery and no
"patch failed to apply" failure mode. Every overlay's `Cargo.toml` is the
upstream (cargo-vendor-normalized) manifest with the **same dependency set** as
crates.io — a precondition for the `paths` override (a `paths` override may not
change the overridden crate's dep set).

## How auto-apply works (the `paths` override)

`cargo dotnet build/run` (and `dev.sh pal-build`, which delegates to it) runs
`apply_overlays` in the shared pipeline core (`feasibility/_cargo_dotnet_core.sh`):

1. Ensure the project has a `Cargo.lock` (generate one if absent).
2. Parse each `[[overlay]]` from `REGISTRY.toml` (name + version + dir).
3. For every overlay whose `name` appears in the project's `Cargo.lock` with a
   **matching version**, add `/work/dotnet_overlays/<dir>` to a paths list.
   On a name match with a **version mismatch**, warn loudly and skip (the
   "overlay silently not applied -> miscompile" footgun).
4. Regenerate the project's `.cargo/config.toml` from scratch (idempotent):
   preserve `[build].target` + `[unstable].build-std`, then add a top-level
   `paths = [ ... ]`.

`paths` is keyed by crate **name** and is graph-wide, so one entry covers both a
direct dep (`mio` in `pal_mio`) and a transitive dep (`mio` under `tokio` in
`pal_tokio_net`). It needs **zero** edits to the user's tracked `Cargo.toml`
(unlike `[patch.crates-io]`, which is illegal in `.cargo/config.toml` and must
live in a manifest).

## Recipe: add a new crate overlay

1. Vendor the exact pinned upstream version into `dotnet_overlays/<crate>/`
   (`cargo vendor`, or copy the registry source). Keep its `Cargo.toml`
   byte-identical to upstream — do not change the dep set, or `paths` will refuse.
2. Apply the minimal source edit(s). Mark **every** dotnet-specific line with a
   `// DOTNET PAL` comment and a one-line rationale, so a future upstream refresh
   re-applies only the marked lines.
3. Add a `[[overlay]]` block to `REGISTRY.toml`: `name`, the upstream `version`
   you vendored, and `dir`.
4. Pin the consuming project's `Cargo.lock` to that same version (cargo does this
   on resolve). If a project locks a different version, bump the overlay (next
   recipe) — a version mismatch makes `paths` warn-and-skip, never silently apply.

## Recipe: refresh an overlay to a newer upstream

1. Re-vendor the new upstream version over `dotnet_overlays/<crate>/`.
2. Re-apply ONLY the `// DOTNET PAL`-marked lines (grep the old tree for the
   marker; they are deliberately small and isolated).
3. Bump `version` in `REGISTRY.toml`.
4. Re-resolve the consuming projects so their `Cargo.lock` pins the new version.

## Dormant: the treat-as-linux wrapper (selector crates) — NOT in the build path

For the day a *future* crate keys its backend purely on `target_os` (a "selector"
crate) and the overlay arms above aren't enough, a crate-scoped `RUSTC_WRAPPER`
can present it as Linux. It is **not needed** for mio/socket2/tokio (the global
`target-family=["unix"]` flip + the baked mio selector arm cover them), so it is
shipped dormant, documented here, not wired into `dev.sh`.

- **Recover the original wrapper:** `git show 5dfc51b:feasibility/rcc-rustc-wrapper.sh`
  (commit `5dfc51b`, the Cap-2.5 wrapper).
- **Generalize it:** replace the hardcoded `crate_name == "mio"` test with
  membership against a `SELECTOR_CRATES` list (env var or a `REGISTRY.toml`
  `selector_crates = [...]` key).
- **Inject for listed crates only:**
  `-A explicit_builtin_cfgs_in_flags --cfg unix --cfg target_os=linux --cfg target_env=gnu`.
  **libc is EXCLUDED** — forcing libc's linux module under a multi-valued
  `target_os` breaks libc 0.2's module tree (E0433/E0432). The nightly DENY of
  `--cfg` builtins (`explicit_builtin_cfgs_in_flags`) means these cfgs can only be
  set via the wrapper + `-A`, or via the target spec.
- **Auto-detect (honest form):** on a build failure whose
  `feasibility/_lastbuild.log` names a crate via E0432/E0433/E0583/E0412 in a
  `target_os` cascade that has NO overlay, print a one-line actionable hint
  ("crate X needs a dotnet_overlays/X overlay — see the recipe") and stop. A
  bounded retry-loop that fabricates cfgs is the wrong move under the flip; the
  correct auto-detect surfaces "this crate needs an overlay."

## The overlays, line by line

**mio 1.2.1** (`mio/src/`) — backend selection has no `dotnet` concept:
- `sys/unix/mod.rs`: `#[cfg_attr(target_os="dotnet", path="selector/epoll.rs")]`
  selector arm; `target_os="dotnet"` waker arm (`waker/dotnet.rs`); uds + pipe
  gated `not(target_os="dotnet")`.
- `sys/unix/net.rs`: `target_os="dotnet"` added to the `SOCK_NONBLOCK|SOCK_CLOEXEC`
  list (atomic non-blocking socket creation; the shim honours both).
- `sys/unix/tcp.rs`: `target_os="dotnet"` added to the `accept4` list.
- `net/mod.rs` + `lib.rs`: uds + pipe modules gated off for dotnet.
- `sys/unix/waker/dotnet.rs`: NEW — an `OwnedFd`-backed eventfd waker (the stock
  eventfd waker needs an fd-backed `std::fs::File`, which the dotnet
  GCHandle/FileStream `File` is not yet).

**socket2 0.6.4** (`socket2/src/sys/unix.rs`): one crate-internal line —
`#[cfg(target_os="dotnet")] type IovLen = libc::size_t;` (dotnet matches none of
socket2's named-OS `IovLen` arms; an external libc widening cannot supply it).

**tokio 1.52.3** (`tokio/src/`):
- `macros/cfg.rs`: `cfg_net_unix!` excludes dotnet (AF_UNIX + `unix::pipe` need
  mio's uds + an fd-backed `fs::File`; pal_tokio_net is TCP-only).
- `util/memchr.rs`: both arms get `not(target_os="dotnet")` so dotnet routes to
  the pure-Rust memchr fallback (the dotnet libc face does not model `memchr`).
