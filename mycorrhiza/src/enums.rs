//! `.NET enum` ↔ Rust enum bridge.
//!
//! A C# `enum` is an integer-backed value type with no GC reference, so it is bit-identical to its
//! underlying integer. [`dotnet_enum!`] declares a `#[repr(..)]` Rust enum mirroring the C# one and
//! generates the boundary conversions:
//!
//! * [`value`](trait method) / `from_value` — the underlying integer ↔ the Rust variant.
//! * `to_handle` / `from_handle` — the managed `valuetype` handle a .NET method takes / returns.
//!
//! ```ignore
//! use mycorrhiza::dotnet_enum;
//!
//! dotnet_enum! {
//!     pub enum DayOfWeek = ["System.Private.CoreLib"] "System.DayOfWeek" (i32, 4) {
//!         Sunday = 0, Monday = 1, Tuesday = 2, Wednesday = 3,
//!         Thursday = 4, Friday = 5, Saturday = 6,
//!     }
//! }
//!
//! // Pass to a .NET API expecting `DayOfWeek`:  some_api(DayOfWeek::Wednesday.to_handle());
//! // Receive one back and `match` on it:        match DayOfWeek::from_handle(h) { .. }
//! ```

/// Declare a Rust mirror of a .NET `enum` plus its boundary conversions. See the [module docs](self).
///
/// Syntax: `enum <Name> = ["<assembly>"] "<Class.Path>" (<repr>, <byte-size>) { Variant = value, .. }`
/// — `<repr>` is the enum's underlying integer type (`i8`/`i16`/`i32`/`i64`/`u8`/…) and `<byte-size>`
/// its size in bytes (1/2/4/8), matching the C# enum's base type.
#[macro_export]
macro_rules! dotnet_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $Name:ident = [ $asm:tt ] $class:tt ( $repr:ty, $size:literal ) {
            $( $Variant:ident = $val:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        #[repr($repr)]
        $vis enum $Name {
            $( $Variant = $val ),+
        }

        impl $Name {
            /// The underlying integer value of this variant.
            #[inline]
            pub fn value(self) -> $repr {
                self as $repr
            }
            /// The Rust variant for an underlying integer, or `None` if it names no known variant.
            #[inline]
            pub fn from_value(v: $repr) -> ::core::option::Option<Self> {
                match v {
                    $( $val => ::core::option::Option::Some(Self::$Variant), )+
                    _ => ::core::option::Option::None,
                }
            }
            /// The managed `valuetype` handle, to pass this enum to a .NET API expecting the C# enum.
            /// Bit-identical to the underlying integer (an enum has no GC reference).
            #[inline]
            pub fn to_handle(
                self,
            ) -> $crate::intrinsics::RustcCLRInteropManagedStruct<{ $asm }, { $class }, $size> {
                let v = self as $repr;
                unsafe { ::core::mem::transmute_copy(&v) }
            }
            /// Reconstruct from a managed handle returned by a .NET API (reads the underlying integer;
            /// `None` if it is not a known variant).
            #[inline]
            pub fn from_handle(
                h: $crate::intrinsics::RustcCLRInteropManagedStruct<{ $asm }, { $class }, $size>,
            ) -> ::core::option::Option<Self> {
                let v: $repr = unsafe { ::core::mem::transmute_copy(&h) };
                Self::from_value(v)
            }
        }
    };
}
