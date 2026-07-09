//! WF-7 P3 — defining a managed .NET class *from Rust* via `dotnet_typedef!`.
//!
//! `dotnet_typedef!` expands to a `rustc_codegen_clr_comptime_entrypoint` fn whose MIR calls the four
//! "magic" intrinsics below. Those intrinsic bodies `abort()` and are never executed — instead the
//! backend's comptime interpreter (`src/comptime.rs`) reads their MIR and, as a side effect, registers
//! a real .NET `ClassDef` into the produced assembly. So the `RustObj` declared at the bottom becomes a
//! managed class `RustObj : System.Object` with an `int32 value` field and a `virtual int32 get_value()`
//! method that aliases the ordinary Rust fn `get_value::rustc_codegen_clr_not_magic`.
//!
//! Verify with: `ikdasm librust_typedef.so | grep -A6 'class.*RustObj'`.

#![feature(adt_const_params, unsized_const_params, core_intrinsics)]
#![allow(
    internal_features,
    incomplete_features,
    unused_variables,
    dead_code,
    improper_ctypes_definitions,
    improper_ctypes
)]

use core::hint::black_box;

pub struct ClassDef {
    prevent_construction: usize,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct RustcCLRInteropManagedClass<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str> {
    prevent_construction: usize,
}
type RustObj_ = RustcCLRInteropManagedClass<"", "RustObj">;

#[inline(never)]
pub fn rustc_codegen_clr_add_field_def<T, const FNAME: &'static str>(class: ClassDef) -> ClassDef {
    black_box(());
    loop {
        if black_box(true) {
            core::intrinsics::abort()
        }
    }
}
#[inline(never)]
pub fn rustc_codegen_clr_add_method_def<
    const VIS: &'static str,
    const MODIFIERS: &'static str,
    const FNAME: &'static str,
    FnType,
>(
    class: ClassDef,
    fn_type: FnType,
) -> ClassDef {
    black_box(());
    loop {
        if black_box(true) {
            core::intrinsics::abort()
        }
    }
}
#[inline(never)]
pub fn rustc_codegen_clr_new_typedef<
    const NAME: &'static str,
    const IS_VALUETYPE: bool,
    const INHERITS_ASM: &'static str,
    const INHERITS: &'static str,
    // `dotnet_typedef!` is single-entrypoint (no `#[dotnet_methods]`-style re-opening exists for
    // it), so its `IS_VALUETYPE` is always an authoritative opinion — see
    // `mycorrhiza::comptime::rustc_codegen_clr_new_typedef`'s doc for why this flag exists.
    const HAS_TYPE_KIND_OPINION: bool,
>() -> ClassDef {
    black_box(());
    loop {
        if black_box(true) {
            core::intrinsics::abort()
        }
    }
}
#[inline(never)]
pub fn rustc_codegen_clr_finish_type(class: ClassDef) {
    black_box(());
    loop {
        if black_box(true) {
            core::intrinsics::abort()
        }
    }
}

macro_rules! typedef_fields {
    ($typedef:ident,) => {};
    ($typedef:ident, $field_name:ident : $field_type:ty, $($tail:tt)*) => {
        const $field_name: &str = stringify!($field_name);
        $typedef = $crate::rustc_codegen_clr_add_field_def::<$field_type, $field_name>($typedef);
        typedef_fields!($typedef, $($tail)*)
    };
    ($typedef:ident, virtual fn $fname:ident($($args:tt)*)->$ret:ty{$($inner:tt)*}, $($tail:tt)*) => {
        use super::*;
        mod $fname {
            use super::super::*;
            #[inline(never)]
            pub extern "C" fn rustc_codegen_clr_not_magic($($args)*) -> $ret {
                $($inner)*
            }
        }
        const FNAME: &str = stringify!($fname);
        #[used]
        static KEEP_FN: extern "C" fn($($args)*) -> $ret = $fname::rustc_codegen_clr_not_magic;

        $typedef = $crate::rustc_codegen_clr_add_method_def::<"pub", "virtual", FNAME, _>(
            $typedef,
            $fname::rustc_codegen_clr_not_magic,
        );
        typedef_fields!($typedef, $($tail)*)
    };
}

macro_rules! dotnet_typedef {
    () => {};
    (class $name:ident inherits [$superasm:path] $superclass:path { $($inner:tt)* }) => {
        mod $name {
            #[used]
            static PREVENT_DEAD_CODE_REMOVAL: fn() = rustc_codegen_clr_comptime_entrypoint;
            #[inline(never)]
            pub fn rustc_codegen_clr_comptime_entrypoint() {
                const NAME: &str = stringify!($name);
                const SUPER_CLASS: &str = stringify!($superclass);
                const SUPER_ASM: &str = stringify!($superasm);
                let mut class =
                    $crate::rustc_codegen_clr_new_typedef::<NAME, false, SUPER_ASM, SUPER_CLASS, true>();
                typedef_fields!(class, $($inner)*);
                $crate::rustc_codegen_clr_finish_type(class);
            }
        }
    };
}

dotnet_typedef! {
    class RustObj inherits [System::Runtime]System::Object {
        value : i32,
        virtual fn get_value(this: RustObj_) -> i32 {
            42
        },
    }
}
