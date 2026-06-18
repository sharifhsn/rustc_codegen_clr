// L1 interop de-risk, probe C (catching): does the dedicated `rustc_clr_interop_try_catch`
// primitive catch a *foreign* (.NET BCL) exception?
//
// Background: `interop_catch.rs` showed `core::intrinsics::catch_unwind` does NOT catch a BCL
// exception — its CIL handler rethrows anything that is not a `RustException`. `try_catch`
// lowers to the `interop_try_catch` builtin: the same try/catch shape but catching
// `[System.Runtime]System.Object` (everything). Expected output: 1, 5001, 6009, 9999 (no abort).
#![feature(core_intrinsics, unsized_const_params, adt_const_params)]
#![allow(internal_features, incomplete_features, dead_code, unused_variables, unused_unsafe)]
include!("../common.rs"); // rustc_clr_interop_managed_call{0,1,2}_, Put, lang items

#[derive(Clone, Copy)]
struct RustcCLRInteropManagedClass<const ASM: &'static str, const CLASS: &'static str> {
    _o: usize,
}
#[inline(never)]
fn rustc_clr_interop_managed_call3_<
    const ASM: &'static str, const CLASS: &'static str, const IS_VT: bool,
    const METHOD: &'static str, const IS_STATIC: bool, Ret, A1, A2, A3,
>(a1: A1, a2: A2, a3: A3) -> Ret { core::intrinsics::abort(); }
#[inline(never)]
fn rustc_clr_interop_managed_ctor0_<const ASM: &'static str, const CLASS: &'static str, const V: bool>()
    -> RustcCLRInteropManagedClass<ASM, CLASS> { core::intrinsics::abort(); }
// The dedicated interop try/catch primitive (recognized by name in the codegen): runs
// `try_fn(data)`; if it throws ANY .NET exception, runs `catch_fn(data)` and returns 1,
// otherwise returns 0.
#[inline(never)]
fn rustc_clr_interop_try_catch(try_fn: fn(*mut u8), data: *mut u8, catch_fn: fn(*mut u8)) -> i32 {
    core::intrinsics::abort();
}
type StringBuilder = RustcCLRInteropManagedClass<"System.Runtime", "System.Text.StringBuilder">;

static mut CAUGHT: i32 = 7; // unchanged=7, catch_fn ran -> 9

fn try_body(_: *mut u8) {
    // Build a length-1 StringBuilder, then Remove(0,999) -> ArgumentOutOfRangeException (a BCL throw)
    let sb: StringBuilder =
        rustc_clr_interop_managed_ctor0_::<"System.Runtime", "System.Text.StringBuilder", false>();
    let _: StringBuilder = rustc_clr_interop_managed_call2_::<
        "System.Runtime", "System.Text.StringBuilder", false, "Append", false, StringBuilder, StringBuilder, i32,
    >(sb, 7);
    let _: StringBuilder = rustc_clr_interop_managed_call3_::<
        "System.Runtime", "System.Text.StringBuilder", false, "Remove", false, StringBuilder, StringBuilder, i32, i32,
    >(sb, 0, 999);
    <i32 as Put>::putnl(8888); // should NOT print (Remove throws above)
}
fn catch_body(_data: *mut u8) {
    unsafe { CAUGHT = 9 };
}

fn main() {
    <i32 as Put>::putnl(1); // start marker
    let r = rustc_clr_interop_try_catch(try_body, core::ptr::null_mut(), catch_body);
    // r: 0 == try_body returned normally; 1 == it threw and catch_body ran
    <i32 as Put>::putnl(5000 + r);                 // 5001 == the .NET exception WAS caught
    <i32 as Put>::putnl(6000 + unsafe { CAUGHT }); // 6009 == catch_body ran
    <i32 as Put>::putnl(9999);                     // reached the end == no abort
}
