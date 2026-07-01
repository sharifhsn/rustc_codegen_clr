//! One-glance import surface: `use mycorrhiza::prelude::*;` brings the everyday interop types and
//! macros into scope so a Rust-targets-.NET program reads like `std`.
//!
//! ```ignore
//! use mycorrhiza::prelude::*;
//!
//! let mut xs = List::<i32>::new();
//! xs.push(1);
//! xs.push(2);
//! let ys: List<i32> = vec![1, 2].into();
//! assert_eq!(xs, ys);                       // element-wise PartialEq
//!
//! let s = DotNetString::from("hi");
//! println!("{s}");                          // Display via the managed string
//! ```
//!
//! It intentionally re-exports only the *idiomatic* surface — the ready-to-use collection wrappers,
//! the managed-string type, and the WF-9 generic-bridge macros — not the low-level
//! [`crate::intrinsics`] magic. Reach into the fuller module paths when you need the raw handles.

// The .NET generic collections, used like `std`.
pub use crate::collections::{Dictionary, HashSet, List, ListIter, Queue, Stack};

// The enumerator bridge — `for x in &collection` over the reference-type collections, backed by the
// .NET `IEnumerator<T>`. The `Enumerable` trait provides `.iter_enumerator()`; `Enumerator<T>` is the
// resulting `Iterator`.
pub use crate::enumerate::{Enumerable, Enumerator};

// The idiomatic managed `System.String` wrapper (Display / == / Hash) and the raw handle alias.
pub use crate::system::{DotNetString, MString};

// The single-codepoint managed `char`.
pub use crate::DotNetChar;

// The WF-9 generic-interop macros, for hand-rolling a wrapper over any other generic .NET type.
// (`#[macro_export]`ed, so they also live at the crate root; re-exported here for discoverability.)
pub use crate::{dotnet_generic, dotnet_generic_impl, gen};
