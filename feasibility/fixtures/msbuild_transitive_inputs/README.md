# MSBuild transitive-input regression fixture

`csharp/FixtureConsumer.csproj` imports `RustDotnet.targets` and builds the nested
`rustlib` Cargo workspace. The selected `rustlib` package has both a path dependency
(`deps/fixture_dep`) and a `build.rs` input (`build-input.txt`) outside the current
MSBuild automatic input set.

Run `feasibility/transitive_input_invalidation_acceptance.sh` after building the
release `cargo-dotnet` driver. The script deliberately fails on the current
implementation after proving the initial build and a genuine no-op. Its failure is
the regression contract: changing either the path dependency or the build-script
input **must** invoke `cargo dotnet build` and change the managed DLL observed by C#.

Do not add `RustDotnetInput` items here. That manual escape hatch is documented, but
this fixture exists to require Cargo-metadata-derived transitive fingerprinting.
