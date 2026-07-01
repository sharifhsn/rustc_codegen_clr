//! Idiomatic Rust wrapper over `System.Text.Json` (assembly `System.Text.Json`) — **parse** a JSON
//! string into a navigable document, **navigate** it (property lookup, array indexing, typed scalar
//! reads), and **serialize** it back to a string.
//!
//! The value model is `System.Text.Json.Nodes.JsonNode` (the mutable DOM). Unlike the read-only
//! `JsonElement` *value type* — which embeds a managed `JsonDocument` reference and so cannot be
//! carried inside a Rust `Option`/enum at the interop seam — `JsonNode` is a plain reference type: a
//! bare managed-object handle that composes cleanly. It exposes non-generic instance members that map
//! onto Rust: an object indexer (`node["prop"]`), an array indexer (`node[i]`), an array `Count`, a
//! `GetValueKind()` discriminant, and `ToJsonString()`/`ToString()` for (re)serialization. No
//! enumerators, delegates, or generic method instantiations are needed at the seam.
//!
//! ```ignore
//! use mycorrhiza::bcl::json::Json;
//!
//! let doc = Json::parse(r#"{ "name": "ada", "age": 36, "tags": ["a", "b"] }"#).unwrap();
//! assert_eq!(doc.get("name").and_then(|n| n.as_str()).as_deref(), Some("ada"));
//! assert_eq!(doc.get("age").and_then(|n| n.as_i64()), Some(36));
//! assert_eq!(doc.get("tags").map(|t| t.len()), Some(2));
//! assert_eq!(doc.get("tags").and_then(|t| t.index(0)).and_then(|n| n.as_str()).as_deref(), Some("a"));
//! let s = doc.to_json_string();      // re-serialize
//! ```
//!
//! ## Scope
//! `parse` / navigation (`get`, `index`, `len`, `kind`) / the scalar reads
//! (`as_str`, `as_i64`, `as_f64`, `as_bool`, `is_null`) / `to_json_string` are surfaced. Anything
//! beyond — construction/mutation, `JsonSerializer<T>` (needs a generic method instantiation),
//! enumeration of an object's properties (needs the enumerator bridge) — is not, but the raw
//! [`JsonNode`](NodeHandle) handle is reachable via [`Json::handle`] for lower-level BCL calls.
//!
//! ## `as_i64` / `as_f64` honesty
//! `JsonNode`'s typed value reads (`GetValue<long>()`) are *generic method* instantiations, which the
//! interop seam does not model. So numeric reads decode the node's own canonical text
//! (`JsonNode.ToJsonString()`, which for a number is its exact JSON token) with Rust's `str::parse`.
//! This is deterministic and lossless for values that fit the target type; anything else yields
//! `None`. `as_str` is exact: `JsonNode.ToString()` on a string value returns its raw content.

use crate::error::try_managed;
use crate::intrinsics::{RustcCLRInteropManagedClass, RustcCLRInteropManagedStruct};
use crate::system::{DotNetString, MString};

const ASM: &str = "System.Text.Json";
const JSON_NODE: &str = "System.Text.Json.Nodes.JsonNode";
const JSON_ARRAY: &str = "System.Text.Json.Nodes.JsonArray";
const JSON_NODE_OPTS_N: &str = "System.Nullable`1<System.Text.Json.Nodes.JsonNodeOptions>";
const JSON_DOC_OPTS: &str = "System.Text.Json.JsonDocumentOptions";

/// A managed `System.Text.Json.Nodes.JsonNode` handle (an object, array, value, or `null`).
type NodeHandle = RustcCLRInteropManagedClass<{ ASM }, { JSON_NODE }>;
/// A managed `System.Text.Json.Nodes.JsonArray` handle (a `JsonNode` subclass exposing `Count`).
type ArrayHandle = RustcCLRInteropManagedClass<{ ASM }, { JSON_ARRAY }>;
/// A managed `System.Nullable<JsonNodeOptions>` value (2 bytes: a bool flag + a bool option). Only
/// its all-zero (`None`) default is ever passed to `Parse`.
type NodeOptsHandle = RustcCLRInteropManagedStruct<{ ASM }, { JSON_NODE_OPTS_N }, 2>;
/// A managed `System.Text.Json.JsonDocumentOptions` value (8 bytes: an `int`, a byte enum, a bool).
/// Only its all-zero default is ever passed to `Parse`.
type DocOptsHandle = RustcCLRInteropManagedStruct<{ ASM }, { JSON_DOC_OPTS }, 8>;

/// The `JsonValueKind` discriminant (`System.Text.Json.JsonValueKind`), an `int32`-backed enum.
/// The numeric values are the stable .NET enum values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// No value (a detached / default node) — `JsonValueKind.Undefined` (0).
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

/// A parsed, navigable JSON document — a handle to a managed `JsonNode`.
///
/// A move-only handle; the .NET GC owns the underlying object (no `Drop`). Obtain the root with
/// [`Json::parse`], then walk it with [`get`](Json::get) / [`index`](Json::index) and read leaves
/// with [`as_str`](Json::as_str) / [`as_i64`](Json::as_i64) / [`as_bool`](Json::as_bool).
pub struct Json {
    h: NodeHandle,
}

