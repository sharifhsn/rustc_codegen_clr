//! Idiomatic Rust wrapper over `System.Uri` (assembly `System.Private.CoreLib`, type-forwarded from
//! `System.Runtime`).
//!
//! A [`Uri`] is a thin, `Copy` handle to a managed `System.Uri` object living on the CLR heap; this
//! module wraps the low-level bindings in [`crate::bindings::System::Uri`] so a `Uri` reads like a
//! normal Rust type — construct with [`Uri::new`], read the components as snake_case getters that
//! hand back Rust [`String`]s, and format it with [`Display`](core::fmt::Display) (via .NET
//! `ToString`).
//!
//! ```ignore
//! use mycorrhiza::bcl::uri::Uri;
//!
//! let u = Uri::new("https://user@example.com:8443/path/page?q=1#frag");
//! assert_eq!(u.scheme(), "https");
//! assert_eq!(u.host(), "example.com");
//! assert_eq!(u.port(), 8443);
//! assert_eq!(u.absolute_path(), "/path/page");
//! assert_eq!(u.query(), "?q=1");
//! assert_eq!(u.fragment(), "#frag");
//! println!("{u}"); // the canonical absolute URI, via ToString
//! ```
//!
//! This is a thin, honest mapping: every method delegates straight to the corresponding `System.Uri`
//! member and nothing is emulated. In particular [`Uri::new`] mirrors the managed constructor, which
//! **throws** (a managed `UriFormatException`) on a malformed or relative-only string rather than
//! returning an error — there is no fallible Rust-style constructor because that would require the
//! try/catch interop primitive this wrapper deliberately does not reach for.

use crate::bindings::System::Uri as MUri;
use crate::system::DotNetString;

/// A managed `System.Uri` — an absolute or relative URI. See the [module docs](self).
///
/// This is a move/`Copy` handle to a managed object; the .NET GC owns the underlying `System.Uri`,
/// so there is no `Drop`.
#[derive(Clone, Copy)]
pub struct Uri {
    h: MUri,
}

impl Uri {
    /// Parse an absolute URI (`new Uri(string)`).
    ///
    /// Mirrors the managed constructor: it **throws** a `UriFormatException` at runtime if `uri` is
    /// not a valid absolute URI. Use a syntactically valid, absolute string (e.g.
    /// `"https://example.com/path"`).
    pub fn new(uri: &str) -> Self {
        Self {
            h: MUri::new(DotNetString::from(uri).handle()),
        }
    }

    /// Wrap a raw managed `System.Uri` handle (e.g. one returned by another BCL call).
    #[inline(always)]
    pub fn from_handle(h: MUri) -> Self {
        Self { h }
    }

    /// The underlying managed handle, for lower-level BCL calls.
    #[inline(always)]
    pub fn handle(&self) -> MUri {
        self.h
    }

    /// The scheme, lower-cased and without the trailing `:` — e.g. `"https"` (`Uri.Scheme`).
    pub fn scheme(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_scheme()).to_rust_string()
    }

    /// The host component — e.g. `"example.com"` (`Uri.Host`).
    pub fn host(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_host()).to_rust_string()
    }

    /// The port, or the scheme's default port if none was specified (`Uri.Port`; `-1` if unknown).
    pub fn port(&self) -> i32 {
        self.h.get_port()
    }

    /// The absolute path — e.g. `"/path/page"` (`Uri.AbsolutePath`).
    pub fn absolute_path(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_absolute_path()).to_rust_string()
    }

    /// The full canonical absolute URI (`Uri.AbsoluteUri`).
    pub fn absolute_uri(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_absolute_uri()).to_rust_string()
    }

    /// The query component, including the leading `?` (empty if none) (`Uri.Query`).
    pub fn query(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_query()).to_rust_string()
    }

    /// The fragment, including the leading `#` (empty if none) (`Uri.Fragment`).
    pub fn fragment(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_fragment()).to_rust_string()
    }

    /// The user-info portion (`user:password`) before the `@`, if any (`Uri.UserInfo`).
    pub fn user_info(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_user_info()).to_rust_string()
    }

    /// The Authority component — host plus a non-default port (`Uri.Authority`).
    pub fn authority(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_authority()).to_rust_string()
    }

    /// The path and query together, e.g. `"/path/page?q=1"` (`Uri.PathAndQuery`).
    pub fn path_and_query(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_path_and_query()).to_rust_string()
    }

    /// The original, un-canonicalized string this `Uri` was built from (`Uri.OriginalString`).
    pub fn original_string(&self) -> std::string::String {
        DotNetString::from_handle(self.h.get_original_string()).to_rust_string()
    }

    /// `true` if this is an absolute URI (`Uri.IsAbsoluteUri`).
    pub fn is_absolute(&self) -> bool {
        self.h.get_is_absolute_uri()
    }

    /// `true` if the scheme is `file:` (`Uri.IsFile`).
    pub fn is_file(&self) -> bool {
        self.h.get_is_file()
    }

    /// `true` if this URI references the local host (`Uri.IsLoopback`).
    pub fn is_loopback(&self) -> bool {
        self.h.get_is_loopback()
    }

    /// `true` if the port is the default for the scheme (`Uri.IsDefaultPort`).
    pub fn is_default_port(&self) -> bool {
        self.h.get_is_default_port()
    }

    /// Whether this URI is a base of `other` — i.e. `other` is reachable relative to `self`
    /// (`Uri.IsBaseOf`).
    pub fn is_base_of(&self, other: &Uri) -> bool {
        self.h.is_base_of(other.h)
    }

    /// Percent-encode a string for use as a URI *data* segment (`Uri.EscapeDataString`) — escapes
    /// everything that is not an unreserved character, including `/`, `?`, `#` and `&`.
    pub fn escape_data_string(value: &str) -> std::string::String {
        DotNetString::from_handle(MUri::escape_data_string(DotNetString::from(value).handle()))
            .to_rust_string()
    }

    /// Reverse [`Uri::escape_data_string`] — decode `%XX` sequences back to their characters
    /// (`Uri.UnescapeDataString`).
    pub fn unescape_data_string(value: &str) -> std::string::String {
        DotNetString::from_handle(MUri::unescape_data_string(
            DotNetString::from(value).handle(),
        ))
        .to_rust_string()
    }
}

/// The canonical absolute URI (`Uri.ToString`).
impl core::fmt::Display for Uri {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = DotNetString::from_handle(self.h.to_string());
        core::fmt::Display::fmt(&s, f)
    }
}

impl core::fmt::Debug for Uri {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = DotNetString::from_handle(self.h.to_string()).to_rust_string();
        f.debug_tuple("Uri").field(&s).finish()
    }
}

/// Value equality via `Uri.op_Equality` (component comparison, not reference identity).
impl PartialEq for Uri {
    fn eq(&self, other: &Self) -> bool {
        MUri::op_equality(self.h, other.h)
    }
}
impl Eq for Uri {}
