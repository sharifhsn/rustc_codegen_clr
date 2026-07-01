use runtime::interop_services::Marshal;

pub mod console;
pub mod diagnostics;
pub mod runtime;
pub mod text;
// `System.String` physically lives in `System.Private.CoreLib` (it's only type-*forwarded* from
// `System.Runtime`). Binding the assembly to `System.Runtime` makes instance-method calls emit a
// `call instance ... [System.Runtime]System.String::method` methodref that the JIT rejects as
// "Bad IL format" once the value is a real CoreLib String (e.g. a `get_FullName()` result). Use
// the defining assembly, matching every other `System.String` binding in the tree.
pub type MString =
    crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.String">;

impl From<&str> for MString {
    fn from(val: &str) -> Self {
        Marshal::static2::<"PtrToStringUTF8", isize, i32, MString>(
            val.as_ptr() as isize,
            val.len() as i32,
        )
    }
}

/// An idiomatic, first-class wrapper over a managed `System.String`.
///
/// Unlike the raw [`MString`] handle (which is a bare managed-class alias, so a blanket trait impl on
/// it would wrongly cover *every* managed class), `DotNetString` is a newtype that can carry the
/// std traits that genuinely map to `System.String`'s *value* semantics:
///
/// * [`Display`](core::fmt::Display) / [`Debug`](core::fmt::Debug) — via the actual UTF-16 content
///   (round-trips: `format!("{}", DotNetString::from("hi")) == "hi"`).
/// * [`PartialEq`] / [`Eq`] — via `String.op_Equality` (ordinal *content* equality, not reference
///   identity).
/// * [`Hash`](core::hash::Hash) — via `String.GetHashCode` (content-based, so it is consistent with
///   the content equality above).
///
/// Construct one from a Rust string with `DotNetString::from("…")`; get the underlying handle with
/// [`DotNetString::handle`] for lower-level BCL calls.
#[derive(Clone, Copy)]
pub struct DotNetString(MString);

impl DotNetString {
    /// Wrap a raw managed `System.String` handle.
    #[inline(always)]
    pub fn from_handle(h: MString) -> Self {
        DotNetString(h)
    }
    /// The underlying managed handle, for lower-level BCL calls.
    #[inline(always)]
    pub fn handle(self) -> MString {
        self.0
    }
    /// Number of UTF-16 code units (`String.Length`) — note this is code *units*, matching .NET.
    #[inline(always)]
    pub fn len_utf16(self) -> i32 {
        self.0.instance0::<"get_Length", i32>()
    }
    /// The UTF-16 code unit at `idx` (`String.get_Chars`), as a raw `u16`.
    #[inline(always)]
    fn code_unit_at(self, idx: i32) -> u16 {
        self.0
            .instance1::<"get_Chars", i32, crate::DotNetChar>(idx)
            .as_u16()
    }
    /// Content equality (`String.op_Equality`, ordinal).
    #[inline(always)]
    pub fn equals(self, other: Self) -> bool {
        MString::static2::<"op_Equality", MString, MString, bool>(self.0, other.0)
    }
    /// The managed hash code (`String.GetHashCode`) — content-based within a process.
    #[inline(always)]
    pub fn hash_code(self) -> i32 {
        self.0.virt0::<"GetHashCode", i32>()
    }
    /// Copy the managed string's content into a Rust [`String`], decoding UTF-16 (surrogate pairs
    /// handled; a lone surrogate becomes U+FFFD).
    pub fn to_rust_string(self) -> std::string::String {
        let n = self.len_utf16();
        let mut units = std::vec::Vec::with_capacity(n.max(0) as usize);
        let mut i = 0;
        while i < n {
            units.push(self.code_unit_at(i));
            i += 1;
        }
        std::char::decode_utf16(units.into_iter())
            .map(|r| r.unwrap_or(core::char::REPLACEMENT_CHARACTER))
            .collect()
    }

    // ---- The common `System.String` methods, surfaced idiomatically. Each delegates to the real
    // ---- BCL member on the underlying `System.String` handle (the same concrete type as
    // ---- `System::String`, so its instance methods are inherent on `MString`).

