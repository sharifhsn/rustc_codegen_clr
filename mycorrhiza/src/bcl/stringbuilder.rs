//! An idiomatic wrapper over the managed `System.Text.StringBuilder`
//! (assembly `System.Private.CoreLib`) — a growable, mutable buffer for building strings without
//! allocating a fresh `System.String` per concatenation.
//!
//! Use it like a Rust string builder; no knowledge of the CLR interop machinery (`instanceN`,
//! `System.String` marshalling, `callvirt`) is needed at the call site:
//!
//! ```ignore
//! use mycorrhiza::bcl::stringbuilder::StringBuilder;
//!
//! let mut sb = StringBuilder::new();
//! sb.append("Hello, ");
//! sb.append("world");
//! sb.append_char('!');
//! assert_eq!(sb.len(), 13);
//! assert_eq!(sb.to_rust_string(), "Hello, world!");
//! println!("{sb}"); // Display goes through StringBuilder.ToString()
//! ```
//!
//! **What this maps to.** [`StringBuilder`] is a thin newtype over the raw managed
//! `System.Text.StringBuilder` handle (a real object on the CLR heap, GC-owned — there is no
//! `Drop`). Every method delegates straight to the corresponding .NET member via the generated
//! low-level bindings; nothing is emulated in Rust except the `&str` → `System.String` marshalling
//! (which reuses [`crate::system::MString`]'s `From<&str>`) and the UTF-16 → Rust `String` decode
//! used by [`to_rust_string`](StringBuilder::to_rust_string) / [`Display`](core::fmt::Display).
//!
//! **Move-only.** Like [`crate::collections::List`], the wrapper is move-only rather than `Copy`: a
//! `StringBuilder` is mutable managed state, so copying the handle would silently alias one buffer.
//! Use [`handle`](StringBuilder::handle) to get the raw managed handle for lower-level BCL calls.

// The raw, generated low-level binding for `System.Text.StringBuilder`. It is defined in the impl
// assembly `System.Private.CoreLib` (where `System.String` also physically lives — binding against a
// forwarding assembly like `System.Runtime` makes the JIT reject the `System.String` methodrefs),
// which is exactly what an idiomatic wrapper wants to delegate to.
use crate::System::Text::StringBuilder as Raw;
use crate::system::{DotNetString, MString};

/// A managed `System.Text.StringBuilder`. See the [module docs](self).
pub struct StringBuilder {
    h: Raw,
}

impl StringBuilder {
    /// `new StringBuilder()` — an empty builder with the default capacity.
    #[inline]
    pub fn new() -> Self {
        Self { h: Raw::new() }
    }

    /// `new StringBuilder(capacity)` — an empty builder pre-sized to at least `capacity` chars.
    #[inline]
    pub fn with_capacity(capacity: i32) -> Self {
        // The `StringBuilder(int capacity)` ctor.
        Self {
            h: Raw::ctor1::<i32>(capacity),
        }
    }

    /// Build from an existing string, seeding the buffer with its content (`new StringBuilder(value)`).
    #[inline]
    pub fn from_str(value: &str) -> Self {
        Self {
            h: Raw::ctor1::<MString>(MString::from(value)),
        }
    }

    /// Wrap a raw managed `System.Text.StringBuilder` handle.
    #[inline]
    pub fn from_handle(h: Raw) -> Self {
        Self { h }
    }

    /// The underlying managed handle, for lower-level BCL calls.
    #[inline]
    pub fn handle(&self) -> Raw {
        self.h
    }

    /// The number of characters currently in the buffer (`Length`), in UTF-16 code units (matching
    /// .NET). This is the *content* length, not the capacity.
    #[inline]
    pub fn len(&self) -> i32 {
        self.h.get_length()
    }

    /// `true` if the buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Set the content length (`Length`). Growing pads with `'\0'`; shrinking truncates.
    #[inline]
    pub fn set_len(&mut self, length: i32) {
        self.h.set_length(length)
    }

    /// The current capacity (`Capacity`) — the size the buffer can hold before it must reallocate.
    #[inline]
    pub fn capacity(&self) -> i32 {
        self.h.get_capacity()
    }

    /// Set the capacity (`Capacity`). Must be at least the current [`len`](StringBuilder::len) or
    /// .NET throws.
    #[inline]
    pub fn set_capacity(&mut self, capacity: i32) {
        self.h.set_capacity(capacity)
    }

    /// The maximum capacity this builder can ever reach (`MaxCapacity`).
    #[inline]
    pub fn max_capacity(&self) -> i32 {
        self.h.get_max_capacity()
    }

    /// Ensure the capacity is at least `capacity`, reallocating if needed; returns the new capacity
    /// (`EnsureCapacity`).
    #[inline]
    pub fn ensure_capacity(&mut self, capacity: i32) -> i32 {
        self.h.ensure_capacity(capacity)
    }

    /// Append a string to the end of the buffer (`Append(string)`).
    #[inline]
    pub fn append(&mut self, value: &str) {
        // `Append` returns the same builder (for C# chaining); we discard it — the mutation is
        // in-place on the managed object `self.h` points at.
        let _ = self.h.append(MString::from(value));
    }

    /// Append a managed [`DotNetString`] without re-marshalling (`Append(string)`).
    #[inline]
    pub fn append_dotnet_string(&mut self, value: DotNetString) {
        let _ = self.h.append(value.handle());
    }

