//! Partition a too-large `MainModule` across per-module .NET classes.
//!
//! CoreCLR identifies every method in a type by a 16-bit slot number, so a single type can hold at
//! most 65,535 methods (effectively 65,521 static / 65,520 instance once the reserved + `Object`
//! slots are subtracted). A whole-program Rust→.NET build funnels every monomorphized free function
//! into one `MainModule` class; for a large program (e.g. the rust-lang/rust `coretests` test
//! harness — ~70k methods) that blows past the cap and the assembly fails to load with
//! `TypeLoadException: … contains more methods than the current implementation allows`.
//!
//! We split `MainModule`'s methods into one .NET class **per Rust module path**
//! (`MainModule.core.num.int_sqrt`, …). Module grouping — rather than a name hash — keeps
//! mutually-calling methods (a function, its closures, an `impl`'s methods, all monomorphizations of
//! one generic) in the same type, which matches how CoreCLR loads types: a type's MethodTable /
//! MethodDescChunks are built when the type is first touched, so co-locating callers with callees
//! minimizes how many types a given run has to load. Synthetic backend helpers (`transmute`, array
//! accessors, math intrinsics) and the entrypoint have non-demanglable names and stay in
//! `MainModule`. Any single module that still exceeds the cap is bin-packed across `name$1`, `$2`…
//! chunks (whole same-named overload sets kept together).
//!
//! **Correctness invariant:** the class assignment is a pure function of the method *name*, so a
//! method definition and every `call`/`ldftn`/`calli` reference to it resolve to the same class
//! with no cross-checking — and overloaded synthetic names (e.g. ~2k `transmute` signatures) all
//! share one name token → one class.

use crate::ir::MethodDefIdx;
use crate::{Assembly, IString, Interned};
use rustc_demangle::demangle;
use std::collections::{BTreeMap, HashMap};

/// Per-class method ceiling. Headroom below CoreCLR's 65,521 effective static-method limit so the
/// reserved slots + a margin always fit.
pub const PARTITION_LIMIT: usize = 60_000;

/// The literal residual class. Holds synthetic helpers, the entrypoint, and anything whose name
/// does not demangle to a Rust module path. C#-facing exported names (interop) live here too — and
/// because partitioning only triggers for assemblies far larger than any interop library, their
/// class stays stable in practice.
const RESIDUAL: &str = "MainModule";

pub struct ModulePartition {
    /// method-name token → the class that method is emitted in.
    name_to_class: HashMap<Interned<IString>, String>,
    /// (class name, its methods), deterministically ordered; includes the `MainModule` residual.
    classes: Vec<(String, Vec<MethodDefIdx>)>,
}

impl ModulePartition {
    /// The class a method NAME is emitted in, if it was re-homed away from the default.
    pub fn class_of(&self, name: Interned<IString>) -> Option<&str> {
        self.name_to_class.get(&name).map(String::as_str)
    }
    /// Methods that remain in the literal `MainModule` class.
    pub fn residual_methods(&self) -> &[MethodDefIdx] {
        self.classes
            .iter()
            .find(|(c, _)| c == RESIDUAL)
            .map_or(&[][..], |(_, m)| m.as_slice())
    }
    /// The extra per-module classes to emit (everything except the `MainModule` residual).
    pub fn extra_classes(&self) -> impl Iterator<Item = (&str, &[MethodDefIdx])> {
        self.classes
            .iter()
            .filter(|(c, _)| c != RESIDUAL)
            .map(|(c, m)| (c.as_str(), m.as_slice()))
    }
}

