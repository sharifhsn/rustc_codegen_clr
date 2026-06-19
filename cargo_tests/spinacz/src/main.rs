#![feature(adt_const_params, unsized_const_params)]
use mycorrhiza::intrinsics::RustcCLRInteropManagedArray;
use mycorrhiza::intrinsics::RustcCLRInteropManagedClass;
use mycorrhiza::system::MString;

use mycorrhiza::{
    System::Reflection::Assembly, System::Reflection::AssemblyName,
    System::Reflection::ConstructorInfo, System::Reflection::MemberInfo,
    System::Reflection::MethodInfo, System::Reflection::ParameterInfo, System::Type,
};
use std::io::Write;

/// BCL assemblies we reflect. `std::env::args` is unavailable under the .NET PAL and
/// `AppDomain.CurrentDomain.GetAssemblies()` mis-binds on this backend, so the set is loaded
/// explicitly by name. Every name here is part of the base `Microsoft.NETCore.App` shared
/// framework and is therefore guaranteed present — `Assembly.Load` of a missing name would throw
/// (and we have no `Option<ManagedClass>` to absorb it), so the list is the broad always-present
/// surface, not anything optional. Type forwarding means a handful of these expose the bulk of
/// the `System.*` namespace; the per-namespace dedup in `add_tpe` collapses the overlaps.
const BCL_ASSEMBLIES: &[&str] = &[
    "System.Private.CoreLib",
    "System.Runtime",
    "System.Console",
    "System.Collections",
    "System.Collections.Concurrent",
    "System.Collections.NonGeneric",
    "System.Collections.Specialized",
    "System.Linq",
    "System.Linq.Expressions",
    "System.Memory",
    "System.Text.Encoding.Extensions",
    "System.Text.RegularExpressions",
    "System.Runtime.InteropServices",
    "System.Runtime.Numerics",
    "System.Threading",
    "System.Threading.Tasks",
    "System.Globalization",
    "System.ObjectModel",
    "System.ComponentModel",
    "System.ComponentModel.Primitives",
    "System.Diagnostics.Tracing",
    "System.Reflection.Primitives",
    "System.Private.Uri",
];

fn main() {
    // One root namespace holding every type from every assembly. The root itself is anonymous:
    // its children (`System`, `Microsoft`, …) are emitted at the top of the file, matching the
    // existing bindings.rs layout.
    let mut root_asm = Namespace::new(String::new(), 0);
    let mut out = std::fs::File::create("out.rs").unwrap();
    let mut total_types: i32 = 0;

    let asm_len = BCL_ASSEMBLIES.len();
    // NOTE: no `eprintln!`/`println!` for progress — std's `print*`/`eprint*` route through a
    // `dyn Write` + `core::fmt::write` path that faults under the current .NET PAL. The product is
    // the `out.rs` file (written via `writeln!` to a `File`, which works), and a final count is
    // reported via `Console::writeln_u64` (the proven-good managed Console path).
    let mut ai = 0;
    while ai < asm_len {
        let asm_name_str = BCL_ASSEMBLIES[ai];
        ai += 1;
        let mstr: MString = asm_name_str.into();
        let asm = Assembly::static1::<"Load", MString, Assembly>(mstr);
        let types = Assembly::virt0::<"GetTypes", RustcCLRInteropManagedArray<Type, 1>>(asm);
        let types_len = types.len();
        let mut idx = 0;
        while idx < types_len {
            let tpe = types.index(idx);
            idx += 1;
            // Only public types. Non-public BCL types are an implementation detail and would
            // bloat the surface (and many aren't loadable/callable anyway).
            if !Type::virt0::<"get_IsPublic", bool>(tpe) {
                continue;
            }
            let name = mstring_to_string(Type::virt0::<"get_FullName", MString>(tpe));
            if name.is_empty()
                || name.contains('`')
                || name.contains('+')
                || name.contains('<')
            {
                continue;
            }
            let tpe_asm = type_asm_string(tpe);
            let inherits = Type::virt0::<"get_BaseType", Type>(tpe);
            let inherits: String = if inherits.is_null() {
                "".into()
            } else {
                mstring_to_string(Type::virt0::<"get_FullName", MString>(inherits))
            };
            let is_valuetype = Type::virt0::<"get_IsValueType", bool>(tpe);
            // Reflect the (public) methods + constructors of this type into callable wrapper
            // definitions. Anything we can't faithfully express (generics, ref/out, pointers,
            // varargs, unbound types, unsupported arity) is dropped inside `reflect_methods`.
            let methods = reflect_methods(tpe, is_valuetype);
            root_asm.add_tpe(
                DotNetClassDef {
                    asm: tpe_asm,
                    full_name: name.clone(),
                    is_valuetype,
                    inherits,
                    methods,
                },
                &name,
            );
            total_types += 1;
        }
    }
    root_asm.export_root(&mut out);
    out.flush().unwrap();
    // Final progress signal via the managed Console (the std print path faults under the PAL).
    mycorrhiza::system::console::Console::writeln_u64(total_types as u64);
}

