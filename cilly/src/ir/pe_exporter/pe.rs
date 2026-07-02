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

// ---------------------------------------------------------------------------------------------
// PE/COFF/CLI constants (§II.25). Kept as named constants rather than inline literals so the
// byte-layout code below reads as "which field", not "which magic number".
// ---------------------------------------------------------------------------------------------

/// `e_lfanew` value for the fixed 128-byte MS-DOS stub this writer emits (§II.25.2.1 — the stub
/// contents are conventional, not semantically load-bearing; every ilasm-produced image, and this
/// one, points `e_lfanew` at offset `0x80` immediately following it).
const DOS_STUB_LEN: u32 = 0x80;

/// COFF `Machine` (§II.25.2.2): `IMAGE_FILE_MACHINE_I386`. CIL-only images target "AnyCPU" via
/// this 32-bit convention regardless of the eventual JIT architecture — confirmed against a real
/// Mono-ilasm-produced image (`machine = 0x14c`) during implementation.
const IMAGE_FILE_MACHINE_I386: u16 = 0x14C;
/// COFF `Characteristics` bit: `IMAGE_FILE_EXECUTABLE_IMAGE`.
const IMAGE_FILE_EXECUTABLE_IMAGE: u16 = 0x0002;
/// COFF `Characteristics` bit: `IMAGE_FILE_LARGE_ADDRESS_AWARE`. Verified against a real
/// Mono-ilasm-produced `.exe` (`Characteristics = 0x10e =
/// EXECUTABLE_IMAGE|LINE_NUMS_STRIPPED|LOCAL_SYMS_STRIPPED|LARGE_ADDRESS_AWARE`); this writer
/// emits the `EXECUTABLE_IMAGE|LARGE_ADDRESS_AWARE` subset that's actually meaningful to a CIL
/// loader (the STRIPPED bits are debug-info housekeeping ilasm sets for its own reasons and are
/// not required).
const IMAGE_FILE_LARGE_ADDRESS_AWARE: u16 = 0x0100;
/// COFF `Characteristics` bit: `IMAGE_FILE_DLL`.
const IMAGE_FILE_DLL: u16 = 0x2000;

/// PE32 (not PE32+) optional-header magic (§II.25.2.3.1).
const PE32_MAGIC: u16 = 0x10B;
/// `ImageBase` (§II.25.2.3.1) — the conventional default every ilasm-produced image uses.
const IMAGE_BASE: u32 = 0x0040_0000;
/// `SectionAlignment` (§II.25.2.3.1): RVAs of each section start at a multiple of this once
/// mapped into memory.
const SECTION_ALIGNMENT: u32 = 0x2000;
/// `FileAlignment` (§II.25.2.3.1): each section's raw (on-disk) data starts at a multiple of
/// this.
const FILE_ALIGNMENT: u32 = 0x0200;
/// `Subsystem` (§II.25.2.3.3): `IMAGE_SUBSYSTEM_WINDOWS_CUI` (console), matching ilasm's default.
const SUBSYSTEM_CUI: u16 = 3;
/// `NumberOfRvaAndSizes` (§II.25.2.3.3) — this writer always emits the full 16-entry data
/// directory, all-zero except the CLI header entry.
const NUMBER_OF_RVA_AND_SIZES: u32 = 16;
/// Index of the "CLI Header" entry within the optional header's data-directory array
/// (§II.25.2.3.3, table titled "Optional Header Data Directories").
const DATA_DIRECTORY_CLI_HEADER: usize = 14;

/// `.text` section characteristics: `IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE |
/// IMAGE_SCN_MEM_READ`. Matches the `0x6000_0020` observed in a real ilasm-produced image.
const TEXT_SECTION_CHARACTERISTICS: u32 = 0x6000_0020;
/// `.sdata` section characteristics: `IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ |
/// IMAGE_SCN_MEM_WRITE` — mutable storage, not `.rodata` (`il_exporter`'s FieldRVA statics are
/// non-`initonly` fields, so the backing store must be writable).
const SDATA_SECTION_CHARACTERISTICS: u32 = 0xC000_0040;

/// CLI header `cb` (§II.25.3.3): the header's own fixed size in bytes.
const CLI_HEADER_CB: u32 = 0x48;
const CLI_MAJOR_RUNTIME_VERSION: u16 = 2;
const CLI_MINOR_RUNTIME_VERSION: u16 = 5;
/// CLI header `Flags` bit: `COMIMAGE_FLAGS_ILONLY`. This is the *only* flag this writer sets —
/// `COMIMAGE_FLAGS_32BITREQUIRED` (`0x2`, sometimes called `32BITPREFERRED` in combination with
/// bit 0x20000) is deliberately never set, matching ilasm's AnyCPU output.
const COMIMAGE_FLAGS_ILONLY: u32 = 0x1;

