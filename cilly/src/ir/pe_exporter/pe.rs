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
    /// The Portable PDB's [`super::pdb::DebugDirectoryEntry`], if a PDB was built for this image
    /// (see `export::export_pe`'s wiring) — emits a `DataDirectory[6]` (Debug) entry pointing at a
    /// small table of `IMAGE_DEBUG_DIRECTORY` rows in `.text`: `IMAGE_DEBUG_TYPE_CODEVIEW`
    /// (RSDS payload) always, plus `IMAGE_DEBUG_TYPE_PDBCHECKSUM` when
    /// [`pdb_checksum`](Self::pdb_checksum) is also `Some` (see that field's doc for why BOTH are
    /// required in practice, not just documented as spec-optional). `None` omits the directory
    /// entirely (all-zero, exactly like every other unused directory slot) — a valid image with no
    /// debug info, matching this writer's pre-Phase-2 output.
    pub debug_directory: Option<super::pdb::DebugDirectoryEntry>,
    /// The `IMAGE_DEBUG_TYPE_PDBCHECKSUM` entry's payload — a SHA-256 digest of the COMPLETE
    /// on-disk PDB file bytes (see [`super::pdb::sha256`]'s doc for the full root-cause writeup:
    /// CoreCLR's runtime `StackTraceSymbols` provider silently refuses to resolve file:line info
    /// for a portable PDB that lacks this entry, even though the PDB is otherwise fully
    /// spec-conformant and readable by `System.Reflection.Metadata`'s own static reader). Ignored
    /// if [`debug_directory`](Self::debug_directory) is `None` (no PDB, nothing to checksum).
    pub pdb_checksum: Option<super::pdb::PdbChecksumEntry>,
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
/// mapped into memory. §II.25.2.3.1 only requires this to be a power of two `>= FileAlignment`;
/// ilasm conventionally uses `0x2000` (8KiB), which this writer matched until the following bug
/// was found.
///
/// **Bumped from `0x2000` to `0x4000` (16KiB) to fix a real, confirmed `FileLoadException
/// 0x8007000C` load bug on macOS ARM64 CoreCLR 8.0.28**, root-caused via `lldb` against the
/// release `libcoreclr.dylib` (no managed reader — `System.Reflection.Metadata`, ilverify, Mono —
/// or unit test can see this; it is a native-loader-only invariant): CoreCLR's non-Windows PE
/// loader (`FlatImageLayout::LoadImageByCopyingParts`, the manual "PAL" path every non-Windows
/// host takes because `mmap`ing a Windows-style PE section table directly isn't portable) copies
/// each section into one big anonymous mapping and then calls `mprotect`/`ClrVirtualProtect`
/// **per section boundary** to apply that section's own protection bits (`.text` -> RX/RWX,
/// `.sdata` -> RW). `mprotect(2)` requires its `addr` argument to be aligned to the HOST OS's
/// native page size — 4KiB on Linux/Windows/x86_64 macOS, but **16KiB on Apple Silicon
/// (macOS ARM64)**. An `0x2000`-aligned section boundary is only ever `0x0` or `0x2000` (mod
/// `0x4000`) — exactly a coin flip whether it lands on a real 16KiB page boundary — so whenever a
/// `.text` section's content length happened NOT to round to a 16KiB multiple (i.e. most of the
/// time; every dll in this writer's own E2E tests happened to round the other way, which is why
/// this shipped undetected), the `.sdata` section's `mprotect` call landed mid-page and macOS's
/// kernel rejected it, which CoreCLR's loader converts into exactly the `FileLoadException`
/// naming this assembly's own `"_"` identity that four prior A/B differential rounds chased
/// through the metadata-table layer and never found (`ilverify`/SRM/Mono only ever see the file
/// AFTER a successful `mmap`+`mprotect`, so this class of bug is invisible to every managed-reader
/// check this project has). Confirmed via `lldb`: the failing `ClrVirtualProtect(addr, 0x23f10,
/// PAGE_READWRITE)` call for `.sdata` returned `0` (failure) exactly when `.sdata`'s RVA was
/// `0x2000`-but-not-`0x4000`-aligned; the SAME bytes with a `0x4000`-aligned `.sdata` RVA load and
/// run cleanly. `0x4000` is still spec-legal (a power of two, still `>= FILE_ALIGNMENT`) and is
/// exactly what real multi-platform toolchains (e.g. modern Mono/Roslyn-adjacent AOT linkers) use
/// for the same reason; it costs a little more padding in `.text`/`.sdata`, not correctness.
pub(super) const SECTION_ALIGNMENT: u32 = 0x4000;
/// `FileAlignment` (§II.25.2.3.1): each section's raw (on-disk) data starts at a multiple of
/// this.
const FILE_ALIGNMENT: u32 = 0x0200;
/// `Subsystem` (§II.25.2.3.3): `IMAGE_SUBSYSTEM_WINDOWS_CUI` (console), matching ilasm's default.
const SUBSYSTEM_CUI: u16 = 3;
/// `NumberOfRvaAndSizes` (§II.25.2.3.3) — this writer always emits the full 16-entry data
/// directory, all-zero except the CLI header entry.
const NUMBER_OF_RVA_AND_SIZES: u32 = 16;
/// `DllCharacteristics` (§II.25.2.3.2) — ilasm's real value, confirmed byte-for-byte against a
/// live-built `.exe` (`IMAGE_DLLCHARACTERISTICS_DYNAMIC_BASE (0x0040)
/// | IMAGE_DLLCHARACTERISTICS_NX_COMPAT (0x0100) | IMAGE_DLLCHARACTERISTICS_NO_SEH (0x0400)
/// | IMAGE_DLLCHARACTERISTICS_TERMINAL_SERVER_AWARE (0x8000)` = `0x8540`), NOT the all-zero value
/// this writer previously hardcoded. This was a real, previously-undiagnosed bug (not cosmetic):
/// leaving `DllCharacteristics` at 0 made `AssemblyLoadContext.InternalLoad`'s native binder
/// reject the image with `FileLoadException … The access code is invalid. (0x8007000C)` — CoreCLR's
/// native PE loader treats an all-zero `DllCharacteristics` (no `NX_COMPAT`/`DYNAMIC_BASE` bits) as
/// a signal the image predates those flags' existence and does not defensively "just allow it"; a
/// managed reader (`System.Reflection.Metadata`) never even inspects this field, which is why every
/// prior round's structural check (SRM `PEReader`, Mono) found nothing wrong in a file that failed
/// to load. Reproduced as the root cause of the `pal_threads`/`cd_interop` `FileLoadException`
/// residual carried across three A/B differential rounds.
const DLL_CHARACTERISTICS: u16 = 0x8540;
/// Index of the "CLI Header" entry within the optional header's data-directory array
/// (§II.25.2.3.3, table titled "Optional Header Data Directories").
const DATA_DIRECTORY_CLI_HEADER: usize = 14;
/// Index of the "Debug" data directory (§II.25.2.3.3's referenced Windows PE data-directory
/// table — the Debug Directory isn't an ECMA-335 CLI concept, it's the plain PE/COFF mechanism
/// CoreCLR repurposes to locate a `.pdb`; see `pdb::DebugDirectoryEntry`'s doc). Points at one or
/// more `IMAGE_DEBUG_DIRECTORY` (§ "Debug Directory", 28 bytes each) entries placed in `.text`;
/// this writer emits exactly one, type `IMAGE_DEBUG_TYPE_CODEVIEW` (2), carrying an "RSDS" payload.
const DATA_DIRECTORY_DEBUG: usize = 6;

/// `.text` section characteristics: `IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE |
/// IMAGE_SCN_MEM_READ`. Matches the `0x6000_0020` observed in a real ilasm-produced image.
const TEXT_SECTION_CHARACTERISTICS: u32 = 0x6000_0020;
/// `.sdata` section characteristics: `IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ |
/// IMAGE_SCN_MEM_WRITE` — mutable storage, not `.rodata` (`il_exporter`'s FieldRVA statics are
/// non-`initonly` fields, so the backing store must be writable).
const SDATA_SECTION_CHARACTERISTICS: u32 = 0xC000_0040;

/// CLI header `cb` (§II.25.3.3): the header's own fixed size in bytes.
pub(super) const CLI_HEADER_CB: u32 = 0x48;
const CLI_MAJOR_RUNTIME_VERSION: u16 = 2;
const CLI_MINOR_RUNTIME_VERSION: u16 = 5;
/// CLI header `Flags` bit: `COMIMAGE_FLAGS_ILONLY`. This is the *only* flag this writer sets —
/// `COMIMAGE_FLAGS_32BITREQUIRED` (`0x2`, sometimes called `32BITPREFERRED` in combination with
/// bit 0x20000) is deliberately never set, matching ilasm's AnyCPU output.
const COMIMAGE_FLAGS_ILONLY: u32 = 0x1;

