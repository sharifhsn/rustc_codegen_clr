#![no_std]

// Design:
// - safety: safe creation of any machine type is done only by instance methods of a
//   Machine (which is a ZST + Copy type), which can only by created unsafely or safely
//   through feature detection (e.g. fn AVX2::try_get() -> Option<Machine>).

mod soft;
mod types;
pub use self::types::*;

#[cfg(all(
    target_arch = "x86_64",
    target_feature = "sse2",
    not(target_os = "dotnet"), // DOTNET PAL: the CLR has no native x86 SIMD ABI for this crate.
    not(feature = "no_simd"),
    not(miri)
))]
pub mod x86_64;
#[cfg(all(
    target_arch = "x86_64",
    target_feature = "sse2",
    not(target_os = "dotnet"), // DOTNET PAL: route runtime-dispatchable vectors to the portable implementation.
    not(feature = "no_simd"),
    not(miri)
))]
use self::x86_64 as arch;

#[cfg(any(
    feature = "no_simd",
    target_os = "dotnet", // DOTNET PAL: use the scalar/generic vector representation.
    miri,
    not(target_arch = "x86_64"),
    all(target_arch = "x86_64", not(target_feature = "sse2"))
))]
pub mod generic;
#[cfg(any(
    feature = "no_simd",
    target_os = "dotnet", // DOTNET PAL: keep the generic backend paired with the module above.
    miri,
    not(target_arch = "x86_64"),
    all(target_arch = "x86_64", not(target_feature = "sse2"))
))]
use self::generic as arch;

pub use self::arch::{vec128_storage, vec256_storage, vec512_storage};
