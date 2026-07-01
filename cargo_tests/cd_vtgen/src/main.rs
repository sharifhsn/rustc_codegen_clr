// Value-type generic instance-method unlock (Capability A) — a self-contained proof.
//
// Before this, `call_generic` asserted `!is_valuetype` for KIND=1 (instance): you could NOT call an
// instance method on a *generic value type* (`KeyValuePair<K,V>`, `Nullable<T>`, `Span<T>`). The fix
// types the receiver as a managed pointer (`valuetype Foo<..>&`) and reaches it with `call instance`,
// exactly as the non-generic `vt_instance*` path does — the wrapper hands `&self`.
//
// This exercises it at the rawest level (no ergonomic macro yet): construct a KeyValuePair<K,V> value
// (newobj on a generic valuetype ctor) and read `get_Key()`/`get_Value()` (instance methods on the
// generic valuetype). Uses ASYMMETRIC type args (<i32,i64> and <i64,i32>) so a swapped `!0`/`!1`
// binding cannot pass by coincidence.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use mycorrhiza::gen;
use mycorrhiza::intrinsics::{
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2,
    rustc_clr_interop_generic_call3, rustc_clr_interop_generic_ctor0,
    rustc_clr_interop_generic_ctor2, RustcCLRInteropManagedGeneric, RustcCLRInteropTypeGeneric,
};
use mycorrhiza::system::console::Console;

const CORELIB: &str = "System.Private.CoreLib";
const KVP: &str = "System.Collections.Generic.KeyValuePair";
const LIST: &str = "System.Collections.Generic.List";

// ---- Capability B: nested-generic binding. `List<T>.GetRange(i,n)` returns `List<!0>` in def shape;
// the concrete local is `List<i32>`. Exercises the `is_assignable_to` nested-ClassRef arm + the
// recursive `check_generic_marker`, with a TOP-LEVEL nested generic (no nested-type-name concerns).
type RList<T> = RustcCLRInteropManagedGeneric<CORELIB, LIST, (T,)>;

fn list_new<T>() -> RList<T> {
    rustc_clr_interop_generic_ctor0::<CORELIB, LIST, false, (T,), ((),), RList<T>>()
}
fn list_add<T>(l: RList<T>, x: T) {
    rustc_clr_interop_generic_call2::<CORELIB, LIST, false, "Add", 2, (T,), ((), gen!(0)), (), RList<T>, T>(l, x)
}
fn list_count<T>(l: RList<T>) -> i32 {
    rustc_clr_interop_generic_call1::<CORELIB, LIST, false, "get_Count", 2, (T,), (i32,), i32, RList<T>>(l)
}
fn list_get<T>(l: RList<T>, idx: i32) -> T {
    rustc_clr_interop_generic_call2::<CORELIB, LIST, false, "get_Item", 2, (T,), (gen!(0), i32), T, RList<T>, i32>(l, idx)
}
// GetRange(int32,int32) -> List<!0> : the def-shape return is the nested generic `List`1<!0>`.
fn list_get_range<T>(l: RList<T>, index: i32, count: i32) -> RList<T> {
    rustc_clr_interop_generic_call3::<
        CORELIB, LIST, false, "GetRange", 2, (T,),
        (RList<RustcCLRInteropTypeGeneric<0>>, i32, i32), // Sig: (List<!0>, int32, int32)
        RList<T>, RList<T>, i32, i32,
    >(l, index, count)
}

// KeyValuePair<i32,i64>: 4-byte key + 4 pad + 8-byte value = 16 bytes.
type KvpIL = mycorrhiza::intrinsics::RustcCLRInteropManagedGenericStruct<CORELIB, KVP, 16, (i32, i64)>;
// KeyValuePair<i64,i32>: 8-byte key + 4-byte value (+4 pad) = 16 bytes.
type KvpLI = mycorrhiza::intrinsics::RustcCLRInteropManagedGenericStruct<CORELIB, KVP, 16, (i64, i32)>;