/// `.reloc` section characteristics: `IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ |
/// IMAGE_SCN_MEM_DISCARDABLE`. Matches the `0x4200_0040` observed in a real ilasm-produced image
/// (the loader discards `.reloc` after applying fixups — it isn't needed once the image is bound).
const RELOC_SECTION_CHARACTERISTICS: u32 = 0x4200_0040;

/// Index of the "Import Table" data directory (§II.25.2.3.3).
const DATA_DIRECTORY_IMPORT_TABLE: usize = 1;
/// Index of the "Base Relocation Table" data directory (§II.25.2.3.3).
const DATA_DIRECTORY_BASE_RELOCATION_TABLE: usize = 5;
/// Index of the "IAT" (Import Address Table) data directory (§II.25.2.3.3).
const DATA_DIRECTORY_IAT: usize = 12;

/// `IMAGE_REL_BASED_HIGHLOW` — a 32-bit base-relocation fixup type (the only kind this writer's
/// single stub-address fixup needs).
const IMAGE_REL_BASED_HIGHLOW: u16 = 3;

/// The native x86 bootstrap thunk every ilasm/Roslyn-produced managed `.exe` carries:
/// `jmp dword ptr [addr]` (`FF 25` + a 4-byte absolute VA operand, patched in by
/// [`write_bootstrap_stub`] once the IAT's RVA is known). This is what
/// `AddressOfEntryPoint` points at — the OS loader binds the IAT slot the operand addresses to
/// `mscoree.dll!_CorExeMain`'s real address before this instruction executes; `_CorExeMain` is
/// what actually reads the CLI header and starts the CLR. Without this, the *native* PE loader
/// (not the CLR) rejects the image before ever inspecting the CLI header — see the module doc's
/// "Risk #1 confirmed" note.
const ENTRY_STUB_OPCODE: [u8; 2] = [0xFF, 0x25];
/// Total stub length: the 2-byte opcode + 4-byte operand. ilasm pads the rest of a 16-byte-aligned
/// region with zeros; this writer does the same (harmless — never executed).
const ENTRY_STUB_LEN: u32 = 6;

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

/// The `mscoree.dll!_CorExeMain` bootstrap thunk's fixed-shape sub-layout (§II.25.4's referenced
/// `.idata`/import-table conventions — this writer needs exactly one imported function, so the
/// generic multi-import table shape collapses to fixed offsets computed once here). Byte shapes
/// were confirmed against a real CoreCLR-`ilasm`-produced `.exe` (see `write_pe`'s module doc).
///
/// Layout of the "Import Table + stub" region this describes (placed at the tail of `.text`,
/// after the method bodies — see `write_import_table_and_stub`):
/// ```text
/// offset 0   : Import Directory Table: 1 IMAGE_IMPORT_DESCRIPTOR (20B) + 1 null terminator (20B)
/// offset 40  : Import Lookup Table (ILT): Hint/Name RVA (4B) + null terminator (4B)
/// offset 48  : Hint/Name entry: Hint (2B, always 0) + "_CorExeMain\0" (12B, already even)
/// offset 62  : "mscoree.dll\0" (12B)
/// offset 74  : padding to a 16-byte-aligned stub start
/// offset ??  : native stub: FF 25 <abs VA> (6B), zero-padded to a 16-byte region
/// ```
#[derive(Debug, Clone, Copy)]
struct BootstrapLayout;

/// `IMAGE_IMPORT_DESCRIPTOR` size (§II.25.3.1's referenced Windows import-table format): 20 bytes.
const IMPORT_DESCRIPTOR_LEN: u32 = 20;
/// Import Directory Table: one real descriptor + one all-zero terminator descriptor.
const IMPORT_DIRECTORY_LEN: u32 = IMPORT_DESCRIPTOR_LEN * 2;
/// ILT: one Hint/Name RVA `DWORD` + one null-terminator `DWORD` (mirrors the IAT's own shape
/// before the loader binds it — both tables are byte-identical pre-bind, §II conventions).
const ILT_LEN: u32 = 8;
/// Hint/Name entry: 2-byte Hint (always 0, no ordinal-only import) + `"_CorExeMain\0"` (12 bytes,
/// already an even length so no extra padding byte is needed).
const HINT_NAME_LEN: u32 = 2 + 12;
const COR_EXE_MAIN: &[u8] = b"_CorExeMain\0";
/// `"mscoree.dll\0"` (12 bytes).
const MSCOREE_DLL: &[u8] = b"mscoree.dll\0";
/// `.reloc` section content: one relocation block, one page, one `HIGHLOW` entry, padded to a
/// 4-byte `DWORD` boundary (§II.25.3 base relocation block shape: `PageRVA(4) BlockSize(4)` +
/// `N` × 2-byte `(type<<12)|offset` entries, `BlockSize` rounded up to a multiple of 4).
const RELOC_CONTENT_LEN: u32 = 12; // 8-byte block header + 1 entry (2B) + 2B padding to align.

impl BootstrapLayout {
    fn plan() -> Self {
        BootstrapLayout
    }
    /// IAT length: identical shape/size to the ILT (one Hint/Name RVA + a null terminator).
    fn iat_len(self) -> u32 {
        ILT_LEN
    }
    /// Total length of the "Import Table + stub" region placed at the tail of `.text`.
    fn import_and_stub_len(self) -> u32 {
        self.stub_offset_in_import_region() + ENTRY_STUB_LEN
    }
    /// Byte offset, within the "Import Table + stub" region, of the Import Directory Table.
    fn import_directory_offset(self) -> u32 {
        0
    }
    /// Byte offset, within the region, of the Import Lookup Table.
    fn ilt_offset(self) -> u32 {
        self.import_directory_offset() + IMPORT_DIRECTORY_LEN
    }
    /// Byte offset, within the region, of the Hint/Name entry.
    fn hint_name_offset(self) -> u32 {
        self.ilt_offset() + ILT_LEN
    }
    /// Byte offset, within the region, of the `"mscoree.dll\0"` name string.
    fn dll_name_offset(self) -> u32 {
        self.hint_name_offset() + HINT_NAME_LEN
    }
    /// Byte offset, within the region, of the native entry stub — 16-byte aligned (matches the
    /// reference image; not load-bearing, just conventional tidiness for disassembly).
    fn stub_offset_in_import_region(self) -> u32 {
        align_up(self.dll_name_offset() + u32::try_from(MSCOREE_DLL.len()).unwrap(), 16)
    }
}

/// The number of bytes `write_pe` places in `.text` BEFORE the CLI header — `0` for a `.dll`
/// (`has_entry_point = false`), or [`BootstrapLayout::iat_len`] for an `.exe` (the IAT the native
/// bootstrap stub needs, see `write_pe`'s module doc). **Single source of truth**: callers that
/// need to pre-compute an RVA before calling `write_pe` (e.g. `export::export_pe`'s two-pass
/// metadata-length-probe layout) must use this instead of re-deriving the same constant, so the
/// two can never drift out of sync — an earlier version of `export_pe` hardcoded `0` here and
/// produced `MethodDef.RVA` values 8 bytes short of the bodies' real position once the bootstrap
/// stub's IAT was added, which a real `dotnet` load surfaced as
/// `BadImageFormatException: Index not found` (a corrupted-looking method body, since every
/// `call`/`ldstr` token inside it was being read from 8 bytes into the ACTUAL body instead of its
/// start).
#[must_use]
pub fn text_header_len(has_entry_point: bool) -> u32 {
    if has_entry_point {
        BootstrapLayout::plan().iat_len()
    } else {
        0
    }
}

/// The RVA `.sdata` (the section holding `FieldRVA` blobs) will start at, given the sizes of the
/// pieces that precede it in `.text`. **Single source of truth**, same rationale as
/// [`text_header_len`]: a caller (`export::export_pe`) must call [`MetadataBuilder::set_field_rva`]
/// (`super::tables::MetadataBuilder::set_field_rva`) with real RVAs before the FINAL
/// `MetadataBuilder::serialize()` call, i.e. before `write_pe` itself runs and could otherwise be
/// the only place this arithmetic lives — re-deriving `write_pe`'s internal `.text`-content-length
/// math (header + CLI header + metadata + method bodies + bootstrap import table/stub tail, all
/// `SectionAlignment`-rounded) independently would risk the same "8 bytes short" class of bug
/// `text_header_len`'s doc comment describes, just for `FieldRVA.RVA` instead of `MethodDef.RVA`.
///
/// Mirrors `write_pe`'s own `sdata` [`SectionLayout::plan`] call exactly: `.text`'s content is
/// `text_header_len(has_entry_point) + CLI_HEADER_CB + metadata_len + method_bodies_len +
/// debug_dir_len [+ bootstrap import-table-and-stub tail when `has_entry_point`]`, and `.sdata`
/// starts at that, rounded up to `SectionAlignment` from a `SectionAlignment`-aligned `.text` base.
///
/// `debug_dir_len` is `0` when no PDB is being emitted for this image, or
/// [`debug_directory_region_len`] of the [`super::pdb::DebugDirectoryEntry`] that will be passed
/// as `PeOptions::debug_directory` — see that function's doc for why callers can't just recompute
/// it inline (RSDS payload length depends on the PDB path string length).
#[must_use]
pub fn field_rva_section_start(has_entry_point: bool, metadata_len: usize, method_bodies_len: usize, debug_dir_len: u32) -> u32 {
    let iat_len = text_header_len(has_entry_point);
    let tail_len = if has_entry_point {
        BootstrapLayout::plan().import_and_stub_len()
    } else {
        0
    };
    let text_content_len = iat_len
        + CLI_HEADER_CB
        + u32::try_from(metadata_len).expect("metadata exceeds u32")
        + u32::try_from(method_bodies_len).expect("method bodies exceed u32")
        + debug_dir_len
        + tail_len;
    align_up(SECTION_ALIGNMENT + text_content_len, SECTION_ALIGNMENT)
}