/// Build a partition for `MainModule`'s methods, or `None` if it fits in one class (the common case
/// — a literal no-op, so ordinary builds and the `::stable` suite are untouched).
pub fn build(asm: &Assembly, main_module_methods: &[MethodDefIdx]) -> Option<ModulePartition> {
    if main_module_methods.len() <= PARTITION_LIMIT {
        return None;
    }
    // Group methods by name token first: overload sets (one mangled name with many .NET signatures,
    // e.g. ~2k `transmute`s) MUST share a class, since references key on the name alone.
    // (`Interned` is `Hash + Eq` but not `Ord`, so the by-token map is a `HashMap`; determinism is
    // restored by sorting the distinct names by their string before grouping.)
    let mut name_methods: HashMap<Interned<IString>, Vec<MethodDefIdx>> = HashMap::new();
    for &mid in main_module_methods {
        name_methods
            .entry(asm.method_def(mid).name())
            .or_default()
            .push(mid);
    }
    let mut names: Vec<Interned<IString>> = name_methods.keys().copied().collect();
    names.sort_by(|a, b| asm[*a].cmp(&asm[*b]));
    // Group names by their module class (`BTreeMap` over the `String` keys ⇒ deterministic class
    // order; each bucket's names stay in the sorted order built above).
    let mut base_names: BTreeMap<String, Vec<Interned<IString>>> = BTreeMap::new();
    for nid in names {
        base_names
            .entry(module_class(&asm[nid]))
            .or_default()
            .push(nid);
    }

    let mut name_to_class: HashMap<Interned<IString>, String> = HashMap::new();
    let mut class_methods: BTreeMap<String, Vec<MethodDefIdx>> = BTreeMap::new();
    for (base, names) in base_names {
        let total: usize = names.iter().map(|n| name_methods[n].len()).sum();
        let split = total > PARTITION_LIMIT;
        // Bin-pack whole name sets into `base`, `base$1`, … chunks when a single module is itself
        // over the cap (rare). Same-named overloads never straddle a chunk boundary.
        let (mut chunk, mut cur) = (0usize, 0usize);
        for nid in names {
            let cnt = name_methods[&nid].len();
            if split && cur > 0 && cur + cnt > PARTITION_LIMIT {
                chunk += 1;
                cur = 0;
            }
            let cname = if chunk == 0 {
                base.clone()
            } else {
                format!("{base}${chunk}")
            };
            cur += cnt;
            class_methods
                .entry(cname.clone())
                .or_default()
                .extend(&name_methods[&nid]);
            name_to_class.insert(nid, cname);
        }
    }
    Some(ModulePartition {
        name_to_class,
        classes: class_methods.into_iter().collect(),
    })
}

/// The .NET class name for a (possibly mangled) method name. Internal `_R…` symbols group by their
/// Rust module path under a `MainModule.` namespace prefix (so they never collide with data-type or
/// BCL class names); everything else stays in the `MainModule` residual.
fn module_class(name: &str) -> String {
    if !name.starts_with("_R") {
        return RESIDUAL.to_string();
    }
    match container(&format!("{:#}", demangle(name))) {
        Some(path) => format!("{RESIDUAL}.{}", sanitize(&path)),
        None => RESIDUAL.to_string(),
    }
}

/// Split a string on top-level `::` (bracket depth 0 w.r.t. `<>` and `()`).
fn top_segments(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'<' | b'(' => {
                depth += 1;
                cur.push(b[i] as char);
            }
            b'>' | b')' => {
                depth -= 1;
                cur.push(b[i] as char);
            }
            b':' if depth == 0 && i + 1 < b.len() && b[i + 1] == b':' => {
                out.push(std::mem::take(&mut cur));
                i += 1;
            }
            c => cur.push(c as char),
        }
        i += 1;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn strip_generics(seg: &str) -> String {
    seg.split('<').next().unwrap_or(seg).trim().to_string()
}

/// The "container" (module / `impl`-type path) grouping key for a demangled symbol:
///   `core::num::int_sqrt::isqrt_check`   → `core.num.int_sqrt`
///   `core::option::Option<i32>::unwrap`  → `core.option.Option`
///   `<core::slice::Iter<T> as Iterator>::next` → container of the implementing type
///   `…::{{closure}}`                     → the enclosing item's container
fn container(dem: &str) -> Option<String> {
    let segs = top_segments(dem);
    if segs.is_empty() {
        return None;
    }
    // `<X as Y>::method`: group by the implementing type `X`'s container.
    if let Some(first) = segs.first().filter(|s| s.starts_with('<')) {
        let end = first.rfind('>').unwrap_or(first.len());
        let inner = &first[1..end.max(1)];
        let x = inner.split(" as ").next().unwrap_or(inner).trim();
        return container(x);
    }
    // Drop trailing `{{closure}}` / `{{constant}}` / shim markers, then the leaf (the function name).
    let mut keep = segs.len();
    while keep > 1 && (segs[keep - 1].starts_with('{') || segs[keep - 1].is_empty()) {
        keep -= 1;
    }
    if keep > 1 {
        keep -= 1;
    }
    let path: Vec<String> = segs[..keep]
        .iter()
        .map(|s| strip_generics(s))
        .filter(|s| !s.is_empty())
        .collect();
    (!path.is_empty()).then(|| path.join("."))
}

/// Make a class-name fragment a valid (quoted) IL identifier and bound its length. CoreCLR caps a
/// class name at 1023 chars; module paths are short, but guard the pathological case with an FNV
/// suffix so the head stays readable and the whole stays unique + within the limit.
fn sanitize(path: &str) -> String {
    let mut s: String = path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.len() > 900 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in s.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        s.truncate(880);
        s.push_str(&format!("_{hash:016x}"));
    }
    s
}
