# rust-analyzer setup for `cargo dotnet` consumers

This is for people writing Rust that gets compiled to .NET via `cargo dotnet build`/`run` — **not**
for developing `rustc_codegen_clr` itself (that has its own, harder `rustc_private` concerns; not
covered here).

## The short version

rust-analyzer works fine on `cargo dotnet`-managed code, with one required override: point its check
diagnostics at your host triple instead of letting it follow the crate's `.cargo/config.toml`.

## Why this is needed

`cargo dotnet` writes a `.cargo/config.toml` into every crate it manages:

```toml
[build]
target = "/path/to/.cargo-dotnet/target/x86_64-unknown-dotnet.json"
[unstable]
build-std = ["core", "alloc", "std", "panic_unwind"]
```

That's a custom JSON target spec as the crate's *default* build target, requiring `-Z
build-std`/`-Z unstable-options` on every invocation that uses it — `cargo dotnet` passes those
itself, but a bare `cargo check` (which is what rust-analyzer runs under the hood by default for
live diagnostics) does not, and fails outright:

```
error: `.json target specs require -Zjson-target-spec to be added to the cargo invocation`
```

This is **not** a fundamental limitation. None of the ".NET-ness" of the code happens at
type-checking time — it's all `-Z codegen-backend`-time MIR→CIL lowering, well after rust-analyzer's
own analysis has already run. rust-analyzer's hover/go-to-definition/inline type inference use its
own HIR/trait-solver engine, not a literal `cargo check` invocation, and don't need the custom
target at all. Only the *live-diagnostics* checker (flycheck) shells out to `cargo check`/`clippy`,
and that's the one call site that trips over the custom target.

## The fix

Add to `.vscode/settings.json` (or the equivalent for your editor's rust-analyzer client):

```json
{
  "rust-analyzer.cargo.target": "aarch64-apple-darwin",
  "rust-analyzer.check.overrideCommand": [
    "cargo", "check",
    "--target", "aarch64-apple-darwin",
    "--message-format=json"
  ]
}
```

Substitute your actual host triple (`x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, etc. —
run `rustc -vV` and read `host:` if unsure). Both settings matter: `cargo.target` steers
rust-analyzer's own project-model resolution (so it doesn't try to load the JSON spec at all), and
`check.overrideCommand` steers the flycheck subprocess specifically (the actual thing that was
failing).

## What this does and doesn't give you

- **Works correctly**: diagnostics, hover, go-to-definition, inline hints, refactoring — for your
  own logic. This covers the overwhelming majority of what you're doing day to day.
- **Doesn't model**: the `.NET`-specific PAL internals inside `std` itself (the dotnet-ported
  filesystem/socket/process backends this project builds). If you're writing ordinary business logic
  against `mycorrhiza`'s wrappers (`mycorrhiza::collections`, `mycorrhiza::bcl`, `#[dotnet_class]`,
  etc.) rather than touching `std::os`-level PAL internals directly, this gap is invisible to you.
- **Still applies as-is**: `mycorrhiza` itself uses `adt_const_params`/`unsized_const_params`
  (the `RustcCLRInteropManagedClass<"System.String", "System.Private.CoreLib">`-shaped const-generic
  types everywhere in its API). This is genuinely cutting-edge unstable Rust; rust-analyzer's
  const-generic inference has historically lagged real `rustc` here, so expect the occasional
  false-positive red squiggle on code that compiles fine — not a real error, just rust-analyzer being
  less confident than `rustc` about an unusual generic-parameter shape. If you're only calling into
  `mycorrhiza`'s existing wrappers (not writing your own `#[dotnet_class]`/raw
  `RustcCLRInteropManagedClass<...>` code), you'll rarely hit this in practice — it's the
  const-generic *definitions* that are exotic, not most call sites.

## Toolchain

You still need the pinned nightly this project builds against (see `rust-toolchain.toml` in this
repo, or whatever your installed `cargo-dotnet` was built from) — rust-analyzer picks this up
automatically via `rustup` the same way `cargo` does, no extra config needed there.
