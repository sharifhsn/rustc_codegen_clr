//! Phase-2 acceptance probe for docs/PE_EMISSION_PLAN.md: under the DEFAULT `DIRECT_PE=1` path (no
//! `ilasm` anywhere), does the standalone Portable PDB `cilly::pe_exporter::pdb`/the linker's
//! `<exe-stem>.pdb` write actually give managed stack traces real Rust file:line info?
//! `Environment.StackTrace` resolves frames through the loaded portable PDB, so if the sequence
//! points our writer built from `CILRoot::SourceFileInfo` are sound — and the PE's Debug Directory
//! correctly points at that PDB (see `cilly/src/ir/pe_exporter/pe.rs`'s
//! `PORTABLE_CODEVIEW_MAJOR_VERSION`/`DebugDirectoryEntry::stamp` docs for the two real bugs this
//! probe caught and pinned) — the trace below names this very file.
//!
//! Originally a Phase-0 probe against the `ilasm -debug` path (superseded now that `DIRECT_PE=1`
//! is the default); kept the exact same assert shape (file name / `.rs:line` / innermost-fn-name
//! substrings) so this file doubles as the parity check the Phase-2 task description calls for —
//! swap only the producer (our writer instead of ilasm), keep the same consumer-side check.

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
    let asm = mycorrhiza::System::Reflection::Assembly::get_executing_assembly();
    let location = DotNetString::from_handle(asm.get_location()).to_rust_string();
    println!("=== assembly location ===");
    println!("{location}");
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
