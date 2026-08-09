#![feature(
    lang_items,
    adt_const_params,
    associated_type_defaults,
    core_intrinsics,
    unsized_const_params
)]
#![allow(
    internal_features,
    incomplete_features,
    unused_variables,
    dead_code,
    improper_ctypes_definitions
)]

include!("../common.rs");

#[cfg(target_arch = "x86_64")]
fn result_array(result: core::arch::x86_64::CpuidResult) -> [u32; 4] {
    [result.eax, result.ebx, result.ecx, result.edx]
}

/// The exact RBX-preserving shape used by stdarch, with all four outputs retained separately.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
fn cpuid_via_inline_asm(leaf: u32, subleaf: u32) -> [u32; 4] {
    let eax;
    let ebx;
    let ecx;
    let edx;
    unsafe {
        core::arch::asm!(
            "mov {0:r}, rbx",
            "cpuid",
            "xchg {0:r}, rbx",
            out(reg) ebx,
            inout("eax") leaf => eax,
            inout("ecx") subleaf => ecx,
            out("edx") edx,
            options(nostack, preserves_flags),
        );
    }
    [eax, ebx, ecx, edx]
}

/// EAX writes into the same Rust local that supplies ECX's input. Inline-asm inputs are captured
/// simultaneously, so lowering must evaluate the helper call before committing any output place.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
fn cpuid_with_cross_aliased_input(leaf: u32, subleaf: u32) -> [u32; 4] {
    let mut eax_output_and_ecx_input = subleaf;
    let ebx;
    let ecx;
    let edx;
    unsafe {
        core::arch::asm!(
            "mov {0:r}, rbx",
            "cpuid",
            "xchg {0:r}, rbx",
            out(reg) ebx,
            inout("eax") leaf => eax_output_and_ecx_input,
            inout("ecx") eax_output_and_ecx_input => ecx,
            out("edx") edx,
            options(nostack, preserves_flags),
        );
    }
    [eax_output_and_ecx_input, ebx, ecx, edx]
}

#[cfg(target_arch = "x86_64")]
fn test_cpuid() {
    use core::arch::x86_64::{__cpuid, __cpuid_count};

    let basic = __cpuid(0);
    let basic_counted = __cpuid_count(0, 0);
    let basic_direct = cpuid_via_inline_asm(0, 0);

    // Every x86-64 CPU exposes at least leaf 1, and the three vendor registers cannot all be zero.
    test!(basic.eax >= 1);
    test!((basic.ebx | basic.ecx | basic.edx) != 0);
    test_eq!(result_array(basic), result_array(basic_counted));
    test_eq!(result_array(basic), basic_direct);

    let feature = __cpuid_count(1, 0);
    test_eq!(result_array(feature), cpuid_via_inline_asm(1, 0));

    // Structured-feature leaf 7 uses ECX as a real subleaf selector. This catches output/input
    // aliasing bugs that leaf 0 or 1 (which ignore ECX) cannot expose.
    if basic.eax >= 7 {
        let structured = __cpuid_count(7, 0);
        test_eq!(
            result_array(structured),
            cpuid_with_cross_aliased_input(7, 0)
        );
    }
}

fn main() {
    #[cfg(target_arch = "x86_64")]
    test_cpuid();

    // The checked-in fixture remains buildable on the public Apple-Silicon host. Its behavioral
    // assertions execute on Linux/Windows x64 runners, where stdarch exposes CPUID. On non-x86,
    // the backend helper's `X86Base.IsSupported == false` branch is covered only when this source is
    // cross-compiled with the x86_64 .NET target and then run under the local CoreCLR.
    #[cfg(not(target_arch = "x86_64"))]
    black_box(());
}
