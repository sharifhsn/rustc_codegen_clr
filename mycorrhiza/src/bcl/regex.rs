//! Idiomatic wrapper over `System.Text.RegularExpressions.Regex` (assembly
//! `System.Text.RegularExpressions`), backed by a real managed `Regex` object on the CLR heap.
//!
//! This is a thin, honest mapping onto the low-level BCL bindings
//! ([`crate::System::Text::RegularExpressions`]): constructors are associated fns, methods are
//! `snake_case`, and .NET properties are getters. Rust `&str` inputs are marshalled to managed
//! `System.String` via [`crate::system::DotNetString`]; managed results come back either as a
//! [`Match`]/[`Matches`] handle or, for string-valued members, as a Rust [`String`].
//!
//! ```ignore
//! use mycorrhiza::bcl::regex::Regex;
//!
//! let re = Regex::new(r"(\d+)-(\d+)");
//! assert!(re.is_match("10-20"));
//! let m = re.find("10-20").unwrap();
//! assert_eq!(m.value(), "10-20");
//! assert_eq!(re.replace_all("a1-2b", "#"), "a#b");
//! ```
//!
//! Only the ~most-used surface is exposed. For anything beyond it (timeouts, options, split, named
//! groups by name, `MatchEvaluator` callbacks) reach for the raw bindings under
//! [`crate::System::Text::RegularExpressions`].

use crate::system::{DotNetString, MString};
use crate::System::String as NetString;
use crate::System::Text::RegularExpressions as bcl;

/// Marshal a Rust `&str` into the managed `System.String` handle the bindings expect.
#[inline(always)]
fn net(s: &str) -> NetString {
    // `DotNetString::handle()` and the bindings' `System::String` are the SAME concrete
    // `RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.String">` alias, so the handle
    // passes through unchanged.
    DotNetString::from(s).handle()
}

/// Decode a managed `System.String` handle (as returned by the bindings) into a Rust [`String`].
#[inline(always)]
fn rust(s: MString) -> std::string::String {
    DotNetString::from_handle(s).to_rust_string()
}

/// A compiled `System.Text.RegularExpressions.Regex`.
///
/// A move-only handle to a managed `Regex`; the .NET GC owns the object (no `Drop`). Construct with
/// [`Regex::new`], then query with [`is_match`](Regex::is_match) / [`find`](Regex::find) /
/// [`find_all`](Regex::find_all) / [`replace_all`](Regex::replace_all).
pub struct Regex {
    h: bcl::Regex,
}

impl Regex {
    /// Compile `pattern` into a new `Regex` (`new Regex(string)`). An invalid pattern throws a
    /// `RegexParseException` on the .NET side at construction time.
    pub fn new(pattern: &str) -> Self {
        Self {
            h: bcl::Regex::new(net(pattern)),
        }
    }

    /// Whether `input` contains a match for this pattern (`Regex.IsMatch`, instance semantics via a
    /// zero-length replacement probe is *not* used — see note). Here it delegates to the static
    /// `Regex.IsMatch(input, pattern)` using this instance's `ToString()` pattern so the call is a
    /// true instance-configured match.
    pub fn is_match(&self, input: &str) -> bool {
        // The generated bindings only expose the *static* `IsMatch(input, pattern)`. Feed it this
        // instance's own pattern text so the result matches the compiled `Regex`.
        bcl::Regex::is_match(net(input), self.h.to_string())
    }

    /// The first match in `input`, or `None` if there is none (`Regex.Match`).
    pub fn find(&self, input: &str) -> Option<Match> {
        let m = bcl::Regex::r#match(net(input), self.h.to_string());
        let m = Match { h: m };
        if m.success() {
            Some(m)
        } else {
            None
        }
    }

    /// All matches in `input`, in order (`Regex.Matches`). Iterate with [`Matches::iter`] / indexing.
    pub fn find_all(&self, input: &str) -> Matches {
        Matches {
            h: bcl::Regex::matches(net(input), self.h.to_string()),
        }
    }

