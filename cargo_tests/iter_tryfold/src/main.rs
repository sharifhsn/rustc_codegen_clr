// Iterator try_fold / nth / try_rfold codegen repro.
// Bodies are VERBATIM from upstream coretests step_by + flat_map + double_ended,
// which exercise i8::checked_add as the try_fold closure over (in)finite ranges,
// nth/try_fold interaction, and try_rfold. A miscompile in sub-word arithmetic,
// ControlFlow lowering, or the nth/try_fold path fires an assert.
use std::ops::ControlFlow;

macro_rules! ae {
    ($label:expr, $got:expr, $exp:expr) => {{
        let g = $got;
        let e = $exp;
        println!("{}: {:?}|{:?}", $label, g, e);
        assert_eq!(g, e, "MISMATCH {}", $label);
    }};
}

fn step_by_nth_try_fold() {
    let mut it = (0..).step_by(10);
    ae!("a1", it.try_fold(0, i8::checked_add), None);
    ae!("a2", it.next(), Some(60));
    ae!("a3", it.try_fold(0, i8::checked_add), None);
    ae!("a4", it.next(), Some(90));

    let mut it = (100..).step_by(10);
    ae!("a5", it.try_fold(50, i8::checked_add), None);
    ae!("a6", it.next(), Some(110));

    let mut it = (100..=100).step_by(10);
    ae!("a7", it.next(), Some(100));
    ae!("a8", it.try_fold(0, i8::checked_add), Some(0));
}

fn step_by_nth_try_rfold() {
    let mut it = (0..100).step_by(10);
    ae!("r1", it.try_rfold(0, i8::checked_add), None);
    ae!("r2", it.next_back(), Some(70));
    ae!("r3", it.next(), Some(0));
    ae!("r4", it.try_rfold(0, i8::checked_add), None);
    ae!("r5", it.next_back(), Some(30));

    let mut it = (0..100).step_by(10);
    ae!("r6", it.try_rfold(50, i8::checked_add), None);
    ae!("r7", it.next_back(), Some(80));

    let mut it = (100..=100).step_by(10);
    ae!("r8", it.next_back(), Some(100));
    ae!("r9", it.try_fold(0, i8::checked_add), Some(0));
}

fn flat_map_try_folds() {
    // upstream test_flat_map_try_folds shape
    let f = &|acc, x| i32::checked_add(acc * 2 / 3, x);
    let mr = &|x: i32| (5 * x)..(5 * x + 5);
    // fresh iterator for each comparison
    ae!("fm1", (0..10).flat_map(mr).try_fold(7, f), (0..50).try_fold(7, f));
    ae!("fm2", (0..10).flat_map(mr).try_rfold(7, f), (0..50).rev().try_fold(7, f));

    let mut iter = (0..10).flat_map(mr);
    let next = iter.next().unwrap();
    let _back = iter.next_back().unwrap();
    ae!("fm_next", next, 0);

    let iter = (0..10).flat_map(mr).rev();
    ae!("fm_rev_fold", iter.fold(0i32, |acc, x| acc + x), (0..50).sum::<i32>());
}

fn rev_try_folds() {
    // upstream test_rev_try_folds shape
    let f = &|acc, x| i32::checked_add(2 * acc, x);
    ae!("rev1", (1..10).rev().try_fold(7, f), (1..10).rev().try_fold(7, f));
    ae!("rev2", (1..10).rev().try_rfold(7, f), (1..10).rev().try_rfold(7, f));

    let a = [10, 20, 30, 40, 100, 60, 70, 80, 90];
    let mut iter = a.iter().rev();
    ae!("rev3", iter.try_fold(0_i8, |acc, &x| acc.checked_add(x)), None);
    ae!("rev4", iter.next(), Some(&70));
}

fn control_flow_basics() {
    let r = [1, 2, 3, 4, 5].iter().try_fold(0i32, |acc, &x| {
        if x > 3 { ControlFlow::Break(acc) } else { ControlFlow::Continue(acc + x) }
    });
    ae!("cf_break", r, ControlFlow::Break(6));
    let r2 = [1, 2, 3].iter().try_fold(0i32, |acc, &x| ControlFlow::Continue::<(), i32>(acc + x));
    ae!("cf_cont", r2, ControlFlow::Continue(6));
    // find/position/all/any use try_fold internally
    ae!("find", [1, 3, 5, 8, 9].iter().find(|&&x| x % 2 == 0), Some(&8));
    ae!("position", [10, 20, 30].iter().position(|&x| x == 20), Some(1));
    ae!("all", [2, 4, 6].iter().all(|&x| x % 2 == 0), true);
    ae!("any", [1, 3, 5].iter().any(|&x| x % 2 == 0), false);
}

fn main() {
    step_by_nth_try_fold();
    step_by_nth_try_rfold();
    flat_map_try_folds();
    rev_try_folds();
    control_flow_basics();
    println!("iter_tryfold OK");
}