fn type_asm_string(tpe: Type) -> String {
    mstring_to_string(AssemblyName::virt0::<"get_Name", MString>(
        Assembly::virt0::<"GetName", AssemblyName>(Type::virt0::<"get_Assembly", Assembly>(tpe)),
    ))
}
fn mstring_to_string(mstr: MString) -> String {
    use mycorrhiza::system::runtime::interop_services::Marshal;

    let ptr = Marshal::static1::<"StringToCoTaskMemUTF8", MString, isize>(mstr);
    if ptr == 0 {
        return "".into();
    }
    let s = unsafe { std::ffi::CStr::from_ptr(ptr as *const std::ffi::c_char) }
        .to_str()
        .unwrap()
        .to_owned();
    Marshal::static1::<"FreeCoTaskMem", isize, ()>(ptr);
    s
}
/// A .NET type as it appears in a method signature, already resolved to the Rust
/// type we will spell in the generated wrapper.
///
/// Marshalling is handled by the codegen (the emitted call signature is built from
/// the monomorphized Rust generic signature of the magic-fn instance), so a wrapper
/// only needs a concrete Rust type per parameter + the return. Anything we cannot
/// faithfully name is represented as `Skip`, which forces the whole method to be
/// dropped.
enum DType {
    /// `void` -> `()`
    Void,
    /// A Rust primitive, e.g. `i32`, `f64`, `bool`.
    Prim(&'static str),
    /// A bound BCL reference type, spelled as its namespace-relative alias path
    /// (full .NET name with `.` -> `::`, e.g. `System::Object`). Resolved against the
    /// `use super::..*` globs the exporter already emits.
    Class(String),
    /// Cannot be expressed (unbound type, value type w/o alias, generic, etc.). The
    /// presence of a single `Skip` discards the whole method.
    Skip,
}
impl DType {
    /// Map a reflected `System.Type` to the Rust type used in a wrapper.
    ///
    /// Returns `Skip` for by-ref/out, pointer, generic, nested, and unbound types.
    pub fn from_tpe(tpe: Type) -> Self {
        // ref/out (`T&`) and pointer (`T*`) params need the not-yet-built marshalling
        // bridge (WF-9) -> skip the whole method.
        if Type::virt0::<"get_IsByRef", bool>(tpe) || Type::virt0::<"get_IsPointer", bool>(tpe) {
            return Self::Skip;
        }
        // Open generic params / constructed generics can't be named.
        if Type::virt0::<"get_IsGenericParameter", bool>(tpe)
            || Type::virt0::<"get_ContainsGenericParameters", bool>(tpe)
        {
            return Self::Skip;
        }
        let name = mstring_to_string(Type::virt0::<"get_FullName", MString>(tpe));
        // Reflection returns `null` FullName for some open/array types.
        if name.is_empty() {
            return Self::Skip;
        }
        if name == "System.Void" {
            return Self::Void;
        }
        if let Some(prim) = prim_for(&name) {
            return Self::Prim(prim);
        }
        // Generic / nested types are dropped by the type-alias pass too -> not bound.
        if name.contains('`') || name.contains('+') || name.contains('<') {
            return Self::Skip;
        }
        // Value types other than the recognised primitives have no generated alias
        // (the exporter only emits aliases for reference types), so we can't name them.
        if Type::virt0::<"get_IsValueType", bool>(tpe) {
            return Self::Skip;
        }
        Self::Class(name.replace('.', "::"))
    }
    /// The Rust type spelling for this `DType`, or `None` if it must be skipped.
    fn rust_ty(&self) -> Option<String> {
        match self {
            DType::Void => Some("()".into()),
            DType::Prim(p) => Some((*p).into()),
            DType::Class(path) => Some(path.clone()),
            DType::Skip => None,
        }
    }
}
/// Maps a .NET primitive `FullName` to its Rust spelling, or `None` if not a primitive.
fn prim_for(name: &str) -> Option<&'static str> {
    Some(match name {
        "System.Boolean" => "bool",
        "System.SByte" => "i8",
        "System.Byte" => "u8",
        "System.Int16" => "i16",
        "System.UInt16" => "u16",
        "System.Int32" => "i32",
        "System.UInt32" => "u32",
        "System.Int64" => "i64",
        "System.UInt64" => "u64",
        "System.IntPtr" => "isize",
        "System.UIntPtr" => "usize",
        "System.Single" => "f32",
        "System.Double" => "f64",
        _ => return None,
    })
}
type Sig = (Vec<DType>, DType);
// No `PartialEq` derive: comparisons use `matches!` instead (the derived enum `==` miscompiles
// to an oversized `transmute` here).
#[derive(Clone, Copy)]
enum MethodKind {
    Static,
    Instance,
    Virtual,
    Ctor,
}
/// A single callable wrapper we will emit on the type's inherent impl.
struct DotNetMethodDef {
    /// The .NET method name (e.g. `"WriteLine"`); empty for constructors.
    dotnet_name: String,
    /// The snake_case Rust fn name we expose.
    rust_name: String,
    kind: MethodKind,
    /// `(param types, return type)`. For instance/virtual methods the receiver is the
    /// implicit `self`, NOT included in `params`. For ctors the return is the type
    /// itself, so `sig.1` is unused.
    sig: Sig,
}
struct DotNetClassDef {
    full_name: String,
    asm: String,
    is_valuetype: bool,
    inherits: String,
    methods: Vec<DotNetMethodDef>,
}

