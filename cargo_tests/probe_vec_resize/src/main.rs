// KNOWN-FAILING regression probe (root cause of the regex/globset AccessViolation).
// `Vec::resize(n, v)` goes through `Vec::extend_with` + `SetLenOnDrop`; the .NET backend
// sets the final length to n+1 (the n>0-path inlined `SetLenOnDrop` drop emits
// `*len = local_len + 1` instead of `*len = local_len`). vec![x; n] (from_elem) and
// extend(range) use different paths and are CORRECT. The off-by-one is silent (wrong len)
// until capacity == len exactly (e.g. regex_automata's SparseSet::new(state_count)), where
// the phantom (n+1)th slot is out of bounds → heap corruption → AV.
fn main() {
    let mut bad = 0;
    for n in 0..32usize {
        let mut v: Vec<u32> = Vec::new();
        v.resize(n, 7);
        if v.len() != n { bad += 1; }
    }
    println!("resize length-mismatch count (expect 0) = {}", bad);
    // from_elem + extend are the correct paths (control):
    println!("vec![9;5].len() = {} (expect 5)", vec![9u8; 5].len());
}
