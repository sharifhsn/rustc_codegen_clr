use either::Either::{self, Left, Right};

fn main() {
    // --- Construct Left / Right ---
    let l: Either<i32, String> = Left(42);
    let r: Either<i32, String> = Right(String::from("hello"));

    println!("l_is_left = {}", l.is_left());
    println!("l_is_right = {}", l.is_right());
    println!("r_is_left = {}", r.is_left());
    println!("r_is_right = {}", r.is_right());

    // --- left() / right() accessors (Option, handled without unwrap) ---
    match l.clone().left() {
        Some(v) => println!("l_left_val = {}", v),
        None => println!("l_left_val = <none>"),
    }
    match l.clone().right() {
        Some(v) => println!("l_right_val = {}", v),
        None => println!("l_right_val = <none>"),
    }
    match r.clone().left() {
        Some(v) => println!("r_left_val = {}", v),
        None => println!("r_left_val = <none>"),
    }
    match r.clone().right() {
        Some(v) => println!("r_right_val = {}", v),
        None => println!("r_right_val = <none>"),
    }

    // --- map_left / map_right ---
    // map_left on a Left transforms the value; on a Right it is a no-op.
    let ml: Either<i32, String> = l.clone().map_left(|n| n * 2);
    let mr: Either<i32, String> = r.clone().map_right(|s| format!("{s}!"));

    match ml.left() {
        Some(v) => println!("map_left_on_left = {}", v),
        None => println!("map_left_on_left = <none>"),
    }
    match mr.right() {
        Some(v) => println!("map_right_on_right = {}", v),
        None => println!("map_right_on_right = <none>"),
    }

    // map_left is a no-op on a Right.
    let noop: Either<i32, String> = r.clone().map_left(|n| n + 1000);
    match noop.right() {
        Some(v) => println!("map_left_noop_on_right = {}", v),
        None => println!("map_left_noop_on_right = <none>"),
    }

    // --- either(): collapse both arms to a common type ---
    // Map both sides to the same type (i32 length / value) and fold to one value.
    let collapse_l: i32 = l.clone().either(|n| n, |s| s.len() as i32);
    let collapse_r: i32 = r.clone().either(|n| n, |s| s.len() as i32);
    println!("either_collapse_l = {}", collapse_l);
    println!("either_collapse_r = {}", collapse_r);

    // --- Vec<Either<i32,String>>: partition + derived sums / joined strings ---
    let items: Vec<Either<i32, String>> = vec![
        Left(1),
        Right(String::from("alpha")),
        Left(2),
        Right(String::from("beta")),
        Left(3),
        Right(String::from("gamma")),
        Left(-4),
    ];

    // partition_map splits into (lefts, rights) deterministically, preserving order.
    let (lefts, rights): (Vec<i32>, Vec<String>) =
        items.iter().cloned().partition_map(|e| match e {
            Left(n) => either::Either::Left(n),
            Right(s) => either::Either::Right(s),
        });

    let left_count = lefts.len();
    let right_count = rights.len();
    println!("left_count = {}", left_count);
    println!("right_count = {}", right_count);

    // Derived sum over the Left ints (fold from 0, deterministic integer arithmetic).
    let left_sum: i32 = lefts.iter().fold(0i32, |acc, n| acc + *n);
    println!("left_sum = {}", left_sum);

    // Joined strings over the Right values (order-preserving join).
    let right_joined = rights.join(",");
    println!("right_joined = {}", right_joined);

    // Sum of the lengths of the Right strings (another derived integer).
    let right_len_sum: usize = rights.iter().fold(0usize, |acc, s| acc + s.len());
    println!("right_len_sum = {}", right_len_sum);

    // --- fold over the whole Vec via either(): total = sum of ints + sum of lengths ---
    let total: i64 = items.iter().fold(0i64, |acc, e| {
        acc + e.as_ref().either(|n| *n as i64, |s| s.len() as i64)
    });
    println!("either_fold_total = {}", total);

    println!("== soak_either done ==");
}

// Bring the local partition_map extension trait into scope.
use itertools_shim::PartitionMapExt as _;

// `partition_map` normally lives in `itertools`; to keep deps MINIMAL (the hint
// only requires `either`), we provide a tiny local extension that splits an
// iterator of `Either<A,B>` into two collections, preserving order.
mod itertools_shim {
    use either::Either;
    pub trait PartitionMapExt: Iterator + Sized {
        fn partition_map<A, B, F, L, R>(self, mut f: F) -> (L, R)
        where
            F: FnMut(Self::Item) -> Either<A, B>,
            L: Default + Extend<A>,
            R: Default + Extend<B>,
        {
            let mut left = L::default();
            let mut right = R::default();
            for item in self {
                match f(item) {
                    Either::Left(a) => left.extend(core::iter::once(a)),
                    Either::Right(b) => right.extend(core::iter::once(b)),
                }
            }
            (left, right)
        }
    }
    impl<I: Iterator> PartitionMapExt for I {}
}