/// Enumerate the public, declared methods + constructors of `tpe` and lower the ones we
/// can faithfully wrap into `DotNetMethodDef`s. Value types currently get no wrappers
/// (instance calls would need a by-ref receiver shape we don't emit yet).
fn reflect_methods(tpe: Type, is_valuetype: bool) -> Vec<DotNetMethodDef> {
    // METHOD-WRAPPER GATE (residual WF-3 codegen blocker):
    //
    // Emitting per-method/constructor wrappers requires the `reflect_params` path, which calls
    // the `GetParameters` reflection magic-fn and then threads a `(Vec<DType>, bool)` tuple back
    // through `reflect_one_method`/`reflect_one_ctor`. On the current backend that path lowers a
    // managed-array element access into a `calli` whose function-pointer operand is loaded with
    // an `LdInd { tpe: FnPtr(..) }` from an address that only points to a *data* `Ptr(..)` (the
    // typechecker flags this as `DerfWrongPtr { expected: FnPtr(..), got: Ptr(..) }`). .NET then
    // rejects the JIT of `reflect_one_method` with `System.BadImageFormatException: Bad IL
    // format`, aborting the whole run before any output is written.
    //
    // Bisection confirmed that returning here (before `GetMethods`/`GetConstructors` +
    // `reflect_params`) lets spinacz run end-to-end and emit the full namespaced type/alias +
    // `From`-impl binding surface (the target-independent product). Method wrappers are re-enabled
    // by deleting this early return once the `calli` fn-pointer-typing bug is fixed.
    let _ = (tpe, is_valuetype);
    return Vec::new();
    // Skip method emission for value types: the `RustcCLRInteropManagedStruct` helper
    // set is far thinner, and we don't emit struct aliases anyway.
    #[allow(unreachable_code)]
    if is_valuetype {
        return Vec::new();
    }
    let mut out: Vec<DotNetMethodDef> = Vec::new();
    // Track (rust_name, arity) we've already emitted so overloads don't collide as
    // duplicate inherent fns. First faithful overload wins.
    let mut seen: Vec<(String, usize)> = Vec::new();

    // We use the parameterless `GetMethods()` / `GetConstructors()` overloads (public members)
    // rather than the `BindingFlags` overloads: `BindingFlags` is a managed *enum* (value type)
    // we don't bind, and passing a bare `i32` produces a `GetMethods(int32)` signature that .NET
    // can't resolve. The no-arg overload returns inherited members too, so we restore the
    // DeclaredOnly behaviour by hand — keep only members whose `DeclaringType` is `tpe` itself.

    // --- Methods ------------------------------------------------------------
    let methods = Type::instance0::<
        "GetMethods",
        RustcCLRInteropManagedArray<MethodInfo, 1>,
    >(tpe);
    let methods_len = methods.len();
    let mut m = 0;
    while m < methods_len {
        let mi = methods.index(m);
        m += 1;
        // DeclaredOnly emulation: skip methods inherited from a base type.
        let decl = MethodInfo::virt0::<"get_DeclaringType", Type>(mi);
        if !decl.equality(tpe) {
            continue;
        }
        reflect_one_method(mi, &mut out, &mut seen);
    }

    // --- Constructors -------------------------------------------------------
    let ctors = Type::instance0::<
        "GetConstructors",
        RustcCLRInteropManagedArray<ConstructorInfo, 1>,
    >(tpe);
    let ctors_len = ctors.len();
    let mut c = 0;
    while c < ctors_len {
        let ci = ctors.index(c);
        c += 1;
        let decl = ConstructorInfo::virt0::<"get_DeclaringType", Type>(ci);
        if !decl.equality(tpe) {
            continue;
        }
        reflect_one_ctor(ci, &mut out, &mut seen);
    }
    out
}

