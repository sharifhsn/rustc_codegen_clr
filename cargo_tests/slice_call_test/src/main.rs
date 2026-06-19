// Calls the method wrappers emitted by the `spinacz` binding generator (see
// mycorrhiza::slice_bindings) and prints the results, to diff against native .NET.
//
// Each wrapper is a thin shim over the staticN/instanceN/virt0/ctorN helpers; calling them here
// exercises the full pipeline: generated wrapper -> magic-fn -> emitted .NET call.
#![allow(
    internal_features,
    unused_imports,
    incomplete_features,
    unused_variables,
    dead_code,
    improper_ctypes_definitions
)]
#![feature(lang_items, adt_const_params, associated_type_defaults, core_intrinsics)]

use mycorrhiza::slice_bindings::slice::System::Math;
use mycorrhiza::slice_bindings::slice::System::Text::StringBuilder;
use mycorrhiza::system::console::Console;

fn main() {
    // --- System.Math: static method wrappers ---
    Console::writeln_u64(Math::max(3, 7) as u64); // expect 7
    Console::writeln_u64(Math::min(3, 7) as u64); // expect 3
    Console::writeln_u64(Math::abs(-5) as u64); // expect 5
    Console::writeln_f64(Math::sqrt(144.0)); // expect 12

    // --- System.Text.StringBuilder: ctor + instance + virtual(property) wrappers ---
    // new() -> append(7) -> append(42) ; "742" has length 3.
    let sb = StringBuilder::new();
    let sb = sb.append(7);
    let sb = sb.append(42);
    Console::writeln_u64(sb.get_length() as u64); // expect 3
}
