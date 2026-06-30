// WF-9 generic interop bridge — proof that Rust (compiled by this backend) can construct and call
// methods on *generic* .NET instantiations from the BCL: `List<i32>` and `Dictionary<i32, i32>`.
//
// The key subtlety this exercises: a method reference on a generic instantiation must use the
// method's *definition*-shape signature — `List<int32>::Add(!0)`, not `Add(int32)` — even though
// the runtime value pushed is a concrete `int32`. The `RustcCLRInteropTypeGeneric<N>` markers
// describe the `!N` positions; the concrete `(i32,)` / `(i32, i32)` tuples instantiate the class.
//
// Each result is printed via the managed Console; the expected .NET values are in trailing comments.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use mycorrhiza::intrinsics::{
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2,
    rustc_clr_interop_generic_call3, rustc_clr_interop_generic_ctor0, RustcCLRInteropManagedGeneric,
    RustcCLRInteropTypeGeneric,
};
use mycorrhiza::system::console::Console;

// The core BCL generic collections live in the implementation assembly `System.Private.CoreLib`
// (method-body refs must name the impl assembly — a reference assembly such as `System.Collections`
// resolves to a type-forwarder and throws `TypeLoadException` at JIT time; same rule as ThreadLocal).
const CORELIB: &str = "System.Private.CoreLib";

// ===================== List<i32> =====================
// `CLASS_PATH` is the *open* generic name (no backtick) — the exporter appends `` `1 `` from the arity.
type ListI32 =
    RustcCLRInteropManagedGeneric<CORELIB, "System.Collections.Generic.List", (i32,)>;

fn list_new() -> ListI32 {
    // `new List<i32>()` — ctor, no args. Sig tuple = `((),)` (just the ignored-return slot).
    rustc_clr_interop_generic_ctor0::<
        CORELIB,
        "System.Collections.Generic.List",
        false,
        (i32,),  // class generics
        ((),),   // ctor sig: ignored-return only, no explicit inputs
        ListI32, // returned handle
    >()
}

fn list_add(list: ListI32, item: i32) {
    // `List<i32>::Add(!0)` — instance, void return, one `!0` arg. KIND = 2 (callvirt: the
    // universally-valid dispatch for a reference-type receiver). Runtime args: (receiver, item).
    rustc_clr_interop_generic_call2::<
        CORELIB,
        "System.Collections.Generic.List",
        false,
        "Add",
        2,
        (i32,),                              // class generics
        ((), RustcCLRInteropTypeGeneric<0>), // sig: output = void, In0 = !0
        (),                                  // Rust return
        ListI32,                             // Arg1 = receiver
        i32,                                 // Arg2 = the item (concrete `int32`, bound to `!0`)
    >(list, item)
}

fn list_count(list: ListI32) -> i32 {
    // `List<i32>::get_Count() -> int32` — instance, returns a *concrete* int32 (Count is NOT generic).
    rustc_clr_interop_generic_call1::<
        CORELIB,
        "System.Collections.Generic.List",
        false,
        "get_Count",
        2,
        (i32,),
        (i32,), // sig: output = int32, no explicit inputs
        i32,    // Rust return
        ListI32, // Arg1 = receiver
    >(list)
}

fn list_get(list: ListI32, idx: i32) -> i32 {
    // `List<i32>::get_Item(int32) -> !0` — the index is a *concrete* int32 (not `!0`), the return is
    // `!0`. This is the case where a concrete arg type coincides with the class generic yet must
    // stay concrete in the signature.
    rustc_clr_interop_generic_call2::<
        CORELIB,
        "System.Collections.Generic.List",
        false,
        "get_Item",
        2,
        (i32,),
        (RustcCLRInteropTypeGeneric<0>, i32), // sig: output = !0, In0 = int32 (the index)
        i32,                                  // Rust return
        ListI32,                              // Arg1 = receiver
        i32,                                  // Arg2 = idx
    >(list, idx)
}

// ===================== Dictionary<i32, i32> =====================
type DictI32 =
    RustcCLRInteropManagedGeneric<CORELIB, "System.Collections.Generic.Dictionary", (i32, i32)>;

fn dict_new() -> DictI32 {
    rustc_clr_interop_generic_ctor0::<
        CORELIB,
        "System.Collections.Generic.Dictionary",
        false,
        (i32, i32),
        ((),),
        DictI32,
    >()
}

fn dict_add(dict: DictI32, key: i32, value: i32) {
    // `Dictionary<i32,i32>::Add(!0, !1)` — instance, void, two generic args.
    rustc_clr_interop_generic_call3::<
        CORELIB,
        "System.Collections.Generic.Dictionary",
        false,
        "Add",
        2,
        (i32, i32),
        (
            (),
            RustcCLRInteropTypeGeneric<0>,
            RustcCLRInteropTypeGeneric<1>,
        ), // sig: output = void, In0 = !0, In1 = !1
        (),
        DictI32, // receiver
        i32,     // key
        i32,     // value
    >(dict, key, value)
}

fn dict_get(dict: DictI32, key: i32) -> i32 {
    // `Dictionary<i32,i32>::get_Item(!0) -> !1`.
    rustc_clr_interop_generic_call2::<
        CORELIB,
        "System.Collections.Generic.Dictionary",
        false,
        "get_Item",
        2,
        (i32, i32),
        (RustcCLRInteropTypeGeneric<1>, RustcCLRInteropTypeGeneric<0>), // output = !1, In0 = !0
        i32,
        DictI32, // receiver
        i32,     // key
    >(dict, key)
}

fn main() {
    // ----- List<i32> -----
    let list = list_new();
    list_add(list, 10);
    list_add(list, 20);
    list_add(list, 30);
    Console::writeln_u64(list_count(list) as u64); // expect 3
    Console::writeln_u64(list_get(list, 0) as u64); // expect 10
    Console::writeln_u64(list_get(list, 1) as u64); // expect 20
    Console::writeln_u64(list_get(list, 2) as u64); // expect 30

    // ----- Dictionary<i32, i32> -----
    let dict = dict_new();
    dict_add(dict, 1, 100);
    dict_add(dict, 2, 200);
    Console::writeln_u64(dict_get(dict, 1) as u64); // expect 100
    Console::writeln_u64(dict_get(dict, 2) as u64); // expect 200
}