/// Rounds `value` up to the next multiple of `align` (`align` must be a power of two). Used for
/// both `FileAlignment` (raw section placement) and `SectionAlignment` (mapped RVA placement).
#[must_use]
fn align_up(value: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two(), "alignment {align} is not a power of two");
    (value + align - 1) & !(align - 1)
}

/// One section's layout, computed by [`SectionLayout::plan`] before any bytes are written so
/// `write_pe` can fill in the section table, the CLI header's `Metadata` directory entry, and the
/// data placement in a single further pass, all from the same numbers.
#[derive(Debug, Clone, Copy)]
struct SectionLayout {
    /// File offset of the section's raw data (`FileAlignment`-aligned).
    file_offset: u32,
    /// On-disk size of the section's raw data (`FileAlignment`-aligned; per §II.25.3 the raw
    /// size is padded even though `VirtualSize` reports the exact content length).
    raw_size: u32,
    /// Mapped RVA of the section start (`SectionAlignment`-aligned).
    rva: u32,
    /// Exact content length before padding (COFF `VirtualSize`).
    virtual_size: u32,
}

impl SectionLayout {
    /// Lays out one section of `content_len` bytes starting at file offset `file_cursor` / RVA
    /// `rva_cursor` (both already `FileAlignment`/`SectionAlignment`-aligned by the caller),
    /// returning the layout plus the (unaligned) end-of-content cursors for the next section to
    /// align from.
    fn plan(file_cursor: u32, rva_cursor: u32, content_len: u32) -> Self {
        debug_assert_eq!(file_cursor % FILE_ALIGNMENT, 0, "file cursor must start FileAlignment-aligned");
        debug_assert_eq!(rva_cursor % SECTION_ALIGNMENT, 0, "RVA cursor must start SectionAlignment-aligned");
        SectionLayout {
            file_offset: file_cursor,
            raw_size: align_up(content_len, FILE_ALIGNMENT),
            rva: rva_cursor,
            virtual_size: content_len,
        }
    }

    /// The file offset immediately after this section's (aligned) raw data — the natural start
    /// for the next section on disk (still needs `align_up(.., FILE_ALIGNMENT)`, which is a
    /// no-op here since `raw_size` is already aligned).
    fn next_file_offset(&self) -> u32 {
        self.file_offset + self.raw_size
    }

