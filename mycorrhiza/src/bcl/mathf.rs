//! Idiomatic Rust over [`System.Math`] — the .NET double-precision math routines
//! (`System.Private.CoreLib`), used like Rust's own `f64` methods and `std::f64::consts`.
//!
//! `System.Math` is a *static* class (a namespace of `static` methods and `const` fields), so there is
//! nothing to construct: every member here is an associated function on the [`Math`] zero-sized marker,
//! or a plain [`const`](Math::PI). The functions are thin, honest 1:1 forwards to the managed routines —
//! no extra rounding, clamping, or NaN massaging beyond what the CLR already does.
//!
//! ```ignore
//! use mycorrhiza::bcl::mathf::Math;
//!
//! let x = Math::sqrt(2.0);          // System.Math.Sqrt
//! let y = Math::pow(x, 2.0);        // System.Math.Pow  -> ~2.0
//! let a = Math::atan2(1.0, 1.0);    // System.Math.Atan2 -> PI/4
//! assert!((Math::PI - 3.141592653589793).abs() < 1e-12);
//! ```
//!
//! **Why a newtype and not the raw binding.** The generated low-level [`crate::Math`] binding already
//! carries an `impl` on the underlying managed type, and it exposes only *one* overload per name (its
//! `abs`/`max`/`min` resolve to the `i16`/`u8` overloads, not `double`). This module is an independent,
//! idiomatic `f64` surface: a distinct zero-sized [`Math`] whose methods name the `double` overload
//! explicitly through the shared `static1`/`static2` interop entry — so the mapping is always the `f64`
//! form and there is no collision with the raw bindings' inherent methods.

/// The raw generated binding for the managed `System.Math` class (impl assembly
/// `System.Private.CoreLib`). Used only as the interop entry point for the wrappers below.
type RawMath = crate::System::Math;

/// Idiomatic double-precision `System.Math`. A zero-sized marker: all members are associated
/// functions / constants — there is no value to construct or hold.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Math;

impl Math {
    // ---- constants (`const double` fields in .NET; exposed as Rust consts) ----

    /// The ratio of a circle's circumference to its diameter, `System.Math.PI`.
    pub const PI: f64 = std::f64::consts::PI;
    /// The base of natural logarithms, `System.Math.E`.
    pub const E: f64 = std::f64::consts::E;
    /// The number of radians in one turn, `System.Math.Tau` (`2 * PI`).
    pub const TAU: f64 = std::f64::consts::TAU;

    // ---- roots, powers, exponentials, logs ----

    /// Square root (`System.Math.Sqrt`).
    pub fn sqrt(x: f64) -> f64 {
        RawMath::static1::<"Sqrt", f64, f64>(x)
    }
    /// Cube root (`System.Math.Cbrt`).
    pub fn cbrt(x: f64) -> f64 {
        RawMath::static1::<"Cbrt", f64, f64>(x)
    }
    /// `base` raised to `exp` (`System.Math.Pow`).
    pub fn pow(base: f64, exp: f64) -> f64 {
        RawMath::static2::<"Pow", f64, f64, f64>(base, exp)
    }
    /// `e` raised to `x` (`System.Math.Exp`).
    pub fn exp(x: f64) -> f64 {
        RawMath::static1::<"Exp", f64, f64>(x)
    }
    /// Natural (base-`e`) logarithm (`System.Math.Log`).
    pub fn ln(x: f64) -> f64 {
        RawMath::static1::<"Log", f64, f64>(x)
    }
    /// Base-2 logarithm (`System.Math.Log2`).
    pub fn log2(x: f64) -> f64 {
        RawMath::static1::<"Log2", f64, f64>(x)
    }
    /// Base-10 logarithm (`System.Math.Log10`).
    pub fn log10(x: f64) -> f64 {
        RawMath::static1::<"Log10", f64, f64>(x)
    }

    // ---- trigonometry ----