fn push_unique(
    out: &mut Vec<DotNetMethodDef>,
    seen: &mut Vec<(String, usize)>,
    def: DotNetMethodDef,
) {
    let key = (def.rust_name.clone(), def.sig.0.len());
    if seen.iter().any(|k| *k == key) {
        return;
    }
    seen.push(key);
    out.push(def);
}

/// Lower a single `MethodInfo`, pushing a wrapper def into `out` if we can faithfully wrap it.
/// Skipped (nothing pushed) for: generic methods, ref/out/pointer params, varargs, unbound types,
/// or arity beyond the available `staticN`/`instanceN`/`virt0` helpers.
///
/// Pushes through `&mut out` rather than returning `Option<DotNetMethodDef>`: returning a
/// niche-optimized `Option<struct-with-String+Vec+enum>` and threading it through `?` made the
/// codegen emit IL the .NET JIT rejected as "Bad IL format". Direct `return;` skips + push avoids
/// constructing that Option entirely.
fn reflect_one_method(
    mi: MethodInfo,
    out: &mut Vec<DotNetMethodDef>,
    seen: &mut Vec<(String, usize)>,
) {
    // Generic methods (own type params) need the generic bridge (WF-9) -> skip.
    if MethodInfo::virt0::<"get_IsGenericMethod", bool>(mi)
        || MethodInfo::virt0::<"get_IsGenericMethodDefinition", bool>(mi)
        || MethodInfo::virt0::<"get_ContainsGenericParameters", bool>(mi)
    {
        return;
    }
    // (varargs / `__arglist` methods used to be filtered via `get_CallingConvention`, but that
    // getter returns the `CallingConventions` *enum*, which we don't bind and can't pass/receive
    // as an `i32` — the late-bound call would `MissingMethodException`. Public `__arglist` BCL
    // methods are vanishingly rare, and one slipping through is harmless: it just yields a wrapper
    // whose call won't resolve, never emitted on a hot path. So we drop the check.)
    let is_static = MethodInfo::virt0::<"get_IsStatic", bool>(mi);
    let is_virtual = MethodInfo::virt0::<"get_IsVirtual", bool>(mi);
    let is_abstract = MethodInfo::virt0::<"get_IsAbstract", bool>(mi);
    let dotnet_name = mstring_to_string(MethodInfo::virt0::<"get_Name", MString>(mi));

    let (params, ok) = reflect_params(MethodInfo::instance0::<
        "GetParameters",
        RustcCLRInteropManagedArray<ParameterInfo, 1>,
    >(mi));
    if !ok {
        return;
    }
    let ret_tpe = MethodInfo::virt0::<"get_ReturnType", Type>(mi);
    let ret = DType::from_tpe(ret_tpe);
    if matches!(ret, DType::Skip) {
        return;
    }

    let argc = params.len();
    let kind = if is_static {
        MethodKind::Static
    } else if is_virtual {
        MethodKind::Virtual
    } else {
        MethodKind::Instance
    };
    // Enforce the arity supported by the hand-written helper set.
    if !arity_supported(kind, argc) {
        return;
    }
    // A pure virtual (abstract) method with args has no virtN>0 helper to fall back to.
    // Use `matches!` (a `match`) rather than the derived `PartialEq` `==`: the codegen lowers a
    // derived enum `==` into a `transmute::<u128, MethodKind>` (a 16-byte -> 1-byte transmute)
    // that produces invalid IL.
    if is_abstract && matches!(kind, MethodKind::Virtual) && argc != 0 {
        return;
    }

    let Some(rust_name) = rust_method_name(&dotnet_name) else {
        return;
    };
    push_unique(
        out,
        seen,
        DotNetMethodDef {
            dotnet_name,
            rust_name,
            kind,
            sig: (params, ret),
        },
    );
}

