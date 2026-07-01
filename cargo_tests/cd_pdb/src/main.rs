//! Phase-0 probe for docs/PE_EMISSION_PLAN.md: does the PDB that ilasm -debug already produces
//! give managed stack traces real Rust file:line info? `Environment.StackTrace` resolves frames
//! through the loaded portable PDB, so if the sequence points ilasm built from our `.line`
//! directives are sound, the trace below names this very file.

use mycorrhiza::system::DotNetString;

#[inline(never)]
fn deep_leaf_for_pdb_probe() -> String {
    DotNetString::from_handle(mycorrhiza::System::Environment::get_stack_trace()).to_rust_string()
}

#[inline(never)]
fn middle_frame_for_pdb_probe() -> String {
    deep_leaf_for_pdb_probe()
}

fn main() {
    let trace = middle_frame_for_pdb_probe();
    println!("=== managed stack trace ===");
    println!("{trace}");
    println!("=== verdict ===");
    // .NET renders resolved frames as "at M() in /abs/path/main.rs:line N"
    println!("names this file:      {}", trace.contains("main.rs"));
    println!("has file:line frames: {}", trace.contains(".rs:line"));
    println!(
        "names probe fn:       {}",
        trace.contains("deep_leaf_for_pdb_probe")
    );
}
