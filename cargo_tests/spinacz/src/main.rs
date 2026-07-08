#![feature(adt_const_params, unsized_const_params)]
mod reflect;
use mycorrhiza::system::MString;
use mycorrhiza::System::Reflection::Assembly;
use reflect::{reflect_assembly, Namespace};
use std::io::Write;

/// BCL assemblies we reflect. `std::env::args` is unavailable under the .NET PAL and
/// `AppDomain.CurrentDomain.GetAssemblies()` mis-binds on this backend, so the set is loaded
/// explicitly by name. Every name here is part of the base `Microsoft.NETCore.App` shared
/// framework and is therefore guaranteed present — `Assembly.Load` of a missing name would throw
/// (and we have no `Option<ManagedClass>` to absorb it), so the list is the broad always-present
/// surface, not anything optional. Type forwarding means a handful of these expose the bulk of
/// the `System.*` namespace; the per-namespace dedup in `add_tpe` collapses the overlaps.
const BCL_ASSEMBLIES: &[&str] = &[
    "System.Private.CoreLib",
    "System.Runtime",
    "System.Console",
    "System.Collections",
    "System.Collections.Concurrent",
    "System.Collections.NonGeneric",
    "System.Collections.Specialized",
    "System.Linq",
    "System.Linq.Expressions",
    "System.Memory",
    "System.Text.Encoding.Extensions",
    "System.Text.RegularExpressions",
    "System.Runtime.InteropServices",
    "System.Runtime.Numerics",
    "System.Threading",
    "System.Threading.Tasks",
    "System.Globalization",
    "System.ObjectModel",
    "System.ComponentModel",
    "System.ComponentModel.Primitives",
    "System.Diagnostics.Tracing",
    "System.Reflection.Primitives",
    "System.Private.Uri",
];

fn main() {
    // One root namespace holding every type from every assembly. The root itself is anonymous:
    // its children (`System`, `Microsoft`, …) are emitted at the top of the file, matching the
    // existing bindings.rs layout.
    let mut root_asm = Namespace::new(String::new(), 0);
    let mut out = std::fs::File::create("out.rs").unwrap();
    let mut total_types: i32 = 0;

    let asm_len = BCL_ASSEMBLIES.len();
    // NOTE: no `eprintln!`/`println!` for progress — std's `print*`/`eprint*` route through a
    // `dyn Write` + `core::fmt::write` path that faults under the current .NET PAL. The product is
    // the `out.rs` file (written via `writeln!` to a `File`, which works), and a final count is
    // reported via `Console::writeln_u64` (the proven-good managed Console path).
    let mut ai = 0;
    while ai < asm_len {
        let asm_name_str = BCL_ASSEMBLIES[ai];
        ai += 1;
        let mstr: MString = asm_name_str.into();
        let asm = Assembly::static1::<"Load", MString, Assembly>(mstr);
        reflect_assembly(asm, &mut root_asm, &mut total_types);
    }
    root_asm.export_root(&mut out);
    out.flush().unwrap();
    // Final progress signal via the managed Console (the std print path faults under the PAL).
    mycorrhiza::system::console::Console::writeln_u64(total_types as u64);
}
