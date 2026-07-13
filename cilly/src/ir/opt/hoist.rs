//! Hoist loop-invariant constant materialization to the method entry.
//!
//! The backend materializes an aggregate *constant* (e.g. the `Layout {size, align}` every `Box`/`Vec`
//! allocation needs) as `transmute(Const::U128(bits))` — an opaque, struct-returning helper call over a
//! compile-time constant. That node is a true constant expression: pure, non-throwing (a reinterpret),
//! and dependent on nothing but the constant. But when it sits in a non-entry block (a loop body) it is
//! rebuilt every iteration, and because RyuJIT will neither inline the struct-returning `transmute` nor
//! hoist across the opaque call, the `newobj UInt128 + transmute` runs per element.
//!
//! This pass materializes each such node ONCE at the start of the entry block into a fresh local and
//! replaces every occurrence with a load of that local — which RyuJIT keeps in a register across the
//! loop. Sound: the entry block dominates all uses (reducible CFGs from MIR), the node has no side
//! effects and cannot throw, and its value is constant, so moving its single evaluation to method entry
//! is semantics-preserving. Scoped to `transmute(<const>)` specifically — the backend's const-aggregate
//! materializer — because that is provably non-throwing; a general "pure call of constants" hoist could
//! eagerly raise an exception a conditionally-reached call would not.

use crate::{
    Assembly, BasicBlock, CILIter, CILIterElem, Const, IString, Type, bimap::Interned,
    cilnode::CILNode, cilroot::CILRoot, method::LocalDef,
};
use std::collections::{HashMap, HashSet};

/// `transmute(<const>)` — the const-aggregate materializer applied to a compile-time constant.
fn is_const_transmute(
    asm: &Assembly,
    node: &CILNode,
    transmute_name: Interned<IString>,
) -> Option<Interned<crate::MethodRef>> {
    if let CILNode::Call(info) = node {
        let (mref, args, _pure) = info.as_ref();
        if asm[*mref].name() == transmute_name
            && args.len() == 1
            && matches!(asm.get_node(args[0]), CILNode::Const(_))
        {
            return Some(*mref);
        }
    }
    None
}

/// A process-independent key for the constant accepted by `is_const_transmute`.
///
/// Interned handles cannot be used as sort keys: their numeric values reflect allocation history,
/// not semantic identity. Resolve every handle here so the hoisted initializer order is stable
/// across independently linked assemblies.
fn const_key(value: &Const, asm: &Assembly) -> Vec<u8> {
    let mut key = Vec::new();
    macro_rules! scalar {
        ($tag:expr, $value:expr) => {{
            key.push($tag);
            key.extend_from_slice(&$value.to_le_bytes());
        }};
    }
    match value {
        Const::I8(value) => scalar!(0, *value),
        Const::I16(value) => scalar!(1, *value),
        Const::I32(value) => scalar!(2, *value),
        Const::I64(value) => scalar!(3, *value),
        Const::I128(value) => scalar!(4, *value),
        Const::ISize(value) => scalar!(5, *value),
        Const::U8(value) => scalar!(6, *value),
        Const::U16(value) => scalar!(7, *value),
        Const::U32(value) => scalar!(8, *value),
        Const::U64(value) => scalar!(9, *value),
        Const::U128(value) => scalar!(10, *value),
        Const::USize(value) => scalar!(11, *value),
        Const::PlatformString(value) => {
            key.push(12);
            key.extend_from_slice(asm[*value].as_bytes());
        }
        Const::Bool(value) => key.extend_from_slice(&[13, u8::from(*value)]),
        Const::F32(value) => scalar!(14, value.to_bits()),
        Const::F64(value) => scalar!(15, value.to_bits()),
        Const::Null(class) => {
            key.push(16);
            key.extend_from_slice(Type::ClassRef(*class).mangle(asm).as_bytes());
        }
        Const::ByteBuffer { data, tpe } => {
            key.push(17);
            key.extend_from_slice(asm[*tpe].mangle(asm).as_bytes());
            key.push(0);
            key.extend_from_slice(&asm.const_data[*data]);
        }
    }
    key
}

