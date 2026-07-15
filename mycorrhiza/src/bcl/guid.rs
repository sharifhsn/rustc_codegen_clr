//! An idiomatic Rust wrapper over the .NET value type `System.Guid`
//! (assembly `System.Private.CoreLib`) — a 128-bit globally-unique identifier.
//!
//! `Guid` is a managed **value type** (a `struct` packing four integer fields into 16 bytes), so a
//! [`Guid`] here is stored inline, is `Copy`, and never touches the GC heap. It maps the most-used
//! members of the BCL type onto Rust names:
//!
//! * **Constructors / factories** → associated fns: [`new_v4`](Guid::new_v4) (`Guid.NewGuid()`,
//!   a fresh random UUID), [`empty`](Guid::empty) (`Guid.Empty`, the all-zero GUID), and
//!   [`parse`](Guid::parse) (`Guid.Parse(string)`).
//! * **Instance queries** → [`is_empty`](Guid::is_empty) and the managed
//!   [`hash_code`](Guid::hash_code) (`Guid.GetHashCode()`).
//! * **Std traits** → [`Display`](core::fmt::Display)/[`Debug`](core::fmt::Debug) (via `ToString`,
//!   the canonical `xxxxxxxx-xxxx-…` form), [`PartialEq`]/[`Eq`] (via `Guid.Equals`),
//!   [`PartialOrd`]/[`Ord`] (via `Guid.CompareTo`), [`Hash`](core::hash::Hash) (via
//!   `Guid.GetHashCode`), and [`Default`] (the empty GUID).
//!
//! ```ignore
//! use mycorrhiza::bcl::guid::Guid;
//!
//! let a = Guid::new_v4();                 // Guid.NewGuid()
//! let b = Guid::new_v4();
//! assert!(a != b);                        // (astronomically) distinct
//! assert_eq!(Guid::empty(), Guid::default());
//! let parsed = Guid::parse(mstr);         // Guid.Parse("…")
//! println!("{a}");                        // ToString() -> canonical form
//! ```
//!
//! This is a thin, honest mapping: every method delegates straight to the corresponding managed
//! member, with no added behaviour. The broader formatting/parsing surface (`ToString("N"/"B"/…)`,
//! `TryParse`, the many-argument ctors) is out of scope — reach for the raw handle via
//! [`Guid::handle`] for anything not surfaced here.

use crate::intrinsics::RustcCLRInteropManagedStruct;
use crate::system::MString;

// `System.Guid` physically lives in `System.Private.CoreLib` (it is only type-*forwarded* from
// `System.Runtime`), so — like `System.String`/`System.DateTime`/`System.TimeSpan` — method/ctor
// refs must name the defining assembly, or the JIT rejects the emitted IL once a real CoreLib `Guid`
// flows through it.
const CORELIB: &str = "System.Private.CoreLib";
const GUID: &str = "System.Guid";

/// The size (in bytes) of a managed `System.Guid`: four packed integer fields totalling 16 bytes
/// (`sizeof(Guid) == 16`).
const GUID_SIZE: usize = 16;

/// A managed `System.Guid` — a 128-bit globally-unique identifier, stored inline as a value type
/// (`Copy`, no GC).
///
/// This aliases the compiler's managed-value marker directly, so exported signatures and DTO
/// properties retain the genuine CLR `System.Guid` identity.
/// See the [module docs](self) for the full member map.
pub type Guid = RustcCLRInteropManagedStruct<{ CORELIB }, { GUID }, GUID_SIZE>;

impl Guid {
    // --- constructors / factories -----------------------------------------------------------------

    /// `Guid.NewGuid()` — a fresh, cryptographically-random version-4 UUID.
    #[inline(always)]
    pub fn new_v4() -> Self {
        // A zero-argument static factory returning a `Guid` value → a static value-type `call`.
        Self::vt_static0::<"NewGuid", Self>()
    }

    /// `Guid.Empty` — the all-zero GUID (`00000000-0000-0000-0000-000000000000`).
    #[inline(always)]
    pub fn empty() -> Self {
        // `Guid.Empty` is a static readonly *field*, not a method, so the method-based interop
        // machinery cannot read it. Parsing the canonical all-zero string yields the identical value.
        Self::parse(MString::from("00000000-0000-0000-0000-000000000000"))
    }

    /// `Guid.Parse(text)` — parse a managed string in any of the canonical GUID formats
    /// (e.g. `"xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"`). Malformed input throws `FormatException` in
    /// managed code.
    #[inline(always)]
    pub fn parse(text: MString) -> Self {
        // `Guid.Parse` is a *static* method returning a `Guid` value directly — no `newobj`/ctor is
        // needed, so this is a clean 1-arg static value-type `call` yielding the struct handle (the
        // ctor+transmute trick is both unnecessary here and rejected by the CIL type-verifier).
        Self::vt_static1::<"Parse", MString, Self>(text)
    }

    // --- instance queries -------------------------------------------------------------------------

    /// `true` if this is the empty (all-zero) GUID (`self == Guid.Empty`).
    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.equals(Self::empty())
    }

    /// The managed hash code (`Guid.GetHashCode`) — content-based within a process, so equal GUIDs
    /// hash equally.
    #[inline(always)]
    pub fn hash_code(self) -> i32 {
        self.vt_instance0::<"GetHashCode", i32>()
    }

    // --- comparison -------------------------------------------------------------------------------

    /// Value equality (`Guid.Equals(Guid)`) — two GUIDs are equal iff every byte matches, which is
    /// what a Rust user means by `==` on an identifier.
    #[inline(always)]
    pub fn equals(self, other: Self) -> bool {
        // A value-type instance method taking a `Guid` argument: `call instance` on the `valuetype`
        // receiver, argument by value.
        self.vt_instance1::<"Equals", Self, bool>(other)
    }

    /// Lexicographic comparison (`Guid.CompareTo`): negative if `self` sorts before `other`, zero if
    /// equal, positive if after. Matches .NET's field-by-field ordering, not raw byte order.
    #[inline(always)]
    pub fn compare_to(self, other: Self) -> i32 {
        self.vt_instance1::<"CompareTo", Self, i32>(other)
    }

    // --- interop escape hatch ---------------------------------------------------------------------

    /// The raw managed value-type handle, for lower-level BCL calls not surfaced here.
    #[inline(always)]
    pub fn handle(self) -> Self {
        self
    }

    /// Wrap a raw `System.Guid` value handle (e.g. one returned by another BCL call).
    #[inline(always)]
    pub fn from_raw(raw: Self) -> Self {
        raw
    }
}

impl Default for Guid {
    /// The empty (all-zero) GUID (`Guid.Empty`).
    #[inline(always)]
    fn default() -> Self {
        Self::empty()
    }
}

impl core::fmt::Display for Guid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `Guid.ToString()` yields the canonical `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` form; print
        // its UTF-16 content through the idiomatic string wrapper, which decodes to Rust text.
        let s =
            crate::system::DotNetString::from_handle((*self).vt_instance0::<"ToString", MString>());
        core::fmt::Display::fmt(&s, f)
    }
}

impl core::fmt::Debug for Guid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl PartialEq for Guid {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.equals(*other)
    }
}
impl Eq for Guid {}

impl PartialOrd for Guid {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Guid {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // `CompareTo` already returns the total-order sign, so map it straight onto `Ordering`.
        self.compare_to(*other).cmp(&0)
    }
}

impl core::hash::Hash for Guid {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // The managed content hash — equal GUIDs hash equally, matching `PartialEq` above.
        state.write_i32(self.hash_code());
    }
}
