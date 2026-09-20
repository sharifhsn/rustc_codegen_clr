//! DOTNET PAL: the entropy backend for `target_os = "dotnet"`.
//!
//! getrandom has no built-in arm for our `x86_64-unknown-dotnet` target, so this
//! overlay supplies one. It funnels getrandom's `fill_inner` to the dotnet PAL's
//! CSPRNG hook `rcl_dotnet_random_fill`, which the cilly linker patches to
//! `System.Security.Cryptography.RandomNumberGenerator.Fill`
//! (see `cilly/src/ir/builtins/dotnet.rs::insert_dotnet_random_fill`).
//!
//! Because this overlay IS getrandom (patched), the backend is defined INTERNALLY:
//! no consumer-provided `__getrandom_v03_custom` symbol, no `getrandom_backend=custom`
//! cfg, no `getrandom_dotnet` dependency. The extern is declared directly here,
//! exactly like mio's dotnet arm declares its libc symbols.
use crate::Error;
use core::mem::MaybeUninit;

// `inner_u32`/`inner_u64` are the default `fill_inner`-backed implementations
// getrandom's lib.rs calls for the integer entry points; every backend re-exports
// them (see custom.rs / getentropy.rs).
pub use crate::util::{inner_u32, inner_u64};

// DOTNET PAL: the CSPRNG hook the cilly linker overrides into
// `RandomNumberGenerator.Fill(new Span<byte>((void*)ptr, (int)len))`. The name must
// match EXACTLY the symbol `insert_dotnet_random_fill` patches in.
unsafe extern "C" {
    fn rcl_dotnet_random_fill(ptr: *mut u8, len: usize); // DOTNET PAL
}

// DOTNET PAL: fill `dest` from the BCL CSPRNG. It always fully initializes `dest`
// (the .NET `RandomNumberGenerator.Fill` writes every byte) and never fails, so we
// always return `Ok(())` — a getrandom::Error is never constructed.
#[inline]
pub fn fill_inner(dest: &mut [MaybeUninit<u8>]) -> Result<(), Error> {
    // An empty slice has nothing to fill; skip the call so we never hand the BCL a
    // (possibly dangling) zero-length pointer.
    if !dest.is_empty() {
        // SAFETY: `dest` is a valid, exclusively-borrowed slice of `dest.len()`
        // writable bytes; `rcl_dotnet_random_fill` writes exactly that many bytes
        // through the `(ptr, len)` pair, so on return `dest` is fully initialized.
        unsafe { rcl_dotnet_random_fill(dest.as_mut_ptr().cast(), dest.len()) }
    }
    Ok(())
}
