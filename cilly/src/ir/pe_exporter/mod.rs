//! Direct ECMA-335 PE emission — writes the final `.dll`/`.exe` (and, later, the Portable PDB)
//! straight from the interned IR, with no textual `.il` and no external `ilasm`.
//!
//! Design, construct inventory, phasing, and validation strategy: `docs/PE_EMISSION_PLAN.md`.
//! The [`il_exporter`](super::il_exporter) remains the default until this path survives the full
//! `::stable` gate under the `DIRECT_PE=1` A/B differential; the emitted subset of ECMA-335 is
//! exactly the subset `il_exporter` emits today — nothing more.
//!
//! Layout of the writer (each stage is independently unit-tested):
//! * [`heaps`] — the four metadata heaps (`#Strings`, `#Blob`, `#GUID`, `#US`), interned + deduped.
//! * [`sig`] — `Type` → `ELEMENT_TYPE_*` signature-blob encoding (fields, methods, locals,
//!   `MethodSpec`, `calli` stand-alone sigs).
//! * [`tables`] — the metadata tables + coded-index/heap-index width computation and the
//!   populate → size → serialize pipeline. *(Phase 1a: implemented + unit-tested)*
//! * [`body`] — method bodies: tiny/fat headers, opcode bytes, branch layout, fat EH sections.
//!   *(Phase 1a: implemented + unit-tested)*
//! * [`pe`] — the PE/COFF container and CLI header, including the native `mscoree.dll`
//!   `_CorExeMain` bootstrap stub (IAT/Import Table/`.reloc`) an `.exe` needs to satisfy the OS's
//!   native PE loader before the CLR ever inspects the CLI header. *(Phase 1a: implemented +
//!   unit-tested)*
//! * [`export`] — `export_pe`: the top-level driver wiring `tables::MetadataBuilder` +
//!   `body::assemble_method` + the RVA layout pass + `pe::write_pe` into one entry point.
//!   *(Phase 1a MILESTONE PROVEN 2026-07-02: a hand-built static-entrypoint-calling-
//!   `Console.WriteLine` `Assembly`, exported with no `ilasm` anywhere, loads and runs under a
//!   real `dotnet` host — `export::tests::e2e_hand_built_assembly_runs_under_dotnet`. Only the
//!   inventory subset that test exercises is wired; const-data `FieldRVA` blobs, non-`ByteBuffer`
//!   static-field defaults, and `MainModule` method-count partitioning are loud `todo!()`s left
//!   for Phase 1b — see `export`'s module doc.)*
//! * [`pdb`] — Portable PDB (dotnet/runtime `PortablePdb-Metadata.md`): `#Pdb` stream +
//!   `Document`/`MethodDebugInformation` tables from `CILRoot::SourceFileInfo` sequence points,
//!   plus the PE-side Debug Directory (CodeView/RSDS) hook. *(Phase 2: interface-pinning stub —
//!   see that module's doc for the parity bar against `il_exporter`'s `.line` + `ilasm -debug`.)*

pub mod body;
pub mod export;
pub mod heaps;
pub mod pdb;
pub mod pe;
pub mod sig;
pub mod tables;
