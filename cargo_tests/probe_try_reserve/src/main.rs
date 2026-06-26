#![allow(unused)]
#![feature(try_reserve_kind)]
use std::collections::TryReserveErrorKind::*;
macro_rules! assert_matches {
    ($e:expr, $p:pat, $($m:tt)*) => { match $e { $p => {}, ref v => panic!("{}: got {:?}", format_args!($($m)*), v) } };
    ($e:expr, $p:pat) => { match $e { $p => {}, ref v => panic!("no match: {:?}", v) } };
}
fn main() { test_try_reserve(); println!("test_try_reserve ok"); }

fn test_try_reserve() {
    // These are the interesting cases:
    // * exactly isize::MAX should never trigger a CapacityOverflow (can be OOM)
    // * > isize::MAX should always fail
    //    * On 16/32-bit should CapacityOverflow
    //    * On 64-bit should OOM
    // * overflow may trigger when adding `len` to `cap` (in number of elements)
    // * overflow may trigger when multiplying `new_cap` by size_of::<T> (to get bytes)

    const MAX_CAP: usize = isize::MAX as usize;
    const MAX_USIZE: usize = usize::MAX;

    {
        // Note: basic stuff is checked by test_reserve
        let mut empty_bytes: Vec<u8> = Vec::new();

        // Check isize::MAX doesn't count as an overflow
        if let Err(CapacityOverflow) = empty_bytes.try_reserve(MAX_CAP).map_err(|e| e.kind()) {
            panic!("isize::MAX shouldn't trigger an overflow!");
        }
        // Play it again, frank! (just to be sure)
        if let Err(CapacityOverflow) = empty_bytes.try_reserve(MAX_CAP).map_err(|e| e.kind()) {
            panic!("isize::MAX shouldn't trigger an overflow!");
        }

        // Check isize::MAX + 1 does count as overflow
        assert_matches!(
            empty_bytes.try_reserve(MAX_CAP + 1).map_err(|e| e.kind()),
            Err(CapacityOverflow),
            "isize::MAX + 1 should trigger an overflow!"
        );

        // Check usize::MAX does count as overflow
        assert_matches!(
            empty_bytes.try_reserve(MAX_USIZE).map_err(|e| e.kind()),
            Err(CapacityOverflow),
            "usize::MAX should trigger an overflow!"
        );
    }

    {
        // Same basic idea, but with non-zero len
        let mut ten_bytes: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        if let Err(CapacityOverflow) = ten_bytes.try_reserve(MAX_CAP - 10).map_err(|e| e.kind()) {
            panic!("isize::MAX shouldn't trigger an overflow!");
        }
        if let Err(CapacityOverflow) = ten_bytes.try_reserve(MAX_CAP - 10).map_err(|e| e.kind()) {
            panic!("isize::MAX shouldn't trigger an overflow!");
        }

        assert_matches!(
            ten_bytes.try_reserve(MAX_CAP - 9).map_err(|e| e.kind()),
            Err(CapacityOverflow),
            "isize::MAX + 1 should trigger an overflow!"
        );

        // Should always overflow in the add-to-len
        assert_matches!(
            ten_bytes.try_reserve(MAX_USIZE).map_err(|e| e.kind()),
            Err(CapacityOverflow),
            "usize::MAX should trigger an overflow!"
        );
    }

    {
        // Same basic idea, but with interesting type size
        let mut ten_u32s: Vec<u32> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        if let Err(CapacityOverflow) = ten_u32s.try_reserve(MAX_CAP / 4 - 10).map_err(|e| e.kind())
        {
            panic!("isize::MAX shouldn't trigger an overflow!");
        }
        if let Err(CapacityOverflow) = ten_u32s.try_reserve(MAX_CAP / 4 - 10).map_err(|e| e.kind())
        {
            panic!("isize::MAX shouldn't trigger an overflow!");
        }

        assert_matches!(
            ten_u32s.try_reserve(MAX_CAP / 4 - 9).map_err(|e| e.kind()),
            Err(CapacityOverflow),
            "isize::MAX + 1 should trigger an overflow!"
        );

        // Should fail in the mul-by-size
        assert_matches!(
            ten_u32s.try_reserve(MAX_USIZE - 20).map_err(|e| e.kind()),
            Err(CapacityOverflow),
            "usize::MAX should trigger an overflow!"
        );
    }
}
