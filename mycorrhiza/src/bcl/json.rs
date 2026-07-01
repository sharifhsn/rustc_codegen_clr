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
use crate::intrinsics::{
    RustcCLRInteropManagedClass, RustcCLRInteropManagedGenericStruct, RustcCLRInteropManagedStruct,
};
use crate::system::{DotNetString, MObject, MString};

const ASM: &str = "System.Text.Json";
const CORELIB: &str = "System.Private.CoreLib";
const JSON_NODE: &str = "System.Text.Json.Nodes.JsonNode";
const JSON_ARRAY: &str = "System.Text.Json.Nodes.JsonArray";
const NULLABLE: &str = "System.Nullable";
const JSON_NODE_OPTS: &str = "System.Text.Json.Nodes.JsonNodeOptions";
const JSON_DOC_OPTS: &str = "System.Text.Json.JsonDocumentOptions";
const JSON_VALUE_KIND: &str = "System.Text.Json.JsonValueKind";
const JSON_SERIALIZER_OPTS: &str = "System.Text.Json.JsonSerializerOptions";

/// A managed `System.Text.Json.Nodes.JsonNode` handle (an object, array, value, or `null`).
type NodeHandle = RustcCLRInteropManagedClass<{ ASM }, { JSON_NODE }>;
/// A managed `System.Text.Json.Nodes.JsonArray` handle (a `JsonNode` subclass exposing `Count`).
type ArrayHandle = RustcCLRInteropManagedClass<{ ASM }, { JSON_ARRAY }>;
/// The managed value type `System.Text.Json.JsonValueKind` — an `int32`-backed enum (4 bytes). It is
/// what `GetValueKind()` returns; read as its 4-byte `int32` payload.
type ValueKindHandle = RustcCLRInteropManagedStruct<{ ASM }, { JSON_VALUE_KIND }, 4>;
/// The reference type `System.Text.Json.JsonSerializerOptions`. Only a managed `null` (= "use the
/// defaults") is ever passed, to the `ToJsonString(JsonSerializerOptions?)` overload.
type SerializerOptsHandle = RustcCLRInteropManagedClass<{ ASM }, { JSON_SERIALIZER_OPTS }>;
/// The managed value type `System.Text.Json.Nodes.JsonNodeOptions` (one `bool` field → 1 byte). Only
/// used as the generic argument of the `Nullable<..>` below.
type NodeOpts = RustcCLRInteropManagedStruct<{ ASM }, { JSON_NODE_OPTS }, 1>;
/// A managed `System.Nullable<JsonNodeOptions>` value (2 bytes: a `bool has_value` + the 1-byte
/// option). A *generic value type* — its open type `System.Nullable`1` lives in `System.Private.CoreLib`,
/// instantiated with `JsonNodeOptions`. Only its all-zero (`None`) default is ever passed to `Parse`.
type NodeOptsHandle =
    RustcCLRInteropManagedGenericStruct<{ CORELIB }, { NULLABLE }, 2, (NodeOpts,)>;
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

/// Whether a `JsonNode` handle is a managed `null` reference. `JsonNode` overloads `==` in C#
/// (value-ish comparison) but does **not** expose a static `op_Equality` methodref usable here, so
/// reference-null is tested with `System.Object.ReferenceEquals(node, null)` — a CoreLib static that
/// works for any reference type. The `JsonNode` handle upcasts to `System.Object` (both are
/// pointer-sized object-reference handles), so the reinterpret is sound.
#[inline(always)]
fn node_is_null(h: NodeHandle) -> bool {
    let obj: MObject = unsafe { core::mem::transmute::<NodeHandle, MObject>(h) };
    MObject::static2::<"ReferenceEquals", MObject, MObject, bool>(obj, MObject::null())
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
        //
        // The managed `JsonNode` reference is written into `out` via the closure's captured `&mut`,
        // NOT returned through `try_managed` — a `Result<NodeHandle, _>` would place the object
        // reference inside a Rust enum niche, which the CLR layout rejects (a managed ref cannot be
        // overlapped by a discriminant). The closure returns `()`, so nothing managed crosses the
        // `try/catch` boundary; on a `JsonException` (malformed input) `out` stays null → `None`.
        let mut out = NodeHandle::null();
        let ran = try_managed(|| {
            out = crate::intrinsics::rustc_clr_interop_managed_call3_::<
                { ASM },
                { JSON_NODE },
                false,
                "Parse",
                true,
                NodeHandle,
                MString,
                NodeOptsHandle,
                DocOptsHandle,
            >(net_text, node_opts, doc_opts);
        });
        if ran.is_err() {
            return None;
        }
        Self::wrap(out)
    }

    #[inline(always)]
    fn wrap(h: NodeHandle) -> Option<Json> {
        if node_is_null(h) {
            None
        } else {
            Some(Json { h })
        }
    }

    /// The value kind of this node (`JsonNode.GetValueKind()`).
    pub fn kind(&self) -> Kind {
        // `GetValueKind` returns the `JsonValueKind` enum value type; read it as its 4-byte `int32`
        // payload (a .NET enum is exactly its underlying integer).
        let vk = self.h.instance0::<"GetValueKind", ValueKindHandle>();
        let raw = unsafe { core::mem::transmute::<ValueKindHandle, i32>(vk) };
        Kind::from_i32(raw)
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

    /// `JsonNode.ToJsonString(JsonSerializerOptions?)` — the node's compact JSON representation. The
    /// only IL overload takes an options argument (a *reference-type* `JsonSerializerOptions`, so a
    /// plain managed `null` selects the defaults — no generic value type needed here).
    #[inline(always)]
    fn json_text(&self) -> std::string::String {
        let opts = SerializerOptsHandle::null();
        DotNetString::from_handle(
            self.h
                .instance1::<"ToJsonString", SerializerOptsHandle, MString>(opts),
        )
        .to_rust_string()
    }
}

impl core::fmt::Display for Json {
    /// The re-serialized JSON (`ToJsonString`).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_json_string())
    }
}
