// Exercises memcpy-family intrinsics over ZST elements (Type::Void), which
// previously fired the size_of(Void) assert. A copy/write/swap of N ZSTs moves
// zero bytes, so each lowering must short-circuit to a no-op.
//
// NOTE: `Vec::<()>::resize(n, ())` is intentionally NOT exercised here — it has a
// SEPARATE, pre-existing off-by-one for ZSTs (`resize(n)` yields len `n+1` for
// n>0) that is unrelated to the memcpy family (push, extend_from_slice and
// copy_nonoverlapping over ZSTs all produce correct lengths). It was previously
// masked because the ZST `resize`/`extend_with` path hit the size_of(Void) ICE
// and was recovered; fixing the copy guards makes it runnable and surfaces the
// arithmetic bug. Tracked separately.

use std::collections::HashSet;

fn main() {
    // Vec<()>: clone() copies ZST elements (memcpy of [(); n] internally).
    let v: Vec<()> = vec![(); 5];
    let w = v.clone();
    println!("vec len={} clone len={}", v.len(), w.len());

    // HashSet<()> insert: moves ZSTs around.
    let mut s: HashSet<()> = HashSet::new();
    for _ in 0..3 {
        s.insert(());
    }
    println!("set len={}", s.len());

    // slice copy_from_slice over a ZST element: copy_nonoverlapping of [(); 8].
    let src = [(); 8];
    let mut dst = [(); 8];
    dst.copy_from_slice(&src);
    println!("copied dst len={}", dst.len());

    // extend_from_slice over ZSTs (a copy path).
    let mut vv: Vec<()> = Vec::new();
    vv.extend_from_slice(&[(); 4]);
    println!("extended vec len={}", vv.len());

    // swap two ZST elements in a slice (typed_swap_nonoverlapping path).
    let mut zs = [(); 2];
    zs.swap(0, 1);
    println!("swap zst len={}", zs.len());

    // direct intrinsic exercise: copy_nonoverlapping over ZST pointers
    // (the MIR-statement path: NonDivergingIntrinsic::CopyNonOverlapping).
    let zsrc = [(); 3];
    let mut zdst = [(); 3];
    unsafe {
        std::ptr::copy_nonoverlapping(zsrc.as_ptr(), zdst.as_mut_ptr(), 3);
    }
    println!("copy_nonoverlapping zst len={}", zdst.len());

    // write_bytes over a ZST element (the intrinsic mem::write_bytes path).
    let mut wb = [(); 5];
    unsafe {
        std::ptr::write_bytes(wb.as_mut_ptr(), 0u8, 5);
    }
    println!("write_bytes zst len={}", wb.len());

    println!("== pal_zst done ==");
}
