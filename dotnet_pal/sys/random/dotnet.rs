//! Random data generation for the .NET ("dotnet") platform.
//!
//! Milestone note: the fixed PAL extern contract (alloc/free/stdio) does not yet
//! expose a .NET RNG hook, and we are not allowed to add one to the linker for
//! this milestone. So `fill_bytes` uses a **deterministic** SplitMix64-style
//! fallback, seeded from a couple of address bits for a sliver of per-run
//! entropy. This is NOT cryptographically secure and NOT a real entropy source;
//! it exists purely so that std (HashMap's SipHash keys, `RandomState`, etc.)
//! links and runs on .NET. The intended follow-up is to back this with an
//! `rcl_dotnet_random_fill` extern wired to `System.Security.Cryptography
//! .RandomNumberGenerator.Fill` in the cilly linker, mirroring the alloc/stdio
//! hooks, at which point this fallback should be replaced.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::ptr;

/// Fill `bytes` with (weakly, deterministically) pseudo-random data.
///
/// See the module comment: this is a placeholder generator, sufficient to let
/// `std` run on .NET but not suitable for any security-sensitive use.
pub fn fill_bytes(bytes: &mut [u8]) {
    // Seed from address bits so that distinct calls/runs differ a little. This
    // mirrors `random/unsupported.rs`'s `hashmap_random_keys`, which leans on
    // allocation addresses for the same reason.
    let stack = 0u8;
    let mut state = ptr::from_ref(&stack).addr() as u64;
    // Mix in the destination address too, so independent buffers diverge.
    state ^= ptr::from_ref(bytes).cast::<u8>().addr() as u64;
    state ^= bytes.len() as u64;
    // Avoid the all-zero fixed point of the mixer.
    state |= 1;

    // SplitMix64: a small, well-distributed PRNG with no external dependencies.
    let mut next = || -> u64 {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };

    let mut chunks = bytes.chunks_exact_mut(8);
    for chunk in &mut chunks {
        chunk.copy_from_slice(&next().to_ne_bytes());
    }
    let rem = chunks.into_remainder();
    if !rem.is_empty() {
        let word = next().to_ne_bytes();
        rem.copy_from_slice(&word[..rem.len()]);
    }
}