    /// The empty managed string (`""`).
    #[inline(always)]
    pub fn empty() -> Self {
        DotNetString::from("")
    }
    /// Whether the string has zero UTF-16 code units.
    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.len_utf16() == 0
    }
    /// `String.Contains` — whether `needle` occurs as a substring.
    #[inline(always)]
    pub fn contains(self, needle: DotNetString) -> bool {
        self.0.instance1::<"Contains", MString, bool>(needle.0)
    }
    /// `String.StartsWith` (ordinal) — whether the string begins with `prefix`.
    #[inline(always)]
    pub fn starts_with(self, prefix: DotNetString) -> bool {
        self.0.instance1::<"StartsWith", MString, bool>(prefix.0)
    }
    /// `String.EndsWith` (ordinal) — whether the string ends with `suffix`.
    #[inline(always)]
    pub fn ends_with(self, suffix: DotNetString) -> bool {
        self.0.instance1::<"EndsWith", MString, bool>(suffix.0)
    }
    /// `String.IndexOf` — the UTF-16 code-unit index of the first occurrence of `needle`, or `-1`.
    #[inline(always)]
    pub fn index_of(self, needle: DotNetString) -> i32 {
        self.0.instance1::<"IndexOf", MString, i32>(needle.0)
    }
    /// `String.ToUpperInvariant` — an uppercased copy (culture-invariant).
    #[inline(always)]
    pub fn to_upper(self) -> DotNetString {
        DotNetString(self.0.instance0::<"ToUpperInvariant", MString>())
    }
    /// `String.ToLowerInvariant` — a lowercased copy (culture-invariant).
    #[inline(always)]
    pub fn to_lower(self) -> DotNetString {
        DotNetString(self.0.instance0::<"ToLowerInvariant", MString>())
    }
    /// `String.Trim` — a copy with leading/trailing whitespace removed.
    #[inline(always)]
    pub fn trim(self) -> DotNetString {
        DotNetString(self.0.instance0::<"Trim", MString>())
    }
    /// `String.Substring(start)` — the tail of the string from UTF-16 index `start`.
    #[inline(always)]
    pub fn substring(self, start: i32) -> DotNetString {
        DotNetString(self.0.instance1::<"Substring", i32, MString>(start))
    }
    /// `String.Replace(old, new)` — every occurrence of `old` replaced by `new`.
    #[inline(always)]
    pub fn replace(self, old: DotNetString, new: DotNetString) -> DotNetString {
        DotNetString(
            self.0
                .instance2::<"Replace", MString, MString, MString>(old.0, new.0),
        )
    }
    /// `String.Concat(a, b)` — the concatenation of two managed strings.
    #[inline(always)]
    pub fn concat(self, other: DotNetString) -> DotNetString {
        DotNetString(MString::static2::<"Concat", MString, MString, MString>(
            self.0, other.0,
        ))
    }
    /// `String.CompareOrdinal` — ordinal (code-unit) ordering: `<0`, `0`, or `>0`.
    #[inline(always)]
    fn compare_ordinal(self, other: DotNetString) -> i32 {
        MString::static2::<"CompareOrdinal", MString, MString, i32>(self.0, other.0)
    }
}

impl From<&str> for DotNetString {
    fn from(val: &str) -> Self {
        DotNetString(MString::from(val))
    }
}

impl From<&std::string::String> for DotNetString {
    fn from(val: &std::string::String) -> Self {
        DotNetString::from(val.as_str())
    }
}

impl From<DotNetString> for std::string::String {
    fn from(val: DotNetString) -> Self {
        val.to_rust_string()
    }
}

impl core::str::FromStr for DotNetString {
    type Err = core::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DotNetString::from(s))
    }
}

impl Default for DotNetString {
    /// The empty managed string (`""`).
    fn default() -> Self {
        DotNetString::empty()
    }
}

// Seamless `&str` comparison so `dotnet_string == "literal"` reads naturally.
impl PartialEq<&str> for DotNetString {
    fn eq(&self, other: &&str) -> bool {
        *self == DotNetString::from(*other)
    }
}
impl PartialEq<DotNetString> for &str {
    fn eq(&self, other: &DotNetString) -> bool {
        DotNetString::from(*self) == *other
    }
}

impl core::cmp::PartialOrd for DotNetString {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl core::cmp::Ord for DotNetString {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Ordinal (code-unit) ordering, consistent with the ordinal `PartialEq` above.
        self.compare_ordinal(*other).cmp(&0)
    }
}

// `a + b` and `a += b` concatenate, like Rust's `String`.
impl core::ops::Add for DotNetString {
    type Output = DotNetString;
    fn add(self, rhs: DotNetString) -> DotNetString {
        self.concat(rhs)
    }
}
impl core::ops::AddAssign for DotNetString {
    fn add_assign(&mut self, rhs: DotNetString) {
        *self = self.concat(rhs);
    }
}

impl core::fmt::Display for DotNetString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let n = self.len_utf16();
        let mut units = std::vec::Vec::with_capacity(n.max(0) as usize);
        let mut i = 0;
        while i < n {
            units.push(self.code_unit_at(i));
            i += 1;
        }
        for r in std::char::decode_utf16(units.into_iter()) {
            f.write_str(
                r.unwrap_or(core::char::REPLACEMENT_CHARACTER)
                    .encode_utf8(&mut [0u8; 4]),
            )?;
        }
        Ok(())
    }
}

impl core::fmt::Debug for DotNetString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.to_rust_string(), f)
    }
}

impl PartialEq for DotNetString {
    fn eq(&self, other: &Self) -> bool {
        self.equals(*other)
    }
}
impl Eq for DotNetString {}

impl core::hash::Hash for DotNetString {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // The managed content hash — equal strings hash equally, matching `PartialEq` above.
        state.write_i32(self.hash_code());
    }
}
