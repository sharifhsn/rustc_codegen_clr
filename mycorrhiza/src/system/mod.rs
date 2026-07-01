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
}

impl From<&str> for DotNetString {
    fn from(val: &str) -> Self {
        DotNetString(MString::from(val))
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
