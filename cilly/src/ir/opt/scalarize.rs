//! Scalar Replacement of (non-escaping) Aggregates — "SROA-lite".
//!
//! Rust's zero-cost abstractions (iterators, `Option`/`Result`, tuples, small wrappers) constantly
//! build a tiny struct/enum, read a field or two, and drop it. rustc deliberately leaves that
//! round-trip for LLVM's SROA to clean up; our backend hands MIR to RyuJIT, which is much weaker at
//! scalar-replacing value types — and worse, we lower a field-built local via `ldloca; stfld`, whose
//! address-taken form makes RyuJIT mark the local **address-exposed** and refuse to enregister it.
//! So `(0..n).map(f).filter(g).sum()` spills its per-element `Option<T>` (2 stores + 2 loads) to the
//! stack every iteration even after the chain is fully inlined.
//!
//! This pass replaces such an aggregate **local** with one fresh scalar local per accessed field, so
//! the field writes/reads become plain `StLoc`/`LdLoc` that the existing copy-prop + dead-store passes
//! forward and delete, and that RyuJIT keeps in registers. The discriminant write (`Some`'s tag) and
//! the whole struct vanish; only the live scalar payload survives.
//!
//! It fires only when the local **provably cannot escape and its fields cannot alias**:
//!   * every occurrence of the local is a field store `SetField(LdLocA(L), F, _)` or a field load
//!     `LdField(LdLocA(L) | LdLoc(L), F)` — never `&L`/`L` used as a whole value, never a whole-value
//!     `StLoc(L, _)` (the count `ok == total` test below enforces this over a complete `CILIter`
//!     traversal, the same traversal `remove_dead_writes` trusts to find every read); and
//!   * the accessed fields have pairwise-disjoint storage: distinct field names AND, for
//!     explicit-layout value types (where enum variants can union), non-overlapping
//!     `[offset, offset+size)` byte ranges. This rejects type-puns and union reads.
//! Under those conditions a struct has no observable identity beyond its independent fields, so the
//! split is semantics-preserving. (`SROA=0` disables it for A/B measurement / emergency off.)

use super::OptFuel;
use crate::{
    bimap::Interned, cilnode::CILNode, cilroot::CILRoot, field::FieldDesc, method::LocalDef,
    Assembly, BasicBlock, CILIter, CILIterElem, IString,
};
use std::collections::{HashMap, HashSet};

