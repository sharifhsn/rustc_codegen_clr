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

But a handful of load-bearing crates need a *source* edit that no cfg flip can
supply, and the edits are **heterogeneous** — there is no single trick:

| crate | overlay shape | why a cfg flip can't do it |
|---|---|---|
| `mio` | ~22 `#[cfg(target_os="dotnet")]` arm lines across 5 files + 1 new `waker/dotnet.rs` | mio selects its readiness backend/waker by `target_os`, and has no `dotnet` concept; the dotnet waker is a new file |
| `socket2` | 1 crate-INTERNAL `type IovLen = libc::size_t` alias | a private type alias inside the crate — nothing external can inject it |
| `tokio` | 2 lines (a `cfg_net_unix!` gate + a `memchr` fallback arm) | excludes dotnet from the AF_UNIX/`unix::pipe` surface and from the libc memchr arm |
| `getrandom` ×3 (0.2/0.3/0.4) | 1 new `dotnet` backend module + 1 marked `target_os="dotnet"` arm in the backend cascade, per major | getrandom hard-`compile_error!`s on os=dotnet (no built-in arm); the overlay IS getrandom (patched), so it DEFINES the entropy backend internally (calls the PAL CSPRNG `rcl_dotnet_random_fill`) — NO consumer symbol/macro/feature, NO `getrandom_dotnet` dep |

So the generalization is a **bundled overlay registry the framework auto-applies**,
not a flag. Each overlay is stored ONCE here and serves every consuming project
(direct or transitive — e.g. `mio` is pulled in by `tokio`; `getrandom` by
`rand`/`uuid`/`ahash`).

### getrandom: the multi-version case (same NAME, three majors)

getrandom is the first overlaid crate with **multiple coexisting majors** —
`rand`→`getrandom 0.2`, `ahash`→`0.3`, `uuid`→`0.4`, and all three can be live in
one dependency graph at once. The registry therefore carries **three** `[[overlay]]`
entries all named `getrandom` (versions 0.2.17 / 0.3.4 / 0.4.3, dirs
`getrandom-0.2` / `getrandom-0.3` / `getrandom-0.4`).

The auto-apply step handles this with a **single-version-per-name rule**: cargo's
`paths` override picks exactly ONE version per crate NAME for the whole graph (with
several same-name dirs it takes the highest and warns *"path override … has altered
the original list of dependencies"*, which would wrongly link e.g. rand_core against
the 0.4 overlay). So for a name with multiple overlays, the framework generates the
`Cargo.lock` FIRST and then emits **only the overlay dir whose version is locked** in
this project — turning each multi-major case back into the proven-reliable
single-version override that mio/socket2/tokio use. (Single-overlay names are always
emitted; cargo ignores entries whose crate isn't in the graph.)

Once the overlay supplies a real dotnet backend, **no consumer wiring is needed at
all** — `rand`/`uuid`/`ahash` just build. There is also no `--cfg
getrandom_backend="custom"` RUSTFLAG (it was removed: for 0.3/0.4 the `custom` arm is
the FIRST branch of getrandom's cascade, so the cfg would win over the overlay's
dotnet arm and pull the now-undefined `__getrandom_v03_custom` extern → link error).

## Layout

```
dotnet_overlays/
  REGISTRY.toml   # data-driven manifest the auto-apply step reads (name/version/dir)
  README.md       # this file
  mio/            # full crate: upstream mio 1.2.1 + the marked dotnet arms
  socket2/        # full crate: upstream socket2 0.6.4 + the 1 marked line
  tokio/          # full crate: upstream tokio 1.52.3 + the 2 marked lines
  getrandom-0.4/  # full crate: upstream getrandom 0.4.3 + the dotnet backend arm
  getrandom-0.3/  # full crate: upstream getrandom 0.3.4 + the dotnet backend arm
  getrandom-0.2/  # full crate: upstream getrandom 0.2.17 + the dotnet backend arm
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

**getrandom 0.4.3 / 0.3.4** (`getrandom-0.4/src/`, `getrandom-0.3/src/`) — these two
majors are STRUCTURALLY IDENTICAL; the dotnet backend file is byte-identical between
them:
- `src/backends/dotnet.rs`: NEW — exports `fill_inner(dest: &mut [MaybeUninit<u8>])`
  over the PAL CSPRNG (`extern "C" { fn rcl_dotnet_random_fill(ptr, len); }`) plus
  `pub use crate::util::{inner_u32, inner_u64}` (the backend contract lib.rs calls).
  Infallible (RandomNumberGenerator.Fill always succeeds) → always `Ok(())`, so a
  `getrandom::Error` is never constructed.
- `src/backends.rs`: one `// DOTNET PAL`-marked `else if #[cfg(target_os = "dotnet")]`
  arm, placed AFTER the last `getrandom_backend="…"` opt-in arm (so an explicit cfg
  still wins) and BEFORE the first per-os arm + the `compile_error!` tail.

**getrandom 0.2.17** (`getrandom-0.2/src/`) — the older single-`imp` model; the
contract differs (`getrandom_inner`, no `inner_u32`/`inner_u64`):
- `src/dotnet.rs`: NEW — exports `getrandom_inner(dest: &mut [MaybeUninit<u8>])` over
  the same PAL CSPRNG (modelled on `fuchsia.rs`), using
  `crate::util::uninit_slice_fill_zero`. Always `Ok(())`.
- `src/lib.rs`: one `// DOTNET PAL`-marked `if #[cfg(target_os = "dotnet")] { #[path =
  "dotnet.rs"] mod imp; }` as the FIRST arm of the imp-selector `cfg_if!`, so it
  pre-empts the late `feature = "custom"` arm regardless of feature unification. (0.2
  uses the `custom` FEATURE, not the `getrandom_backend` cfg — editing the in-crate
  cascade sidesteps the feature+macro entirely, so NO consumer wiring is needed; this
  is more than the external custom hatch can do.)