    /// The RVA immediately after this section once mapped — the next section's RVA must be
    /// `align_up` of this to `SectionAlignment` (never a no-op in practice, since
    /// `SectionAlignment` >> `FileAlignment`).
    fn next_rva_floor(&self) -> u32 {
        self.rva + self.virtual_size
    }
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
///
/// # Layout
///
/// `.text` = CLI header (`CLI_HEADER_CB` bytes) + `metadata` + `method_bodies`, back to back,
/// starting at RVA `SECTION_ALIGNMENT`. This ordering means the CLI header's own RVA — and thus
/// the metadata root's RVA (`CLI header start + CLI_HEADER_CB`) — is fixed *before*
/// `method_bodies` is even known, which is exactly the property [`layout_text_section`] exposes
/// so callers can compute a metadata RVA before body RVAs, if some future caller needs to (today
/// every caller follows the four-phase pipeline in the module doc and already has final
/// `metadata`/`method_bodies` bytes by the time `write_pe` runs). `.sdata` (if `field_rva_data`
/// is non-empty) follows `.text`, `SectionAlignment`-aligned.
///
/// No import table, IAT, or `.reloc` native-bootstrap stub is emitted (deviation from ilasm/Mono
/// output, which still carries the legacy `mscoree.dll`-load stub for `.NET Framework`-era
/// loaders). Modern CoreCLR (`dotnet foo.dll`, or an `apphost`-produced native launcher) reads
/// only the CLI header and never walks the import table, so this should be a pure size reduction.
/// If a `dotnet run`/E2E test against a real CoreCLR host rejects an import-table-free image, the
/// fallback plan (`docs/PE_EMISSION_PLAN.md` Risk #1) is adding the standard 3-piece stub back:
/// `.reloc` fixup for the bootstrap's absolute jump, an Import Table row for
/// `mscoree.dll!_CorExeMain`/`_CorDllMain`, and a matching IAT entry.
#[must_use]
pub fn write_pe(
    metadata: &[u8],
    method_bodies: &[u8],
    field_rva_data: &[u8],
    options: &PeOptions,
) -> Vec<u8> {
    let has_sdata = !field_rva_data.is_empty();

    // --- Layout pass -----------------------------------------------------------------------
    // Section count fixes the header table sizes, which fixes where the first section's raw
    // data may start on disk.
    let num_sections: u16 = if has_sdata { 2 } else { 1 };

    let optional_header_size = optional_header_len();
    let headers_len = DOS_STUB_LEN
        + 4 // "PE\0\0"
        + COFF_HEADER_LEN
        + u32::from(optional_header_size)
        + u32::from(num_sections) * SECTION_HEADER_LEN;
    let headers_raw_size = align_up(headers_len, FILE_ALIGNMENT);

    let text_content_len = CLI_HEADER_CB
        + u32::try_from(metadata.len()).expect("metadata exceeds u32")
        + u32::try_from(method_bodies.len()).expect("method bodies exceed u32");
    let text = SectionLayout::plan(headers_raw_size, SECTION_ALIGNMENT, text_content_len);

    let sdata = has_sdata.then(|| {
        let rva = align_up(text.next_rva_floor(), SECTION_ALIGNMENT);
        let file_offset = text.next_file_offset();
        debug_assert_eq!(file_offset % FILE_ALIGNMENT, 0);
        SectionLayout::plan(
            file_offset,
            rva,
            u32::try_from(field_rva_data.len()).expect("field RVA data exceeds u32"),
        )
    });

    let cli_header_rva = text.rva;
    let metadata_rva = cli_header_rva + CLI_HEADER_CB;
    let metadata_len = u32::try_from(metadata.len()).expect("metadata exceeds u32");

    let last_section = sdata.as_ref().unwrap_or(&text);
    let size_of_image = align_up(last_section.next_rva_floor(), SECTION_ALIGNMENT);
    let size_of_headers = headers_raw_size;

    // --- Emit --------------------------------------------------------------------------------
    let mut out = Vec::with_capacity(
        headers_raw_size as usize + text.raw_size as usize + sdata.map_or(0, |s| s.raw_size as usize),
    );

    write_dos_header_and_stub(&mut out);
    debug_assert_eq!(out.len() as u32, DOS_STUB_LEN);

    out.extend_from_slice(b"PE\0\0");

    write_coff_header(&mut out, num_sections, optional_header_size, options.is_dll);

    write_optional_header(
        &mut out,
        size_of_image,
        size_of_headers,
        text.virtual_size,
        text.rva,
        cli_header_rva,
    );

    write_section_header(&mut out, b".text", &text, TEXT_SECTION_CHARACTERISTICS);
    if let Some(sdata) = &sdata {
        write_section_header(&mut out, b".sdata", sdata, SDATA_SECTION_CHARACTERISTICS);
    }

    // Pad up to the end of the (FileAlignment-aligned) header region.
    out.resize(headers_raw_size as usize, 0);
    debug_assert_eq!(out.len() as u32 % FILE_ALIGNMENT, 0);

    // .text: CLI header, then metadata, then method bodies.
    debug_assert_eq!(out.len() as u32, text.file_offset);
    write_cli_header(
        &mut out,
        metadata_rva,
        metadata_len,
        options.entry_point.unwrap_or(0),
    );
    out.extend_from_slice(metadata);
    out.extend_from_slice(method_bodies);
    out.resize(text.next_file_offset() as usize, 0);
    debug_assert_eq!(out.len() as u32 % FILE_ALIGNMENT, 0);

    // .sdata: FieldRVA blobs, verbatim.
    if let Some(sdata) = &sdata {
        debug_assert_eq!(out.len() as u32, sdata.file_offset);
        out.extend_from_slice(field_rva_data);
        out.resize(sdata.next_file_offset() as usize, 0);
        debug_assert_eq!(out.len() as u32 % FILE_ALIGNMENT, 0);
    }

    out
}

/// COFF header size (§II.25.2.2): 20 bytes, fixed.
const COFF_HEADER_LEN: u32 = 20;
/// Section header row size (§II.25.3): 40 bytes, fixed.
const SECTION_HEADER_LEN: u32 = 40;

/// PE32 optional header total length: the fixed "standard fields" + "NT-specific fields"
/// (§II.25.2.3.1/.2, 96 bytes for PE32) plus 16 data-directory entries × 8 bytes each
/// (§II.25.2.3.3).
#[must_use]
fn optional_header_len() -> u16 {
    // 96 fixed bytes (standard + NT-specific fields, PE32 form) + N * 8-byte directory entries.
    96 + (NUMBER_OF_RVA_AND_SIZES as u16) * 8
}

/// The canonical 128-byte MS-DOS header + stub (§II.25.2.1), byte-identical to the one Mono
/// ilasm emits (diffed against a real `ilasm`-produced `.exe` while implementing this writer).
/// Only two fields are load-bearing to any PE32/PE32+ loader: the `"MZ"` signature at offset 0
/// and `e_lfanew` at offset `0x3C` (here `0x80`, i.e. [`DOS_STUB_LEN`], pointing immediately past
/// this stub at the `PE\0\0` signature). Everything else — the rest of the legacy
/// `IMAGE_DOS_HEADER` fields and the tiny 16-bit DOS program that prints "This program cannot be
/// run in DOS mode." and exits — is dead weight kept only because every PE image conventionally
/// carries it; reproduced verbatim rather than hand (re)constructed field-by-field.
#[rustfmt::skip]
const DOS_HEADER_AND_STUB: [u8; DOS_STUB_LEN as usize] = [
    0x4d, 0x5a, 0x90, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00,
    0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00,
    0x0e, 0x1f, 0xba, 0x0e, 0x00, 0xb4, 0x09, 0xcd, 0x21, 0xb8, 0x01, 0x4c, 0xcd, 0x21, 0x54, 0x68,
    0x69, 0x73, 0x20, 0x70, 0x72, 0x6f, 0x67, 0x72, 0x61, 0x6d, 0x20, 0x63, 0x61, 0x6e, 0x6e, 0x6f,
    0x74, 0x20, 0x62, 0x65, 0x20, 0x72, 0x75, 0x6e, 0x20, 0x69, 0x6e, 0x20, 0x44, 0x4f, 0x53, 0x20,
    0x6d, 0x6f, 0x64, 0x65, 0x2e, 0x0d, 0x0d, 0x0a, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

fn write_dos_header_and_stub(out: &mut Vec<u8>) {
    out.extend_from_slice(&DOS_HEADER_AND_STUB);
}

/// Writes the 20-byte COFF header (§II.25.2.2), *excluding* the `PE\0\0` signature (written by
/// the caller immediately before this).
fn write_coff_header(out: &mut Vec<u8>, num_sections: u16, optional_header_size: u16, is_dll: bool) {
    let mut characteristics = IMAGE_FILE_EXECUTABLE_IMAGE | IMAGE_FILE_LARGE_ADDRESS_AWARE;
    if is_dll {
        characteristics |= IMAGE_FILE_DLL;
    }
    out.extend_from_slice(&IMAGE_FILE_MACHINE_I386.to_le_bytes());
    out.extend_from_slice(&num_sections.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp = 0: determinism, no wall clock.
    out.extend_from_slice(&0u32.to_le_bytes()); // PointerToSymbolTable: unused, always 0.
    out.extend_from_slice(&0u32.to_le_bytes()); // NumberOfSymbols: unused, always 0.
    out.extend_from_slice(&optional_header_size.to_le_bytes());
    out.extend_from_slice(&characteristics.to_le_bytes());
}

/// Writes the PE32 optional header (§II.25.2.3.1/.2/.3): standard fields, NT-specific fields,
/// then the 16-entry data directory (all zero except `DataDirectory[14]`, the CLI header).
fn write_optional_header(
    out: &mut Vec<u8>,
    size_of_image: u32,
    size_of_headers: u32,
    size_of_text: u32,
    text_rva: u32,
    cli_header_rva: u32,
) {
    // --- Standard fields (§II.25.2.3.1) ---
    out.extend_from_slice(&PE32_MAGIC.to_le_bytes());
    out.push(8); // LMajor: matches ilasm output.
    out.push(0); // LMinor.
    out.extend_from_slice(&align_up(size_of_text, FILE_ALIGNMENT).to_le_bytes()); // SizeOfCode.
    out.extend_from_slice(&0u32.to_le_bytes()); // SizeOfInitializedData (folded into .text/.sdata's own accounting; ECMA-335 images conventionally leave this 0, matching ilasm).
    out.extend_from_slice(&0u32.to_le_bytes()); // SizeOfUninitializedData.
    out.extend_from_slice(&text_rva.to_le_bytes()); // AddressOfEntryPoint: no native entry stub, so this points at .text start (unused by CoreCLR, which reads the CLI header's EntryPointToken instead).
    out.extend_from_slice(&text_rva.to_le_bytes()); // BaseOfCode.
    out.extend_from_slice(&0u32.to_le_bytes()); // BaseOfData (PE32-only field).

    // --- NT-specific fields (§II.25.2.3.2) ---
    out.extend_from_slice(&IMAGE_BASE.to_le_bytes());
    out.extend_from_slice(&SECTION_ALIGNMENT.to_le_bytes());
    out.extend_from_slice(&FILE_ALIGNMENT.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes()); // OS Major.
    out.extend_from_slice(&0u16.to_le_bytes()); // OS Minor.
    out.extend_from_slice(&0u16.to_le_bytes()); // User Major.
    out.extend_from_slice(&0u16.to_le_bytes()); // User Minor.
    out.extend_from_slice(&4u16.to_le_bytes()); // SubSys Major.
    out.extend_from_slice(&0u16.to_le_bytes()); // SubSys Minor.
    out.extend_from_slice(&0u32.to_le_bytes()); // Reserved.
    out.extend_from_slice(&size_of_image.to_le_bytes());
    out.extend_from_slice(&size_of_headers.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // CheckSum: 0 is valid (unchecked by loaders unless the image is a driver/signed).
    out.extend_from_slice(&SUBSYSTEM_CUI.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // DllCharacteristics: none set (matches ilasm's unsigned/non-ASLR-flagged output).
    out.extend_from_slice(&0x0010_0000u32.to_le_bytes()); // SizeOfStackReserve: conventional default.
    out.extend_from_slice(&0x0000_1000u32.to_le_bytes()); // SizeOfStackCommit.
    out.extend_from_slice(&0x0010_0000u32.to_le_bytes()); // SizeOfHeapReserve.
    out.extend_from_slice(&0x0000_1000u32.to_le_bytes()); // SizeOfHeapCommit.
    out.extend_from_slice(&0u32.to_le_bytes()); // LoaderFlags: reserved, always 0.
    out.extend_from_slice(&NUMBER_OF_RVA_AND_SIZES.to_le_bytes());

    // --- Data directories (§II.25.2.3.3) ---
    for i in 0..NUMBER_OF_RVA_AND_SIZES as usize {
        if i == DATA_DIRECTORY_CLI_HEADER {
            out.extend_from_slice(&cli_header_rva.to_le_bytes());
            out.extend_from_slice(&CLI_HEADER_CB.to_le_bytes());
        } else {
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }
    }
}

/// Writes one 40-byte section header row (§II.25.3).
fn write_section_header(out: &mut Vec<u8>, name: &[u8], layout: &SectionLayout, characteristics: u32) {
    debug_assert!(name.len() <= 8, "section name {name:?} exceeds the 8-byte COFF field");
    let mut name_field = [0u8; 8];
    name_field[..name.len()].copy_from_slice(name);
    out.extend_from_slice(&name_field);
    out.extend_from_slice(&layout.virtual_size.to_le_bytes()); // VirtualSize.
    out.extend_from_slice(&layout.rva.to_le_bytes()); // VirtualAddress.
    out.extend_from_slice(&layout.raw_size.to_le_bytes()); // SizeOfRawData.
    out.extend_from_slice(&layout.file_offset.to_le_bytes()); // PointerToRawData.
    out.extend_from_slice(&0u32.to_le_bytes()); // PointerToRelocations: unused (no COFF relocations in a managed image).
    out.extend_from_slice(&0u32.to_le_bytes()); // PointerToLinenumbers: unused, deprecated.
    out.extend_from_slice(&0u16.to_le_bytes()); // NumberOfRelocations.
    out.extend_from_slice(&0u16.to_le_bytes()); // NumberOfLinenumbers.
    out.extend_from_slice(&characteristics.to_le_bytes());
}

/// Writes the 72-byte CLI header (§II.25.3.3). `metadata_rva`/`metadata_len` describe the
/// `MetaData` directory entry; `entry_point_token` is the raw `MethodDef` token value (0 for a
/// library with no managed entry point).
fn write_cli_header(out: &mut Vec<u8>, metadata_rva: u32, metadata_len: u32, entry_point_token: u32) {
    out.extend_from_slice(&CLI_HEADER_CB.to_le_bytes()); // cb.
    out.extend_from_slice(&CLI_MAJOR_RUNTIME_VERSION.to_le_bytes());
    out.extend_from_slice(&CLI_MINOR_RUNTIME_VERSION.to_le_bytes());
    out.extend_from_slice(&metadata_rva.to_le_bytes()); // MetaData.VirtualAddress.
    out.extend_from_slice(&metadata_len.to_le_bytes()); // MetaData.Size.
    out.extend_from_slice(&COMIMAGE_FLAGS_ILONLY.to_le_bytes());
    out.extend_from_slice(&entry_point_token.to_le_bytes());
    // Remaining directory entries this backend never populates (Resources, StrongNameSignature,
    // CodeManagerTable, VTableFixups, ExportAddressTableJumps, ManagedNativeHeader) — all
    // RVA/Size pairs zeroed. 6 pairs * 8 bytes = 48, bringing the total to
    // 4 (cb) + 2 + 2 (versions) + 8 (MetaData dir) + 4 (Flags) + 4 (EntryPointToken) + 48 = 72 =
    // CLI_HEADER_CB, matching §II.25.3.3.
    for _ in 0..6 {
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }
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

    // -----------------------------------------------------------------------------------------
    // A minimal test-only PE parser: reads back exactly the fields `write_pe` promises, so tests
    // assert against the file structure rather than against `write_pe`'s own internal offsets
    // (which would make the tests tautological). Not a general-purpose PE reader — panics/
    // indexes eagerly, since a malformed image is a test failure, not an input to handle
    // gracefully.
    // -----------------------------------------------------------------------------------------

    struct ParsedSection {
        name: [u8; 8],
        virtual_size: u32,
        rva: u32,
        raw_size: u32,
        file_offset: u32,
        characteristics: u32,
    }

    struct ParsedPe {
        e_lfanew: u32,
        machine: u16,
        num_sections: u16,
        time_date_stamp: u32,
        characteristics: u16,
        magic: u16,
        image_base: u32,
        section_alignment: u32,
        file_alignment: u32,
        subsystem: u16,
        number_of_rva_and_sizes: u32,
        cli_header_dir: (u32, u32),
        sections: Vec<ParsedSection>,
        // CLI header, already resolved from RVA to file offset via `sections`.
        cli_cb: u32,
        cli_major_rv: u16,
        cli_minor_rv: u16,
        cli_metadata_rva: u32,
        cli_metadata_size: u32,
        cli_flags: u32,
        cli_entry_point_token: u32,
    }

    fn read_u16(data: &[u8], off: usize) -> u16 {
        u16::from_le_bytes(data[off..off + 2].try_into().unwrap())
    }
    fn read_u32(data: &[u8], off: usize) -> u32 {
        u32::from_le_bytes(data[off..off + 4].try_into().unwrap())
    }

    /// Resolves an RVA to a file offset by finding the section that contains it (§II.25.3's
    /// "RVA falls within `[VirtualAddress, VirtualAddress + VirtualSize)`" rule) and adding the
    /// RVA's offset within that section to the section's file offset.
    fn rva_to_file_offset(sections: &[ParsedSection], rva: u32) -> u32 {
        for s in sections {
            if rva >= s.rva && rva < s.rva + s.virtual_size.max(s.raw_size) {
                return s.file_offset + (rva - s.rva);
            }
        }
        panic!("RVA {rva:#x} not covered by any section");
    }

    fn parse_pe(data: &[u8]) -> ParsedPe {
        let e_lfanew = read_u32(data, 0x3C);
        assert_eq!(&data[0..2], b"MZ", "DOS signature");
        assert_eq!(
            &data[e_lfanew as usize..e_lfanew as usize + 4],
            b"PE\0\0",
            "PE signature"
        );
        let coff = e_lfanew as usize + 4;
        let machine = read_u16(data, coff);
        let num_sections = read_u16(data, coff + 2);
        let time_date_stamp = read_u32(data, coff + 4);
        let opt_header_size = read_u16(data, coff + 16);
        let characteristics = read_u16(data, coff + 18);

        let opt = coff + 20;
        let magic = read_u16(data, opt);
        let image_base = read_u32(data, opt + 28);
        let section_alignment = read_u32(data, opt + 32);
        let file_alignment = read_u32(data, opt + 36);
        let subsystem = read_u16(data, opt + 68);
        let number_of_rva_and_sizes = read_u32(data, opt + 92);
        let dir_base = opt + 96;
        let cli_header_dir = (
            read_u32(data, dir_base + DATA_DIRECTORY_CLI_HEADER * 8),
            read_u32(data, dir_base + DATA_DIRECTORY_CLI_HEADER * 8 + 4),
        );

        let sec_table = opt + opt_header_size as usize;
        let mut sections = Vec::new();
        for i in 0..num_sections as usize {
            let s = sec_table + i * 40;
            let mut name = [0u8; 8];
            name.copy_from_slice(&data[s..s + 8]);
            sections.push(ParsedSection {
                name,
                virtual_size: read_u32(data, s + 8),
                rva: read_u32(data, s + 12),
                raw_size: read_u32(data, s + 16),
                file_offset: read_u32(data, s + 20),
                characteristics: read_u32(data, s + 36),
            });
        }

        let cli_file_off = rva_to_file_offset(&sections, cli_header_dir.0) as usize;
        let cli_cb = read_u32(data, cli_file_off);
        let cli_major_rv = read_u16(data, cli_file_off + 4);
        let cli_minor_rv = read_u16(data, cli_file_off + 6);
        let cli_metadata_rva = read_u32(data, cli_file_off + 8);
        let cli_metadata_size = read_u32(data, cli_file_off + 12);
        let cli_flags = read_u32(data, cli_file_off + 16);
        let cli_entry_point_token = read_u32(data, cli_file_off + 20);

        ParsedPe {
            e_lfanew,
            machine,
            num_sections,
            time_date_stamp,
            characteristics,
            magic,
            image_base,
            section_alignment,
            file_alignment,
            subsystem,
            number_of_rva_and_sizes,
            cli_header_dir,
            sections,
            cli_cb,
            cli_major_rv,
            cli_minor_rv,
            cli_metadata_rva,
            cli_metadata_size,
            cli_flags,
            cli_entry_point_token,
        }
    }

    fn section_named<'a>(pe: &'a ParsedPe, name: &str) -> Option<&'a ParsedSection> {
        pe.sections
            .iter()
            .find(|s| s.name.iter().take_while(|&&b| b != 0).eq(name.bytes().collect::<Vec<_>>().iter()))
    }

    // --- (a) synthetic write round-trips its own header fields ------------------------------

    #[test]
    fn roundtrip_exe_header_fields() {
        let metadata = vec![0xABu8; 37]; // deliberately not 4-aligned in length.
        let bodies = vec![0xCDu8; 19];
        let opts = PeOptions {
            is_dll: false,
            entry_point: Some(0x0600_0001),
        };
        let image = write_pe(&metadata, &bodies, &[], &opts);
        let pe = parse_pe(&image);

        assert_eq!(pe.e_lfanew, DOS_STUB_LEN);
        assert_eq!(pe.machine, IMAGE_FILE_MACHINE_I386);
        assert_eq!(pe.num_sections, 1, "no field_rva_data => no .sdata section");
        assert_eq!(pe.time_date_stamp, 0, "determinism: zero COFF timestamp");
        assert_eq!(
            pe.characteristics,
            IMAGE_FILE_EXECUTABLE_IMAGE | IMAGE_FILE_LARGE_ADDRESS_AWARE
        );
        assert_eq!(pe.magic, PE32_MAGIC);
        assert_eq!(pe.image_base, IMAGE_BASE);
        assert_eq!(pe.section_alignment, SECTION_ALIGNMENT);
        assert_eq!(pe.file_alignment, FILE_ALIGNMENT);
        assert_eq!(pe.subsystem, SUBSYSTEM_CUI);
        assert_eq!(pe.number_of_rva_and_sizes, NUMBER_OF_RVA_AND_SIZES);

        // The CLI header directory resolves through the section table to a real file offset
        // that actually holds a CLI header (cb == 0x48, matching the entry_point token).
        assert_eq!(pe.cli_header_dir.1, CLI_HEADER_CB);
        assert_eq!(pe.cli_cb, CLI_HEADER_CB);
        assert_eq!(pe.cli_major_rv, CLI_MAJOR_RUNTIME_VERSION);
        assert_eq!(pe.cli_minor_rv, CLI_MINOR_RUNTIME_VERSION);
        assert_eq!(pe.cli_flags, COMIMAGE_FLAGS_ILONLY);
        assert_eq!(pe.cli_entry_point_token, 0x0600_0001);

        // The metadata directory the CLI header points at resolves to a file offset whose bytes
        // are exactly the `metadata` buffer passed in.
        let md_file_off = rva_to_file_offset(&pe.sections, pe.cli_metadata_rva) as usize;
        assert_eq!(pe.cli_metadata_size, metadata.len() as u32);
        assert_eq!(&image[md_file_off..md_file_off + metadata.len()], &metadata[..]);

        // Method bodies immediately follow the metadata in `.text`.
        let bodies_file_off = md_file_off + metadata.len();
        assert_eq!(&image[bodies_file_off..bodies_file_off + bodies.len()], &bodies[..]);
    }

    #[test]
    fn roundtrip_dll_has_no_entry_point_and_dll_characteristic() {
        let opts = PeOptions {
            is_dll: true,
            entry_point: None,
        };
        let image = write_pe(&[1, 2, 3], &[4, 5, 6], &[], &opts);
        let pe = parse_pe(&image);
        assert_eq!(pe.cli_entry_point_token, 0, "library => EntryPointToken 0");
        assert_eq!(
            pe.characteristics & IMAGE_FILE_DLL,
            IMAGE_FILE_DLL,
            "is_dll must set IMAGE_FILE_DLL"
        );
    }

    // --- (b) alignment invariants across several odd-sized inputs ----------------------------

    #[test]
    fn alignment_invariants_hold_for_odd_sized_inputs() {
        let opts = PeOptions {
            is_dll: false,
            entry_point: Some(0x0600_0001),
        };
        for metadata_len in [0usize, 1, 3, 200, 4095, 4096, 8193] {
            for body_len in [0usize, 1, 5, 511, 512, 513, 10_000] {
                let metadata = vec![0x11u8; metadata_len];
                let bodies = vec![0x22u8; body_len];
                let image = write_pe(&metadata, &bodies, &[], &opts);
                let pe = parse_pe(&image);

                for s in &pe.sections {
                    assert_eq!(
                        s.file_offset % FILE_ALIGNMENT,
                        0,
                        "section file offset must honor FileAlignment (metadata_len={metadata_len}, body_len={body_len})"
                    );
                    assert_eq!(
                        s.raw_size % FILE_ALIGNMENT,
                        0,
                        "section raw size must honor FileAlignment"
                    );
                    assert_eq!(
                        s.rva % SECTION_ALIGNMENT,
                        0,
                        "section RVA must honor SectionAlignment"
                    );
                    assert!(
                        s.raw_size >= s.virtual_size,
                        "raw size must cover the actual content"
                    );
                }
                // Total file length itself must be FileAlignment-aligned (no trailing partial
                // write past the last section's padded end).
                assert_eq!(image.len() as u32 % FILE_ALIGNMENT, 0);
            }
        }
    }

    #[test]
    fn align_up_examples() {
        assert_eq!(align_up(0, 0x200), 0);
        assert_eq!(align_up(1, 0x200), 0x200);
        assert_eq!(align_up(0x200, 0x200), 0x200);
        assert_eq!(align_up(0x201, 0x200), 0x400);
        assert_eq!(align_up(0x2000, 0x2000), 0x2000);
        assert_eq!(align_up(0x2001, 0x2000), 0x4000);
    }

    // --- (c) .sdata appears only when FieldRVA data is present, with the RVA the layout promised

    #[test]
    fn sdata_absent_when_no_field_rva_data() {
        let opts = PeOptions {
            is_dll: true,
            entry_point: None,
        };
        let image = write_pe(&[1, 2, 3], &[4, 5, 6], &[], &opts);
        let pe = parse_pe(&image);
        assert_eq!(pe.num_sections, 1);
        assert!(section_named(&pe, ".sdata").is_none());
    }

    #[test]
    fn sdata_present_with_correct_characteristics_and_content_when_field_rva_data_given() {
        let opts = PeOptions {
            is_dll: true,
            entry_point: None,
        };
        let metadata = vec![0xAAu8; 100];
        let bodies = vec![0xBBu8; 50];
        let field_rva_data = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
        let image = write_pe(&metadata, &bodies, &field_rva_data, &opts);
        let pe = parse_pe(&image);

        assert_eq!(pe.num_sections, 2);
        let text = section_named(&pe, ".text").expect(".text must exist");
        let sdata = section_named(&pe, ".sdata").expect(".sdata must exist when field_rva_data is non-empty");

        assert_eq!(text.characteristics, TEXT_SECTION_CHARACTERISTICS);
        assert_eq!(sdata.characteristics, SDATA_SECTION_CHARACTERISTICS);

        // .sdata starts strictly after .text, section-aligned, matching what the layout pass in
        // `write_pe` promises (`align_up(text.next_rva_floor(), SECTION_ALIGNMENT)`).
        let expected_sdata_rva = align_up(text.rva + text.virtual_size, SECTION_ALIGNMENT);
        assert_eq!(sdata.rva, expected_sdata_rva);
        assert_eq!(sdata.rva % SECTION_ALIGNMENT, 0);
        assert!(sdata.rva >= text.rva + text.virtual_size);

        // The bytes actually placed at .sdata's file offset are exactly `field_rva_data`.
        let off = sdata.file_offset as usize;
        assert_eq!(&image[off..off + field_rva_data.len()], &field_rva_data[..]);
        assert_eq!(sdata.virtual_size, field_rva_data.len() as u32);
    }

    #[test]
    fn total_image_size_matches_layout_for_synthetic_inputs() {
        let opts = PeOptions {
            is_dll: false,
            entry_point: Some(0x0600_0001),
        };
        let metadata = vec![0u8; 1000];
        let bodies = vec![0u8; 2000];
        let field_rva_data = vec![0u8; 300];
        let image = write_pe(&metadata, &bodies, &field_rva_data, &opts);
        let pe = parse_pe(&image);
        let last = pe.sections.last().unwrap();
        // The file must contain at least through the last section's raw data (padding after is
        // permitted by the format but this writer doesn't add any).
        assert_eq!(image.len() as u32, last.file_offset + last.raw_size);
    }
}