/// De-call the backend's pure field-wise tuple constructors so the SROA below can dissolve them.
/// `ovf_check_tuple(v, ovf)` (emitted for every `*WithOverflow` checked op) builds
/// `{Item1: v, Item2: ovf}` and returns it — so a checked `a + b` becomes a per-element CALL that
/// builds an (address-exposed) `(T, bool)` even though, in release, the overflow `assert` is elided
/// and `Item2` is dead. Rewriting `StLoc(t, ovf_check_tuple(v, ovf))` into
/// `SetField(&t, Item1, v); SetField(&t, Item2, ovf)` field-builds `t` in the caller; the SROA pass
/// then scalarizes it, copy-prop forwards the live value, and dead-store-elim deletes the dead
/// overflow flag (and its now-unused computation) — collapsing `iter.sum()` to a plain wrapping add.
/// Sound because the helper is a pure constructor with no other effect and arg order is preserved.
fn decall_tuple_ctors(blocks: &mut [BasicBlock], locals: &[LocalDef], asm: &mut Assembly) -> bool {
    let ovf_name = asm.alloc_string("ovf_check_tuple");
    let item1_s = asm.alloc_string("Item1");
    let item2_s = asm.alloc_string("Item2");
    let mut changed = false;
    for block in blocks.iter_mut() {
        let old = std::mem::take(block.roots_mut());
        let mut new_roots = Vec::with_capacity(old.len());
        for rid in old {
            if let CILRoot::StLoc(t, val) = asm.get_root(rid).clone() {
                if let CILNode::Call(info) = asm.get_node(val).clone() {
                    let (mref, args, _pure) = *info;
                    if asm[mref].name() == ovf_name && args.len() == 2 {
                        if let Some(cref) = asm[locals[t as usize].1].as_class_ref() {
                            if let Some(cdef) = asm.class_ref_to_def(cref) {
                                let it1 = asm[cdef]
                                    .fields()
                                    .iter()
                                    .find(|(_, n, _)| *n == item1_s)
                                    .map(|(t, _, _)| *t);
                                let it2 = asm[cdef]
                                    .fields()
                                    .iter()
                                    .find(|(_, n, _)| *n == item2_s)
                                    .map(|(t, _, _)| *t);
                                if let (Some(it1), Some(it2)) = (it1, it2) {
                                    let f1 = asm.alloc_field(FieldDesc::new(cref, item1_s, it1));
                                    let f2 = asm.alloc_field(FieldDesc::new(cref, item2_s, it2));
                                    let addr = asm.alloc_node(CILNode::LdLocA(t));
                                    new_roots.push(
                                        asm.alloc_root(CILRoot::SetField(Box::new((f1, addr, args[0])))),
                                    );
                                    new_roots.push(
                                        asm.alloc_root(CILRoot::SetField(Box::new((f2, addr, args[1])))),
                                    );
                                    changed = true;
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
            new_roots.push(rid);
        }
        *block.roots_mut() = new_roots;
    }
    changed
}

/// Default ON; `SROA=0` disables (A/B measurement, emergency off).
#[must_use]
pub fn sroa_enabled() -> bool {
    !matches!(std::env::var("SROA").as_deref(), Ok("0"))
}

/// `LdLocA(L)` -> `L` (the address form a field STORE must use).
fn store_addr_local(asm: &Assembly, addr: Interned<CILNode>) -> Option<u32> {
    match asm.get_node(addr) {
        CILNode::LdLocA(l) => Some(*l),
        _ => None,
    }
}
/// `LdLocA(L)` or `LdLoc(L)` -> `L` (the two forms a field LOAD addresses a local by: by-ref, or the
/// whole-value-then-`ldfld` form the exporter emits for a by-value field projection).
fn load_addr_local(asm: &Assembly, addr: Interned<CILNode>) -> Option<u32> {
    match asm.get_node(addr) {
        CILNode::LdLocA(l) | CILNode::LdLoc(l) => Some(*l),
        _ => None,
    }
}

/// Are the accessed fields of one local pairwise non-aliasing (safe to split into scalars)?
fn fields_disjoint(asm: &Assembly, fields: &HashSet<Interned<FieldDesc>>) -> bool {
    // (name, explicit-layout offset if any, byte size) per accessed field.
    let mut ranges: Vec<(Interned<IString>, Option<u32>, u32)> = Vec::with_capacity(fields.len());
    let mut explicit = false;
    for &f in fields {
        let fd = *asm.get_field(f);
        let Some(cdef_idx) = asm.class_ref_to_def(fd.owner()) else {
            return false; // can't resolve the owning layout -> bail conservatively
        };
        let cdef = &asm[cdef_idx];
        explicit |= cdef.has_explicit_layout();
        let off = cdef
            .fields()
            .iter()
            .find(|(_, n, _)| *n == fd.name())
            .and_then(|(_, _, o)| *o);
        ranges.push((fd.name(), off, asm.sizeof_type(fd.tpe())));
    }
    // Two distinct `FieldDesc`s that share a NAME are the same storage slot accessed at two types (a
    // type-pun) — never safe to split (the write goes to one scalar, the read from an uninit other).
    for i in 0..ranges.len() {
        for j in (i + 1)..ranges.len() {
            if ranges[i].0 == ranges[j].0 {
                return false;
            }
        }
    }
    // Sequential layout: distinct names are non-overlapping by construction.
    if !explicit {
        return true;
    }
    // Explicit layout (enum/union): require known offsets and pairwise-disjoint byte ranges.
    if ranges.iter().any(|(_, off, _)| off.is_none()) {
        return false;
    }
    for i in 0..ranges.len() {
        for j in (i + 1)..ranges.len() {
            let (o1, s1) = (ranges[i].1.unwrap(), ranges[i].2);
            let (o2, s2) = (ranges[j].1.unwrap(), ranges[j].2);
            // [o1,o1+s1) overlaps [o2,o2+s2)?  (saturating to avoid u32 overflow on absurd sizes)
            if o1 < o2.saturating_add(s2) && o2 < o1.saturating_add(s1) {
                return false;
            }
        }
    }
    true
}

/// Split every eligible non-escaping aggregate local into per-field scalar locals. Returns whether
/// anything changed (the normal copy-prop + dead-store passes then dissolve the temporaries).
pub fn scalarize_aggregates(
    blocks: &mut [BasicBlock],
    locals: &mut Vec<LocalDef>,
    asm: &mut Assembly,
    fuel: &mut OptFuel,
) -> bool {
    let nlocals = locals.len();
    if nlocals == 0 || !fuel.consume(4) {
        return false;
    }
    // First field-build any pure tuple-constructor calls (checked-arith `ovf_check_tuple`) so the
    // scalarizer below can treat their results like any other field-built local.
    decall_tuple_ctors(blocks, locals, asm);
    // total[L] = every `LdLoc(L)`/`LdLocA(L)` node occurrence; ok[L] = those that are a field-op
    // address. disq[L] = a hard disqualifier (whole-value `StLoc`). fields[L] = accessed FieldDescs.
    let mut total = vec![0u32; nlocals];
    let mut ok = vec![0u32; nlocals];
    let mut disq = vec![false; nlocals];
    let mut fields: Vec<HashSet<Interned<FieldDesc>>> = vec![HashSet::new(); nlocals];

    let root_ids: Vec<_> = blocks.iter().flat_map(BasicBlock::iter_roots).collect();
    for rid in &root_ids {
        let root = asm.get_root(*rid).clone();
        for elem in CILIter::new(root, asm) {
            match elem {
                CILIterElem::Root(CILRoot::SetField(info)) => {
                    if let Some(l) = store_addr_local(asm, info.1) {
                        ok[l as usize] += 1;
                        fields[l as usize].insert(info.0);
                    }
                }
                // A whole-value write of the local can't be split without exploding its rvalue.
                CILIterElem::Root(CILRoot::StLoc(l, _)) => disq[l as usize] = true,
                CILIterElem::Node(CILNode::LdLoc(l) | CILNode::LdLocA(l)) => {
                    total[l as usize] += 1;
                }
                CILIterElem::Node(CILNode::LdField { addr, field }) => {
                    if let Some(l) = load_addr_local(asm, addr) {
                        ok[l as usize] += 1;
                        fields[l as usize].insert(field);
                    }
                }
                _ => {}
            }
        }
    }

    // Decide which locals to scalarize and mint their per-field scalar replacements.
    let mut field_to_nl: HashMap<(u32, Interned<FieldDesc>), u32> = HashMap::new();
    let mut changed = false;
    for l in 0..nlocals {
        if disq[l] || fields[l].is_empty() || ok[l] != total[l] {
            continue;
        }
        if !fields_disjoint(asm, &fields[l]) {
            continue;
        }
        for &f in &fields[l] {
            let tpe = asm.get_field(f).tpe();
            let ity = asm.alloc_type(tpe);
            let nl = locals.len() as u32;
            locals.push((None, ity));
            field_to_nl.insert((l as u32, f), nl);
        }
        changed = true;
    }
    if !changed {
        return false;
    }

    // Rewrite: `SetField(&L, F, v)` -> `StLoc(NL, v)`, `LdField(L|&L, F)` -> `LdLoc(NL)`. The original
    // aggregate local is now unreferenced and `realloc_locals` compacts it away.
    for block in blocks.iter_mut() {
        block.map_roots(
            asm,
            &mut |root, asm| {
                if let CILRoot::SetField(info) = &root {
                    if let Some(l) = store_addr_local(asm, info.1) {
                        if let Some(&nl) = field_to_nl.get(&(l, info.0)) {
                            return CILRoot::StLoc(nl, info.2);
                        }
                    }
                }
                root
            },
            &mut |node, asm| {
                if let CILNode::LdField { addr, field } = node {
                    if let Some(l) = load_addr_local(asm, addr) {
                        if let Some(&nl) = field_to_nl.get(&(l, field)) {
                            return CILNode::LdLoc(nl);
                        }
                    }
                }
                node
            },
        );
    }
    true
}
