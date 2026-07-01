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
//! * `sig` — `Type` → `ELEMENT_TYPE_*` signature-blob encoding (fields, methods, locals,
//!   `MethodSpec`, `calli` stand-alone sigs). *(next)*
//! * `tables` — the metadata tables + coded-index/heap-index width computation and the
//!   populate → size → serialize pipeline. *(next)*
//! * `body` — method bodies: tiny/fat headers, opcode bytes, branch layout, fat EH sections.
//!   *(next)*
//! * `pe` — the PE/COFF container and CLI header. *(next)*

pub mod heaps;
pub mod sig;
