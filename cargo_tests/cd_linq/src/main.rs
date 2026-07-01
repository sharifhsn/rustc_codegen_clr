// In-memory LINQ: `Enumerable.Count<T>(src)` and `Enumerable.Where<T>(src, predicate)` — generic
// methods on the static `System.Linq.Enumerable` class, the predicate a CAPTURING Rust closure.
// This ties together generic methods (!!N) + closures + nested-generic returns.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use mycorrhiza::intrinsics::{
    rustc_clr_interop_generic_method_call1, rustc_clr_interop_generic_method_call2,
    RustcCLRInteropManagedGeneric, RustcCLRInteropMethodGeneric,
};
use mycorrhiza::prelude::*;
use mycorrhiza::system::console::Console;

const CORELIB: &str = "System.Private.CoreLib";
const LINQ: &str = "System.Linq";
const ENUMERABLE: &str = "System.Linq.Enumerable";
type IEnum<T> = RustcCLRInteropManagedGeneric<CORELIB, "System.Collections.Generic.IEnumerable", (T,)>;
type FuncTB<T> = RustcCLRInteropManagedGeneric<CORELIB, "System.Func", (T, bool)>;
// def-shape method-generic markers
type IEnumM = RustcCLRInteropManagedGeneric<CORELIB, "System.Collections.Generic.IEnumerable", (RustcCLRInteropMethodGeneric<0>,)>;
type FuncM = RustcCLRInteropManagedGeneric<CORELIB, "System.Func", (RustcCLRInteropMethodGeneric<0>, bool)>;

// Enumerable.Count<T>(IEnumerable<T>) -> int
fn linq_count(src: IEnum<i32>) -> i32 {
    rustc_clr_interop_generic_method_call1::<
        LINQ, ENUMERABLE, false, "Count", 0, (), (i32,),
        (i32, IEnumM), i32, IEnum<i32>,
    >(src)
}
// Enumerable.Where<T>(IEnumerable<T>, Func<T,bool>) -> IEnumerable<T>
fn linq_where(src: IEnum<i32>, pred: FuncTB<i32>) -> IEnum<i32> {
    rustc_clr_interop_generic_method_call2::<
        LINQ, ENUMERABLE, false, "Where", 0, (), (i32,),
        (IEnumM, IEnumM, FuncM), IEnum<i32>, IEnum<i32>, FuncTB<i32>,
    >(src, pred)
}

fn main() -> std::process::ExitCode {
    let mut pass = 0u32; let mut total = 0u32;
    macro_rules! chk { ($g:expr,$w:expr) => {{ total+=1; if $g==$w {pass+=1;} else {Console::writeln_u64(900_000_000+total as u64);} }}; }

    let mut list: List<i32> = List::new();
    for v in [1, 2, 3, 4, 5, 6] { list.push(v); }
    let src: IEnum<i32> = unsafe { mycorrhiza::enumerate::as_enumerable_handle::<_, i32>(list.handle()) };

    // Count over the whole thing.
    chk!(linq_count(src), 6);

    // Where with a CAPTURING closure predicate (threshold captured from a local), then Count.
    let threshold = 3;
    let pred = Func1::<i32, bool>::from_closure(move |x| x > threshold);
    let filtered = linq_where(src, pred.handle());
    chk!(linq_count(filtered), 3); // {4,5,6}

    // A different threshold — distinct closure, distinct result.
    let pred2 = Func1::<i32, bool>::from_closure(move |x| x % 2 == 0);
    let evens = linq_where(src, pred2.handle());
    chk!(linq_count(evens), 3); // {2,4,6}

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
}
