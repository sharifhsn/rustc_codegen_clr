//! `getrandom` custom-backend forwarding to the dotnet PAL's CSPRNG.
//!
//! ## SUPERSEDED for the cargo-dotnet auto path (kept as the manual reference primitive)
//!
//! As of the getrandom overlays in `dotnet_overlays/getrandom-{0.2,0.3,0.4}`, the
//! `cargo dotnet` / `dev.sh pal-build` path makes `getrandom` (and `rand`/`uuid`/
//! `ahash`) **auto-work with ZERO wiring**: each overlay IS getrandom (patched) with a
//! self-contained `target_os="dotnet"` backend arm that calls `rcl_dotnet_random_fill`
//! directly, so NO consumer-provided symbol/macro/feature and NO dependency on this
//! crate is needed. Consumers just `use uuid`/`rand`/`ahash`.
//!
//! This crate is RETAINED as the documented reference fill primitive + the correct
//! manual escape hatch for ad-hoc / non-overlaid builds (a one-off crate built without
//! the cargo-dotnet overlay registry). The per-major wiring recipes below still apply
//! to that manual path.
//!
//! ## Why this exists
//!
//! Our custom rustc target (`x86_64-unknown-dotnet.json`) advertises `os = "dotnet"`, which
//! does not match any arm of `getrandom`'s hardcoded per-target allow-list. On
//! such a target `getrandom` falls through to a `compile_error!(...)` in the
//! *front end* (build exit 101, before codegen ever runs). Every crate that
//! pulls `getrandom` transitively — `rand`, `uuid`, `ahash`, … — therefore
//! fails to build on the PAL.
//!
//! `getrandom` ships an official escape hatch for exactly this situation: a
//! user-provided **custom backend**. This crate forwards that backend to the
//! PAL's existing CSPRNG hook, `rcl_dotnet_random_fill`, which the cilly linker
//! patches to `System.Security.Cryptography.RandomNumberGenerator.Fill`
//! (see `cilly/src/ir/builtins/dotnet.rs::insert_dotnet_random_fill` and
//! `dotnet_pal/sys/random/dotnet.rs`).
//!
//! ## How to use it (per getrandom major)
//!
//! This crate provides only the version-agnostic primitive [`fill`]. The custom
//! *symbol* differs per getrandom major and (for 0.2) must live in the **root
//! binary crate**, so each consumer wires it up against its own `getrandom`:
//!
//! ### getrandom 0.3 and 0.4  (same symbol + cfg for both)
//!
//! Build with `RUSTFLAGS=... --cfg getrandom_backend="custom"` (wired into
//! `feasibility/dev.sh pal-build` already) and, in the consuming crate:
//!
//! ```ignore
//! /// getrandom 0.3/0.4 custom backend -> dotnet PAL CSPRNG.
//! #[unsafe(no_mangle)]
//! unsafe extern "Rust" fn __getrandom_v03_custom(
//!     dest: *mut u8,
//!     len: usize,
//! ) -> Result<(), getrandom::Error> {
//!     getrandom_dotnet::fill(core::slice::from_raw_parts_mut(dest, len));
//!     Ok(())
//! }
//! ```
//!
//! No Cargo feature is needed for 0.3/0.4 — the cfg selects the backend.
//!
//! ### getrandom 0.2  (Cargo feature + macro, no cfg)
//!
//! 0.2 selects its custom backend via the Cargo **feature** `custom` (the
//! `getrandom_backend` cfg is a 0.3+ invention and is ignored by 0.2), and the
//! symbol is installed by the `register_custom_getrandom!` macro, which must be
//! invoked in the root binary crate. In the consuming crate's `Cargo.toml`:
//!
//! ```toml
//! getrandom = { version = "0.2", features = ["custom"] }
//! ```
//!
//! and in `main.rs`:
//!
//! ```ignore
//! fn dotnet_getrandom(buf: &mut [u8]) -> Result<(), getrandom::Error> {
//!     getrandom_dotnet::fill(buf);
//!     Ok(())
//! }
//! getrandom::register_custom_getrandom!(dotnet_getrandom);
//! ```
//!
//! The 0.2 symbol (`__getrandom_custom`) and the 0.3/0.4 symbol
//! (`__getrandom_v03_custom`) are distinct, so a crate pulling several majors
//! can register all of them without clashing.
#![no_std]
#![forbid(unsafe_op_in_unsafe_fn)]

// The dotnet PAL's CSPRNG hook. The name must match EXACTLY the symbol the
// cilly linker patches in (`insert_dotnet_random_fill`) — do not rename it.
// It resolves at link time exactly like std's own use in
// `dotnet_pal/sys/random/dotnet.rs`.
unsafe extern "C" {
    /// `RandomNumberGenerator.Fill(new Span<byte>((void*)ptr, (int)len))`.
    fn rcl_dotnet_random_fill(ptr: *mut u8, len: usize);
}

/// Fill `bytes` with cryptographically-secure random data from the dotnet PAL.
///
/// This is the single point that talks to the PAL; the per-version custom
/// symbols defined by consumers all funnel through here. It never fails
/// (`rcl_dotnet_random_fill` returns nothing and the BCL CSPRNG does not
/// surface errors), so callers always return `Ok(())`.
#[inline]
pub fn fill(bytes: &mut [u8]) {
    // An empty slice has nothing to fill; skip the call so we never hand the
    // BCL a (possibly dangling) zero-length pointer.
    if bytes.is_empty() {
        return;
    }
    // SAFETY: `bytes` is a valid, exclusively-borrowed slice of `bytes.len()`
    // writable `u8`s; `rcl_dotnet_random_fill` writes exactly that many bytes
    // through the `(ptr, len)` pair (it wraps them in a `Span<byte>` of the
    // same length on the managed side).
    unsafe { rcl_dotnet_random_fill(bytes.as_mut_ptr(), bytes.len()) }
}
