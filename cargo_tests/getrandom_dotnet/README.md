# getrandom_dotnet

A reusable shim that makes the [`getrandom`](https://crates.io/crates/getrandom)
crate build and work on the dotnet PAL target (`os = "dotnet"`), which `getrandom`
otherwise rejects with a front-end `compile_error!`.

It forwards `getrandom`'s official **custom backend** to the PAL's existing
CSPRNG hook `rcl_dotnet_random_fill` (→
`System.Security.Cryptography.RandomNumberGenerator.Fill`). Any crate pulling
`getrandom` transitively (`rand`, `uuid`, `ahash`, …) is unblocked by this.

This is the pattern a real H2 consumer would follow: depend on `getrandom_dotnet`,
wire the version-appropriate custom symbol, and set the cfg.

## Versions covered

| getrandom major | selector                              | symbol you provide                         |
|-----------------|---------------------------------------|--------------------------------------------|
| 0.3 / 0.4       | `--cfg getrandom_backend="custom"`    | `__getrandom_v03_custom` (extern "Rust")   |
| 0.2             | Cargo feature `custom`                | `register_custom_getrandom!(fn)` macro     |

`getrandom_dotnet` exposes only the version-agnostic primitive `fill(&mut [u8])`.
The custom *symbol* is version-specific (and, for 0.2, must live in the root
binary crate), so the consumer defines it against its own `getrandom`. See the
crate docs (`src/lib.rs`) for copy-paste snippets.

## Build wiring

`feasibility/dev.sh pal-build` already appends `--cfg getrandom_backend="custom"`
to its `RUSTFLAGS`, so the 0.3/0.4 selector is on for every PAL build. For 0.2
you additionally enable the Cargo `custom` feature in the consuming crate's
`Cargo.toml`:

```toml
getrandom = { version = "0.2", features = ["custom"] }
```

(Feature unification turns it on for the transitive copy that `rand_core` pulls.)