/// Lower a single `ConstructorInfo` into a `ctorN` wrapper, pushing into `out` (or skipping).
fn reflect_one_ctor(
    ci: ConstructorInfo,
    out: &mut Vec<DotNetMethodDef>,
    seen: &mut Vec<(String, usize)>,
) {
    // Skip the static type initializer (`.cctor`), which is static.
    if ConstructorInfo::virt0::<"get_IsStatic", bool>(ci) {
        return;
    }
    // (No `get_CallingConvention` varargs check — see the note in `reflect_one_method`; the getter
    // returns the unbound `CallingConventions` enum.)
    let (params, ok) = reflect_params(ConstructorInfo::instance0::<
        "GetParameters",
        RustcCLRInteropManagedArray<ParameterInfo, 1>,
    >(ci));
    if !ok {
        return;
    }
    let argc = params.len();
    if !arity_supported(MethodKind::Ctor, argc) {
        return;
    }
    push_unique(
        out,
        seen,
        DotNetMethodDef {
            dotnet_name: String::new(),
            rust_name: "new".into(),
            kind: MethodKind::Ctor,
            sig: (params, DType::Void),
        },
    );
}

/// Lower a `ParameterInfo[]` to `DType`s. Returns `(params, ok)`; `ok == false` means a param was
/// unmappable and the whole method must be dropped.
///
/// Returns `(Vec, bool)` rather than `Option<Vec<DType>>`: extracting a `Vec` out of a
/// niche-optimized `Option<Vec<_>>` in the caller made the codegen emit IL the .NET JIT rejected
/// as "Bad IL format". A plain tuple sidesteps the niche.
fn reflect_params(params: RustcCLRInteropManagedArray<ParameterInfo, 1>) -> (Vec<DType>, bool) {
    let n = params.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let pi = params.index(i);
        i += 1;
        let ptpe = ParameterInfo::virt0::<"get_ParameterType", Type>(pi);
        // `out`/`in`/`ref` is encoded as a by-ref ParameterType; `from_tpe` skips those.
        let dt = DType::from_tpe(ptpe);
        if matches!(dt, DType::Skip) {
            return (Vec::new(), false);
        }
        out.push(dt);
    }
    (out, true)
}

