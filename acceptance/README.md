# Product acceptance

This directory describes the user journeys exercised by CI and release builds. The public 0.0.1
contract is intentionally small:

- .NET 10
- Linux x64, macOS Apple Silicon, and Windows x64
- release builds for published artifacts
- the Rust nightly pinned in `rust-toolchain.toml`

`capabilities.toml` maps each journey to its fixture, command, expected completion marker, and
artifacts. It is test configuration, not a claim that every experimental integration is supported.

The main smoke runner is:

```bash
bash feasibility/e2e_matrix.sh
```

It compares representative native Rust and .NET runs and writes its logs and summary under `/tmp`
unless `RCL_MATRIX_LOG_DIR` and `RCL_MATRIX_SUMMARY` are set. CI uploads those files when a case
fails.

Focused release checks cover the downloadable SDK bundle, a clean install, C# interop, NuGet
package generation, MSBuild integration, and public API compatibility. See
[`docs/RELEASE_AND_ROLLBACK.md`](../docs/RELEASE_AND_ROLLBACK.md) for the release procedure.

For ordinary use, start with [`QUICKSTART.md`](../QUICKSTART.md). The acceptance scripts are mainly
for compiler contributors and release debugging.