    /// Sine of `x` (radians) — `System.Math.Sin`.
    pub fn sin(x: f64) -> f64 {
        RawMath::static1::<"Sin", f64, f64>(x)
    }
    /// Cosine of `x` (radians) — `System.Math.Cos`.
    pub fn cos(x: f64) -> f64 {
        RawMath::static1::<"Cos", f64, f64>(x)
    }
    /// Tangent of `x` (radians) — `System.Math.Tan`.
    pub fn tan(x: f64) -> f64 {
        RawMath::static1::<"Tan", f64, f64>(x)
    }
    /// Arc sine, in radians (`System.Math.Asin`).
    pub fn asin(x: f64) -> f64 {
        RawMath::static1::<"Asin", f64, f64>(x)
    }
    /// Arc cosine, in radians (`System.Math.Acos`).
    pub fn acos(x: f64) -> f64 {
        RawMath::static1::<"Acos", f64, f64>(x)
    }
    /// Arc tangent, in radians (`System.Math.Atan`).
    pub fn atan(x: f64) -> f64 {
        RawMath::static1::<"Atan", f64, f64>(x)
    }
    /// The angle (radians) of the vector `(x, y)` — `System.Math.Atan2` (note .NET's `(y, x)` order).
    pub fn atan2(y: f64, x: f64) -> f64 {
        RawMath::static2::<"Atan2", f64, f64, f64>(y, x)
    }

    // ---- rounding ----

    /// Round toward positive infinity (`System.Math.Ceiling`).
    pub fn ceil(x: f64) -> f64 {
        RawMath::static1::<"Ceiling", f64, f64>(x)
    }
    /// Round toward negative infinity (`System.Math.Floor`).
    pub fn floor(x: f64) -> f64 {
        RawMath::static1::<"Floor", f64, f64>(x)
    }
    /// Round to the nearest integer, ties-to-even (`System.Math.Round`, banker's rounding).
    pub fn round(x: f64) -> f64 {
        RawMath::static1::<"Round", f64, f64>(x)
    }
    /// Discard the fractional part (`System.Math.Truncate`).
    pub fn trunc(x: f64) -> f64 {
        RawMath::static1::<"Truncate", f64, f64>(x)
    }

    // ---- sign, magnitude, comparison ----

    /// Absolute value (`System.Math.Abs`, the `double` overload).
    pub fn abs(x: f64) -> f64 {
        RawMath::static1::<"Abs", f64, f64>(x)
    }
    /// `-1`, `0`, or `+1` per the sign of `x` (`System.Math.Sign`; throws on `NaN`, as in .NET).
    pub fn sign(x: f64) -> i32 {
        RawMath::static1::<"Sign", f64, i32>(x)
    }
    /// The larger of two values (`System.Math.Max`, the `double` overload).
    pub fn max(a: f64, b: f64) -> f64 {
        RawMath::static2::<"Max", f64, f64, f64>(a, b)
    }
    /// The smaller of two values (`System.Math.Min`, the `double` overload).
    pub fn min(a: f64, b: f64) -> f64 {
        RawMath::static2::<"Min", f64, f64, f64>(a, b)
    }
    /// The IEEE 754 remainder of `x / y` (`System.Math.IEEERemainder`; differs from `%`).
    pub fn ieee_remainder(x: f64, y: f64) -> f64 {
        RawMath::static2::<"IEEERemainder", f64, f64, f64>(x, y)
    }
    /// `x` with the sign of `y` (`System.Math.CopySign`).
    pub fn copy_sign(x: f64, y: f64) -> f64 {
        RawMath::static2::<"CopySign", f64, f64, f64>(x, y)
    }

    // ---- hyperbolic ----

    /// Hyperbolic sine (`System.Math.Sinh`).
    pub fn sinh(x: f64) -> f64 {
        RawMath::static1::<"Sinh", f64, f64>(x)
    }
    /// Hyperbolic cosine (`System.Math.Cosh`).
    pub fn cosh(x: f64) -> f64 {
        RawMath::static1::<"Cosh", f64, f64>(x)
    }
    /// Hyperbolic tangent (`System.Math.Tanh`).
    pub fn tanh(x: f64) -> f64 {
        RawMath::static1::<"Tanh", f64, f64>(x)
    }
}