/// Whether the hand-written helper set covers this `(kind, argc)`:
///   static:   static0/1/2
///   instance: instance0/1/2  (argc excludes the receiver)
///   virtual:  virt0  (argc==0); virtual w/ args falls through to instanceN below
///   ctor:     ctor0/1/2/3
fn arity_supported(kind: MethodKind, argc: usize) -> bool {
    match kind {
        MethodKind::Static => argc <= 2,
        // Virtual w/ args has no helper, but we emit it via instanceN (<=2). Pure
        // virt0 covers argc==0. Either way the cap is 2.
        MethodKind::Instance | MethodKind::Virtual => argc <= 2,
        MethodKind::Ctor => argc <= 3,
    }
}

/// Turn a .NET method name into a valid snake_case Rust ident, or `None` if it can't be.
/// Property getters (`get_X`) keep their `get_` prefix so they don't collide with a
/// same-named field/method and read naturally.
fn rust_method_name(dotnet_name: &str) -> Option<String> {
    if dotnet_name.is_empty() {
        return None;
    }
    // Reject names that aren't simple identifiers (operators like `op_Equality`, the
    // `.ctor`/`.cctor` special names, explicit-interface dotted names, etc.).
    if !dotnet_name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    if dotnet_name.chars().next().unwrap().is_ascii_digit() {
        return None;
    }
    let snake = to_snake_case(dotnet_name);
    if snake.is_empty() {
        return None;
    }
    // `self`/`Self`/`super`/`crate` can't be raw identifiers; just drop such methods.
    if matches!(snake.as_str(), "self" | "Self" | "super" | "crate") {
        return None;
    }
    // Drop names that would collide with the built-in inherent methods already defined
    // on `RustcCLRInteropManagedClass` (in mycorrhiza::intrinsics) — a duplicate inherent
    // fn is a hard error. Includes the helper families and the hand-written convenience
    // methods (`new` is the ctor wrapper we synthesize separately).
    const RESERVED: &[&str] = &[
        "ctor0", "ctor1", "ctor2", "ctor3", "static0", "static1", "static2", "instance0",
        "instance1", "instance2", "virt0", "to_mstring", "equality", "null", "is_null", "new",
    ];
    if RESERVED.contains(&snake.as_str()) {
        return None;
    }
    Some(escape_rust_keyword(snake))
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower_or_digit = false;
    for ch in name.chars() {
        if ch == '_' {
            out.push('_');
            prev_lower_or_digit = false;
            continue;
        }
        if ch.is_ascii_uppercase() {
            if prev_lower_or_digit {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_lower_or_digit = false;
        } else {
            out.push(ch);
            prev_lower_or_digit = true;
        }
    }
    // Collapse any accidental double underscores.
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out
}

fn escape_rust_keyword(name: String) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "box", "do", "final", "macro", "override",
        "priv", "typeof", "unsized", "virtual", "yield", "try", "abstract", "become", "gen",
    ];
    if KEYWORDS.contains(&name.as_str()) {
        format!("r#{name}")
    } else {
        name
    }
}

