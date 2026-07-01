//! An idiomatic Rust wrapper over the managed [`System.Random`] pseudo-random number generator
//! (assembly `System.Private.CoreLib`).
//!
//! This is a thin, honest handle to a real managed `Random` object living on the CLR heap — every
//! method delegates straight to the corresponding .NET member, so the sequences produced match what
//! the equivalent C# would produce (including the seeded-reproducibility contract). No extra Rust-side
//! randomness is invented.
//!
//! ```ignore
//! use mycorrhiza::bcl::random::Random;
//!
//! let mut rng = Random::with_seed(42);        // reproducible, like `new Random(42)`
//! let d6 = rng.next_range(1, 7);              // Next(1, 7) → 1..=6
//! let p  = rng.next_f64();                    // NextDouble() → [0.0, 1.0)
//!
//! // The process-wide thread-safe instance (`Random.Shared`):
//! let coin = Random::shared().next_below(2);  // 0 or 1
//! ```
//!
//! ## Mapping to the .NET surface
//!
//! | Rust                       | .NET member                          |
//! |----------------------------|--------------------------------------|
//! | [`Random::new`]            | `new Random()` (time-seeded)         |
//! | [`Random::with_seed`]      | `new Random(int Seed)`               |
//! | [`Random::shared`]         | `static Random.Shared { get; }`      |
//! | [`Random::next`]           | `int Next()`  → `[0, i32::MAX)`       |
//! | [`Random::next_below`]     | `int Next(int maxValue)`             |
//! | [`Random::next_range`]     | `int Next(int minValue, int maxValue)` |
//! | [`Random::next_i64`]       | `long NextInt64()`                   |
//! | [`Random::next_i64_below`] | `long NextInt64(long maxValue)`      |
//! | [`Random::next_i64_range`] | `long NextInt64(long min, long max)` |
//! | [`Random::next_f64`]       | `double NextDouble()` → `[0.0, 1.0)` |
//! | [`Random::next_f32`]       | `float NextSingle()`  → `[0.0, 1.0)` |
//! | [`Display`](core::fmt::Display) | `object.ToString()`             |
//!
//! `NextBytes(byte[])` is intentionally omitted here — filling a managed `byte[]` needs the
//! array-marshalling surface and is not a thin one-liner; use the raw handle via [`Random::handle`]
//! if you need it.

use crate::system::MString;

/// The raw managed-handle alias for `System.Random` (from the generated BCL bindings). A `Random`
/// wraps one of these; [`Random::handle`] hands it back for lower-level BCL calls.
pub type MRandom = crate::intrinsics::RustcCLRInteropManagedClass<
    "System.Private.CoreLib",
    "System.Random",
>;

/// A managed `System.Random` — a pseudo-random number generator on the CLR heap.
///
/// Move-only (a plain handle to a managed object; the .NET GC owns the object, so there is no
/// `Drop`). See the [module docs](self) for the full member mapping and semantics.
#[derive(Clone, Copy)]
pub struct Random(MRandom);

impl Random {
    /// `new Random()` — seeded from a time-dependent default, so each instance yields a different
    /// sequence (matches C#'s parameterless constructor).
    #[inline(always)]
    pub fn new() -> Self {
        Random(MRandom::ctor0())
    }

    /// `new Random(int Seed)` — a reproducible generator: two `Random::with_seed(s)` values with the
    /// same `s` produce identical sequences (the .NET seeded-reproducibility contract).
    #[inline(always)]
    pub fn with_seed(seed: i32) -> Self {
        Random(MRandom::ctor1::<i32>(seed))
    }

    /// The process-wide, thread-safe shared instance (`Random.Shared`). Cheap to fetch repeatedly;
    /// safe to use from any thread (unlike an owned `Random`, whose methods are not thread-safe).
    #[inline(always)]
    pub fn shared() -> Self {
        Random(MRandom::static0::<"get_Shared", MRandom>())
    }

    /// Wrap an existing managed `System.Random` handle (e.g. one returned by another BCL call).
    #[inline(always)]
    pub fn from_handle(h: MRandom) -> Self {
        Random(h)
    }

    /// The underlying managed handle, for lower-level BCL calls.
    #[inline(always)]
    pub fn handle(self) -> MRandom {
        self.0
    }

    /// `Next()` — a non-negative `i32` in `[0, i32::MAX)`.
    #[inline(always)]
    pub fn next(&mut self) -> i32 {
        self.0.instance0::<"Next", i32>()
    }

    /// `Next(int maxValue)` — a non-negative `i32` in `[0, max)`. `max` must be `>= 0`
    /// (a negative `max` throws `ArgumentOutOfRangeException` on the .NET side); `max == 0` yields `0`.
    #[inline(always)]
    pub fn next_below(&mut self, max: i32) -> i32 {
        self.0.instance1::<"Next", i32, i32>(max)
    }

    /// `Next(int minValue, int maxValue)` — an `i32` in `[min, max)`. Requires `min <= max`
    /// (otherwise .NET throws `ArgumentOutOfRangeException`); `min == max` yields `min`.
    #[inline(always)]
    pub fn next_range(&mut self, min: i32, max: i32) -> i32 {
        self.0.instance2::<"Next", i32, i32, i32>(min, max)
    }

    /// `NextInt64()` — a non-negative `i64` in `[0, i64::MAX)`.
    #[inline(always)]
    pub fn next_i64(&mut self) -> i64 {
        self.0.instance0::<"NextInt64", i64>()
    }

    /// `NextInt64(long maxValue)` — a non-negative `i64` in `[0, max)`. Same range rules as
    /// [`Random::next_below`].
    #[inline(always)]
    pub fn next_i64_below(&mut self, max: i64) -> i64 {
        self.0.instance1::<"NextInt64", i64, i64>(max)
    }

    /// `NextInt64(long minValue, long maxValue)` — an `i64` in `[min, max)`. Same range rules as
    /// [`Random::next_range`].
    #[inline(always)]
    pub fn next_i64_range(&mut self, min: i64, max: i64) -> i64 {
        self.0.instance2::<"NextInt64", i64, i64, i64>(min, max)
    }

    /// `NextDouble()` — an `f64` in `[0.0, 1.0)`.
    #[inline(always)]
    pub fn next_f64(&mut self) -> f64 {
        self.0.instance0::<"NextDouble", f64>()
    }

    /// `NextSingle()` — an `f32` in `[0.0, 1.0)`.
    #[inline(always)]
    pub fn next_f32(&mut self) -> f32 {
        self.0.instance0::<"NextSingle", f32>()
    }

    /// The managed `ToString()` of the underlying object, as an idiomatic Rust [`String`].
    #[inline(always)]
    pub fn to_rust_string(self) -> std::string::String {
        crate::system::DotNetString::from_handle(self.0.to_mstring()).to_rust_string()
    }
}

impl Default for Random {
    /// Same as [`Random::new`] — a fresh time-seeded generator.
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Display for Random {
    /// The managed `object.ToString()` (for the base `System.Random`, its type name).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s: MString = self.0.to_mstring();
        core::fmt::Display::fmt(&crate::system::DotNetString::from_handle(s), f)
    }
}
