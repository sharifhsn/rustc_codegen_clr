//! Round 5: the comparison `n <= MAX` is TRUE as a value (G) but selects the WRONG arm when
//! used as an `if` BRANCH CONDITION. Not enum-specific, not u64-specific (u32 also failed).
//! Pin down which operator / operand pattern miscompiles in BRANCH position.

#[inline(never)]
fn bb_u64(x: u64) -> u64 { unsafe { core::ptr::read_volatile(&x) } }
#[inline(never)]
fn bb_u32(x: u32) -> u32 { unsafe { core::ptr::read_volatile(&x) } }

// Each fn: compute `cond` as a VALUE and ALSO as a BRANCH, return both, so we see divergence.
macro_rules! probe {
    ($name:ident, $ty:ty, $op:tt, $rhs:expr) => {
        #[inline(never)]
        fn $name(n: $ty) -> (bool, u32) {
            let as_value = n $op $rhs;            // comparison as a bool value
            let as_branch = if n $op $rhs { 1u32 } else { 0u32 }; // same as a branch
            (as_value, as_branch)
        }
    };
}

// u64 <= against various RHS
probe!(le_u64_max,    u64, <=, u64::MAX);
probe!(le_u64_big,    u64, <=, 0xFFFF_FFFF_FFFF_FFF0u64);
probe!(le_u64_small,  u64, <=, 100u64);
probe!(le_u64_hi,     u64, <=, 0x8000_0000_0000_0000u64);
// other operators against MAX
probe!(lt_u64_max,    u64, <,  u64::MAX);
probe!(ge_u64_zero,   u64, >=, 0u64);
probe!(gt_u64_zero,   u64, >,  0u64);
// u32
probe!(le_u32_max,    u32, <=, u32::MAX);
probe!(le_u32_small,  u32, <=, 100u32);

fn report<T: std::fmt::Display>(name: &str, n: T, r: (bool, u32), exp: bool) {
    let branch_ok = r.1 == 1;
    let val_str = if r.0 == exp { "OK" } else { "WRONG" };
    let br_str = if branch_ok == exp { "OK" } else { "WRONG" };
    println!(
        "{name}(n={n}): value={} [{val_str}]  branch={} [{br_str}]  expected={exp}",
        r.0, r.1
    );
}

fn main() {
    println!("== pal_wideint start ==");
    let n = bb_u64(17);
    let n32 = bb_u32(17);

    report("le_u64_max  ", n, le_u64_max(n), true);
    report("le_u64_big  ", n, le_u64_big(n), true);
    report("le_u64_small", n, le_u64_small(n), true);
    report("le_u64_hi   ", n, le_u64_hi(n), true);
    report("lt_u64_max  ", n, lt_u64_max(n), true);
    report("ge_u64_zero ", n, ge_u64_zero(n), true);
    report("gt_u64_zero ", n, gt_u64_zero(n), true);
    report("le_u32_max  ", n32, le_u32_max(n32), true);
    report("le_u32_small", n32, le_u32_small(n32), true);

    println!("== pal_wideint done ==");
}