impl Json {
    /// Parse `text` into a `JsonNode` DOM
    /// (`JsonNode.Parse(string, JsonNodeOptions?, JsonDocumentOptions)`, default options). The JSON
    /// literal `null` parses to a managed-`null` node and is surfaced as `None`; malformed input
    /// throws a `JsonException` on the .NET side, which is caught and also surfaced as `None`.
    pub fn parse(text: &str) -> Option<Json> {
        let node_opts: NodeOptsHandle = unsafe { core::mem::zeroed() };
        let doc_opts: DocOptsHandle = unsafe { core::mem::zeroed() };
        let net_text = net(text);
        // `JsonNode.Parse` is a 3-arg static (the two option params have C# defaults but the IL method
        // takes all three). No `static3` on the class wrapper, so call the raw call3 intrinsic with
        // IS_STATIC = true.
        let parsed = try_managed(|| {
            crate::intrinsics::rustc_clr_interop_managed_call3_::<
                { ASM },
                { JSON_NODE },
                false,
                "Parse",
                true,
                NodeHandle,
                MString,
                NodeOptsHandle,
                DocOptsHandle,
            >(net_text, node_opts, doc_opts)
        })
        .ok()?;
        Self::wrap(parsed)
    }

    #[inline(always)]
    fn wrap(h: NodeHandle) -> Option<Json> {
        let j = Json { h };
        if j.h.is_null() {
            None
        } else {
            Some(j)
        }
    }

    /// The value kind of this node (`JsonNode.GetValueKind()`).
    pub fn kind(&self) -> Kind {
        Kind::from_i32(self.h.instance0::<"GetValueKind", i32>())
    }

    /// The child under property `name` for an **object** node (`JsonNode.get_Item(string)`), or
    /// `None` if this is not an object or the property is absent / JSON `null`.
    pub fn get(&self, name: &str) -> Option<Json> {
        if self.kind() != Kind::Object {
            return None;
        }
        Self::wrap(self.h.instance1::<"get_Item", MString, NodeHandle>(net(name)))
    }

    /// The element at `idx` for an **array** node (`JsonNode.get_Item(int)`), or `None` if this is
    /// not an array, `idx` is out of range, or the element is JSON `null`.
    pub fn index(&self, idx: i32) -> Option<Json> {
        if self.kind() != Kind::Array || idx < 0 || idx >= self.len() {
            return None;
        }
        Self::wrap(self.h.instance1::<"get_Item", i32, NodeHandle>(idx))
    }

    /// The number of elements for an **array** node (`JsonArray.Count`); `0` for anything else.
    pub fn len(&self) -> i32 {
        if self.kind() != Kind::Array {
            return 0;
        }
        // `JsonArray` is-a `JsonNode`; the underlying managed reference is identical and both handle
        // aliases are the same `RustcCLRInteropManagedClass` layout (one pointer-sized field), so
        // reinterpreting the handle as `JsonArray` to reach its `get_Count` member is sound.
        let arr: ArrayHandle = unsafe { core::mem::transmute::<NodeHandle, ArrayHandle>(self.h) };
        arr.instance0::<"get_Count", i32>()
    }

    /// `true` if this array node has no elements (or is not an array).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// This node's string value if it is a JSON string (`kind() == String`), else `None`. Backed by
    /// `JsonNode.ToString()`, which for a string value returns the raw (unescaped) content.
    pub fn as_str(&self) -> Option<std::string::String> {
        if self.kind() != Kind::String {
            return None;
        }
        Some(self.text())
    }

    /// This node's value as an `i64` if it is a JSON number that fits, else `None`. See the module
    /// note on numeric honesty (decoded from the node's canonical JSON token).
    pub fn as_i64(&self) -> Option<i64> {
        if self.kind() != Kind::Number {
            return None;
        }
        self.json_text().parse::<i64>().ok()
    }

    /// This node's value as an `f64` if it is a JSON number, else `None`.
    pub fn as_f64(&self) -> Option<f64> {
        if self.kind() != Kind::Number {
            return None;
        }
        self.json_text().parse::<f64>().ok()
    }

    /// This node's boolean value if it is a JSON `true`/`false`, else `None`.
    pub fn as_bool(&self) -> Option<bool> {
        match self.kind() {
            Kind::True => Some(true),
            Kind::False => Some(false),
            _ => None,
        }
    }

    /// Whether this node is the JSON literal `null`. (A *missing* property is `None` from
    /// [`get`](Json::get), not a null node.)
    pub fn is_null(&self) -> bool {
        self.kind() == Kind::Null
    }

    /// Whether this node is a JSON object.
    pub fn is_object(&self) -> bool {
        self.kind() == Kind::Object
    }

    /// Whether this node is a JSON array.
    pub fn is_array(&self) -> bool {
        self.kind() == Kind::Array
    }

    /// Serialize this node (sub-tree) back to a compact JSON string (`JsonNode.ToJsonString()`).
    pub fn to_json_string(&self) -> std::string::String {
        self.json_text()
    }

    /// The raw managed [`JsonNode`](NodeHandle) handle, for lower-level BCL calls.
    pub fn handle(&self) -> NodeHandle {
        self.h
    }

    /// `JsonNode.ToString()` — for a string value node, its raw (unescaped) content.
    #[inline(always)]
    fn text(&self) -> std::string::String {
        DotNetString::from_handle(self.h.instance0::<"ToString", MString>()).to_rust_string()
    }

    /// `JsonNode.ToJsonString()` — the node's compact JSON representation.
    #[inline(always)]
    fn json_text(&self) -> std::string::String {
        DotNetString::from_handle(self.h.instance0::<"ToJsonString", MString>()).to_rust_string()
    }
}

impl core::fmt::Display for Json {
    /// The re-serialized JSON (`ToJsonString`).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_json_string())
    }
}
