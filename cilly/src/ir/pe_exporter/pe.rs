//! The PE/COFF container + CLI header (§II.25): DOS stub, COFF header, PE32 optional header,
//! section table, `.text`/`.sdata` section contents, and the CLI header the CLR loader reads to
//! find the metadata root and entry point.
//!
//! # The RVA-fixup dance
//!
//! Everything in a PE image is addressed by **RVA** (relative virtual address — offset from the
//! image's load base once mapped, §II.25.3), but method bodies and `FieldRVA` blobs are built
//! *before* their final position is known, and the metadata tables that reference them
//! (`MethodDef.RVA`, `FieldRVA.RVA`) are serialized *after*. The pipeline is therefore a strict
//! four-phase sequence — no phase can start before the previous one is complete:
//!
//! 1. **Assemble every body.** The caller calls [`super::body::assemble_method`] for every
//!    `MethodDef` in the `Assembly`, collecting each `AssembledBody`. These are position-
//!    independent byte buffers (no RVA baked in yet — `call`/`ldsflda`/branch targets inside a
//!    body reference *other* methods/fields by metadata *token*, not by address, so body bytes
//!    never need to change once assembled). Likewise collect every `FieldRVA` blob (`const_data`
//!    blobs and any other statically-initialized field data) as a raw byte buffer.
//! 2. **Layout pass.** Decide the file/section layout: a running offset within `.text` for each
//!    method body (4-byte aligned per body, §II.25.4.1) and within `.sdata` for each `FieldRVA`
//!    blob, PLUS the section's file offset and its section-aligned RVA base (`SectionAlignment`
//!    0x2000 / `FileAlignment` 0x200, §II.25.2.3.1 — RVAs are computed by walking sections in
//!    order and rounding each one's start up to `SectionAlignment`; **before this pass runs, the
//!    CLI header + metadata blob's own size must already be known**, since they occupy the head
//!    of `.text` ahead of the method bodies — so `MetadataBuilder::serialize()` size is computed
//!    (or the metadata is serialized once with placeholder token RVAs, since RVAs never appear
//!    *inside* the metadata heaps/tables themselves — only `MethodDef.RVA`/`FieldRVA.RVA`
//!    columns hold them, and those are exactly the values this layout pass produces). This
//!    yields, for every method and every field-data blob, an absolute RVA.
//! 3. **Patch RVAs into the metadata builder.** The caller calls
//!    [`super::tables::MetadataBuilder::set_method_body_rva`] and
//!    [`super::tables::MetadataBuilder::set_field_rva`] for every value the layout pass produced.
//!    Only now does [`super::tables::MetadataBuilder::serialize`] run, producing the final
//!    metadata-root bytes with correct `RVA` columns.
//! 4. **`write_pe` assembles the file.** Given the now-final metadata bytes, the concatenated
//!    (already-laid-out, in the same order the layout pass used) body bytes, and the
//!    (already-laid-out) field-data blobs, this module writes:
//!    * DOS header + stub (§II.25.2.1) — the fixed MS-DOS stub every PE image carries, `e_lfanew`
//!      pointing at the COFF header.
//!    * COFF header (§II.25.2.2): machine `IMAGE_FILE_MACHINE_I386` (0x14C — CIL images target
//!      "AnyCPU" via this 32-bit-with-`ILONLY` convention regardless of the eventual JIT
//!      architecture, matching every ilasm-produced image this backend has emitted so far),
//!      `NumberOfSections`, **zero timestamp** (determinism — no wall-clock bytes anywhere in the
//!      image), characteristics `0x0102` (`EXECUTABLE_IMAGE | 32BIT_MACHINE`; `+0x2000 DLL` when
//!      `is_dll`).
//!    * PE32 optional header (§II.25.2.3.1): magic `0x10B`, `ImageBase = 0x400000`,
//!      `SectionAlignment = 0x2000`, `FileAlignment = 0x200`, `Subsystem = 3` (CUI, matching
//!      ilasm's console-subsystem output), `NumberOfRvaAndSizes = 16`, the `DataDirectory[14]`
//!      "CLI Header" entry pointing at the CLI header below (every other directory zero except
//!      `DataDirectory[1]` Import Table / `DataDirectory[12]` IAT if a native `mscoree.dll`
//!      bootstrap import is emitted, mirroring the standard ilasm-produced native stub — deferred
//!      until byte-diffed against real ilasm output per the plan doc's Risk #1).
//!    * Section table + section bytes: `.text` (CLI header + metadata root + method bodies, in
//!      that order, §II.25.3.3 recommends but does not require this grouping) as
//!      `CODE | EXECUTE | READ`; `.sdata` (the laid-out `FieldRVA` blobs) as
//!      `INITIALIZED_DATA | READ | WRITE` (`il_exporter`'s FieldRVA statics are mutable storage,
//!      not `.rodata` — see the `static c_N` field, not `initonly`).
//!    * CLI header (§II.25.3.3, 72 bytes): `cb = 72`, `MajorRuntimeVersion = 2`,
//!      `MinorRuntimeVersion = 5`, `Flags = COMIMAGE_FLAGS_ILONLY` (`0x1`; `+ 0x10`
//!      `32BITPREFERRED` is NOT set — matches ilasm's AnyCPU output), `EntryPointToken` from
//!      [`PeOptions::entry_point`] (0 for a library with no entry point), `Metadata` RVA/size
//!      pointing at the metadata root placed in step 4's `.text` layout.
//!
//! MVID (the `#GUID` heap's sole entry, embedded in the metadata's `Module` table row) and the
//! PE timestamp are both determinism-constrained per `docs/PE_EMISSION_PLAN.md`: **no timestamps,
//! no randomness** anywhere in emitted bytes. The MVID must be derived from the assembly's
//! content (e.g. a hash of the serialized metadata minus the MVID field itself, or of the
//! `Assembly`'s own `Hash` impl) and the COFF timestamp field is always written as `0`.