    /// Replace **every** match of this pattern in `input` with `replacement`
    /// (`Regex.Replace` — .NET replaces all occurrences by default). `replacement` may use the .NET
    /// substitution syntax (`$1`, `$&`, `${name}`, …).
    pub fn replace_all(&self, input: &str, replacement: &str) -> std::string::String {
        rust(self.h.replace(net(input), net(replacement)))
    }

    /// The number of matches in `input` (`Regex.Count`).
    pub fn count(&self, input: &str) -> i32 {
        self.h.count(net(input))
    }

    /// Whether this regex matches right-to-left (`Regex.RightToLeft`).
    pub fn right_to_left(&self) -> bool {
        self.h.get_right_to_left()
    }

    /// The group number for a named group, or `-1` if absent (`Regex.GroupNumberFromName`).
    pub fn group_number_from_name(&self, name: &str) -> i32 {
        self.h.group_number_from_name(net(name))
    }

    /// The group name for a group number (`Regex.GroupNameFromNumber`).
    pub fn group_name_from_number(&self, number: i32) -> std::string::String {
        rust(self.h.group_name_from_number(number))
    }

    /// The pattern text this regex was constructed from (`Regex.ToString`).
    pub fn pattern(&self) -> std::string::String {
        rust(self.h.to_string())
    }

    /// The raw managed [`Regex`](bcl::Regex) handle, for lower-level BCL calls.
    pub fn handle(&self) -> bcl::Regex {
        self.h
    }

    // --- statics (no instance needed) ---------------------------------------------------------

    /// Whether `input` matches `pattern`, compiling `pattern` on the fly (`Regex.IsMatch` static).
    pub fn is_match_str(input: &str, pattern: &str) -> bool {
        bcl::Regex::is_match(net(input), net(pattern))
    }

    /// Escape the regex metacharacters in `text` (`Regex.Escape`).
    pub fn escape(text: &str) -> std::string::String {
        rust(bcl::Regex::escape(net(text)))
    }

    /// Reverse [`escape`](Regex::escape) — unescape a `\`-escaped pattern (`Regex.Unescape`).
    pub fn unescape(text: &str) -> std::string::String {
        rust(bcl::Regex::unescape(net(text)))
    }
}

impl core::fmt::Display for Regex {
    /// The pattern text (`Regex.ToString`).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.pattern())
    }
}

/// A single successful match (`System.Text.RegularExpressions.Match`).
///
/// A `Match` is-a `Group` is-a `Capture` in .NET, so it carries the capture's `value`/`index`/
/// `length` directly (via the binding's upcast) plus match-specific members (`groups`, `next_match`).
pub struct Match {
    h: bcl::Match,
}

impl Match {
    /// Whether this match succeeded (`Match.Success`). A `Match` obtained from [`Regex::find`] is
    /// always successful; this is meaningful for the tail of a [`next_match`](Match::next_match) chain.
    pub fn success(&self) -> bool {
        // `Success` is defined on the `Group` base; upcast the `Match` handle to reach it.
        bcl::Group::from(self.h).get_success()
    }

    /// The matched substring (`Capture.Value`).
    pub fn value(&self) -> std::string::String {
        rust(self.as_capture().get_value())
    }

    /// The zero-based position of the match in the input (`Capture.Index`).
    pub fn index(&self) -> i32 {
        self.as_capture().get_index()
    }

    /// The length of the matched substring, in UTF-16 code units (`Capture.Length`).
    pub fn length(&self) -> i32 {
        self.as_capture().get_length()
    }

    /// The captured groups of this match (`Match.Groups`); index `0` is the whole match.
    pub fn groups(&self) -> Groups {
        Groups {
            h: self.h.get_groups(),
        }
    }

    /// The next match after this one in the same input (`Match.NextMatch`), or `None` at the end.
    pub fn next_match(&self) -> Option<Match> {
        let m = Match {
            h: self.h.next_match(),
        };
        if m.success() {
            Some(m)
        } else {
            None
        }
    }

    /// The raw managed [`Match`](bcl::Match) handle, for lower-level BCL calls.
    pub fn handle(&self) -> bcl::Match {
        self.h
    }

    /// Upcast the `Match` handle to its `Capture` base to read `Value`/`Index`/`Length`.
    #[inline(always)]
    fn as_capture(&self) -> bcl::Capture {
        bcl::Capture::from(bcl::Group::from(self.h))
    }
}

