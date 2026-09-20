//! Idiomatic Rust wrappers over the most-reached-for **Base Class Library** value types and static
//! helpers — `DateTime`, `DateTimeOffset`, `TimeSpan`, `Guid`, `Uri`, `Regex`, `Random`,
//! `Stopwatch`, `StringBuilder`, `Environment`, and `Math`.
//!
//! Each submodule wraps the low-level BCL surface (the generated [`crate::bindings`] and the raw
//! [`crate::intrinsics`] magic) so the type reads like a normal Rust type: constructors are associated
//! fns, methods are `snake_case`, .NET properties are getters, `&str` goes in / `String` comes out, and
//! the natural std traits (`Display`/`Debug`, `PartialEq`/`Eq`, `PartialOrd`/`Ord`, `Hash`, `Default`)
//! are implemented where they map cleanly onto a managed member. No CLR-interop knowledge is needed at
//! the call site.
//!
//! ```ignore
//! use mycorrhiza::prelude::*;
//!
//! let id = Guid::new_v4();
//! let now = DateTime::now();
//! let mut sb = StringBuilder::new();
//! sb.append("id=");
//! sb.append(&id.to_string());
//! println!("{sb} @ {now}  sqrt2={}", Math::sqrt(2.0));
//! ```
//!
//! Everything here is a thin, honest mapping — no behaviour is emulated in Rust. Long-lived class
//! wrappers keep their CLR object behind an opaque `GCHandle`; a `handle()` escape hatch copies the
//! naked reference out only for an immediate low-level managed call. Do not retain that raw value in
//! Rust-owned aggregates, collections, statics, or async state.

macro_rules! impl_managed_display_value {
    ($ty:ty) => {
        impl core::fmt::Display for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let value = $crate::system::DotNetString::from_handle(
                    (*self).vt_instance0::<"ToString", $crate::system::MString>(),
                );
                core::fmt::Display::fmt(&value, f)
            }
        }

        impl core::fmt::Debug for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Display::fmt(self, f)
            }
        }
    };
}

macro_rules! impl_managed_display_field {
    ($ty:ty, $field:tt) => {
        impl core::fmt::Display for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let value = $crate::system::DotNetString::from_handle(
                    self.$field
                        .vt_instance0::<"ToString", $crate::system::MString>(),
                );
                core::fmt::Display::fmt(&value, f)
            }
        }

        impl core::fmt::Debug for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Display::fmt(self, f)
            }
        }
    };
}

macro_rules! impl_managed_ordering {
    ($ty:ty, equals) => {
        impl PartialEq for $ty {
            #[inline(always)]
            fn eq(&self, other: &Self) -> bool {
                self.equals(*other)
            }
        }
        impl Eq for $ty {}

        impl PartialOrd for $ty {
            #[inline(always)]
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for $ty {
            #[inline(always)]
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.compare_to(*other).cmp(&0)
            }
        }
    };
    ($ty:ty, compare_to) => {
        impl PartialEq for $ty {
            #[inline(always)]
            fn eq(&self, other: &Self) -> bool {
                self.compare_to(*other) == 0
            }
        }
        impl Eq for $ty {}

        impl PartialOrd for $ty {
            #[inline(always)]
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for $ty {
            #[inline(always)]
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.compare_to(*other).cmp(&0)
            }
        }
    };
}

pub mod dateonly;
pub mod datetime;
pub mod datetimeoffset;
pub mod decimal;
pub mod environment;
pub mod guid;
pub mod isoweek;
pub mod json;
pub mod mathf;
pub mod random;
pub mod regex;
pub mod stopwatch;
pub mod stringbuilder;
pub mod timespan;
pub mod uri;

#[cfg(test)]
mod tests {
    use super::{random::Random, stopwatch::Stopwatch, stringbuilder::StringBuilder, uri::Uri};

    fn assert_send<T: Send>() {}

    #[test]
    fn bcl_class_wrappers_own_native_sized_roots() {
        assert_send::<Random>();
        assert_send::<Stopwatch>();
        assert_send::<StringBuilder>();
        assert_send::<Uri>();

        assert_eq!(
            core::mem::size_of::<Random>(),
            core::mem::size_of::<*mut u8>()
        );
        assert_eq!(
            core::mem::size_of::<Stopwatch>(),
            core::mem::size_of::<*mut u8>()
        );
        assert_eq!(
            core::mem::size_of::<StringBuilder>(),
            core::mem::size_of::<*mut u8>()
        );
        assert_eq!(core::mem::size_of::<Uri>(), core::mem::size_of::<*mut u8>());

        // A `ManagedRef` owns and frees its GCHandle. These wrappers must therefore remain
        // move-owned values instead of regressing to naked, freely copied CLR references.
        assert!(core::mem::needs_drop::<Random>());
        assert!(core::mem::needs_drop::<Stopwatch>());
        assert!(core::mem::needs_drop::<StringBuilder>());
        assert!(core::mem::needs_drop::<Uri>());
    }
}