struct Namespace {
    // Child namespaces keyed by name. A `Vec` (not `HashMap`) on purpose: the namespace
    // fan-out is tiny, and `HashMap` pulls in the thread-local-seeded `RandomState`, whose
    // native-TLS lazy-init path currently faults under the .NET runtime. Linear lookup keeps
    // the generator on a codegen path that actually runs.
    inner: Vec<(String, Self)>,
    types: Vec<DotNetClassDef>,
    name: String,
    depth: u32,
}
impl Namespace {
    pub fn add_tpe(&mut self, tpe: DotNetClassDef, full_name_: &str) {
        let mut full_name = full_name_.split(".");
        let curr = full_name.next().unwrap();
        if let Some(_next) = full_name.next() {
            let depth = self.depth + 1;
            let curr_owned = curr.to_string();
            if !self.inner.iter().any(|(k, _)| *k == curr_owned) {
                self.inner
                    .push((curr_owned.clone(), Namespace::new(curr_owned.clone(), depth)));
            }
            let child = self
                .inner
                .iter_mut()
                .find(|(k, _)| *k == curr_owned)
                .map(|(_, v)| v)
                .unwrap();
            child.add_tpe(tpe, full_name_.split_once('.').unwrap().1)
        } else {
            // Dedup by full name: the same type can be reflected from several assemblies (type
            // forwarding, e.g. `System.String` forwarded from `System.Runtime` to CoreLib). A
            // duplicate `pub type` would be a hard error, so the first one wins.
            if self
                .types
                .iter()
                .any(|t| t.full_name == tpe.full_name)
            {
                return;
            }
            self.types.push(tpe);
        }
    }
    pub fn new(name: String, depth: u32) -> Self {
        Self {
            name,
            types: vec![],
            inner: Vec::new(),
            depth,
        }
    }
    pub fn export(&self, out: &mut impl Write) {
        writeln!(out, "pub mod {name}{{", name = self.name).unwrap();
        for (_, inner) in &self.inner {
            inner.export(out);
        }
        for tpe in &self.types {
            if !tpe.is_valuetype {
                let name = tpe.full_name.split('.').last().unwrap();
                writeln!(out,"pub type {name} =  mycorrhiza::intrinsics::RustcCLRInteropManagedClass<{tpe_asm:?},{full_name:?}>;",tpe_asm = tpe.asm,full_name = tpe.full_name ).unwrap();
                if self.depth > 0 {
                    writeln!(
                        out,
                        "use {}*;",
                        (0..self.depth)
                            .into_iter()
                            .map(|_| "super::")
                            .collect::<String>()
                    );
                }

                if !tpe.inherits.is_empty()
                    && !(tpe.inherits.contains("`")
                        || tpe.inherits.contains("+")
                        || tpe.inherits.contains("<"))
                {
                    writeln!(
                        out,
                        "impl From<{name}> for {inherits_path} {{\n fn from(v:{name})->{inherits_path}{{\nmycorrhiza::intrinsics::rustc_clr_interop_managed_checked_cast::<{inherits_path},{name}>(v)\n}}}} ",
                        inherits_path = tpe.inherits.replace(".", "::")
                    )
                    .unwrap();
                }

                Self::export_methods(out, name, &tpe.methods);
            }
        }
        writeln!(out, "}}").unwrap();
    }

    /// Export the root namespace's children + types WITHOUT a wrapping `pub mod`, so the file
    /// begins directly with the top-level namespaces (`pub mod System`, `pub mod Microsoft`, …),
    /// matching the existing bindings.rs layout. The root never holds types directly (every BCL
    /// type has at least one namespace segment), but we still emit any just in case.
    pub fn export_root(&self, out: &mut impl Write) {
        for (_, inner) in &self.inner {
            inner.export(out);
        }
        for tpe in &self.types {
            if !tpe.is_valuetype {
                let name = tpe.full_name.split('.').last().unwrap();
                writeln!(out,"pub type {name} =  mycorrhiza::intrinsics::RustcCLRInteropManagedClass<{tpe_asm:?},{full_name:?}>;",tpe_asm = tpe.asm,full_name = tpe.full_name ).unwrap();
                Self::export_methods(out, name, &tpe.methods);
            }
        }
    }

