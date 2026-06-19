// Differential interop test for the method-wrapper slice emitted by the `spinacz` binding
// generator (mycorrhiza::slice_bindings: Console / Math / StringBuilder / String).
//
// Each generated wrapper is a thin shim that expands to a staticN/instanceN/virt0/ctorN
// magic-fn call with a fixed turbofish. This probe performs the EXACT calls those wrappers
// expand to (raw intrinsics, in the no-mycorrhiza style of interop_derisk.rs, which compiles &
// runs cleanly on this toolchain), so it validates that the emitted call shapes bind to the
// real BCL methods and return the right values. Output is integer markers via Console.WriteLine,
// to be diffed against the expected .NET results.
//
// Wrapper -> expanded call mapping (from mycorrhiza/src/slice_bindings.rs):
//   Math::max(a,b)  -> static2::<"Max", i32,i32, i32>
//   Math::min(a,b)  -> static2::<"Min", i32,i32, i32>
//   Math::abs(a)    -> static1::<"Abs", i32, i32>
//   Math::sqrt(a)   -> static1::<"Sqrt", f64, f64>
//   StringBuilder::new()           -> ctor0
//   sb.append(i32)  -> instance1::<"Append", i32, StringBuilder>   (.NET StringBuilder.Append(Int32))
//   sb.get_length() -> virt0::<"get_Length", i32>
#![feature(adt_const_params, core_intrinsics, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, unused_variables, unused_unsafe)]
include!("../common.rs"); // rustc_clr_interop_managed_call{0,1,2}_, the Put trait, lang items

#[derive(Clone, Copy)]
struct RustcCLRInteropManagedClass<const ASM: &'static str, const CLASS: &'static str> {
    _obj_ref: usize,
}

// Extra intrinsics this probe needs beyond common.rs's call{0,1,2}_.
#[inline(never)]
fn rustc_clr_interop_managed_call_virt1_<
    const ASM: &'static str, const CLASS: &'static str, const IS_VT: bool,
    const METHOD: &'static str, const IS_STATIC: bool, Ret, A1,
>(a1: A1) -> Ret { core::intrinsics::abort(); }
#[inline(never)]
fn rustc_clr_interop_managed_ctor0_<
    const ASM: &'static str, const CLASS: &'static str, const IS_VT: bool,
>() -> RustcCLRInteropManagedClass<ASM, CLASS> { core::intrinsics::abort(); }

type StringBuilder = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Text.StringBuilder">;

fn main() {
    // ---- System.Math static wrappers ----
    // Math::max(3,7) -> static2::<"Max", i32,i32, i32>
    let mx = rustc_clr_interop_managed_call2_::<
        "System.Private.CoreLib", "System.Math", false, "Max", true, i32, i32, i32,
    >(3, 7);
    <i32 as Put>::putnl(mx); // expect 7

    // Math::min(3,7) -> static2::<"Min", i32,i32, i32>
    let mn = rustc_clr_interop_managed_call2_::<
        "System.Private.CoreLib", "System.Math", false, "Min", true, i32, i32, i32,
    >(3, 7);
    <i32 as Put>::putnl(mn); // expect 3

    // Math::abs(-5) -> static1::<"Abs", i32, i32>
    let ab = rustc_clr_interop_managed_call1_::<
        "System.Private.CoreLib", "System.Math", false, "Abs", true, i32, i32,
    >(-5);
    <i32 as Put>::putnl(ab); // expect 5

    // Math::sqrt(144.0) -> static1::<"Sqrt", f64, f64>
    let sq = rustc_clr_interop_managed_call1_::<
        "System.Private.CoreLib", "System.Math", false, "Sqrt", true, f64, f64,
    >(144.0);
    <f64 as Put>::putnl(sq); // expect 12

    // ---- System.Text.StringBuilder ctor + instance + virtual(property) wrappers ----
    // StringBuilder::new() -> ctor0
    let sb: StringBuilder = rustc_clr_interop_managed_ctor0_::<
        "System.Private.CoreLib", "System.Text.StringBuilder", false,
    >();
    // sb.append(7) -> instance1::<"Append", i32, StringBuilder>  (StringBuilder.Append(Int32))
    let sb: StringBuilder = rustc_clr_interop_managed_call2_::<
        "System.Private.CoreLib", "System.Text.StringBuilder", false, "Append", false, StringBuilder, StringBuilder, i32,
    >(sb, 7);
    // sb.append(42)
    let sb: StringBuilder = rustc_clr_interop_managed_call2_::<
        "System.Private.CoreLib", "System.Text.StringBuilder", false, "Append", false, StringBuilder, StringBuilder, i32,
    >(sb, 42);
    // sb.get_length() -> virt0::<"get_Length", i32>  ("7"+"42" -> "742", length 3)
    let len = rustc_clr_interop_managed_call_virt1_::<
        "System.Private.CoreLib", "System.Text.StringBuilder", false, "get_Length", false, i32, StringBuilder,
    >(sb);
    <i32 as Put>::putnl(len); // expect 3
}
