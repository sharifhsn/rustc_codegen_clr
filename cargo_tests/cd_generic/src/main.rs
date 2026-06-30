// WF-9 Stage 1 — Rust constructs and calls *generic* .NET instantiations from the BCL.
//
// The wrappers below are themselves GENERIC over the element type `T`: one set of wrappers,
// monomorphized per `T`, drives `List<i32>`, `List<i64>`, `List<f64>`, `List<Pair>` (a Rust
// value-type element) and `Dictionary<K, V>`. This both reads cleanly and is a stronger test than
// hand-specialized wrappers: it proves the bridge composes with Rust generics.
//
// The crux it exercises: a methodref on a generic instantiation uses the method's *definition*-shape
// signature (`List<int32>::Add(!0)`, never `Add(int32)`), so the `RustcCLRInteropTypeGeneric<N>`
// markers describe the `!N` positions while concrete `(T,)`/`(K, V)` tuples instantiate the class.
//
// All results are compared in-Rust; `main` prints a single `pass/total` tally (and a `9000000xx`
// marker for any failing check) and returns non-zero on any mismatch.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use mycorrhiza::intrinsics::{
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2,
    rustc_clr_interop_generic_call3, rustc_clr_interop_generic_ctor0, RustcCLRInteropManagedGeneric,
    RustcCLRInteropTypeGeneric,
};
use mycorrhiza::system::console::Console;

// Core BCL generic collections live in the implementation assembly (method-body refs must name the
// impl assembly — a ref assembly resolves to a type-forwarder and throws TypeLoadException at JIT).
const CORELIB: &str = "System.Private.CoreLib";
const LIST: &str = "System.Collections.Generic.List";
const DICT: &str = "System.Collections.Generic.Dictionary";

// ===================== generic List<T> wrappers =====================
type RustList<T> = RustcCLRInteropManagedGeneric<CORELIB, LIST, (T,)>;

fn list_new<T>() -> RustList<T> {
    rustc_clr_interop_generic_ctor0::<CORELIB, LIST, false, (T,), ((),), RustList<T>>()
}
fn list_add<T>(list: RustList<T>, item: T) {
    // Add(!0): instance void, one `!0` arg. KIND=2 (callvirt for a ref-type receiver).
    rustc_clr_interop_generic_call2::<
        CORELIB,
        LIST,
        false,
        "Add",
        2,
        (T,),
        ((), RustcCLRInteropTypeGeneric<0>),
        (),
        RustList<T>,
        T,
    >(list, item)
}
fn list_count<T>(list: RustList<T>) -> i32 {
    // get_Count(): instance, returns a CONCRETE int32 (Count is not generic).
    rustc_clr_interop_generic_call1::<CORELIB, LIST, false, "get_Count", 2, (T,), (i32,), i32, RustList<T>>(
        list,
    )
}
fn list_get<T>(list: RustList<T>, idx: i32) -> T {
    // get_Item(int32) -> !0: the index is a concrete int32 (NOT !0); the return is !0.
    rustc_clr_interop_generic_call2::<
        CORELIB,
        LIST,
        false,
        "get_Item",
        2,
        (T,),
        (RustcCLRInteropTypeGeneric<0>, i32),
        T,
        RustList<T>,
        i32,
    >(list, idx)
}
fn list_set<T>(list: RustList<T>, idx: i32, item: T) {
    // set_Item(int32, !0): instance void.
    rustc_clr_interop_generic_call3::<
        CORELIB,
        LIST,
        false,
        "set_Item",
        2,
        (T,),
        ((), i32, RustcCLRInteropTypeGeneric<0>),
        (),
        RustList<T>,
        i32,
        T,
    >(list, idx, item)
}

// ===================== generic Dictionary<K, V> wrappers =====================
type RustDict<K, V> = RustcCLRInteropManagedGeneric<CORELIB, DICT, (K, V)>;

