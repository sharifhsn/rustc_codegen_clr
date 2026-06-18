// L1 interop de-risk probe (see docs/std_research/h2_design.md §3 and the "L1 de-risk results").
// Validates the architectural bet H2 rests on, on a no_std program that compiles today.
//
// VERDICT (run on .NET 8): the core bet HOLDS.
//   A) BCL static call + primitive marshaling      -> WORKS  (Math.Max(3,7)=7, prints 1007)
//   B1) object ctor + instance calls + hold across calls -> WORKS (StringBuilder, prints 2001)
//   C) .NET exception from a BCL call               -> propagates as a real, well-typed .NET
//      exception through the Rust frame (clean managed stack trace). Catching it needs a small
//      `try_catch` interop primitive (the codegen already emits CIL try/catch) — tractable gap.
//   B2) GCHandle store-across-GC round-trip          -> surfaced (and fixed) a real codegen bug:
//      managed value types (RustcCLRInteropManagedStruct<ASM,CLASS,SIZE>) wrongly required 2
//      generics in rustc_codegen_clr_type/src/type.rs (fixed -> 3). A remaining binding issue
//      (GCHandle.Alloc(object)->GCHandle signature resolution) is the next L1 step; see B2 below.
//
// Output is integer markers via Console.WriteLine (string marshaling is a separate concern).
#![feature(adt_const_params, core_intrinsics, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, unused_variables, unused_unsafe)]
include!("../common.rs"); // provides: rustc_clr_interop_managed_call{0,1,2}_, the Put trait, lang items

// Managed reference / value types — the codegen recognizes these by name
// (`name.contains("RustcCLRInteropManaged")`, see rustc_codegen_clr_type/src/utilis.rs).
#[derive(Clone, Copy)]
struct RustcCLRInteropManagedClass<const ASM: &'static str, const CLASS: &'static str> {
    _obj_ref: usize,
}
#[derive(Clone, Copy)]
struct RustcCLRInteropManagedStruct<const ASM: &'static str, const CLASS: &'static str, const SIZE: usize> {
    _bytes: [u8; SIZE],
}

// Extra interop intrinsics this probe needs (call{0,1,2}_ come from common.rs).
#[inline(never)]
fn rustc_clr_interop_managed_call3_<
    const ASM: &'static str, const CLASS: &'static str, const IS_VT: bool,
    const METHOD: &'static str, const IS_STATIC: bool, Ret, A1, A2, A3,
>(a1: A1, a2: A2, a3: A3) -> Ret { core::intrinsics::abort(); }
#[inline(never)]
fn rustc_clr_interop_managed_ctor0_<
    const ASM: &'static str, const CLASS: &'static str, const IS_VT: bool,
>() -> RustcCLRInteropManagedClass<ASM, CLASS> { core::intrinsics::abort(); }
#[inline(never)]
fn rustc_clr_interop_managed_checked_cast<DST, SRC>(src: SRC) -> DST { core::intrinsics::abort(); }

type Object = RustcCLRInteropManagedClass<"System.Runtime", "System.Object">;
type StringBuilder = RustcCLRInteropManagedClass<"System.Runtime", "System.Text.StringBuilder">;
type GCHandle = RustcCLRInteropManagedStruct<
    "System.Runtime", "System.Runtime.InteropServices.GCHandle", { core::mem::size_of::<usize>() }>;

fn main() {
    // ---- Probe A: static BCL call + primitive marshaling ----
    let m = rustc_clr_interop_managed_call2_::<
        "System.Runtime", "System.Math", false, "Max", true, i32, i32, i32,
    >(3, 7);
    <i32 as Put>::putnl(1000 + m); // expect 1007

    // ---- Probe B1: construct an object, call instance methods ----
    let sb: StringBuilder = rustc_clr_interop_managed_ctor0_::<
        "System.Runtime", "System.Text.StringBuilder", false,
    >();
    // sb.Append(7i32) -> appends "7" (StringBuilder.Append(Int32) exists; avoids char marshaling)
    let _sb2: StringBuilder = rustc_clr_interop_managed_call2_::<
        "System.Runtime", "System.Text.StringBuilder", false, "Append", false, StringBuilder, StringBuilder, i32,
    >(sb, 7);
    let len = rustc_clr_interop_managed_call1_::<
        "System.Runtime", "System.Text.StringBuilder", false, "get_Length", false, i32, StringBuilder,
    >(sb);
    <i32 as Put>::putnl(2000 + len); // expect 2001

    // ---- Probe B2 (DISABLED): round-trip the object through a GCHandle ----
    // The GC-boundary mechanism (store a managed ref in unmanaged Rust memory across a GC).
    // Now *compiles* (after the managed-struct generics fix in type.rs:241), but at runtime
    // `GCHandle.Alloc(object) -> GCHandle` doesn't resolve (MissingMethodException) — a BCL
    // signature-identity issue for a value-type-returning method. This is the next L1 step;
    // mycorrhiza/src/class.rs `Class` is the intended pattern. Re-enable once that's solved:
    //
    //   let obj: Object = rustc_clr_interop_managed_checked_cast::<Object, StringBuilder>(sb);
    //   let handle = rustc_clr_interop_managed_call1_::<
    //       "System.Runtime","System.Runtime.InteropServices.GCHandle",false,"Alloc",true,GCHandle,Object>(obj);
    //   let target = rustc_clr_interop_managed_call1_::<
    //       "System.Runtime","System.Runtime.InteropServices.GCHandle",false,"get_Target",false,Object,&GCHandle>(&handle);
    //   let sb_back: StringBuilder = rustc_clr_interop_managed_checked_cast::<StringBuilder, Object>(target);
    //   // ... get_Length on sb_back should still be 1; then GCHandle.Free(&handle).

    // ---- Probe C: trigger a .NET exception from a BCL call, observe propagation ----
    <i32 as Put>::putnl(100); // marker: about to call a throwing method
    // sb.Remove(0, 999) on a length-1 builder -> ArgumentOutOfRangeException
    let _thrown: StringBuilder = rustc_clr_interop_managed_call3_::<
        "System.Runtime", "System.Text.StringBuilder", false, "Remove", false, StringBuilder, StringBuilder, i32, i32,
    >(sb, 0, 999);
    <i32 as Put>::putnl(200); // marker: only printed if the throwing call somehow returned
}
