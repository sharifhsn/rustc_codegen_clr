//! DOTNET PAL: the entropy backend for `target_os = "dotnet"` (getrandom 0.2 model).
//!
//! getrandom 0.2 has no built-in arm for our `x86_64-unknown-dotnet` target, so this
//! overlay supplies one. Unlike 0.3/0.4 (whose backend exports `fill_inner`), 0.2's
//! per-target `imp` exports `getrandom_inner` (and has no `inner_u32`/`inner_u64`
//! API). We funnel it to the dotnet PAL's CSPRNG hook `rcl_dotnet_random_fill`,
//! which the cilly linker patches to
//! `System.Security.Cryptography.RandomNumberGenerator.Fill`
//! (see `cilly/src/ir/builtins/dotnet.rs::insert_dotnet_random_fill`).
//!
//! Because this overlay IS getrandom (patched), the backend is defined INTERNALLY:
//! no `custom` feature, no `register_custom_getrandom!` macro, no `getrandom_dotnet`
//! dependency. The extern is declared directly here, modelled on `fuchsia.rs`.
use crate::{util::uninit_slice_fill_zero, Error};
use core::mem::MaybeUninit;

// DOTNET PAL: the CSPRNG hook the cilly linker overrides into
// `RandomNumberGenerator.Fill(new Span<byte>((void*)ptr, (int)len))`. The name must
// match EXACTLY the symbol `insert_dotnet_random_fill` patches in.
unsafe extern "C" {
    fn rcl_dotnet_random_fill(ptr: *mut u8, len: usize); // DOTNET PAL
}

// DOTNET PAL: fill `dest` from the BCL CSPRNG. `RandomNumberGenerator.Fill` writes
// every byte and never fails, so we always return `Ok(())` — a getrandom::Error is
// never constructed. `uninit_slice_fill_zero` gives an initialized `&mut [u8]` view
// (matching the 0.2 backend convention used by use_file.rs et al.).
pub fn getrandom_inner(dest: &mut [MaybeUninit<u8>]) -> Result<(), Error> {
    let dest = uninit_slice_fill_zero(dest);
    // An empty slice has nothing to fill; skip the call so we never hand the BCL a
    // (possibly dangling) zero-length pointer.
    if !dest.is_empty() {
        // SAFETY: `dest` is a valid, exclusively-borrowed slice of `dest.len()`
        // writable bytes; `rcl_dotnet_random_fill` writes exactly that many bytes
        // through the `(ptr, len)` pair.
        unsafe { rcl_dotnet_random_fill(dest.as_mut_ptr(), dest.len()) }
    }
    Ok(())
}
