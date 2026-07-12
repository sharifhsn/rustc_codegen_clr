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

## Signed, immutable NuGet releases

Signing is an explicit release mode. It requires a release build, package validation, a clean Git
revision, an exact non-placeholder SemVer, embedded provenance, a PFX certificate, and an expected
SHA-256 signer fingerprint. Passwords and feed tokens are accepted only through a named environment
variable; their values are never printed or written to receipts. The checksum and package receipt
are generated only after `dotnet nuget sign` and fingerprint-constrained `dotnet nuget verify`
succeed:

```bash
export NUGET_CERT_PASSWORD='...'
cargo dotnet pack ./mylib --release --validate --version 2.3.4 \
  --sign-certificate release.pfx --sign-password-env NUGET_CERT_PASSWORD \
  --signer-fingerprint '<64 hex digits>' --timestamper 'https://timestamp.example'

export NUGET_API_KEY='...'
cargo dotnet push ./mylib/target/nupkg/MyLib.2.3.4.nupkg \
  --source 'https://feed.example/v3/index.json' --api-key-env NUGET_API_KEY \
  --signer-fingerprint '<64 hex digits>'
```

`push` verifies the package signature again, reads the exact version from the nuspec, and never uses
`--skip-duplicate` or overwrite behavior. A duplicate exact version is therefore a hard failure. On
success it writes a non-secret publish receipt beside the package.

The trusted-publishing boundary starts outside this tool: CI or the operator owns certificate/key
issuance, secret storage, feed authentication, timestamp-service trust, and source authorization.
This tool only enforces how those credentials and identities are consumed. Published NuGet versions
are immutable; rollback means restoring/pinning a previously published signed version (or publishing
a new corrective version), never replacing or deleting bytes at an existing version.

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
