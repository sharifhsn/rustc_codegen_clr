// Generic-METHOD calls (`!!N`): a method that itself takes type arguments — the biggest wall for
// consuming a real .NET codebase (it gates DI's GetService<T>, Deserialize<T>, Map<T>, ...).
// This proves the `rustc_clr_interop_generic_method_call*` family end-to-end.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use mycorrhiza::intrinsics::{
    RustcCLRInteropManagedClass, RustcCLRInteropManagedStruct, RustcCLRInteropMethodGeneric,
    rustc_clr_interop_generic_method_call0, rustc_clr_interop_generic_method_call1,
};
use mycorrhiza::system::{DotNetString, MString, console::Console};

const CORELIB: &str = "System.Private.CoreLib";
const ACTIVATOR: &str = "System.Activator";
type MSB = RustcCLRInteropManagedClass<CORELIB, "System.Text.StringBuilder">;

// A C# enum is an int-backed value type. Construct one from its int by transmute, then pass it to the
// generic method `Enum.GetName<TEnum>(TEnum) -> string` — proving enums round-trip AND exercising a
// generic method whose type arg (the enum) drives the result.
type DayOfWeek = RustcCLRInteropManagedStruct<CORELIB, "System.DayOfWeek", 4>;
fn dow_from_i32(v: i32) -> DayOfWeek {
    unsafe { core::mem::transmute::<i32, DayOfWeek>(v) }
}
fn enum_get_name(dow: DayOfWeek) -> DotNetString {
    let m = rustc_clr_interop_generic_method_call1::<
        CORELIB,
        "System.Enum",
        false,
        "GetName",
        0,
        (),
        (DayOfWeek,),
        (MString, RustcCLRInteropMethodGeneric<0>),
        MString,
        DayOfWeek,
    >(dow);
    DotNetString::from_handle(m)
}

// The ergonomic bridge: a Rust mirror of `System.DayOfWeek` with boundary conversions.
mycorrhiza::dotnet_enum! {
    pub enum Dow = ["System.Private.CoreLib"] "System.DayOfWeek" (i32, 4) {
        Sunday = 0, Monday = 1, Tuesday = 2, Wednesday = 3, Thursday = 4, Friday = 5, Saturday = 6,
    }
}

// Activator.CreateInstance<T>() -> !!0  (static generic method, method generic = T).
fn create_sb() -> MSB {
    rustc_clr_interop_generic_method_call0::<
        CORELIB,
        ACTIVATOR,
        false,
        "CreateInstance",
        0,
        (),
        (MSB,),
        (RustcCLRInteropMethodGeneric<0>,),
        MSB,
    >()
}
fn create_i32() -> i32 {
    rustc_clr_interop_generic_method_call0::<
        CORELIB,
        ACTIVATOR,
        false,
        "CreateInstance",
        0,
        (),
        (i32,),
        (RustcCLRInteropMethodGeneric<0>,),
        i32,
    >()
}
fn create_i64() -> i64 {
    rustc_clr_interop_generic_method_call0::<
        CORELIB,
        ACTIVATOR,
        false,
        "CreateInstance",
        0,
        (),
        (i64,),
        (RustcCLRInteropMethodGeneric<0>,),
        i64,
    >()
}

static mut PASS: u32 = 0;
static mut TOTAL: u32 = 0;
fn chk(id: u32, got: i64, want: i64) {
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
    // CreateInstance<StringBuilder>() returns a real, usable object (a generic method with an `!!0`
    // return, its method type arg carried on the methodref so the CLR binds the instantiation).
    let sb = create_sb();
    chk(1, sb.virt0::<"get_Length", i32>() as i64, 0); // fresh StringBuilder
    let _ = sb.instance1::<"Append", i32, MSB>(42); // Append(int) -> "42"
    chk(2, sb.virt0::<"get_Length", i32>() as i64, 2); // now length 2

    // Value-type method generics: CreateInstance<int>()==0, CreateInstance<long>()==0.
    chk(3, create_i32() as i64, 0);
    chk(4, create_i64(), 0);

    // Enum round-trip via the generic method Enum.GetName<DayOfWeek>(value): 3 == Wednesday.
    let name = enum_get_name(dow_from_i32(3)).to_rust_string();
    chk(5, (name == "Wednesday") as i64, 1);
    let name0 = enum_get_name(dow_from_i32(0)).to_rust_string();
    chk(6, (name0 == "Sunday") as i64, 1);

    // Ergonomic `dotnet_enum!` bridge: Rust variant -> handle -> .NET -> name, and back.
    let n = enum_get_name(Dow::Friday.to_handle()).to_rust_string();
    chk(7, (n == "Friday") as i64, 1);
    // A .NET-produced handle -> Rust variant (match on it natively).
    let got = Dow::from_handle(dow_from_i32(1));
    chk(8, matches!(got, Some(Dow::Monday)) as i64, 1);
    chk(9, Dow::Wednesday.value() as i64, 3);

    unsafe {
        Console::writeln_u64(PASS as u64);
        Console::writeln_u64(TOTAL as u64);
        if PASS == TOTAL {
            println!("== cd_gmethod done ==");
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        }
    }
}
