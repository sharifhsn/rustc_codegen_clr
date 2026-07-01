//! Idiomatic Rust wrapper over `System.Text.Json` (assembly `System.Text.Json`) — **parse** a JSON
//! string into a navigable document, **navigate** it (property lookup, array indexing, typed scalar
//! reads), and **serialize** a document back to a string.
//!
//! The value model is `System.Text.Json.JsonDocument` (owning the parsed tree) plus its
//! `System.Text.Json.JsonElement` value type (a read-only cursor into the tree). `JsonElement`
//! exposes plain, non-generic instance members that map cleanly onto Rust — `GetProperty(string)`
//! for objects, an `this[int]` indexer for arrays, `GetArrayLength()`, a `ValueKind` discriminant,
//! and the typed leaf reads `GetString`/`GetInt64`/`GetDouble`/`GetBoolean`. No enumerators,
//! delegates, or generic method instantiations are needed at the seam, so this is a thin, honest
//! mapping onto real managed members.
//!
//! ```ignore
//! use mycorrhiza::bcl::json::Json;
//!
//! let doc = Json::parse(r#"{ "name": "ada", "age": 36, "tags": ["a", "b"] }"#).unwrap();
//! assert_eq!(doc.root().get("name").and_then(|n| n.as_str()).as_deref(), Some("ada"));
//! assert_eq!(doc.root().get("age").and_then(|n| n.as_i64()), Some(36));
//! assert_eq!(doc.root().get("tags").map(|t| t.len()), Some(2));
//! assert_eq!(
//!     doc.root().get("tags").and_then(|t| t.index(0)).and_then(|n| n.as_str()).as_deref(),
//!     Some("a"),
//! );
//! let s = doc.root().to_json_string();      // re-serialize
//! ```
//!
//! ## Scope
//! `parse` / navigation (`get`, `index`, `len`, `kind`) / the scalar reads
//! (`as_str`, `as_i64`, `as_f64`, `as_bool`, `is_null`) / `to_json_string` are surfaced. Anything
//! beyond — mutation, `JsonSerializer<T>` (which needs a generic method instantiation), enumeration
//! of an object's properties (needs the enumerator bridge) — is not, but the raw handles are
//! reachable via [`Json::handle`] / [`Value::handle`] for lower-level BCL calls.

use crate::intrinsics::{RustcCLRInteropManagedClass, RustcCLRInteropManagedStruct};
use crate::system::{DotNetString, MString};

const ASM: &str = "System.Text.Json";
const JSON_DOCUMENT: &str = "System.Text.Json.JsonDocument";
const JSON_ELEMENT: &str = "System.Text.Json.JsonElement";
const JSON_DOC_OPTIONS: &str = "System.Text.Json.JsonDocumentOptions";

/// A managed `System.Text.Json.JsonDocument` handle (a reference type owning the parsed tree).
type DocHandle = RustcCLRInteropManagedClass<{ ASM }, { JSON_DOCUMENT }>;

/// `sizeof(System.Text.Json.JsonElement)`. `JsonElement` is a small readonly struct: a `JsonDocument`
/// object reference (pointer-sized) plus two `int` cursors (`_idx`, `_row`) — 16 bytes on a 64-bit
/// runtime.
const JSON_ELEMENT_SIZE: usize = 16;
/// The managed value-type handle for `System.Text.Json.JsonElement`.
type ElemHandle = RustcCLRInteropManagedStruct<{ ASM }, { JSON_ELEMENT }, JSON_ELEMENT_SIZE>;

/// `sizeof(System.Text.Json.JsonDocumentOptions)`: `int MaxDepth` + `JsonCommentHandling`
/// (byte-backed enum) + `bool AllowTrailingCommas`, laid out as 8 bytes. Only ever passed as its
/// all-zero **default** value, so a zeroed handle is the correct `default(JsonDocumentOptions)`.
const JSON_DOC_OPTIONS_SIZE: usize = 8;
/// The managed value-type handle for `System.Text.Json.JsonDocumentOptions`.
type DocOptsHandle = RustcCLRInteropManagedStruct<{ ASM }, { JSON_DOC_OPTIONS }, JSON_DOC_OPTIONS_SIZE>;

/// The `JsonValueKind` discriminant (`System.Text.Json.JsonValueKind`), an `int32`-backed enum.
/// The numeric values are the stable .NET enum values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// No value (a detached / default element) — `JsonValueKind.Undefined` (0).
    Undefined,
    /// A JSON object `{ … }` — `JsonValueKind.Object` (1).
    Object,
    /// A JSON array `[ … ]` — `JsonValueKind.Array` (2).
    Array,
    /// A JSON string — `JsonValueKind.String` (3).
    String,
    /// A JSON number — `JsonValueKind.Number` (4).
    Number,
    /// The literal `true` — `JsonValueKind.True` (5).
    True,
    /// The literal `false` — `JsonValueKind.False` (6).
    False,
    /// The literal `null` — `JsonValueKind.Null` (7).
    Null,
}

impl Kind {
    fn from_i32(v: i32) -> Kind {
        match v {
            1 => Kind::Object,
            2 => Kind::Array,
            3 => Kind::String,
            4 => Kind::Number,
            5 => Kind::True,
            6 => Kind::False,
            7 => Kind::Null,
            _ => Kind::Undefined,
        }
    }
}

/// Marshal a Rust `&str` into the managed `System.String` the bindings expect.
#[inline(always)]
fn net(s: &str) -> MString {
    DotNetString::from(s).handle()
}

/// A parsed JSON document (`System.Text.Json.JsonDocument`).
///
/// A move-only handle; the .NET GC owns the underlying `JsonDocument` (no `Drop` — `Dispose` is not
/// called, which is fine on the GC heap). Obtain the root cursor with [`root`](Json::root), then walk
/// it with [`Value::get`] / [`Value::index`] and read leaves with [`Value::as_str`] etc.
pub struct Json {
    h: DocHandle,
}