    /// Append a single `char` (`Append(char)`).
    ///
    /// Note: characters outside the Basic Multilingual Plane (astral, > U+FFFF) are not
    /// representable as a single .NET `System.Char` and are appended as U+FFFD; use
    /// [`append`](StringBuilder::append) with a `&str` for full Unicode.
    #[inline]
    pub fn append_char(&mut self, ch: char) {
        // The generated binding only wraps `Append(string)`; call the `Append(char)` overload
        // directly on the raw managed handle (it returns the same builder, which we discard).
        let mc = crate::DotNetChar::single_codepoint_unchecked(ch);
        let _ = self.h.instance1::<"Append", crate::DotNetChar, Raw>(mc);
    }

    /// Append the default line terminator (`AppendLine()`).
    #[inline]
    pub fn append_line(&mut self) {
        let _ = self.h.append_line();
    }

    /// Append a string followed by the default line terminator (`Append(value)` + `AppendLine()`).
    #[inline]
    pub fn append_line_str(&mut self, value: &str) {
        self.append(value);
        self.append_line();
    }

    /// Insert a string at `index`, shifting the rest right (`Insert(index, value)`).
    #[inline]
    pub fn insert(&mut self, index: i32, value: &str) {
        let _ = self.h.insert(index, MString::from(value));
    }

    /// Remove `length` characters starting at `start` (`Remove(start, length)`).
    #[inline]
    pub fn remove(&mut self, start: i32, length: i32) {
        let _ = self.h.remove(start, length);
    }

    /// Replace every occurrence of `old` with `new` throughout the buffer (`Replace(old, new)`).
    #[inline]
    pub fn replace(&mut self, old: &str, new: &str) {
        let _ = self.h.replace(MString::from(old), MString::from(new));
    }

    /// Remove all characters, leaving an empty buffer (`Clear()`). Capacity is retained.
    #[inline]
    pub fn clear(&mut self) {
        let _ = self.h.clear();
    }

    /// Materialize the built content as a managed [`DotNetString`] (`ToString()`).
    #[inline]
    pub fn to_dotnet_string(&self) -> DotNetString {
        DotNetString::from_handle(self.h.to_string())
    }

    /// Copy the built content into a Rust [`String`] (`ToString()`, then UTF-16 → UTF-8 decode).
    #[inline]
    pub fn to_rust_string(&self) -> std::string::String {
        self.to_dotnet_string().to_rust_string()
    }
}

impl Default for StringBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&str> for StringBuilder {
    fn from(value: &str) -> Self {
        Self::from_str(value)
    }
}

// `Display` (and `Debug`) go through the managed `ToString()`, so `println!("{sb}")` prints the
// built content — the same text a C# `sb.ToString()` would produce.
impl core::fmt::Display for StringBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.to_dotnet_string(), f)
    }
}

impl core::fmt::Debug for StringBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.to_rust_string(), f)
    }
}

// `write!`/`writeln!` into the builder — the idiomatic Rust way to accumulate formatted text. Each
// piece is marshalled through `System.String`; this is what makes `write!(sb, "{x}")` work.
impl core::fmt::Write for StringBuilder {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.append(s);
        Ok(())
    }
    fn write_char(&mut self, c: char) -> core::fmt::Result {
        self.append_char(c);
        Ok(())
    }

    // `core::fmt::Write::write_fmt`'s *default* provided body is `core::fmt::write(self, args)`,
    // where the free fn `write` takes `&mut dyn Write` -- so the default forces an unsizing
    // coercion of `&mut Self` to a `&mut dyn Write` trait object at every `write!(sb, ..)` call
    // site. That coercion builds a fat pointer whose "data" half is type-erased to a raw `void*`
    // (see `src/unsize.rs`, `fat_ptr_to` in `rustc_codegen_clr_type`). `StringBuilder` is a thin
    // newtype directly over a managed `System.Text.StringBuilder` handle -- a real GC-tracked
    // object reference -- so a pointer into it is itself GC-relevant memory; erasing that into an
    // untracked `void*` field would let the CLR's compacting GC relocate/collect the referent out
    // from under a stale, untracked address. The CIL type-verifier's `PtrCast` check correctly
    // refuses to emit that cast (`ManagedPtrCast`, invariant I1 of the absolute-correctness plan)
    // -- it is not a false positive, it is catching a genuine unsoundness in the generic fat-pointer
    // erasure path when the pointee transitively carries a managed reference (the same class of gap
    // documented for `Type::contains_gcref`; a general, sound fix needs a first-class
    // GC-tracked/byref-like fat-pointer representation, which is a larger architectural change
    // outside this fix's scope).
    //
    // The fix here: override `write_fmt` so `StringBuilder` never needs a `dyn Write` trait object
    // at all. Format into a plain `std::string::String` (an ordinary, gcref-free `Write` sink --
    // its own `write_fmt` goes through the very same default-provided coercion, but that is sound
    // because `String` contains no managed reference), then forward the fully rendered text with a
    // single direct (non-virtual) `append` call.
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> core::fmt::Result {
        let mut buf = std::string::String::new();
        buf.write_fmt(args)?;
        self.append(&buf);
        Ok(())
    }
}
