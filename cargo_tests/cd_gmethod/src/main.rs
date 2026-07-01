// Generic-METHOD calls (`!!N`): a method that itself takes type arguments — the biggest wall for
// consuming a real .NET codebase (it gates DI's GetService<T>, Deserialize<T>, Map<T>, ...).
// This proves the `rustc_clr_interop_generic_method_call*` family end-to-end.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use mycorrhiza::intrinsics::{
    rustc_clr_interop_generic_method_call0, RustcCLRInteropManagedClass, RustcCLRInteropMethodGeneric,
};
use mycorrhiza::system::console::Console;

const CORELIB: &str = "System.Private.CoreLib";
const ACTIVATOR: &str = "System.Activator";
type MSB = RustcCLRInteropManagedClass<CORELIB, "System.Text.StringBuilder">;

// Activator.CreateInstance<T>() -> !!0  (static generic method, method generic = T).
fn create_sb() -> MSB {
    rustc_clr_interop_generic_method_call0::<
        CORELIB, ACTIVATOR, false, "CreateInstance", 0, (), (MSB,),
        (RustcCLRInteropMethodGeneric<0>,), MSB,
    >()
}
fn create_i32() -> i32 {
    rustc_clr_interop_generic_method_call0::<
        CORELIB, ACTIVATOR, false, "CreateInstance", 0, (), (i32,),
        (RustcCLRInteropMethodGeneric<0>,), i32,
    >()
}
fn create_i64() -> i64 {
    rustc_clr_interop_generic_method_call0::<
        CORELIB, ACTIVATOR, false, "CreateInstance", 0, (), (i64,),
        (RustcCLRInteropMethodGeneric<0>,), i64,
    >()
}

static mut PASS: u32 = 0;
static mut TOTAL: u32 = 0;
fn chk(id: u32, got: i64, want: i64) {
    unsafe {
        TOTAL += 1;
        if got == want { PASS += 1; } else { Console::writeln_u64(90_000_000 + id as u64); Console::writeln_u64(got as u64); }
    }
}

fn main() -> std::process::ExitCode {
    // CreateInstance<StringBuilder>() returns a real, usable object (a generic method with an `!!0`
    // return, its method type arg carried on the methodref so the CLR binds the instantiation).
    let sb = create_sb();
    chk(1, sb.virt0::<"get_Length", i32>() as i64, 0); // fresh StringBuilder
    let _ = sb.instance1::<"Append", i32, MSB>(42);     // Append(int) -> "42"
    chk(2, sb.virt0::<"get_Length", i32>() as i64, 2);  // now length 2

    // Value-type method generics: CreateInstance<int>()==0, CreateInstance<long>()==0.
    chk(3, create_i32() as i64, 0);
    chk(4, create_i64(), 0);

    unsafe {
        Console::writeln_u64(PASS as u64);
        Console::writeln_u64(TOTAL as u64);
        if PASS == TOTAL { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
    }
}
