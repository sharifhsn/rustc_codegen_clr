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

The CLI, the cargo-subcommand argument convention (`cargo dotnet <cmd>` dispatch), the standard-flag
passthrough (`--features`/`-p`/`--manifest-path`/`--locked`/…), `--version`/`--help`, mode + host
detection, the RUSTFLAGS assembly, and the build/run env orchestration are **Rust**. The inner pipeline
(PAL injection into rust-src, the `dotnet_overlays` apply, the libc-registry patch, build-std, artifact
location, the run) is the proven shell core (`feasibility/_cargo_dotnet_core.sh`), which the Rust binary
drives via the `CD_*` env seam (`CD_EXTRA_CARGO_FLAGS` carries the forwarded standard flags). `setup`,
`pack`, and the Docker dev backend are staged on the bash front-end (clearly marked in the source);
`setup`'s one native upgrade is `cargo install`-ing this binary instead of copying a script.

See [docs/CARGO_DOTNET.md](../../docs/CARGO_DOTNET.md) for the full user guide and
[docs/RESEARCH_CARGO_DOTNET.md](../../docs/RESEARCH_CARGO_DOTNET.md) for the audit this implements.
