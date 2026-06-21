//! H2 real-crate SOAK: ordered-float on the dotnet PAL.
//! Wraps f64 in OrderedFloat so it gets Ord/Eq -> sort a Vec, dedup via a BTreeSet, print sorted.
//! Exercises: f64 total-ordering cmp, derive-generated trait impls over a newtype, Vec::sort,
//! BTreeSet insert/iterate, fmt of the wrapper. Panic-safe (no unwraps, finite inputs only).
//! SUCCESS = "== soak_ordered-float done ==".
use ordered_float::OrderedFloat;
use std::collections::BTreeSet;

fn main() {
    println!("== soak_ordered-float start ==");

    let raw: Vec<f64> = vec![3.5, 1.0, 2.25, 1.0, 9.75, 0.5, 2.25, -4.0, 7.125, 0.5];
    println!("1  raw.len={}", raw.len());

    // Wrap and sort a Vec via OrderedFloat's Ord.
    let mut wrapped: Vec<OrderedFloat<f64>> = raw.iter().copied().map(OrderedFloat).collect();
    wrapped.sort();
    let sorted: Vec<f64> = wrapped.iter().map(|w| w.into_inner()).collect();
    println!("2  sorted: {sorted:?}");

    // Min/max from the sorted Vec without indexing-panic.
    if let (Some(first), Some(last)) = (sorted.first(), sorted.last()) {
        println!("3  min={first} max={last}");
    }

    // Dedup via a BTreeSet<OrderedFloat<f64>>.
    let set: BTreeSet<OrderedFloat<f64>> = raw.iter().copied().map(OrderedFloat).collect();
    let unique: Vec<f64> = set.iter().map(|w| w.into_inner()).collect();
    println!("4  unique.len={} unique={unique:?}", unique.len());

    // Sum the unique set to exercise arithmetic through the wrapper's Deref.
    let sum: f64 = set.iter().map(|w| w.into_inner()).sum();
    println!("5  unique.sum={sum}");

    println!("== soak_ordered-float done ==");
}
