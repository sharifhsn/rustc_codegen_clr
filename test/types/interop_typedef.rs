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
    improper_ctypes_definitions,
    improper_ctypes
)]
use core::hint::black_box;
pub struct ClassDef {
    prevent_construction: usize,
}
//include!("../common.rs");
#[allow(dead_code)]
#[derive(Clone, Copy)]
struct RustcCLRInteropManagedClass<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str> {
    prevent_construction: usize,
}
type Object = RustcCLRInteropManagedClass<"System.Runtime", "System.Object">;
type MString = RustcCLRInteropManagedClass<"System.Runtime", "System.String">;
impl Into<MString> for &str {
    fn into(self) -> MString {
        // This fixture only verifies typedef metadata construction; its managed-string body is
        // never executed. Keep the placeholder representable by the backend's abort intrinsic so
        // the fatal post-link verifier is not defeated by a deliberately unresolved panic shim.
        core::intrinsics::abort()
    }
}
type RustObj_ = RustcCLRInteropManagedClass<"", "RustObj">;
type RustObj2_ = RustcCLRInteropManagedClass<"", "RustObj2">;
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
    const PARAM_NAMES: &'static str,
    const NULLABILITY: &'static str,
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
pub fn rustc_codegen_clr_add_static_method_def<
    const FNAME: &'static str,
    const PARAM_NAMES: &'static str,
    const NULLABILITY: &'static str,
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
    ($typedef:ident,)=>{};
    ($typedef:ident, $field_name:ident : $field_type:ty, $($tail:tt)*) => {
        const $field_name:&str= stringify!( $field_name);
        $typedef = $crate::rustc_codegen_clr_add_field_def::<$field_type, $field_name>($typedef);
        typedef_fields!($typedef, $($tail)*)
    };
    ($typedef:ident, virtual fn $fname:ident($($args:tt)*)->$ret:ty{$($inner:tt)*}, $($tail:tt)*) => {
        mod $fname{
            use super::super::*;
            #[inline(never)]

            pub extern "C" fn rustc_codegen_clr_not_magic ($($args)*)->$ret{
                $($inner)*
            }
        }
        const FNAME:&str = stringify!($fname);
        $typedef = $crate::rustc_codegen_clr_add_method_def::<"pub","virtual",FNAME,"","",_>($typedef,$fname::rustc_codegen_clr_not_magic);
        typedef_fields!($typedef, $($tail)*)
    };
}
macro_rules! dotnet_typedef {
    () => {};

    (class $name:ident inherits [$superasm:path] $superclass:path {  $($inner:tt)* }) => {
        mod $name {
            #[used]
            static PREVENT_DEAD_CODE_REMOVAL: fn() = rustc_codegen_clr_comptime_entrypoint;
            #[doc = "__rustc_codegen_clr_comptime_entrypoint_v1"]
            #[inline(never)]
            pub fn rustc_codegen_clr_comptime_entrypoint() {
                const NAME: &str = stringify!($name);
                const SUPER_CLASS: &str = stringify!($superclass);
                const SUPER_ASM: &str = stringify!($superasm);
                let mut class =
                    $crate::rustc_codegen_clr_new_typedef::<NAME, false, SUPER_ASM, SUPER_CLASS, true>();
                typedef_fields!(class,   $($inner)*);
                $crate::rustc_codegen_clr_finish_type(class);
            }
        }
    };

    (struct $name:ident inherits [$superasm:path]  $superclass:path { $($inner:tt)* }) => {
        mod $name {
            #[used]
            static PREVENT_DEAD_CODE_REMOVAL: fn() = rustc_codegen_clr_comptime_entrypoint;

            #[doc = "__rustc_codegen_clr_comptime_entrypoint_v1"]
            #[inline(never)]
            pub fn rustc_codegen_clr_comptime_entrypoint() {
                const NAME: &str = stringify!($name);
                const SUPER_CLASS: &str = stringify!($superclass);
                const SUPER_ASM: &str = stringify!($superasm);
                let class =
                    $crate::rustc_codegen_clr_new_typedef::<NAME, true, SUPER_ASM, SUPER_CLASS, true>();
                //typedef_fields!(class,  $($inner:tt)*);
                $crate::rustc_codegen_clr_finish_type(class);
            }
        }
    };
}

dotnet_typedef! {
    class RustObj inherits [System::Runtime]System::Runtime::Object{
        a : f32,
        virtual fn ToString(this:RustObj_)->MString{
            "This is a .NET class - defined in Rust !".into()
        },
        // NOTE: the current shape of this macro, and it's implementation is
        // highly experimental. All of this is subject to change,
        // This is a **very** early prototype!
    }
}

// Exact producer regression for `#[dotnet_methods]`' private marshalling shims. Neither the shim nor
// its generic slice helper has an ordinary Rust caller: only the comptime-declared managed static
// method reaches them. The backend must emit the complete rustc mono dependency closure at that
// declaration edge instead of leaving either method as a reachable `MethodImpl::Missing`.
#[inline(never)]
unsafe fn handle_as_mut_slice<T>(data: *mut T, len: usize) -> &'static mut [T] {
    unsafe { core::slice::from_raw_parts_mut(data, len) }
}

#[inline(never)]
fn __dotnet_methods_shim_sum_span(mut value: i32) -> i32 {
    let values = unsafe { handle_as_mut_slice::<i32>(&raw mut value, 1) };
    values[0] + 1
}

mod __dotnet_methods_ShimClass {
    use super::*;

    #[used]
    static PREVENT_DCE: fn() = rustc_codegen_clr_comptime_entrypoint;
    #[doc = "__rustc_codegen_clr_comptime_entrypoint_v1"]
    #[inline(never)]
    pub fn rustc_codegen_clr_comptime_entrypoint() {
        let class = rustc_codegen_clr_new_typedef::<"ShimClass", false, "", "", false>();
        let class = rustc_codegen_clr_add_static_method_def::<"SumSpan", "value", "", _>(
            class,
            __dotnet_methods_shim_sum_span,
        );
        rustc_codegen_clr_finish_type(class);
    }
}

dotnet_typedef! {
    class RustObj2 inherits [System::Runtime]System::Runtime::Object{
        a : f32,
        virtual fn ToString(this:RustObj2_)->MString{
            core::intrinsics::abort()
        },

    }
}
/*dotnet_typedef! {
    class RustObj2 inherits RustObj{

    }
}*/
dotnet_typedef! {
    struct RustStruct inherits [System::Runtime]System::Runtime::ValueType{

    }
}
fn main() {
    let chr: *mut RustObj_ = core::ptr::null_mut();
    black_box(chr);
}