fn kvp_il(key: i32, value: i64) -> KvpIL {
    // newobj KeyValuePair`2<int32,int64>::.ctor(!0, !1)
    rustc_clr_interop_generic_ctor2::<CORELIB, KVP, true, (i32, i64), ((), gen!(0), gen!(1)), KvpIL, i32, i64>(
        key, value,
    )
}
fn kvp_il_key(kvp: &KvpIL) -> i32 {
    // call instance !0 KeyValuePair`2<int32,int64>::get_Key()  (KIND=1, receiver by &)
    rustc_clr_interop_generic_call1::<CORELIB, KVP, true, "get_Key", 1, (i32, i64), (gen!(0),), i32, &KvpIL>(kvp)
}
fn kvp_il_value(kvp: &KvpIL) -> i64 {
    rustc_clr_interop_generic_call1::<CORELIB, KVP, true, "get_Value", 1, (i32, i64), (gen!(1),), i64, &KvpIL>(kvp)
}

fn kvp_li(key: i64, value: i32) -> KvpLI {
    rustc_clr_interop_generic_ctor2::<CORELIB, KVP, true, (i64, i32), ((), gen!(0), gen!(1)), KvpLI, i64, i32>(
        key, value,
    )
}
fn kvp_li_key(kvp: &KvpLI) -> i64 {
    rustc_clr_interop_generic_call1::<CORELIB, KVP, true, "get_Key", 1, (i64, i32), (gen!(0),), i64, &KvpLI>(kvp)
}
fn kvp_li_value(kvp: &KvpLI) -> i32 {
    rustc_clr_interop_generic_call1::<CORELIB, KVP, true, "get_Value", 1, (i64, i32), (gen!(1),), i32, &KvpLI>(kvp)
}

static mut PASS: u32 = 0;
static mut TOTAL: u32 = 0;
fn chk_i64(id: u32, got: i64, want: i64) {
    unsafe {
        TOTAL += 1;
        if got == want {
            PASS += 1;
        } else {
            Console::writeln_u64(90_000_000 + id as u64);
            Console::writeln_u64(got as u64);
        }
    }
}

fn main() -> std::process::ExitCode {
    // KeyValuePair<i32,i64>
    let a = kvp_il(10, 200);
    chk_i64(1, kvp_il_key(&a) as i64, 10);
    chk_i64(2, kvp_il_value(&a), 200);

    // KeyValuePair<i64,i32> — asymmetric: key is the WIDE type, value the narrow one.
    let big: i64 = 1 << 40; // 1099511627776 — would truncate to a small i32 if !0/!1 were swapped
    let b = kvp_li(big, 7);
    chk_i64(3, kvp_li_key(&b), big);
    chk_i64(4, kvp_li_value(&b) as i64, 7);

    // A distinct instance keeps its own value (no aliasing).
    let c = kvp_il(-1, -2);
    chk_i64(5, kvp_il_key(&c) as i64, -1);
    chk_i64(6, kvp_il_value(&c), -2);
    chk_i64(7, kvp_il_key(&a) as i64, 10); // a is unchanged

    // ---- Capability B: nested-generic return (List<!0> bound to concrete List<i32>) ----
    let l = list_new::<i32>();
    list_add::<i32>(l, 10);
    list_add::<i32>(l, 20);
    list_add::<i32>(l, 30);
    list_add::<i32>(l, 40);
    chk_i64(8, list_count::<i32>(l) as i64, 4);
    let sub = list_get_range::<i32>(l, 1, 2); // -> List<i32> {20, 30}
    chk_i64(9, list_count::<i32>(sub) as i64, 2);
    chk_i64(10, list_get::<i32>(sub, 0) as i64, 20);
    chk_i64(11, list_get::<i32>(sub, 1) as i64, 30);
    chk_i64(12, list_count::<i32>(l) as i64, 4); // original unchanged

    unsafe {
        Console::writeln_u64(PASS as u64);
        Console::writeln_u64(TOTAL as u64);
        if PASS == TOTAL {
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        }
    }
}