impl core::fmt::Display for Match {
    /// The matched substring (`Capture.Value`).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.value())
    }
}

/// The collection of matches returned by [`Regex::find_all`] (`MatchCollection`).
pub struct Matches {
    h: bcl::MatchCollection,
}

impl Matches {
    /// Number of matches (`MatchCollection.Count`).
    pub fn len(&self) -> i32 {
        self.h.get_count()
    }

    /// `true` if there were no matches.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The match at `idx`, or `None` if out of range (`MatchCollection[idx]`, bounds-checked).
    pub fn get(&self, idx: i32) -> Option<Match> {
        if idx >= 0 && idx < self.len() {
            Some(Match {
                h: self.h.get_item(idx),
            })
        } else {
            None
        }
    }

    /// Iterate the matches by index (the collection must not change during iteration).
    pub fn iter(&self) -> MatchesIter<'_> {
        MatchesIter {
            matches: self,
            idx: 0,
            len: self.len(),
        }
    }

    /// The raw managed [`MatchCollection`](bcl::MatchCollection) handle.
    pub fn handle(&self) -> bcl::MatchCollection {
        self.h
    }
}

/// Index iterator over a [`Matches`] collection (see [`Matches::iter`]).
pub struct MatchesIter<'a> {
    matches: &'a Matches,
    idx: i32,
    len: i32,
}

impl<'a> Iterator for MatchesIter<'a> {
    type Item = Match;
    fn next(&mut self) -> Option<Match> {
        if self.idx < self.len {
            let m = self.matches.get(self.idx);
            self.idx += 1;
            m
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let rem = (self.len - self.idx).max(0) as usize;
        (rem, Some(rem))
    }
}

/// The captured groups of a [`Match`] (`GroupCollection`); index `0` is the whole match.
pub struct Groups {
    h: bcl::GroupCollection,
}

impl Groups {
    /// Number of groups, including group `0` (`GroupCollection.Count`).
    pub fn len(&self) -> i32 {
        self.h.get_count()
    }

    /// `true` if there are no groups (never true for a successful match — group `0` always exists).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The group at `idx`, or `None` if out of range (`GroupCollection[idx]`, bounds-checked).
    pub fn get(&self, idx: i32) -> Option<Group> {
        if idx >= 0 && idx < self.len() {
            Some(Group {
                h: self.h.get_item(idx),
            })
        } else {
            None
        }
    }

    /// Whether a group named `name` exists in the pattern (`GroupCollection.ContainsKey`).
    pub fn contains_name(&self, name: &str) -> bool {
        self.h.contains_key(net(name))
    }

    /// The raw managed [`GroupCollection`](bcl::GroupCollection) handle.
    pub fn handle(&self) -> bcl::GroupCollection {
        self.h
    }
}

/// A single captured group (`System.Text.RegularExpressions.Group`).
///
/// A `Group` is-a `Capture`, so it carries `value`/`index`/`length` plus the group's `success`/`name`.
pub struct Group {
    h: bcl::Group,
}

impl Group {
    /// Whether this group participated in the match (`Group.Success`).
    pub fn success(&self) -> bool {
        self.h.get_success()
    }

    /// The group's name (`Group.Name`) — the number as text for unnamed groups.
    pub fn name(&self) -> std::string::String {
        rust(self.h.get_name())
    }

    /// The captured substring (`Capture.Value`); empty when the group did not participate.
    pub fn value(&self) -> std::string::String {
        rust(bcl::Capture::from(self.h).get_value())
    }

    /// The zero-based position of the capture (`Capture.Index`).
    pub fn index(&self) -> i32 {
        bcl::Capture::from(self.h).get_index()
    }

    /// The length of the capture, in UTF-16 code units (`Capture.Length`).
    pub fn length(&self) -> i32 {
        bcl::Capture::from(self.h).get_length()
    }

    /// The raw managed [`Group`](bcl::Group) handle, for lower-level BCL calls.
    pub fn handle(&self) -> bcl::Group {
        self.h
    }
}

impl core::fmt::Display for Group {
    /// The captured substring (`Capture.Value`).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.value())
    }
}