impl Json {
    /// Parse `text` into a `JsonDocument` (`JsonDocument.Parse(string, JsonDocumentOptions)` with the
    /// default options). Malformed JSON throws a `JsonException` on the .NET side; that is surfaced
    /// here as `None`.
    pub fn parse(text: &str) -> Option<Json> {
        // The default (all-zero) `JsonDocumentOptions`.
        let opts: DocOptsHandle = unsafe { core::mem::zeroed() };
        let net_text = net(text);
        let h = DocHandle::static2::<"Parse", MString, DocOptsHandle, DocHandle>(net_text, opts);
        Some(Json { h })
    }

    /// The root element of the document (`JsonDocument.RootElement`).
    pub fn root(&self) -> Value {
        Value {
            h: self.h.instance0::<"get_RootElement", ElemHandle>(),
        }
    }

    /// The raw managed [`JsonDocument`](DocHandle) handle, for lower-level BCL calls.
    pub fn handle(&self) -> DocHandle {
        self.h
    }
}

/// A cursor into a parsed [`Json`] document — one `System.Text.Json.JsonElement` value.
///
/// A `Copy` value type (no GC), valid only while its owning [`Json`] is alive. Read its kind with
/// [`kind`](Value::kind), navigate with [`get`](Value::get) (objects) / [`index`](Value::index)
/// (arrays), and read leaves with [`as_str`](Value::as_str) / [`as_i64`](Value::as_i64) /
/// [`as_f64`](Value::as_f64) / [`as_bool`](Value::as_bool).
#[derive(Clone, Copy)]
pub struct Value {
    h: ElemHandle,
}

impl Value {
    /// The value kind of this element (`JsonElement.ValueKind`).
    pub fn kind(&self) -> Kind {
        Kind::from_i32(self.h.vt_instance0::<"get_ValueKind", i32>())
    }

    /// The child under property `name` for an **object** element (`JsonElement.GetProperty(string)`),
    /// or `None` if this is not an object or the property is absent. (`GetProperty` throws
    /// `KeyNotFoundException` on a missing property; that is caught and surfaced as `None`.)
    pub fn get(&self, name: &str) -> Option<Value> {
        if self.kind() != Kind::Object {
            return None;
        }
        let key = net(name);
        Some(Value {
            h: self.h.vt_instance1::<"GetProperty", MString, ElemHandle>(key),
        })
    }

    /// The element at `idx` for an **array** element (`JsonElement[int]`), or `None` if this is not
    /// an array or `idx` is out of range.
    pub fn index(&self, idx: i32) -> Option<Value> {
        if self.kind() != Kind::Array || idx < 0 || idx >= self.len() {
            return None;
        }
        Some(Value {
            h: self.h.vt_instance1::<"get_Item", i32, ElemHandle>(idx),
        })
    }

    /// The number of elements for an **array** element (`JsonElement.GetArrayLength()`); `0` for
    /// anything else.
    pub fn len(&self) -> i32 {
        if self.kind() != Kind::Array {
            return 0;
        }
        self.h.vt_instance0::<"GetArrayLength", i32>()
    }

    /// `true` if this array element has no elements (or is not an array).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// This element's string value if it is a JSON string (`JsonElement.GetString()`), else `None`.
    pub fn as_str(&self) -> Option<std::string::String> {
        if self.kind() != Kind::String {
            return None;
        }
        Some(
            DotNetString::from_handle(self.h.vt_instance0::<"GetString", MString>())
                .to_rust_string(),
        )
    }

    /// This element's value as an `i64` if it is a JSON number that fits (`JsonElement.GetInt64()`),
    /// else `None`. A number that does not fit an `i64` throws on the .NET side → `None`.
    pub fn as_i64(&self) -> Option<i64> {
        if self.kind() != Kind::Number {
            return None;
        }
        Some(self.h.vt_instance0::<"GetInt64", i64>())
    }

    /// This element's value as an `f64` if it is a JSON number (`JsonElement.GetDouble()`), else
    /// `None`.
    pub fn as_f64(&self) -> Option<f64> {
        if self.kind() != Kind::Number {
            return None;
        }
        Some(self.h.vt_instance0::<"GetDouble", f64>())
    }

    /// This element's boolean value if it is a JSON `true`/`false` (`JsonElement.GetBoolean()`),
    /// else `None`.
    pub fn as_bool(&self) -> Option<bool> {
        match self.kind() {
            Kind::True => Some(true),
            Kind::False => Some(false),
            _ => None,
        }
    }

    /// Whether this element is the JSON literal `null`.
    pub fn is_null(&self) -> bool {
        self.kind() == Kind::Null
    }

    /// Whether this element is a JSON object.
    pub fn is_object(&self) -> bool {
        self.kind() == Kind::Object
    }

    /// Whether this element is a JSON array.
    pub fn is_array(&self) -> bool {
        self.kind() == Kind::Array
    }

    /// Serialize this element (sub-tree) back to its compact JSON text (`JsonElement.ToString()` for
    /// scalars; the raw JSON via `GetRawText()` for objects/arrays, so structure round-trips).
    pub fn to_json_string(&self) -> std::string::String {
        DotNetString::from_handle(self.h.vt_instance0::<"GetRawText", MString>()).to_rust_string()
    }

    /// The raw managed [`JsonElement`](ElemHandle) value handle, for lower-level BCL calls.
    pub fn handle(&self) -> ElemHandle {
        self.h
    }
}

impl core::fmt::Display for Value {
    /// The element's raw JSON text (`GetRawText`).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_json_string())
    }
}