/// Top-level knobs `write_pe` needs beyond what `MetadataBuilder`/`AssembledBody` already carry.
pub struct PeOptions {
    /// `true` for a `.dll` (COFF characteristic `IMAGE_FILE_DLL` set, no required entry point);
    /// `false` for a `.exe` (console subsystem, `entry_point` must be `Some`). Mirrors
    /// `ILExporter::is_lib`.
    pub is_dll: bool,
    /// The `MethodDef` token (§II.25.3.3 `EntryPointToken`) of the method named `"entrypoint"`
    /// (see `asm::ENTRYPOINT`), i.e. `Token::new(Token::TABLE_METHOD_DEF, rid).0`. `None` for a
    /// library with no managed entry point (`EntryPointToken` field written as 0).
    pub entry_point: Option<u32>,
}

/// Writes the complete PE image: DOS header, COFF header, PE32 optional header, section table,
/// `.text` (CLI header + metadata + method bodies), `.sdata` (`FieldRVA` data). See the module
/// doc for the RVA-fixup pipeline this function is the last step of.
///
/// # Parameters
/// * `metadata` — the final bytes from [`super::tables::MetadataBuilder::serialize`], produced
///   AFTER every `MethodDef.RVA`/`FieldRVA.RVA` has been patched in (pipeline step 3).
/// * `method_bodies` — every [`super::body::AssembledBody`]'s bytes, already concatenated by the
///   caller's layout pass (pipeline step 2) in the exact order/alignment that produced the RVAs
///   patched into `metadata`. Opaque to this function — it places them verbatim at the offset the
///   layout pass chose (recomputed here identically, since section/file alignment are the fixed
///   constants documented above and thus reproducible from `metadata.len()` +
///   `method_bodies.len()` alone).
/// * `field_rva_data` — every `FieldRVA` blob, concatenated in the same layout-pass order used
///   for the RVAs patched into `metadata`'s `FieldRVA` rows; placed verbatim in `.sdata`.
/// * `options` — see [`PeOptions`].
///
/// Returns the complete image bytes, ready to write to a `.dll`/`.exe` file.
#[must_use]
pub fn write_pe(
    metadata: &[u8],
    method_bodies: &[u8],
    field_rva_data: &[u8],
    options: &PeOptions,
) -> Vec<u8> {
    let _ = (metadata, method_bodies, field_rva_data, options);
    todo!(
        "DOS header -> COFF header -> PE32 optional header -> section table -> \
         .text (CLI header + metadata + bodies) -> .sdata (FieldRVA data)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pe_options_construct() {
        let opts = PeOptions {
            is_dll: false,
            entry_point: Some(0x0600_0001),
        };
        assert!(!opts.is_dll);
        assert_eq!(opts.entry_point, Some(0x0600_0001));
    }

    #[test]
    fn pe_options_lib_has_no_forced_entry_point() {
        let opts = PeOptions {
            is_dll: true,
            entry_point: None,
        };
        assert!(opts.is_dll);
        assert_eq!(opts.entry_point, None);
    }
}
