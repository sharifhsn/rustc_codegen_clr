# cargo-dotnet

The `cargo install`-able **clap Rust binary** for `cargo dotnet …` — compile and run arbitrary Rust
crates on .NET via the [`rustc_codegen_clr`](../..) backend. It replaces the previous bash front-end
(`feasibility/cargo-dotnet`) for end users.

```bash
cargo install --path tools/cargo-dotnet     # -> ~/.cargo/bin/cargo-dotnet => `cargo dotnet …`
cargo dotnet --version
cargo dotnet run  ./mycrate --features foo -p mycrate --locked -- --port 8080
cargo dotnet build ./mylib                  # cdylib -> a C#-referenceable .dll
cargo dotnet setup --from-repo <repo>       # provision toolchain + backend + install home
cargo dotnet pack ./mylib                   # -> a NuGet .nupkg of the .NET assembly
```

This crate is its **own workspace** (`[workspace]` in `Cargo.toml`) so it builds with host
stable/nightly cargo — NOT the pinned nightly + `rustc_private` the backend crates need.

## What is Rust-native vs. shells-to-bash

The **native backend is entirely Rust** — there is no bash core on the user path. The CLI, the
cargo-subcommand convention, the standard-flag passthrough, mode/host detection, AND every inner stage:

| stage | module | replaces (bash) |
|-------|--------|-----------------|
| PAL injection into rust-src | [`palinject`](src/palinject.rs) (declarative manifest + idempotent, anchor-based, unit-tested engine) | `inject_arm`/`inject_arm_anchor`/`inject_method`/`inject_libc` + the BSD/GNU `sed -i` shim |
| typed config (no `CD_*` env) | [`context`](src/context.rs) | the ~13 `CD_*` env vars |
| `dotnet_overlays` apply | [`overlays`](src/overlays.rs) (`toml`) | `apply_overlays` (awk/paste) |
| `build-std` | [`buildstd`](src/buildstd.rs) | the build block + the twice-run libc patch |
| artifact location | [`artifact`](src/artifact.rs) (`serde_json`) | the awk JSON scrape |
| run apphost | [`run`](src/run.rs) | the run block |
| NuGet `.nupkg` | [`pack`](src/pack.rs) (`zip` crate) | `cd_pack` (`zip -X` + heredocs + `uuidgen`) |

It shells out only to external tools any build tool must (cargo/rustc/ilasm/dotnet/the linker). The
self-containment is **verified with `feasibility/_cargo_dotnet_core.sh` physically absent** (J1/J2/J3 +
`pack` all pass). `setup` runs the native [`palinject`] warm and `cargo install`s this binary; its heavy
external-tool provisioning (rustup/dotnet-install/ilasm/backend build, all dev-only `--from-repo`) still
delegates to the bash front-end. The **Docker** dev backend likewise delegates to the bash front-end,
which owns the container mount model.

See [docs/CARGO_DOTNET.md](../../docs/CARGO_DOTNET.md) for the full user guide and
[docs/RESEARCH_CARGO_DOTNET.md](../../docs/RESEARCH_CARGO_DOTNET.md) for the audit this implements.