fn hoist_key(value: &CILNode, asm: &Assembly) -> (String, Vec<u8>) {
    let CILNode::Call(info) = value else {
        unreachable!("hoist target must be a call")
    };
    let (method, args, _) = info.as_ref();
    let CILNode::Const(value) = asm.get_node(args[0]) else {
        unreachable!("hoist target must contain a constant")
    };
    let output = asm[asm[*method].sig()].output().mangle(asm);
    (output, const_key(value, asm))
}

/// Hoist every `transmute(<const>)` used in a non-entry block to a once-evaluated entry-block local.
pub fn hoist_const_calls(
    blocks: &mut [BasicBlock],
    locals: &mut Vec<LocalDef>,
    asm: &mut Assembly,
) -> bool {
    if blocks.len() < 2 {
        return false; // no non-entry block to hoist out of
    }
    let transmute_name = asm.alloc_string("transmute");

    // 1) Collect distinct `transmute(<const>)` node VALUES that appear in a NON-entry block (so a
    //    const used only in the entry block is left alone). Interned handles make value-equality
    //    structural, so de-duplication by value coalesces identical constants to one local.
    let mut targets: HashSet<CILNode> = HashSet::new();
    for block in blocks.iter().skip(1) {
        for root in block.iter_roots() {
            for elem in CILIter::new(asm.get_root(root).clone(), asm) {
                if let CILIterElem::Node(n) = elem {
                    if is_const_transmute(asm, &n, transmute_name).is_some() {
                        targets.insert(n);
                    }
                }
            }
        }
    }
    if targets.is_empty() {
        return false;
    }

    // 2) Mint a fresh local per distinct target (typed by the transmute's return type) and remember
    //    its canonical interned node id for the entry initializer.
    let mut to_local: HashMap<CILNode, u32> = HashMap::new();
    let mut inits: Vec<(u32, Interned<CILNode>)> = Vec::new();
    let mut targets: Vec<_> = targets.into_iter().collect();
    targets.sort_by_cached_key(|value| hoist_key(value, asm));
    for value in targets {
        let mref = is_const_transmute(asm, &value, transmute_name).unwrap();
        let ret = *asm[asm[mref].sig()].output();
        let ty = asm.alloc_type(ret);
        let local = locals.len() as u32;
        locals.push((None, ty));
        let id = asm.alloc_node(value.clone());
        to_local.insert(value, local);
        inits.push((local, id));
    }

    // 3) Replace every occurrence (in all blocks, incl. the entry's own) with a load of the local.
    for block in blocks.iter_mut() {
        block.map_roots(
            asm,
            &mut |root, _| root,
            &mut |node, _| match to_local.get(&node) {
                Some(&local) => CILNode::LdLoc(local),
                None => node,
            },
        );
    }

    // 4) Insert the once-only materializers at the very START of the entry block (they depend only on
    //    constants, so they are valid before any other code). Inserted AFTER the rewrite above, so the
    //    initializer keeps the real `transmute(<const>)` rather than being rewritten to load itself.
    let entry = blocks[0].roots_mut();
    let init_roots: Vec<Interned<CILRoot>> = inits
        .into_iter()
        .map(|(local, id)| asm.alloc_root(CILRoot::StLoc(local, id)))
        .collect();
    entry.splice(0..0, init_roots);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_hoist_key_uses_values_not_interning_history() {
        let mut first = Assembly::default();
        let first_zeta = first.alloc_string("zeta");
        let first_alpha = first.alloc_string("alpha");

        let mut second = Assembly::default();
        let second_alpha = second.alloc_string("alpha");
        let second_zeta = second.alloc_string("zeta");

        assert_eq!(
            const_key(&Const::PlatformString(first_alpha), &first),
            const_key(&Const::PlatformString(second_alpha), &second)
        );
        assert_eq!(
            const_key(&Const::PlatformString(first_zeta), &first),
            const_key(&Const::PlatformString(second_zeta), &second)
        );
        assert!(
            const_key(&Const::PlatformString(first_alpha), &first)
                < const_key(&Const::PlatformString(first_zeta), &first)
        );
    }
}