fn dict_new<K, V>() -> RustDict<K, V> {
    rustc_clr_interop_generic_ctor0::<CORELIB, DICT, false, (K, V), ((),), RustDict<K, V>>()
}
fn dict_set<K, V>(dict: RustDict<K, V>, key: K, value: V) {
    // set_Item(!0, !1): insert-or-overwrite. (Add throws on a duplicate key; the indexer does not.)
    rustc_clr_interop_generic_call3::<
        CORELIB,
        DICT,
        false,
        "set_Item",
        2,
        (K, V),
        (
            (),
            RustcCLRInteropTypeGeneric<0>,
            RustcCLRInteropTypeGeneric<1>,
        ),
        (),
        RustDict<K, V>,
        K,
        V,
    >(dict, key, value)
}
fn dict_get<K, V>(dict: RustDict<K, V>, key: K) -> V {
    // get_Item(!0) -> !1.
    rustc_clr_interop_generic_call2::<
        CORELIB,
        DICT,
        false,
        "get_Item",
        2,
        (K, V),
        (RustcCLRInteropTypeGeneric<1>, RustcCLRInteropTypeGeneric<0>),
        V,
        RustDict<K, V>,
        K,
    >(dict, key)
}
fn dict_count<K, V>(dict: RustDict<K, V>) -> i32 {
    rustc_clr_interop_generic_call1::<CORELIB, DICT, false, "get_Count", 2, (K, V), (i32,), i32, RustDict<K, V>>(
        dict,
    )
}
fn dict_contains<K, V>(dict: RustDict<K, V>, key: K) -> bool {
    // ContainsKey(!0) -> bool.
    rustc_clr_interop_generic_call2::<
        CORELIB,
        DICT,
        false,
        "ContainsKey",
        2,
        (K, V),
        (bool, RustcCLRInteropTypeGeneric<0>),
        bool,
        RustDict<K, V>,
        K,
    >(dict, key)
}

// A Rust value-type element, so a class generic arg is a multi-field valuetype (not a primitive).
#[derive(Clone, Copy, PartialEq)]
#[repr(C)]
struct Pair {
    a: i64,
    b: i64,
}

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                // 9000000xx marks WHICH check failed.
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    // ---- List<i32> (primitive, 4-byte) + growth past initial capacity ----
    let li = list_new::<i32>();
    for i in 0..50i32 {
        list_add::<i32>(li, i * 2);
    }
    chk!(list_count::<i32>(li), 50); // forced several internal reallocs
    chk!(list_get::<i32>(li, 0), 0);
    chk!(list_get::<i32>(li, 49), 98);
    list_set::<i32>(li, 49, -5);
    chk!(list_get::<i32>(li, 49), -5);

    // ---- List<i64> (8-byte int, value beyond 32 bits) ----
    let ll = list_new::<i64>();
    list_add::<i64>(ll, 1i64 << 40);
    list_add::<i64>(ll, -7i64);
    chk!(list_count::<i64>(ll), 2);
    chk!(list_get::<i64>(ll, 0), 1i64 << 40);
    chk!(list_get::<i64>(ll, 1), -7i64);

    // ---- List<f64> (float element) ----
    let lf = list_new::<f64>();
    list_add::<f64>(lf, 3.5);
    list_add::<f64>(lf, -2.25);
    chk!(list_count::<f64>(lf), 2);
    chk!(list_get::<f64>(lf, 0), 3.5);
    chk!(list_get::<f64>(lf, 1), -2.25);

    // ---- List<Pair> (a 16-byte Rust VALUE-TYPE element: class generic arg is a valuetype) ----
    let lp = list_new::<Pair>();
    list_add::<Pair>(lp, Pair { a: 10, b: 20 });
    list_add::<Pair>(lp, Pair { a: 30, b: 40 });
    chk!(list_count::<Pair>(lp), 2);
    chk!(list_get::<Pair>(lp, 1), Pair { a: 30, b: 40 });
    list_set::<Pair>(lp, 0, Pair { a: 99, b: 88 });
    chk!(list_get::<Pair>(lp, 0), Pair { a: 99, b: 88 });

    // ---- Dictionary<i32, i64>: insert, overwrite, missing-key, ContainsKey, Count ----
    let d = dict_new::<i32, i64>();
    dict_set::<i32, i64>(d, 1, 100);
    dict_set::<i32, i64>(d, 2, 200);
    dict_set::<i32, i64>(d, 1, 111); // overwrite key 1
    chk!(dict_count::<i32, i64>(d), 2);
    chk!(dict_get::<i32, i64>(d, 1), 111);
    chk!(dict_get::<i32, i64>(d, 2), 200);
    chk!(dict_contains::<i32, i64>(d, 2), true);
    chk!(dict_contains::<i32, i64>(d, 99), false);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