/// Public wrapper for [`debug_directory_region_len`] — `export.rs`'s two-pass metadata-length
/// probe (mirroring [`field_rva_section_start`]'s own rationale) needs this BEFORE it can call
/// `field_rva_section_start`, since the debug directory sits ahead of `.sdata` in `.text`.
#[must_use]
pub fn debug_directory_len(entry: &super::pdb::DebugDirectoryEntry, checksum: Option<&super::pdb::PdbChecksumEntry>) -> u32 {
    debug_directory_region_len(entry, checksum)
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
/// **Risk #1 confirmed, and the fallback is now the only path this writer emits**: a
/// `.text`-only, import-table-free `.exe` (the shape this function produced before the Phase 1a
/// E2E milestone) loads its metadata fine but CoreCLR's *native* PE loader (not the CLI-aware
/// managed loader — the OS-level image loader `dotnet`'s apphost/corehost invokes first) rejects
/// it with `BadImageFormatException` before the CLR ever inspects the CLI header: a `.exe`'s
/// `AddressOfEntryPoint` must point at *real native code*, and the standard `mscoree.dll`
/// `_CorExeMain` bootstrap thunk is how every ilasm/Mono/Roslyn-produced managed `.exe` satisfies
/// that (byte-diffed against a real CoreCLR-`ilasm`-produced `.exe` while chasing this exact
/// failure — see the `bootstrap_stub_matches_the_ilasm_reference_shape` test below for the
/// annotated reference bytes). So when `options.entry_point.is_some()` (i.e. an `.exe`, not a
/// library `.dll` — a `.dll` is never natively executed, so it carries no bootstrap stub, matching
/// `il_exporter`'s `.dll`-vs-`.exe` split), `write_pe` now also emits:
/// * An **IAT** (Import Address Table, §II.25.4.2) — one `DWORD` (the Hint/Name RVA) + a null
///   terminator `DWORD`, placed at the very start of `.text` (RVA = `.text`'s own base), exactly
///   where a real CoreCLR `ilasm` puts it.
/// * An **Import Table** (§II.25.3.1's referenced `.idata` layout) — one `IMAGE_IMPORT_DESCRIPTOR`
///   (20 bytes) + a null terminator descriptor (20 bytes) naming `mscoree.dll`, plus an Import
///   Lookup Table (ILT, byte-identical to the IAT before the loader binds it), a Hint/Name entry
///   for `_CorExeMain`, and the `mscoree.dll` name string — placed after the method bodies, at the
///   tail of `.text`.
/// * A **native x86 entry stub** (6 bytes: `FF 25 <abs VA of the IAT slot>`, i.e.
///   `jmp dword ptr [IAT slot]`) — `AddressOfEntryPoint` points here; the OS loader binds the IAT
///   slot to `_CorExeMain`'s real address before this instruction ever runs, and `_CorExeMain`
///   is what actually reads the CLI header and hands off to the CLR.
/// * A **`.reloc` section** (§II.25.3, the standard base-relocation table) with one
///   `IMAGE_REL_BASED_HIGHLOW` fixup for the stub's hardcoded absolute address operand — required
///   because the stub bakes in an absolute VA (`ImageBase + IAT RVA`), which only stays correct if
///   the image loads at its preferred `ImageBase`; ASLR/ address-space contention can relocate it.
///
/// A `.dll` (`options.entry_point.is_none()`) skips all of the above — no IAT, Import Table,
/// native stub, or `.reloc` — since `dotnet <name>.dll`/`Assembly.LoadFrom` never executes a
/// native entry point; only the CLI header + metadata matter for a library. This matches
/// `il_exporter`'s `is_lib` split (a library gets no native launcher at all).
#[must_use]
pub fn write_pe(
    metadata: &[u8],
    method_bodies: &[u8],
    field_rva_data: &[u8],
    options: &PeOptions,
) -> Vec<u8> {
    let has_sdata = !field_rva_data.is_empty();
    // An `.exe` (has a managed entry point) needs the native bootstrap stub — see the module doc's
    // "Risk #1 confirmed" note. A `.dll` never carries one (nothing natively executes it).
    let needs_bootstrap = options.entry_point.is_some();
    let bootstrap = needs_bootstrap.then(BootstrapLayout::plan);

    // --- Layout pass -----------------------------------------------------------------------
    // Section count fixes the header table sizes, which fixes where the first section's raw
    // data may start on disk.
    let num_sections: u16 = 1 + u16::from(has_sdata) + u16::from(needs_bootstrap);

    let optional_header_size = optional_header_len();
    let headers_len = DOS_STUB_LEN
        + 4 // "PE\0\0"
        + COFF_HEADER_LEN
        + u32::from(optional_header_size)
        + u32::from(num_sections) * SECTION_HEADER_LEN;
    let headers_raw_size = align_up(headers_len, FILE_ALIGNMENT);

    // `.text` = [IAT (bootstrap only)] + CLI header + metadata + method bodies +
    // [Debug Directory + RSDS payload (only if `options.debug_directory.is_some()`)] +
    // [Import Table + stub (bootstrap only)]. The IAT sits FIRST so its RVA is simply `.text`'s
    // own base RVA (matching a real ilasm image, and letting the stub's hardcoded operand be
    // computed before anything else is laid out). The Debug Directory region sits between the
    // method bodies and the bootstrap tail — position within `.text` is not spec-mandated (the
    // data directory entry carries its own RVA), this placement just keeps every "extra tail
    // region" grouped together the same way the import-table/stub already is.
    let iat_len = if needs_bootstrap { bootstrap.unwrap().iat_len() } else { 0 };
    let cli_header_offset_in_text = iat_len;
    let metadata_offset_in_text = cli_header_offset_in_text + CLI_HEADER_CB;
    let bodies_offset_in_text =
        metadata_offset_in_text + u32::try_from(metadata.len()).expect("metadata exceeds u32");
    let debug_dir_offset_in_text =
        bodies_offset_in_text + u32::try_from(method_bodies.len()).expect("method bodies exceed u32");
    let debug_dir_len = options
        .debug_directory
        .as_ref()
        .map_or(0, |d| debug_directory_region_len(d, options.pdb_checksum.as_ref()));
    let import_stub_offset_in_text = debug_dir_offset_in_text + debug_dir_len;
    let text_content_len = if needs_bootstrap {
        import_stub_offset_in_text + bootstrap.unwrap().import_and_stub_len()
    } else {
        import_stub_offset_in_text
    };
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

    let reloc = needs_bootstrap.then(|| {
        let prev = sdata.as_ref().unwrap_or(&text);
        let rva = align_up(prev.next_rva_floor(), SECTION_ALIGNMENT);
        let file_offset = prev.next_file_offset();
        debug_assert_eq!(file_offset % FILE_ALIGNMENT, 0);
        SectionLayout::plan(file_offset, rva, RELOC_CONTENT_LEN)
    });

    let iat_rva = text.rva;
    let cli_header_rva = text.rva + cli_header_offset_in_text;
    let metadata_rva = text.rva + metadata_offset_in_text;
    let metadata_len = u32::try_from(metadata.len()).expect("metadata exceeds u32");
    let debug_dir_rva = text.rva + debug_dir_offset_in_text;
    let import_table_rva = text.rva + import_stub_offset_in_text;

    let last_section = reloc.as_ref().or(sdata.as_ref()).unwrap_or(&text);
    let size_of_image = align_up(last_section.next_rva_floor(), SECTION_ALIGNMENT);
    let size_of_headers = headers_raw_size;

    // --- Emit --------------------------------------------------------------------------------
    let mut out = Vec::with_capacity(
        headers_raw_size as usize
            + text.raw_size as usize
            + sdata.map_or(0, |s| s.raw_size as usize)
            + reloc.map_or(0, |s| s.raw_size as usize),
    );

    write_dos_header_and_stub(&mut out);
    debug_assert_eq!(out.len() as u32, DOS_STUB_LEN);

    out.extend_from_slice(b"PE\0\0");

    write_coff_header(&mut out, num_sections, optional_header_size, options.is_dll);

    // The native stub's own RVA (§II.25.4 places `AddressOfEntryPoint` at the stub, once a
    // bootstrap is present). §II.25.2.3.1 requires `AddressOfEntryPoint` to be **0** when the
    // image has no native entry point — i.e. every `.dll` (a library is never natively executed;
    // the CLR reaches it only through the CLI header's own `EntryPointToken`/managed loading, per
    // the module doc's "A `.dll` … skips all of the above" note). Pointing it at `.text`'s base
    // instead (as if any in-section RVA were "inert") is what this backend did before this fix —
    // that RVA lands on the CLI header itself (`.text` = [IAT] + CLI header + metadata + bodies,
    // and a `.dll` has no IAT), so a nonzero `AddressOfEntryPoint` told CoreCLR's native PE loader
    // "there is native code to run here", and it tried to validate/treat the CLI header bytes as
    // an executable entry stub — rejected with `BadImageFormatException` at `Assembly.Load`,
    // *before* the CLI-aware managed loader ever got to inspect the metadata (confirmed via the
    // reference-grade `System.Reflection.Metadata` reader accepting the same bytes with zero
    // errors, and a real `ilasm`-produced `.dll` for the identical source loading fine with
    // `AddressOfEntryPoint = 0`).
    let entry_point_rva = if needs_bootstrap {
        import_table_rva + bootstrap.unwrap().stub_offset_in_import_region()
    } else {
        0
    };
    write_optional_header(
        &mut out,
        size_of_image,
        size_of_headers,
        text.virtual_size,
        entry_point_rva,
        text.rva,
        cli_header_rva,
        bootstrap.map(|b| (import_table_rva, iat_rva, b)),
        reloc.map(|r| (r.rva, r.virtual_size)),
        options.debug_directory.as_ref().map(|_| {
            // `DataDirectory[6].Size` is the ROW TABLE's own size only (`N *
            // IMAGE_DEBUG_DIRECTORY_LEN`), NOT the region's total length including payloads —
            // confirmed against a real Roslyn-built `.dll` (`PEReader.ReadDebugDirectory()`'s 3
            // rows summed to exactly `84 = 3 * 28`, not the region's full byte length). Passing
            // `debug_dir_len` (the whole region) here was a real bug caught during Phase 2
            // acceptance testing: it doesn't stop a well-behaved reader from parsing more entries
            // than exist (nothing reads past `Size` on a well-formed table), but is not spec-clean.
            let row_count = 1 + u32::from(options.pdb_checksum.is_some());
            (debug_dir_rva, row_count * IMAGE_DEBUG_DIRECTORY_LEN)
        }),
    );

    write_section_header(&mut out, b".text", &text, TEXT_SECTION_CHARACTERISTICS);
    if let Some(sdata) = &sdata {
        write_section_header(&mut out, b".sdata", sdata, SDATA_SECTION_CHARACTERISTICS);
    }
    if let Some(reloc) = &reloc {
        write_section_header(&mut out, b".reloc", reloc, RELOC_SECTION_CHARACTERISTICS);
    }

    // Pad up to the end of the (FileAlignment-aligned) header region.
    out.resize(headers_raw_size as usize, 0);
    debug_assert_eq!(out.len() as u32 % FILE_ALIGNMENT, 0);

    // .text: [IAT] CLI header, metadata, method bodies, [Debug Directory + RSDS], [Import Table + stub].
    debug_assert_eq!(out.len() as u32, text.file_offset);
    if let Some(b) = bootstrap {
        write_iat(&mut out, import_table_rva, b);
    }
    write_cli_header(
        &mut out,
        metadata_rva,
        metadata_len,
        options.entry_point.unwrap_or(0),
    );
    out.extend_from_slice(metadata);
    out.extend_from_slice(method_bodies);
    if let Some(entry) = &options.debug_directory {
        debug_assert_eq!(out.len() as u32, debug_dir_rva - text.rva + text.file_offset, "debug directory must start exactly where the layout pass placed it");
        let before = out.len();
        write_debug_directory(&mut out, debug_dir_rva, entry, options.pdb_checksum.as_ref());
        debug_assert_eq!(
            u32::try_from(out.len() - before).unwrap(),
            debug_dir_len,
            "write_debug_directory must emit exactly debug_directory_region_len(entry, checksum) bytes"
        );
    }
    if let Some(b) = bootstrap {
        write_import_table_and_stub(&mut out, import_table_rva, iat_rva, b);
    }
    out.resize(text.next_file_offset() as usize, 0);
    debug_assert_eq!(out.len() as u32 % FILE_ALIGNMENT, 0);

    // .sdata: FieldRVA blobs, verbatim.
    if let Some(sdata) = &sdata {
        debug_assert_eq!(out.len() as u32, sdata.file_offset);
        out.extend_from_slice(field_rva_data);
        out.resize(sdata.next_file_offset() as usize, 0);
        debug_assert_eq!(out.len() as u32 % FILE_ALIGNMENT, 0);
    }

    // .reloc: one HIGHLOW fixup for the stub's hardcoded absolute-VA operand.
    if let (Some(reloc), Some(b)) = (&reloc, bootstrap) {
        debug_assert_eq!(out.len() as u32, reloc.file_offset);
        let stub_rva = import_table_rva + b.stub_offset_in_import_region();
        // The fixup targets the stub's 4-byte operand, which starts 2 bytes into the 6-byte
        // `FF 25 <VA>` stub (past the `FF 25` opcode).
        write_base_relocation_block(&mut out, stub_rva + 2);
        out.resize(reloc.next_file_offset() as usize, 0);
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
/// `bootstrap` is `Some((import_table_rva, iat_rva, layout))` when a native bootstrap stub is
/// present (an `.exe` — see `write_pe`'s module doc); `reloc` is `Some((rva, size))` for the
/// matching `.reloc` section. Both `None` for a `.dll`.
fn write_optional_header(
    out: &mut Vec<u8>,
    size_of_image: u32,
    size_of_headers: u32,
    size_of_text: u32,
    entry_point_rva: u32,
    text_rva: u32,
    cli_header_rva: u32,
    bootstrap: Option<(u32, u32, BootstrapLayout)>,
    reloc: Option<(u32, u32)>,
    debug: Option<(u32, u32)>,
) {
    // --- Standard fields (§II.25.2.3.1) ---
    out.extend_from_slice(&PE32_MAGIC.to_le_bytes());
    out.push(8); // LMajor: matches ilasm output.
    out.push(0); // LMinor.
    out.extend_from_slice(&align_up(size_of_text, FILE_ALIGNMENT).to_le_bytes()); // SizeOfCode.
    out.extend_from_slice(&0u32.to_le_bytes()); // SizeOfInitializedData (folded into .text/.sdata's own accounting; ECMA-335 images conventionally leave this 0, matching ilasm).
    out.extend_from_slice(&0u32.to_le_bytes()); // SizeOfUninitializedData.
    out.extend_from_slice(&entry_point_rva.to_le_bytes()); // AddressOfEntryPoint: the native bootstrap stub's RVA for a .exe (see module doc's "Risk #1 confirmed"), or 0 for a .dll (§II.25.2.3.1 requires 0 when there is no native entry point — see this call site's doc).
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
    out.extend_from_slice(&DLL_CHARACTERISTICS.to_le_bytes()); // DllCharacteristics: see that constant's doc — this is the confirmed cause of the FileLoadException 0x8007000C load bug, not a stylistic no-op.
    out.extend_from_slice(&0x0010_0000u32.to_le_bytes()); // SizeOfStackReserve: conventional default.
    out.extend_from_slice(&0x0000_1000u32.to_le_bytes()); // SizeOfStackCommit.
    out.extend_from_slice(&0x0010_0000u32.to_le_bytes()); // SizeOfHeapReserve.
    out.extend_from_slice(&0x0000_1000u32.to_le_bytes()); // SizeOfHeapCommit.
    out.extend_from_slice(&0u32.to_le_bytes()); // LoaderFlags: reserved, always 0.
    out.extend_from_slice(&NUMBER_OF_RVA_AND_SIZES.to_le_bytes());

    // --- Data directories (§II.25.2.3.3) ---
    for i in 0..NUMBER_OF_RVA_AND_SIZES as usize {
        let entry = if i == DATA_DIRECTORY_CLI_HEADER {
            Some((cli_header_rva, CLI_HEADER_CB))
        } else if i == DATA_DIRECTORY_IMPORT_TABLE {
            // Size is the Import Directory Table's own span (descriptor rows only, not the
            // ILT/Hint-Name/name-string/stub bytes that follow it in the same region).
            bootstrap.map(|(import_table_rva, _, _)| (import_table_rva, IMPORT_DIRECTORY_LEN))
        } else if i == DATA_DIRECTORY_IAT {
            bootstrap.map(|(_, iat_rva, b)| (iat_rva, b.iat_len()))
        } else if i == DATA_DIRECTORY_BASE_RELOCATION_TABLE {
            reloc
        } else if i == DATA_DIRECTORY_DEBUG {
            debug
        } else {
            None
        };
        let (rva, size) = entry.unwrap_or((0, 0));
        out.extend_from_slice(&rva.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
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

/// Writes the IAT (§II.25.4.2) at the very start of `.text`: one Hint/Name RVA `DWORD` (pointing
/// at the Hint/Name entry inside the "Import Table + stub" region emitted later, at
/// `import_table_rva + BootstrapLayout::hint_name_offset`) + a null-terminator `DWORD`. Before the
/// OS loader binds it, this is byte-identical to the ILT (both start life as a copy of the same
/// Hint/Name RVA, per §II conventions) — the loader overwrites this slot in memory with
/// `_CorExeMain`'s resolved address at load time; the on-disk bytes are only ever this pre-bind
/// form.
fn write_iat(out: &mut Vec<u8>, import_table_rva: u32, b: BootstrapLayout) {
    let hint_name_rva = import_table_rva + b.hint_name_offset();
    out.extend_from_slice(&hint_name_rva.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // null terminator DWORD.
}

/// Writes the "Import Table + stub" region (§II.25.3.1's referenced import-table conventions +
/// the native bootstrap stub) at the tail of `.text`: Import Directory Table, ILT, Hint/Name
/// entry, `"mscoree.dll\0"` name, padding, then the `FF 25 <abs VA>` stub. `import_table_rva` is
/// this region's own base RVA; `iat_rva` is the (already-emitted, earlier-in-`.text`) IAT's RVA,
/// which both the descriptor's `FirstThunk` and the stub's absolute-VA operand reference.
fn write_import_table_and_stub(out: &mut Vec<u8>, import_table_rva: u32, iat_rva: u32, b: BootstrapLayout) {
    let region_start = out.len();
    let ilt_rva = import_table_rva + b.ilt_offset();
    let hint_name_rva = import_table_rva + b.hint_name_offset();
    let dll_name_rva = import_table_rva + b.dll_name_offset();

    // Import Directory Table: one real IMAGE_IMPORT_DESCRIPTOR (§II conventions, 20 bytes) +
    // one all-zero terminator descriptor (20 bytes).
    out.extend_from_slice(&ilt_rva.to_le_bytes()); // OriginalFirstThunk (the ILT).
    out.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp: 0 (determinism; also "not bound" per §II conventions).
    out.extend_from_slice(&0u32.to_le_bytes()); // ForwarderChain: unused.
    out.extend_from_slice(&dll_name_rva.to_le_bytes()); // Name RVA ("mscoree.dll").
    out.extend_from_slice(&iat_rva.to_le_bytes()); // FirstThunk (the IAT).
    out.extend_from_slice(&[0u8; IMPORT_DESCRIPTOR_LEN as usize]); // null terminator descriptor.

    // ILT: identical pre-bind shape to the IAT (§II conventions — both point at the same
    // Hint/Name RVA before binding).
    out.extend_from_slice(&hint_name_rva.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // null terminator DWORD.

    // Hint/Name entry: 2-byte Hint (0, no ordinal import) + "_CorExeMain\0".
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(COR_EXE_MAIN);

    // "mscoree.dll\0".
    out.extend_from_slice(MSCOREE_DLL);

    // Pad to the 16-byte-aligned stub start (region-relative, not RVA-relative — the region's
    // own start may not itself be 16-byte aligned within .text, but ilasm's convention aligns the
    // STUB specifically, which is what matters for disassembly tidiness; not load-bearing).
    let region_len_so_far = u32::try_from(out.len() - region_start).unwrap();
    let pad = b.stub_offset_in_import_region() - region_len_so_far;
    out.resize(out.len() + pad as usize, 0);
    debug_assert_eq!(u32::try_from(out.len() - region_start).unwrap(), b.stub_offset_in_import_region());

    // Native stub: `jmp dword ptr [iat_rva]` = FF 25 <abs VA of the IAT slot>.
    out.extend_from_slice(&ENTRY_STUB_OPCODE);
    let abs_va = IMAGE_BASE + iat_rva;
    out.extend_from_slice(&abs_va.to_le_bytes());
}

/// `IMAGE_DEBUG_DIRECTORY` (Windows PE "Debug Directory" format, referenced but not defined by
/// ECMA-335 — CoreCLR repurposes it to locate a `.pdb`) fixed row size: `Characteristics`(4) +
/// `TimeDateStamp`(4) + `MajorVersion`(2) + `MinorVersion`(2) + `Type`(4) + `SizeOfData`(4) +
/// `AddressOfRawData`(4) + `PointerToRawData`(4) = 28 bytes. `DataDirectory[6]`'s `Size` is
/// `N * IMAGE_DEBUG_DIRECTORY_LEN` for `N` consecutive rows — a loader iterates
/// `Size / IMAGE_DEBUG_DIRECTORY_LEN` rows starting at `DataDirectory[6].VirtualAddress`, which is
/// how this writer emits more than one Debug Directory entry (CodeView + PdbChecksum) from a
/// single data-directory slot.
const IMAGE_DEBUG_DIRECTORY_LEN: u32 = 28;
/// `IMAGE_DEBUG_TYPE_CODEVIEW`: the Debug Directory entry `Type` value CoreCLR's PDB-matching
/// logic looks for.
const IMAGE_DEBUG_TYPE_CODEVIEW: u32 = 2;
/// `IMAGE_DEBUG_TYPE_PDBCHECKSUM`: see `pdb::sha256`'s doc for why this entry — not just
/// `CodeView`/RSDS — is required for CoreCLR's RUNTIME `StackTraceSymbols` provider (as opposed to
/// the static `System.Reflection.Metadata` reader, which never checks it) to trust a portable PDB.
const IMAGE_DEBUG_TYPE_PDB_CHECKSUM: u32 = 19;
/// The CodeView entry's `MajorVersion`/`MinorVersion` fields, when the payload is a PORTABLE (not
/// classic Windows) PDB pointer: `MajorVersion = 0x0100`, `MinorVersion = 0x504D` (ASCII "PM",
/// dotnet/runtime's `PortablePdbVersions.DefaultMajorVersion`/`DefaultMinorVersion` convention;
/// confirmed byte-for-byte against a real Roslyn-built `.dll`'s CodeView entry).
///
/// **This is the actual root cause of Phase 2's acceptance gap**, found by tracing
/// `System.Reflection.Metadata.PortableExecutable.PEReader.TryOpenAssociatedPortablePdb`'s IL
/// (the exact API `System.Diagnostics.StackTraceSymbols` — the runtime's `StackTrace`/
/// `Environment.StackTrace` file:line resolver — calls): the writer previously left
/// `MajorVersion`/`MinorVersion` at `0`, which is a syntactically valid `IMAGE_DEBUG_DIRECTORY` row
/// (every structural check — RSDS signature, GUID/age match, row counts, sequence-point encoding —
/// passed) but `TryOpenAssociatedPortablePdb` uses these two fields (NOT the RSDS payload alone)
/// to recognize "this CodeView entry points at a Portable PDB, go look for
/// `<dll-stem>.pdb` next to the image" — with the wrong version marker it silently returns `false`
/// without even attempting to open a file, which is exactly the symptom this task's acceptance
/// testing hit (a structurally perfect PDB, confirmed readable by
/// `MetadataReaderProvider.FromPortablePdbStream` directly, that `Environment.StackTrace` never
/// even tried to load).
const PORTABLE_CODEVIEW_MAJOR_VERSION: u16 = 0x0100;
const PORTABLE_CODEVIEW_MINOR_VERSION: u16 = 0x504D;
/// The CodeView "RSDS" signature (PDB 7.0 format — `RSDS` + 16-byte GUID + 4-byte Age +
/// NUL-terminated path), per `pdb::DebugDirectoryEntry`'s doc.
const CV_SIGNATURE_RSDS: &[u8; 4] = b"RSDS";

/// Total byte length of the Debug Directory region `write_pe` places in `.text`: one or two
/// `IMAGE_DEBUG_DIRECTORY` rows (`IMAGE_DEBUG_DIRECTORY_LEN` each — CodeView always, PdbChecksum
/// when `checksum` is `Some`) followed by their payloads in the same order (RSDS, then the
/// checksum payload if present). Computing this as its own function (rather than inline at each of
/// `write_pe`'s two call sites — layout pass and emit pass) is what lets both agree on the
/// region's size without duplicating either payload's shape twice, mirroring how
/// [`BootstrapLayout`] is the single source of truth for the import-table-and-stub region's size.
#[must_use]
fn debug_directory_region_len(entry: &super::pdb::DebugDirectoryEntry, checksum: Option<&super::pdb::PdbChecksumEntry>) -> u32 {
    let row_count = 1 + u32::from(checksum.is_some());
    let rsds_len = 4 + 16 + 4 + u32::try_from(entry.pdb_path.len()).expect("pdb path exceeds u32") + 1;
    let checksum_len = checksum.map_or(0, |c| u32::try_from(c.payload_bytes().len()).unwrap());
    row_count * IMAGE_DEBUG_DIRECTORY_LEN + rsds_len + checksum_len
}

/// Writes the Debug Directory region at `region_rva` (within `.text`, per this writer's layout
/// pass): the `IMAGE_DEBUG_DIRECTORY` row(s) first (CodeView, then PdbChecksum if `checksum` is
/// `Some`), followed by their payloads in the same order — each row's `AddressOfRawData`/
/// `PointerToRawData` point at its own payload placed after ALL the rows (so file-offset and RVA
/// are always `region_rva`-relative + a fixed row-table length — no separate layout step needed
/// for either payload). `TimeDateStamp` is `0` on the PdbChecksum row, but on the CodeView row it
/// is `entry.stamp` (`pdb_id[16..20]`) — NOT zero. That field is the SRM match key `PEReader`
/// pairs with the RSDS GUID to find the PDB (see the in-body comment below), so it must track the
/// PDB id's content hash; only the row's *presence* is deterministic (content-derived, never
/// wall-clock), not its value.
fn write_debug_directory(
    out: &mut Vec<u8>,
    region_rva: u32,
    entry: &super::pdb::DebugDirectoryEntry,
    checksum: Option<&super::pdb::PdbChecksumEntry>,
) {
    let row_count = 1 + u32::from(checksum.is_some());
    let rows_len = row_count * IMAGE_DEBUG_DIRECTORY_LEN;
    let rsds_rva = region_rva + rows_len;
    let rsds_file_offset = u32::try_from(out.len()).unwrap() + rows_len;
    let rsds_len = 4 + 16 + 4 + u32::try_from(entry.pdb_path.len()).expect("pdb path exceeds u32") + 1;

    // Row 0: CodeView. MajorVersion/MinorVersion MUST carry the Portable-PDB marker (see
    // PORTABLE_CODEVIEW_{MAJOR,MINOR}_VERSION's doc) — this is the field
    // `PEReader.TryOpenAssociatedPortablePdb` actually keys off of before it will even attempt to
    // open a `<dll-stem>.pdb` file, independent of the RSDS payload's own correctness.
    //
    // **`TimeDateStamp` is NOT cosmetic here** — this is the SECOND root-cause bug found chasing
    // Phase 2's acceptance gap (see `PORTABLE_CODEVIEW_MAJOR_VERSION`'s doc for the first). Per
    // `System.Reflection.Metadata`'s own decompiled source
    // (`PEReader.TryOpenCodeViewPortablePdb`): the match key it builds is
    // `new BlobContentId(codeViewDebugDirectoryData.Guid, codeViewEntry.Stamp)` — i.e. the GUID
    // comes from the RSDS payload, but the 4-byte "age"/stamp half comes from THIS ROW's
    // `TimeDateStamp` field, NOT from the RSDS payload's own `Age` field (`Age` is read but never
    // compared against anything by this code path). That combined id is then compared against
    // `metadataReaderProvider.GetMetadataReader().DebugMetadataHeader.Id` (the PDB's own `#Pdb`
    // stream `Id`, bytes `0..20`) — a mismatch silently returns `false` with `pdbPath` left `null`,
    // exactly the symptom this task's acceptance testing hit even after every other structural
    // check (RSDS GUID, row counts, entry point, sequence points, PdbChecksum) passed. Writing `0`
    // here — determinism-safe in isolation, since it never touches wall-clock — was thus a REAL
    // correctness bug: it silently mismatched the PDB id's real stamp bytes (`pdb_id[16..20]`,
    // always nonzero for non-degenerate content) whenever they weren't already `0`.
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics: reserved, always 0.
    out.extend_from_slice(&entry.stamp.to_le_bytes()); // TimeDateStamp: pdb_id[16..20] — the SRM match key (see above), still deterministic (content-derived, never wall-clock).
    out.extend_from_slice(&PORTABLE_CODEVIEW_MAJOR_VERSION.to_le_bytes());
    out.extend_from_slice(&PORTABLE_CODEVIEW_MINOR_VERSION.to_le_bytes());
    out.extend_from_slice(&IMAGE_DEBUG_TYPE_CODEVIEW.to_le_bytes());
    out.extend_from_slice(&rsds_len.to_le_bytes()); // SizeOfData.
    out.extend_from_slice(&rsds_rva.to_le_bytes()); // AddressOfRawData.
    out.extend_from_slice(&rsds_file_offset.to_le_bytes()); // PointerToRawData.

    // Row 1 (optional): PdbChecksum.
    if let Some(checksum) = checksum {
        let checksum_len = u32::try_from(checksum.payload_bytes().len()).unwrap();
        let checksum_rva = rsds_rva + rsds_len;
        let checksum_file_offset = rsds_file_offset + rsds_len;
        out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics.
        out.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp: 0.
        out.extend_from_slice(&0u16.to_le_bytes()); // MajorVersion.
        out.extend_from_slice(&0u16.to_le_bytes()); // MinorVersion.
        out.extend_from_slice(&IMAGE_DEBUG_TYPE_PDB_CHECKSUM.to_le_bytes());
        out.extend_from_slice(&checksum_len.to_le_bytes());
        out.extend_from_slice(&checksum_rva.to_le_bytes());
        out.extend_from_slice(&checksum_file_offset.to_le_bytes());
    }

    // Payloads, in row order.
    out.extend_from_slice(CV_SIGNATURE_RSDS);
    out.extend_from_slice(&entry.guid);
    out.extend_from_slice(&entry.age.to_le_bytes());
    out.extend_from_slice(entry.pdb_path.as_bytes());
    out.push(0); // NUL terminator.
    if let Some(checksum) = checksum {
        out.extend_from_slice(&checksum.payload_bytes());
    }
}

/// Writes a `.reloc` section's single base-relocation block (§II.25.3): `PageRVA` (the
/// 4KiB-aligned page `fixup_rva` falls in), `BlockSize` (this block's total byte length, header
/// included, rounded up to a 4-byte boundary per §II.25.3), then one `IMAGE_REL_BASED_HIGHLOW`
/// entry `(type<<12)|(fixup_rva - PageRVA)`, padded with an `IMAGE_REL_BASED_ABSOLUTE`
/// (type 0, a documented no-op padding entry) `u16` if needed to reach the 4-byte boundary.
fn write_base_relocation_block(out: &mut Vec<u8>, fixup_rva: u32) {
    const PAGE_SIZE: u32 = 0x1000;
    let page_rva = fixup_rva & !(PAGE_SIZE - 1);
    let offset_in_page = fixup_rva - page_rva;
    let entry = (u16::from(IMAGE_REL_BASED_HIGHLOW) << 12) | u16::try_from(offset_in_page).unwrap();
    out.extend_from_slice(&page_rva.to_le_bytes());
    out.extend_from_slice(&RELOC_CONTENT_LEN.to_le_bytes()); // BlockSize: 8-byte header + 1 entry (2B) + 2B ABSOLUTE padding = 12.
    out.extend_from_slice(&entry.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // IMAGE_REL_BASED_ABSOLUTE padding entry.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pe_options_construct() {
        let opts = PeOptions {
            is_dll: false,
            entry_point: Some(0x0600_0001),
            debug_directory: None,
        pdb_checksum: None,
        };
        assert!(!opts.is_dll);
        assert_eq!(opts.entry_point, Some(0x0600_0001));
    }

    #[test]
    fn pe_options_lib_has_no_forced_entry_point() {
        let opts = PeOptions {
            is_dll: true,
            entry_point: None,
            debug_directory: None,
        pdb_checksum: None,
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
        address_of_entry_point: u32,
        image_base: u32,
        section_alignment: u32,
        file_alignment: u32,
        subsystem: u16,
        dll_characteristics: u16,
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
        debug_dir: (u32, u32),
        raw: Vec<u8>,
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
        // Optional header layout (§II.25.2.3.1, PE32): Magic(2) LMajor(1) LMinor(1)
        // SizeOfCode(4) SizeOfInitializedData(4) SizeOfUninitializedData(4)
        // AddressOfEntryPoint(4) @ offset 16.
        let address_of_entry_point = read_u32(data, opt + 16);
        let image_base = read_u32(data, opt + 28);
        let section_alignment = read_u32(data, opt + 32);
        let file_alignment = read_u32(data, opt + 36);
        let subsystem = read_u16(data, opt + 68);
        let dll_characteristics = read_u16(data, opt + 70);
        let number_of_rva_and_sizes = read_u32(data, opt + 92);
        let dir_base = opt + 96;
        let cli_header_dir = (
            read_u32(data, dir_base + DATA_DIRECTORY_CLI_HEADER * 8),
            read_u32(data, dir_base + DATA_DIRECTORY_CLI_HEADER * 8 + 4),
        );
        let debug_dir = (
            read_u32(data, dir_base + DATA_DIRECTORY_DEBUG * 8),
            read_u32(data, dir_base + DATA_DIRECTORY_DEBUG * 8 + 4),
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
            address_of_entry_point,
            image_base,
            section_alignment,
            file_alignment,
            subsystem,
            dll_characteristics,
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
            debug_dir,
            raw: data.to_vec(),
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
            debug_directory: None,
        pdb_checksum: None,
        };
        let image = write_pe(&metadata, &bodies, &[], &opts);
        let pe = parse_pe(&image);

        assert_eq!(pe.e_lfanew, DOS_STUB_LEN);
        assert_eq!(pe.machine, IMAGE_FILE_MACHINE_I386);
        // 2, not 1: no field_rva_data => no .sdata, but `entry_point: Some(..)` means this is an
        // `.exe` => the native bootstrap stub's `.reloc` section is always present (see
        // `write_pe`'s module doc's "Risk #1 confirmed" note).
        assert_eq!(pe.num_sections, 2, "no .sdata (no field_rva_data), but .reloc IS present (has an entry point)");
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
            debug_directory: None,
        pdb_checksum: None,
        };
        let image = write_pe(&[1, 2, 3], &[4, 5, 6], &[], &opts);
        let pe = parse_pe(&image);
        assert_eq!(pe.cli_entry_point_token, 0, "library => EntryPointToken 0");
        assert_eq!(
            pe.characteristics & IMAGE_FILE_DLL,
            IMAGE_FILE_DLL,
            "is_dll must set IMAGE_FILE_DLL"
        );
        // §II.25.2.3.1: `AddressOfEntryPoint` shall be 0 when the image has no native entry
        // point. A library never carries the `mscoree.dll`/`_CorExeMain` bootstrap stub — pointing
        // this at `.text`'s base instead (as if any in-section RVA were harmlessly "unexecuted")
        // told CoreCLR's *native* PE loader there WAS a native entry point to validate there, and
        // it rejected the CLI-header bytes sitting at that RVA as malformed code —
        // `System.IO.FileLoadException`/`BadImageFormatException` at `Assembly.Load`, before the
        // CLI-aware managed loader ever inspected the metadata (root-caused via the `cd_interop`
        // C# consumer battery item: MSBuild's `dotnet build` failed with CS0246 because the
        // referenced `cd_interop.dll` — otherwise structurally perfect, confirmed via
        // `System.Reflection.Metadata` and a real `ilasm`-built `.dll` for the same source loading
        // fine — could not be `Assembly.Load`ed at all).
        assert_eq!(
            pe.address_of_entry_point, 0,
            "a .dll with no bootstrap must have AddressOfEntryPoint == 0, not point into .text"
        );
    }

    /// **Regression test for the `pal_threads`/`cd_interop` `FileLoadException 0x8007000C` load
    /// bug** (the residual carried across three prior A/B differential rounds — see
    /// `DLL_CHARACTERISTICS`'s doc for the full root-cause writeup). `DllCharacteristics`
    /// (§II.25.2.3.2) was previously hardcoded to 0, on the (WRONG, never actually checked)
    /// assumption that this "matches ilasm's unsigned/non-ASLR-flagged output". A live-built
    /// ilasm `.exe`'s raw optional-header bytes at this offset are `0x8540` —
    /// `IMAGE_DLLCHARACTERISTICS_DYNAMIC_BASE (0x0040) | IMAGE_DLLCHARACTERISTICS_NX_COMPAT
    /// (0x0100) | IMAGE_DLLCHARACTERISTICS_NO_SEH (0x0400) |
    /// IMAGE_DLLCHARACTERISTICS_TERMINAL_SERVER_AWARE (0x8000)` — and CoreCLR's native PE loader
    /// (not the lenient `System.Reflection.Metadata` reader every prior round's structural check
    /// used) rejects an all-zero `DllCharacteristics` image at `Assembly.Load`, before the
    /// CLI-aware managed loader ever inspects the metadata. Applies to both a `.dll` and an `.exe`
    /// (the field lives in the shared optional header, independent of `is_dll`).
    #[test]
    fn dll_characteristics_matches_ilasms_real_value_not_zero() {
        for is_dll in [false, true] {
            let opts = PeOptions {
                is_dll,
                entry_point: if is_dll { None } else { Some(0x0600_0001) },
                debug_directory: None,
            pdb_checksum: None,
            };
            let image = write_pe(&[1, 2, 3], &[4, 5, 6], &[], &opts);
            let pe = parse_pe(&image);
            assert_eq!(
                pe.dll_characteristics, 0x8540,
                "DllCharacteristics must match ilasm's real (non-zero) value for is_dll={is_dll}"
            );
        }
    }

    // --- (b) alignment invariants across several odd-sized inputs ----------------------------

    #[test]
    fn alignment_invariants_hold_for_odd_sized_inputs() {
        let opts = PeOptions {
            is_dll: false,
            entry_point: Some(0x0600_0001),
            debug_directory: None,
        pdb_checksum: None,
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
            debug_directory: None,
        pdb_checksum: None,
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
            debug_directory: None,
        pdb_checksum: None,
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

    /// `PeOptions::debug_directory` wires a `DataDirectory[6]` (Debug) entry pointing at an
    /// `IMAGE_DEBUG_DIRECTORY` row (type `IMAGE_DEBUG_TYPE_CODEVIEW`) + RSDS payload placed in
    /// `.text`, and `.sdata` (when present) still lands correctly AFTER that region — proving
    /// `field_rva_section_start`'s `debug_dir_len` parameter (and `write_pe`'s own internal
    /// `debug_dir_offset_in_text` arithmetic) actually agree with each other, the same way
    /// `field_rva_section_start_matches_write_pes_actual_sdata_rva` cross-checks the bootstrap-tail
    /// case.
    #[test]
    fn debug_directory_round_trips_through_write_pe() {
        let entry = super::super::pdb::DebugDirectoryEntry {
            guid: [
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
            ],
            age: 7,
            stamp: 0x1234_5678,
            pdb_path: "cd_pdb-deadbeef.pdb".to_string(),
        };
        let opts = PeOptions {
            is_dll: true,
            entry_point: None,
            debug_directory: Some(entry.clone()),
            pdb_checksum: None,
        };
        let metadata = vec![0xAAu8; 100];
        let bodies = vec![0xBBu8; 50];
        let field_rva_data = vec![0x11, 0x22, 0x33, 0x44];
        let image = write_pe(&metadata, &bodies, &field_rva_data, &opts);
        let pe = parse_pe(&image);

        assert_ne!(pe.debug_dir, (0, 0), "DataDirectory[6] must be populated when debug_directory is Some");
        let (debug_dir_rva, debug_dir_size) = pe.debug_dir;
        // `DataDirectory[6].Size` is the row-table's own size only (one CodeView row here, no
        // PdbChecksum row since `pdb_checksum: None`) — see `write_optional_header`'s call site
        // doc for why this is NOT `debug_directory_region_len`'s full region length.
        assert_eq!(debug_dir_size, IMAGE_DEBUG_DIRECTORY_LEN);

        let file_off = rva_to_file_offset(&pe.sections, debug_dir_rva) as usize;
        let data = &pe.raw;
        assert_eq!(read_u32(data, file_off), 0, "Characteristics: reserved, always 0");
        // TimeDateStamp = entry.stamp (pdb_id[16..20], verbatim) — NOT 0 and NOT entry.age; see
        // `DebugDirectoryEntry::stamp`'s doc for why this exact field is the real SRM match key.
        assert_eq!(read_u32(data, file_off + 4), entry.stamp, "TimeDateStamp carries the real PDB-id stamp, the field SRM actually matches on");
        assert_eq!(read_u16(data, file_off + 8), PORTABLE_CODEVIEW_MAJOR_VERSION, "MajorVersion carries the Portable-PDB marker");
        assert_eq!(read_u16(data, file_off + 10), PORTABLE_CODEVIEW_MINOR_VERSION, "MinorVersion carries the Portable-PDB marker ('PM')");
        assert_eq!(read_u32(data, file_off + 12), IMAGE_DEBUG_TYPE_CODEVIEW);
        let size_of_data = read_u32(data, file_off + 16);
        let address_of_raw_data = read_u32(data, file_off + 20);
        let pointer_to_raw_data = read_u32(data, file_off + 24);
        assert_eq!(address_of_raw_data, debug_dir_rva + IMAGE_DEBUG_DIRECTORY_LEN);
        assert_eq!(pointer_to_raw_data, file_off as u32 + IMAGE_DEBUG_DIRECTORY_LEN);

        let rsds_off = pointer_to_raw_data as usize;
        assert_eq!(&data[rsds_off..rsds_off + 4], b"RSDS");
        assert_eq!(&data[rsds_off + 4..rsds_off + 20], &entry.guid);
        assert_eq!(read_u32(data, rsds_off + 20), entry.age);
        let path_start = rsds_off + 24;
        let path_bytes = &data[path_start..path_start + entry.pdb_path.len()];
        assert_eq!(path_bytes, entry.pdb_path.as_bytes());
        assert_eq!(data[path_start + entry.pdb_path.len()], 0, "NUL-terminated");
        assert_eq!(
            size_of_data,
            u32::try_from(entry.pdb_path.len()).unwrap() + 1 + 4 + 16 + 4,
            "SizeOfData is the RSDS payload's own length (signature+guid+age+path+NUL)"
        );

        // .sdata must still exist and land strictly after the Debug Directory region (row table +
        // RSDS payload — NOT just `debug_dir_size`, which is the row-table-only DataDirectory[6]
        // size; see this test's earlier comment on that distinction).
        let sdata = section_named(&pe, ".sdata").expect(".sdata must exist");
        assert!(sdata.rva >= debug_dir_rva + debug_directory_region_len(&entry, None));
    }

    /// Same shape as [`debug_directory_round_trips_through_write_pe`] but WITH a `PdbChecksum`
    /// entry too — the actual fix for Phase 2's acceptance gap (see `pdb::sha256`'s doc): asserts
    /// `DataDirectory[6].Size` covers BOTH rows, the second row's `Type`/payload match, and
    /// `.sdata` lands after both rows' payloads.
    #[test]
    fn debug_directory_with_pdb_checksum_emits_two_rows() {
        let entry = super::super::pdb::DebugDirectoryEntry {
            guid: [0xAAu8; 16],
            age: 3,
            stamp: 0x9abc_def0,
            pdb_path: "mini.pdb".to_string(),
        };
        let checksum = super::super::pdb::PdbChecksumEntry::from_pdb_bytes(b"pretend pdb bytes");
        let opts = PeOptions {
            is_dll: true,
            entry_point: None,
            debug_directory: Some(entry.clone()),
            pdb_checksum: Some(checksum.clone()),
        };
        let metadata = vec![0xAAu8; 40];
        let bodies = vec![0xBBu8; 20];
        let image = write_pe(&metadata, &bodies, &[], &opts);
        let pe = parse_pe(&image);

        let (debug_dir_rva, debug_dir_size) = pe.debug_dir;
        assert_eq!(debug_dir_size, 2 * IMAGE_DEBUG_DIRECTORY_LEN, "two rows: CodeView + PdbChecksum");

        let file_off = rva_to_file_offset(&pe.sections, debug_dir_rva) as usize;
        let data = &pe.raw;
        // Row 0: CodeView (same shape as the single-row test).
        assert_eq!(read_u32(data, file_off + 4), entry.stamp, "TimeDateStamp carries entry.stamp, not entry.age");
        assert_eq!(read_u32(data, file_off + 12), IMAGE_DEBUG_TYPE_CODEVIEW);
        let rsds_size = read_u32(data, file_off + 16);

        // Row 1: PdbChecksum, immediately after row 0 in the table.
        let row1_off = file_off + IMAGE_DEBUG_DIRECTORY_LEN as usize;
        assert_eq!(read_u32(data, row1_off + 12), IMAGE_DEBUG_TYPE_PDB_CHECKSUM);
        let checksum_size = read_u32(data, row1_off + 16);
        let checksum_rva = read_u32(data, row1_off + 20);
        let checksum_ptr = read_u32(data, row1_off + 24);
        let expected_payload = checksum.payload_bytes();
        assert_eq!(checksum_size as usize, expected_payload.len());
        // The checksum payload starts right after the RSDS payload (row-table then RSDS then
        // checksum, per `write_debug_directory`'s doc).
        assert_eq!(checksum_rva, debug_dir_rva + 2 * IMAGE_DEBUG_DIRECTORY_LEN + rsds_size);
        let checksum_off = checksum_ptr as usize;
        assert_eq!(&data[checksum_off..checksum_off + expected_payload.len()], &expected_payload[..]);
        assert_eq!(&data[checksum_off..checksum_off + 7], b"SHA256\0");

        assert!(pe.sections.is_empty() == false, "at least .text must exist");
    }

    /// Regression test for a real, confirmed macOS ARM64 CoreCLR load bug (`FileLoadException
    /// 0x8007000C`, root-caused via `lldb` against the release `libcoreclr.dylib` — see
    /// `SECTION_ALIGNMENT`'s own doc for the full writeup). CoreCLR's non-Windows PE loader
    /// (`FlatImageLayout::LoadImageByCopyingParts`) `mprotect`s each section independently once
    /// it has copied the whole image into one anonymous mapping; `mprotect(2)` requires its
    /// address argument to be aligned to the HOST's native page size, which is 16KiB (`0x4000`)
    /// on Apple Silicon — NOT the 4KiB most other `mprotect`-based platforms use. A
    /// `SectionAlignment` smaller than that (ilasm/this writer's old `0x2000`) lets a section
    /// boundary land at, e.g., RVA `0x27a000` (`% 0x4000 == 0x2000`, exactly half a real page)
    /// whenever the PRECEDING section's content length doesn't happen to round to a 16KiB
    /// multiple — which is content-dependent (method-body/metadata size), so it silently varied
    /// per-assembly and only failed for SOME crates, which is exactly why 4+ prior differential
    /// rounds saw an intermittent, assembly-dependent `FileLoadException` that no managed-reader
    /// check (`System.Reflection.Metadata`, ilverify, Mono) could ever catch — those readers only
    /// ever see bytes AFTER a successful `mmap`+`mprotect`.
    ///
    /// This test pins the actual load-bearing invariant directly (every section's mapped RVA must
    /// be a multiple of the largest plausible host OS page size, not just `SectionAlignment`
    /// itself) using odd, non-16KiB-multiple content lengths chosen to reproduce the exact
    /// "coin flip" shape the real bug had before the `SECTION_ALIGNMENT` bump — a weaker test that
    /// only checked alignment to the OLD `0x2000` would have passed on this exact input and missed
    /// the bug entirely.
    #[test]
    fn every_section_boundary_is_aligned_to_the_largest_plausible_host_page_size() {
        const APPLE_SILICON_PAGE_SIZE: u32 = 0x4000; // 16KiB — the property that actually matters.
        assert_eq!(
            SECTION_ALIGNMENT, APPLE_SILICON_PAGE_SIZE,
            "SECTION_ALIGNMENT itself must be >= the largest real host page size, or this test's \
             boundary check below is not actually guaranteed by construction"
        );

        // A content length deliberately NOT a multiple of 16KiB (`0x4000`) — this is exactly the
        // shape that exposed the bug: `.text`'s end (and thus `.sdata`'s start) landing mid-page.
        let metadata = vec![0xAAu8; 12_345];
        let bodies = vec![0xBBu8; 54_321];
        let field_rva_data = vec![0x01u8; 999];
        let opts = PeOptions {
            is_dll: true,
            entry_point: None,
            debug_directory: None,
        pdb_checksum: None,
        };
        let image = write_pe(&metadata, &bodies, &field_rva_data, &opts);
        let pe = parse_pe(&image);

        assert!(pe.num_sections >= 2, "this input must produce both .text and .sdata");
        for section in &pe.sections {
            assert_eq!(
                section.rva % APPLE_SILICON_PAGE_SIZE,
                0,
                "section {:?} RVA {:#x} is not 16KiB-aligned — mprotect() on macOS ARM64 will reject this",
                String::from_utf8_lossy(&section.name),
                section.rva
            );
        }
    }

    /// [`field_rva_section_start`] is the "single source of truth" callers (`export::export_pe`)
    /// must use to pre-compute `FieldRVA.RVA` values before `write_pe` is even called (see that
    /// function's doc for the two-pass `set_field_rva`-before-`serialize` dance this exists for).
    /// Cross-checks its prediction against `write_pe`'s OWN internal layout for both an `.exe`
    /// (bootstrap stub present — the case with the extra IAT + import-table-and-stub tail term)
    /// and a `.dll` (no bootstrap), so any future drift between the two independent computations
    /// fails a fast unit test instead of only surfacing as a corrupted `FieldRVA.RVA` under `dotnet`.
    #[test]
    fn field_rva_section_start_matches_write_pes_actual_sdata_rva() {
        let metadata = vec![0xAAu8; 137]; // an odd length, to exercise the non-4-aligned case too.
        let bodies = vec![0xBBu8; 61];
        let field_rva_data = vec![0x11, 0x22, 0x33, 0x44];

        // `0x06000001` = `MethodDef` table id (0x06) << 24 | rid 1 — a plausible entry-point
        // token; `field_rva_section_start`/`write_pe` only care whether `entry_point` is `Some`,
        // not its value, so any well-formed token works here.
        for entry_point in [None, Some(0x0600_0001u32)] {
            let opts = PeOptions {
                is_dll: entry_point.is_none(),
                entry_point,
                debug_directory: None,
            pdb_checksum: None,
            };
            let image = write_pe(&metadata, &bodies, &field_rva_data, &opts);
            let pe = parse_pe(&image);
            let sdata = section_named(&pe, ".sdata").expect(".sdata must exist");

            let predicted = field_rva_section_start(entry_point.is_some(), metadata.len(), bodies.len(), 0);
            assert_eq!(
                predicted, sdata.rva,
                "entry_point={entry_point:?}: field_rva_section_start's prediction must match write_pe's actual .sdata RVA"
            );
        }
    }

    #[test]
    fn total_image_size_matches_layout_for_synthetic_inputs() {
        let opts = PeOptions {
            is_dll: false,
            entry_point: Some(0x0600_0001),
            debug_directory: None,
        pdb_checksum: None,
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
