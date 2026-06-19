//! Random data generation for the .NET ("dotnet") platform.
//!
//! Backed by a cryptographically-secure RNG from the BCL through a single
//! `extern` hook that the cilly linker maps to a .NET call:
//!
//! * `rcl_dotnet_random_fill(ptr, len)`
//!   => `RandomNumberGenerator.Fill(new Span<byte>((void*)ptr, (int)len))`
//!      (`System.Security.Cryptography.RandomNumberGenerator`).
//!
//! `RandomNumberGenerator.Fill` is the BCL's CSPRNG, so this is suitable for
//! `std`'s security-sensitive uses — in particular `HashMap`'s `RandomState`
//! / SipHash keys, which previously faulted under the deterministic placeholder
//! this replaces (see the cilly binding in
//! `cilly/src/ir/builtins/dotnet.rs::insert_dotnet_random_fill`).
#![forbid(unsafe_op_in_unsafe_fn)]

// Random hook -> System.Security.Cryptography.RandomNumberGenerator.Fill.
//
// The name must match EXACTLY the symbol the cilly linker patches in. Do not
// rename it.
unsafe extern "C" {
    /// `RandomNumberGenerator.Fill(new Span<byte>((void*)ptr, (int)len))`.
    fn rcl_dotnet_random_fill(ptr: *mut u8, len: usize);
}

/// Fill `bytes` with cryptographically-secure random data.
pub fn fill_bytes(bytes: &mut [u8]) {
    // An empty slice has nothing to fill; skip the call so we never hand the
    // BCL a (possibly dangling) zero-length pointer.
    if bytes.is_empty() {
        return;
    }
    // SAFETY: `bytes` is a valid, exclusively-borrowed slice of `bytes.len()`
    // writable `u8`s; `rcl_dotnet_random_fill` writes exactly that many bytes
    // through the `(ptr, len)` pair (it wraps them in a `Span<byte>` of the same
    // length on the managed side).
    unsafe { rcl_dotnet_random_fill(bytes.as_mut_ptr(), bytes.len()) }
}
