//! `System.Decimal` — an exact base-10 128-bit number, the core numeric type of financial .NET code
//! (money, NAVs, share counts). Rust has no native decimal, so this wraps the managed value type: it is
//! constructed from Rust integers or a string, and its arithmetic / comparison operators go through the
//! CLR's `Decimal` operators — so results are bit-identical to C#. `Display` renders it exactly.

use core::cmp::Ordering;
use core::fmt;
use core::ops::{Add, Div, Mul, Sub};

use crate::intrinsics::RustcCLRInteropManagedStruct;
use crate::system::{DotNetString, MString};

const CORELIB: &str = "System.Private.CoreLib";
const DEC: &str = "System.Decimal";
type Dec = RustcCLRInteropManagedStruct<CORELIB, DEC, 16>;

/// A managed `System.Decimal`. Use it like a number: `a + b`, `a * b`, `a < b`, `a == b`, `println!("{a}")`.
#[derive(Clone, Copy)]
pub struct DotNetDecimal {
    h: Dec,
}

impl DotNetDecimal {
    /// From a Rust `i64` (`(decimal)value`).
    pub fn from_i64(v: i64) -> Self {
        Self {
            h: Dec::vt_static1::<"op_Implicit", i64, Dec>(v),
        }
    }
    /// From a Rust `i32`.
    pub fn from_i32(v: i32) -> Self {
        Self {
            h: Dec::vt_static1::<"op_Implicit", i32, Dec>(v),
        }
    }
    /// Parse a decimal literal (`Decimal.Parse`) — e.g. `"1234.56"`. Throws (a managed exception) on
    /// malformed input, exactly as `decimal.Parse` does in C#.
    pub fn parse(s: &str) -> Self {
        Self {
            h: Dec::vt_static1::<"Parse", MString, Dec>(MString::from(s)),
        }
    }
    /// Convert to `f64` (`Decimal.ToDouble`) — lossy for values outside `f64`'s exact range.
    pub fn to_f64(self) -> f64 {
        Dec::vt_static1::<"ToDouble", Dec, f64>(self.h)
    }
    /// The exact decimal string (`Decimal.ToString`).
    pub fn to_dotnet_string(self) -> DotNetString {
        DotNetString::from_handle(self.h.vt_instance0::<"ToString", MString>())
    }
    /// The raw managed handle, to pass the decimal to a .NET API expecting `System.Decimal`.
    pub fn handle(self) -> Dec {
        self.h
    }
    /// Wrap a `System.Decimal` handle returned by a .NET API.
    pub fn from_handle(h: Dec) -> Self {
        Self { h }
    }
}

macro_rules! dec_binop {
    ($tr:ident, $m:ident, $op:literal) => {
        impl $tr for DotNetDecimal {
            type Output = DotNetDecimal;
            #[inline]
            fn $m(self, rhs: DotNetDecimal) -> DotNetDecimal {
                DotNetDecimal {
                    h: Dec::vt_static2::<$op, Dec, Dec, Dec>(self.h, rhs.h),
                }
            }
        }
    };
}
dec_binop!(Add, add, "op_Addition");
dec_binop!(Sub, sub, "op_Subtraction");
dec_binop!(Mul, mul, "op_Multiply");
dec_binop!(Div, div, "op_Division");

impl PartialEq for DotNetDecimal {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Dec::vt_static2::<"op_Equality", Dec, Dec, bool>(self.h, other.h)
    }
}
impl PartialOrd for DotNetDecimal {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // `Decimal.Compare` returns <0 / 0 / >0 like `strcmp` — total order, so `Some` always.
        Some(Dec::vt_static2::<"Compare", Dec, Dec, i32>(self.h, other.h).cmp(&0))
    }
}
impl fmt::Display for DotNetDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_dotnet_string().to_rust_string())
    }
}
