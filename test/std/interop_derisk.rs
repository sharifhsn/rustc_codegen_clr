// L1 interop de-risk probe (see docs/std_research/h2_design.md §3 and the "L1 de-risk results").
// Validates the architectural bet H2 rests on, on a no_std program that compiles today.
//
// VERDICT (run on .NET 8): the core bet HOLDS. All four probes pass.
//   A) BCL static call + primitive marshaling      -> WORKS  (Math.Max(3,7)=7, prints 1007)
//   B1) object ctor + instance calls + hold across calls -> WORKS (StringBuilder, prints 2001)
//   B2) GCHandle store-across-GC round-trip          -> WORKS (prints 3001 then 3999). Getting here
//      surfaced + fixed TWO real codegen bugs: (1) managed value types
//      (RustcCLRInteropManagedStruct<ASM,CLASS,SIZE>) wrongly required 2 generics in
//      rustc_codegen_clr_type/src/type.rs (fixed -> 3); (2) `System.Object`/`System.String` were
//      emitted as `class [System.Runtime]System.Object` (ELEMENT_TYPE_CLASS) instead of the
//      canonical `object`/`string` element type, so `GCHandle.Alloc(object)` failed to bind
//      (MissingMethodException). Fixed in cilly/.../il_exporter/mod.rs `type_il`.
//   C) .NET exception from a BCL call               -> propagates as a real, well-typed .NET
//      exception through the Rust frame (clean managed stack trace). CATCHING it works via the
//      dedicated `rustc_clr_interop_try_catch` primitive (see test/std/interop_try_catch.rs);
//      `catch_unwind` does NOT catch foreign exceptions (it rethrows non-RustException).
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
    "System.Runtime.InteropServices", "System.Runtime.InteropServices.GCHandle", { core::mem::size_of::<usize>() }>;

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

    // ---- Probe B2: round-trip the object through a GCHandle (the GC-boundary mechanism) ----
    // IS_VALUETYPE must be `true` for GCHandle (a struct) so the declaring type is emitted as a
    // valuetype — the de-risk's `false` (copied from mycorrhiza's unverified Class) made it a
    // `class`, so GCHandle.Alloc didn't resolve.
    let obj: Object = rustc_clr_interop_managed_checked_cast::<Object, StringBuilder>(sb);
    let handle = rustc_clr_interop_managed_call1_::<
        "System.Runtime.InteropServices", "System.Runtime.InteropServices.GCHandle", true, "Alloc", true, GCHandle, Object,
    >(obj);
    let target = rustc_clr_interop_managed_call1_::<
        "System.Runtime.InteropServices", "System.Runtime.InteropServices.GCHandle", true, "get_Target", false, Object, &GCHandle,
    >(&handle);
    let sb_back: StringBuilder = rustc_clr_interop_managed_checked_cast::<StringBuilder, Object>(target);
    let len2 = rustc_clr_interop_managed_call1_::<
        "System.Runtime", "System.Text.StringBuilder", false, "get_Length", false, i32, StringBuilder,
    >(sb_back);
    <i32 as Put>::putnl(3000 + len2); // expect 3001 if the GCHandle round-trip preserved the object
    let _: () = rustc_clr_interop_managed_call1_::<
        "System.Runtime.InteropServices", "System.Runtime.InteropServices.GCHandle", true, "Free", false, (), &GCHandle,
    >(&handle);
    <i32 as Put>::putnl(3999); // reached -> Alloc/get_Target/Free all returned

    // ---- Probe C: trigger a .NET exception from a BCL call, observe propagation ----
    <i32 as Put>::putnl(100); // marker: about to call a throwing method
    // sb.Remove(0, 999) on a length-1 builder -> ArgumentOutOfRangeException
    let _thrown: StringBuilder = rustc_clr_interop_managed_call3_::<
        "System.Runtime", "System.Text.StringBuilder", false, "Remove", false, StringBuilder, StringBuilder, i32, i32,
    >(sb, 0, 999);
    <i32 as Put>::putnl(200); // marker: only printed if the throwing call somehow returned
}
