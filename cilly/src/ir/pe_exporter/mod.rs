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
//! * [`pe`] — the PE/COFF container and CLI header. *(Phase 1a: implemented + unit-tested)*
//!
//! Phase 1a status: each module is unit-tested in isolation (heaps/sig/tables/body/pe), but no
//! end-to-end driver wires `tables::MetadataBuilder` + `body::assemble_method` +
//! `pe::write_pe` into a single `Assembly::export_pe(...)` entry point yet — that integration,
//! plus the "hand-built two-method assembly loads and runs under `dotnet`" acceptance check from
//! `docs/PE_EMISSION_PLAN.md`, is the next step (Phase 1a close-out / Phase 1b).

pub mod heaps;
pub mod sig;
pub mod tables;
pub mod body;
pub mod pe;
