#[cfg(test)]
use super::{
    CILIter, CILIterElem, CILNode, Float, OptFuel, SideEffectInfoCache, Type, propagate_roots,
};
#[cfg(test)]
use crate::{Assembly, BasicBlock, BinOp, CILRoot};

#[test]
fn sfi_dedup() {
    let mut asm = Assembly::default();
    let file = asm.alloc_string("uwu.rs");
    let sfi_a = asm.alloc_root(CILRoot::SourceFileInfo {
        line_start: 0,
        line_len: 0,
        col_start: 0,
        col_len: 0,
        file,
    });
    let mut bb = BasicBlock::new(vec![sfi_a, sfi_a], 0, None);
    assert_eq!(bb.roots().len(), 2);
    bb.remove_duplicate_sfi(&mut asm);
    assert_eq!(bb.roots().len(), 1);
    let file = asm.alloc_string("owo.rs");
    let sfi_b = asm.alloc_root(CILRoot::SourceFileInfo {
        line_start: 0,
        line_len: 0,
        col_start: 0,
        col_len: 0,
        file,
    });
    let mut bb = BasicBlock::new(vec![sfi_a, sfi_b, sfi_a], 0, None);
    assert_eq!(bb.roots().len(), 3);
    bb.remove_duplicate_sfi(&mut asm);
    assert_eq!(bb.roots().len(), 3);
    let mut bb = BasicBlock::new(vec![sfi_a, sfi_a, sfi_b], 0, None);
    assert_eq!(bb.roots().len(), 3);
    bb.remove_duplicate_sfi(&mut asm);
    assert_eq!(bb.roots().len(), 2);
}

/// Regression test found while investigating the fractal-demo Mandelbrot perf gap:
/// `propagate_locals` forward-substitutes a stored expression tree into every syntactic read of the
/// local it finds in the very next statement, with no guard against duplicating a non-trivial tree
/// across more than one read within that ONE statement. Given `zr = zr2 - zi2; zr2 = zr * zr;`
/// (structurally like `render_mandelbrot`'s escape-time update, minus the `+ cr` term for brevity),
/// `zr` is read TWICE in the second statement (`zr * zr`). Before the fix,
/// `propagate_roots` blindly inlined the whole `zr2 - zi2` tree into both operands, doubling the
/// arithmetic every iteration; the local it replaces should either survive verbatim or get folded in
/// a way that does not literally duplicate the `Sub` node.
#[test]
fn propagate_locals_does_not_duplicate_multi_use_expr() {
    let mut asm = Assembly::default();
    let f64_ty = asm.alloc_type(Type::Float(Float::F64));
    let locals = vec![
        (None, f64_ty), // 0: zr
        (None, f64_ty), // 1: zr2
        (None, f64_ty), // 2: zi2
        (None, f64_ty), // 3: (dest for zr * zr)
    ];

    // prev_root: zr(0) = zr2(1) - zi2(2)   -- a 3-node tree (LdLoc, LdLoc, Sub), not a trivial leaf.
    let zr2 = asm.alloc_node(CILNode::LdLoc(1));
    let zi2 = asm.alloc_node(CILNode::LdLoc(2));
    let sub_tree = asm.alloc_node(CILNode::BinOp(zr2, zi2, BinOp::Sub));
    let prev_root = CILRoot::StLoc(0, sub_tree);

    // root: dest(3) = zr(0) * zr(0)  -- reads local 0 TWICE.
    let zr_a = asm.alloc_node(CILNode::LdLoc(0));
    let zr_b = asm.alloc_node(CILNode::LdLoc(0));
    let mul_tree = asm.alloc_node(CILNode::BinOp(zr_a, zr_b, BinOp::Mul));
    let mut root = asm.alloc_root(CILRoot::StLoc(3, mul_tree));

    let sig = asm.sig([], Type::Void);
    let mut cache = SideEffectInfoCache::default();
    let mut fuel = OptFuel::new(u32::MAX);
    propagate_roots(
        &mut asm, &mut root, prev_root, &mut cache, &locals, sig, &mut fuel,
    );

    // Count how many times the `Sub` node (zr2 - zi2) appears in the resulting root. If the guard
    // is doing its job, propagation into the multi-read statement is skipped entirely, so the tree
    // must contain AT MOST the single original `Sub` (zero, since it wasn't touched) -- never two.
    let final_root = asm.get_root(root).clone();
    let sub_occurrences = CILIter::new(final_root, &asm)
        .filter(|elem| matches!(elem, CILIterElem::Node(CILNode::BinOp(_, _, BinOp::Sub))))
        .count();
    assert!(
        sub_occurrences <= 1,
        "propagate_locals duplicated a multi-use, non-trivial expression tree: found {sub_occurrences} copies of `zr2 - zi2` after propagating into `zr * zr` (expected at most 1)"
    );
}