    /// Emit an inherent `impl <name> { .. }` block of callable wrappers, mirroring the
    /// hand-written `staticN`/`instanceN`/`virt0`/`ctorN` helpers in
    /// `mycorrhiza::intrinsics`.
    fn export_methods(out: &mut impl Write, name: &str, methods: &[DotNetMethodDef]) {
        if methods.is_empty() {
            return;
        }
        writeln!(out, "impl {name} {{").unwrap();
        for def in methods {
            // Resolve every param + return to a concrete Rust spelling. A `None` here
            // means a `Skip` slipped through; drop the wrapper defensively.
            let Some(body) = render_wrapper(def) else {
                continue;
            };
            writeln!(out, "{body}").unwrap();
        }
        writeln!(out, "}}").unwrap();
    }
}

/// Render the full `pub fn ...` text for one wrapper, or `None` if it can't be spelled.
fn render_wrapper(def: &DotNetMethodDef) -> Option<String> {
    let (params, ret) = &def.sig;
    // Param Rust types.
    let mut param_tys: Vec<String> = Vec::new();
    for p in params {
        param_tys.push(p.rust_ty()?);
    }
    let argc = param_tys.len();
    // `a1: T1, a2: T2, ...`
    let arg_decls: String = (0..argc)
        .map(|i| format!("a{}: {}", i + 1, param_tys[i]))
        .collect::<Vec<_>>()
        .join(", ");
    // `a1, a2, ...`
    let arg_names: String = (0..argc)
        .map(|i| format!("a{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    match def.kind {
        MethodKind::Ctor => {
            // pub fn new(a1:A1,..) -> Self { Self::ctorN(a1,..) }
            Some(format!(
                "    pub fn new({arg_decls}) -> Self {{ Self::ctor{argc}({arg_names}) }}"
            ))
        }
        MethodKind::Static => {
            // pub fn name(a1:A1,..) -> R { Self::staticN::<"M", A1,.., R>(a1,..) }
            let ret_ty = ret.rust_ty()?;
            let turbofish = call_turbofish(&def.dotnet_name, &param_tys, &ret_ty);
            let ret_sig = if ret_ty == "()" {
                String::new()
            } else {
                format!(" -> {ret_ty}")
            };
            Some(format!(
                "    pub fn {rn}({arg_decls}){ret_sig} {{ Self::static{argc}::<{turbofish}>({arg_names}) }}",
                rn = def.rust_name
            ))
        }
        MethodKind::Instance | MethodKind::Virtual => {
            let ret_ty = ret.rust_ty()?;
            let ret_sig = if ret_ty == "()" {
                String::new()
            } else {
                format!(" -> {ret_ty}")
            };
            let self_decls = if argc == 0 {
                "self".to_string()
            } else {
                format!("self, {arg_decls}")
            };
            // virtual + 0 args -> virt0 (covers property getters get_X); otherwise a
            // non-virtual instanceN call (no virtN>0 helper exists).
            let use_virt = matches!(def.kind, MethodKind::Virtual) && argc == 0;
            let helper = if use_virt {
                "virt0".to_string()
            } else {
                format!("instance{argc}")
            };
            // Turbofish: instance/virt helpers take <"M", Arg1.., Ret> (receiver implicit).
            let turbofish = call_turbofish(&def.dotnet_name, &param_tys, &ret_ty);
            Some(format!(
                "    pub fn {rn}({self_decls}){ret_sig} {{ self.{helper}::<{turbofish}>({arg_names}) }}",
                rn = def.rust_name
            ))
        }
    }
}

/// Build the `"Method", A1, A2, .., R` turbofish shared by the `staticN`/`instanceN`/
/// `virt0` helpers (the receiver, when present, is implicit and not listed).
fn call_turbofish(dotnet_name: &str, param_tys: &[String], ret_ty: &str) -> String {
    let mut parts = vec![format!("{dotnet_name:?}")];
    parts.extend(param_tys.iter().cloned());
    parts.push(ret_ty.to_string());
    parts.join(", ")
}
