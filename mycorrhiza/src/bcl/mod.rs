//! Idiomatic Rust wrappers over the most-reached-for **Base Class Library** value types and static
//! helpers — `DateTime`, `TimeSpan`, `Guid`, `Uri`, `Regex`, `Random`, `Stopwatch`, `StringBuilder`,
//! `Environment`, and `Math`.
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
//! Everything here is a thin, honest mapping — no behaviour is emulated in Rust. For anything beyond
//! the surfaced members, reach for the raw handle (each wrapper exposes a `handle()` escape hatch) and
//! call the low-level bindings directly.

pub mod datetime;
pub mod environment;
pub mod guid;
pub mod mathf;
pub mod random;
pub mod regex;
pub mod stopwatch;
pub mod stringbuilder;
pub mod timespan;
pub mod uri;
