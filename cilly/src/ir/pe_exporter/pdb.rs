//! Portable PDB writer (Phase 2, `docs/PE_EMISSION_PLAN.md`) — a *second* BSJB metadata blob,
//! sibling to the one `tables::MetadataBuilder`/`pe::write_pe` produce for the `.dll`/`.exe`
//! itself, per the dotnet/runtime `PortablePdb-Metadata.md` spec (which augments ECMA-335 §II.24
//! with a `#Pdb` stream and three PDB-only tables: `Document` 0x30, `MethodDebugInformation`
//! 0x31, and the optional `LocalScope`/`LocalVariable` 0x32/0x33).
//!
//! # Parity bar
//!
//! `il_exporter` (`cilly/src/ir/il_exporter/mod.rs:1289-1308`) turns every `CILRoot::SourceFileInfo`
//! root into a `.line {line_start},{line_end}:{col_start},{col_end} '{file}'` directive (the
//! `Modern` ilasm flavour form; `Clasic` drops the end-line/end-col). `ilasm -debug`
//! (`assemble_file`, same file, line 1545-1585, gated `#[cfg(not(target_os = "windows"))]`; a
//! second `#[cfg(target_os = "windows")]` copy at line 1587 mirrors it without the PDB-write
//! retry) turns those directives into the on-disk portable PDB — unconditionally passing
//! `-debug` on the non-Windows path, with a documented retry-without-`-debug` fallback when
//! ilasm's own PDB writer chokes on very large assemblies (`Failed to write PDB file`). That
//! retry path, and the `IlasmFlavour::Modern => fpath.with_extension("pdb").exists()` check in
//! `cilly/src/bin/linker/main.rs:709-717` that decides whether the launcher embeds a `.pdb`, is
//! the operational parity bar this module must eventually clear: every `SourceFileInfo` root that
//! reaches a method body becomes one entry in that method's `MethodDebugInformation` sequence
//! points, full stop, with no silent size-triggered omission (this writer does not shell out, so
//! the ilasm giant-assembly-PDB-write failure mode should not recur, but nothing in this stub
//! quantifies that yet).
//!
//! # Where `SourceFileInfo` already surfaces under `DIRECT_PE`
//!
//! `body.rs`'s method-body linearizer (`emit_root`, `body.rs:970-972`) already visits every
//! `CILRoot::SourceFileInfo` root in the same block/handler order `il_exporter` uses (see that
//! function's module doc on the two-pass branch layout) — it just currently emits zero IL bytes
//! for it, with a comment marking the root as debug-info-only and pointing at this phase:
//! ```text
//! CILRoot::SourceFileInfo { .. } => {
//!     // Debug-info only; the pure-body writer emits no bytes for it (Phase 2 = PDB).
//! }
//! ```
//! The fields carried (`cilroot.rs:20-26`) are `line_start: u32`, `line_len: u16`,
//! `col_start: u16`, `col_len: u16`, `file: Interned<IString>` — i.e. `[line_start, line_start +
//! line_len)` x `[col_start, col_start + col_len)`, matching `span_source_info`
//! (`src/assembly.rs:586-616`), which fills it per-statement/terminator from
//! `TyCtxt::sess.source_map().span_to_location_info`. Because `body.rs` already walks these roots
//! in code-offset order while assembling the IL byte stream, the natural seam for Phase 2 is to
//! have that same walk *additionally* record `(il_offset_at_this_point, file, line_start, ...)`
//! into a side list per method — the [`MethodSequencePoints`] output type below pins that shape
//! without wiring the collection yet (`body.rs`'s `emit_root` arm is untouched by this stub; that
//! wiring is the next task, not this one).
//!
//! # Span-quality gaps (mapped, not fixed, by this task)
//!
//! `docs/PE_EMISSION_PLAN.md`'s Phase 0 section (`§Phasing`, "Phase 0 — harvest the latent PDB")
//! names three gaps future Phase-2 work must close or explicitly accept:
//! * **(a) missing frames** — `il_exporter`'s `aggressiveinlining` heuristic
//!   (`il_exporter/mod.rs:462-471`, scoped to single-block/handler-free/<=24-root bodies) lets
//!   RyuJIT inline user `#[inline(never)]` frames out of managed stack traces. Confirmed **not
//!   ported** to the direct-PE path: `pe_exporter/tables.rs`'s `MethodDefRow.impl_flags`
//!   (`tables.rs:153,745-750`) only ever sets the pinvoke-impl bit (`0x80`); no
//!   `MethodImplAttributes.AggressiveInlining` (`0x100`) bit is written anywhere in
//!   `pe_exporter/`. So under `DIRECT_PE=1` this specific gap does not yet exist — nothing to
//!   flag-gate here today; worth a regression test once/if an equivalent JIT hint is ever added
//!   to this writer.
//! * **(b) wrong attribution under MIR inlining** — `span_source_info`
//!   (`src/assembly.rs:586-616`) records whatever span the calling MIR-statement site carries;
//!   when that statement was itself produced by MIR inlining, the span can point into the
//!   inlined callee's source (the Phase-0 probe's `main` frame resolved to
//!   `<WORKSPACE>/src/slice/memchr.rs:19`). The documented fix shape is an outermost-non-inlined-
//!   scope walk analogous to `get_caller_location`'s `#[track_caller]` handling — a `src/`-side
//!   change, out of scope for this stub, and per the task's hard constraints must be
//!   flag-gated/default-off if it risks altering codegen.
//! * **(c) `<WORKSPACE>` path remap** — the literal string `<WORKSPACE>` in span file paths comes
//!   from the vendored `rust-lang/rust` fork under `rust/` (`setup_rustc_fork.sh` checkout used by
//!   the coretests/ui-test harnesses), i.e. rustc's own `--remap-path-prefix`-style diagnostic
//!   convention for that harness — NOT something this repo's `src/`/`cilly/` emits. It only shows
//!   up for build-std/std-source spans compiled under that harness; ordinary user-crate builds
//!   (e.g. `cargo_tests/cd_pdb`) carry absolute, unremapped paths (confirmed: its `.il` output has
//!   real `/…/cd_pdb/src/main.rs`-shaped `.line` directives, not `<WORKSPACE>/…`). Acceptance
//!   testing for this phase should use a `cargo_tests/` crate specifically to avoid this gap.
//!
//! # The Phase-0 probe's acceptance shape
//!
//! `cargo_tests/cd_pdb/src/main.rs` calls two `#[inline(never)]` wrapper fns down to
//! `mycorrhiza::System::Environment::get_stack_trace()`, then asserts (by printing booleans a
//! human/harness greps for) that the returned `Environment.StackTrace` text contains `"main.rs"`,
//! `".rs:line"`, and the innermost fn's name `"deep_leaf_for_pdb_probe"`. That proved the
//! `.line`-directives -> `ilasm -debug` -> portable-PDB -> CoreCLR `StackTrace(fNeedFileInfo:
//! true)` chain end-to-end for the **ilasm** path. The Phase-2 acceptance assert is the same
//! shape, pointed at the **default** `DIRECT_PE=1` path with no `ilasm` in the loop: build
//! `cd_pdb` (or an equivalent crate) with the direct PE writer, load the produced `.dll`/`.exe`
//! next to a PDB this module wrote, and check the managed trace still resolves
//! `cd_pdb/src/main.rs:<line>` — i.e. swap only the producer, keep the same consumer-side check.
//!
//! # This module's scope (interface-pinning only)
//!
//! Everything below is a **compiling stub**: types and function signatures the real
//! sequence-point collection (`body.rs`), metadata population (`tables.rs`-style
//! populate/size/serialize), and debug-directory PE wiring (`pe.rs`) will be built against, so
//! those three call sites can be implemented independently without re-deciding the shapes that
//! cross module boundaries. No portable-PDB bytes are actually assembled yet
//! ([`PdbBuilder::build`] is a documented `todo!()`).

use super::tables::Token;

/// One sequence point (dotnet/runtime `PortablePdb-Metadata.md` §"Sequence Points Blob"): maps an
/// IL byte offset within a method body to a source location, or marks the offset as a *hidden*
/// point (`is_hidden`, the spec's `0xfeefee`-line convention — used for compiler-generated code
/// with no meaningful source mapping, e.g. state-machine plumbing).
///
/// Mirrors `CILRoot::SourceFileInfo`'s fields (`cilroot.rs:20-26`) plus the `il_offset` `body.rs`'s
/// linearizer already knows at the point it visits each root (see this module's doc). `line`/`col`
/// are the *start* of the range `il_exporter` renders as `{line_start},{line_end}:{col_start},
/// {col_end}`; `end_line`/`end_col` are that range's exclusive end, matching `line_start + line_len`
/// / `col_start + col_len` in the source root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequencePoint {
    /// Byte offset from the start of the method body's IL code (i.e. `body.rs`'s running length
    /// counter at `emit_root`-time), NOT counting the fat header.
    pub il_offset: u32,
    /// Absolute or workspace-relative source path, matching `CILRoot::SourceFileInfo::file`
    /// resolved through the assembly's string interner (`Interned<IString>` -> `&str`) —
    /// deliberately an owned `String` here rather than an `Interned<IString>` handle, since a
    /// `PdbBuilder` builds its OWN `#Document`-heap interning independent of the type-system
    /// assembly's heaps (a portable PDB is a wholly separate BSJB blob, per the module doc).
    pub document_path: String,
    /// 1-based start line (`CILRoot::SourceFileInfo::line_start`).
    pub line: u32,
    /// 1-based start column, 0 meaning "no column info" is NOT used here (`col_len` clamping in
    /// `span_source_info` guarantees `col_start < col_end`; `col_start` is 1-based per the spec).
    pub col: u32,
    /// Exclusive end line (`line_start + line_len`).
    pub end_line: u32,
    /// Exclusive end column (`col_start + col_len`).
    pub end_col: u32,
    /// `true` for a *hidden* sequence point (spec: `startLine = endLine = 0xfeefee, startColumn =
    /// 0, endColumn = 0`) — reserved for future compiler-generated-code spans; `SourceFileInfo`
    /// roots as produced by `span_source_info` today are never hidden (every one traces back to a
    /// real MIR statement span), so this is currently always `false` for real callers, but the
    /// field is here so `body.rs`'s collector doesn't need a breaking signature change once
    /// compiler-generated (e.g. drop-glue, coroutine state machine) spans need it.
    pub is_hidden: bool,
}

/// Per-method sequence-point collection: the output shape `body.rs`'s linearizer will populate
/// (walking `CILRoot::SourceFileInfo` roots the same way `emit_root` already does, per this
/// module's doc) and [`PdbBuilder::add_method`] will consume.
///
/// `local_signature` is the `StandAloneSig` token (§II.22.36) of the method's `.locals` — the
/// Sequence Points blob header carries it (or 0) so a debugger can resolve local *slots* to types
/// without re-deriving them from the method signature; `None` mirrors a `LocalVarSigTok` of 0
/// (matches `body.rs::AssembledBody`'s fat header when a method has no locals).
#[derive(Debug, Clone, Default)]
pub struct MethodSequencePoints {
    pub local_signature: Option<Token>,
    pub points: Vec<SequencePoint>,
}

/// Everything [`PdbBuilder::build`] needs about the *type-system* metadata (the `.dll`/`.exe`'s
/// own `tables::MetadataBuilder` output) to write a spec-conformant `#Pdb` stream: the
/// `PortablePdb-Metadata.md` "PDB Stream" section requires the referencing PDB to declare which
/// type-system tables it references (`ReferencedTypeSystemTables`, a 64-bit mask keyed by table
/// id — reuses `Token::TABLE_*`) and their row counts *as of the referenced type-system metadata*,
/// so a PDB stays valid even if loaded against a metadata blob with additional rows appended later
/// (append-only edit-and-continue scenarios) — Phase 2 doesn't need EnC, but the row-count field is
/// mandatory regardless.
#[derive(Debug, Clone, Default)]
pub struct TypeSystemRowCounts {
    /// `(Token::TABLE_*, row_count)` pairs for every non-empty type-system table the type-system
    /// metadata contains, in table-id order — mirrors how `tables::MetadataBuilder::serialize`
    /// computes the `#~` stream's `Valid` bitmask + per-table row-count array
    /// (`tables.rs` around the `Valid`/`Sorted` bitmask construction), which is the natural source
    /// this will be filled from once wired.
    pub rows: Vec<(u32, u32)>,
    /// The type-system `EntryPointToken` (§II.25.3.3) — `0` for a library with no managed entry
    /// point, matching `PeOptions::entry_point`'s `None` case (`pe.rs:76-79`).
    pub entry_point_token: u32,
}

/// Accumulates a Portable PDB's content (documents + per-method debug info) and serializes it to
/// the standalone BSJB-format PDB file bytes.
///
/// Construction mirrors `tables::MetadataBuilder`'s populate-then-serialize shape: build one of
/// these per assembly being exported, call [`add_method`](Self::add_method) once per `MethodDef`
/// row (the spec requires exactly one `MethodDebugInformation` row per type-system `MethodDef`
/// row, in the same order — methods with no sequence points still get an empty-blob row, not a
/// missing one), then [`build`](Self::build).
#[derive(Debug, Default)]
pub struct PdbBuilder {
    type_system: TypeSystemRowCounts,
    /// Indexed by `MethodDef` row id minus 1 (RID-order, matching the type-system table); `None`
    /// until [`add_method`](Self::add_method) is called for that row.
    methods: Vec<Option<MethodSequencePoints>>,
}

impl PdbBuilder {
    /// Starts a new PDB for an assembly whose type-system metadata has the given row counts /
    /// entry-point token (see [`TypeSystemRowCounts`]). `method_def_row_count` pre-sizes the
    /// per-method debug-info slots so [`add_method`](Self::add_method) can be called in any order
    /// (mirrors `tables::MetadataBuilder`'s RID-indexed tables).
    #[must_use]
    pub fn new(type_system: TypeSystemRowCounts, method_def_row_count: usize) -> Self {
        Self {
            type_system,
            methods: vec![None; method_def_row_count],
        }
    }

    /// Records the [`MethodSequencePoints`] for the `MethodDef` row identified by `method_token`
    /// (a `Token` with `Token::TABLE_METHOD_DEF`'s table id — see `Token::new`/`Token::TABLE_*` in
    /// `tables.rs`). Panics if `method_token` isn't a `MethodDef` token or is out of range for the
    /// row count passed to [`new`](Self::new) — both indicate a caller bug (a token from the wrong
    /// table, or a row count that didn't match the type-system metadata this PDB describes).
    pub fn add_method(&mut self, method_token: Token, info: MethodSequencePoints) {
        let table = method_token.0 >> 24;
        assert_eq!(
            table,
            Token::TABLE_METHOD_DEF,
            "add_method expects a MethodDef token, got table {table:#x}"
        );
        let rid = (method_token.0 & 0x00FF_FFFF) as usize;
        assert!(
            rid >= 1 && rid <= self.methods.len(),
            "MethodDef rid {rid} out of range for {} declared rows",
            self.methods.len()
        );
        self.methods[rid - 1] = Some(info);
    }

    /// Serializes the accumulated documents/method-debug-info into a standalone Portable PDB file
    /// (BSJB header + `#Pdb`/`#~`/`#Strings`/`#Blob`/`#GUID`/`#US` streams — the `#Pdb` stream
    /// carries the 20-byte PDB id, `EntryPointToken`, and [`TypeSystemRowCounts`] this builder was
    /// constructed with, per `PortablePdb-Metadata.md`'s "PDB Stream" section). Returns the file
    /// bytes and the 20-byte PDB id ([`PdbId`]) [`pe`](super::pe)'s debug directory must embed
    /// (bytes 0..16 as the CodeView RSDS GUID, bytes 16..20 as the age/stamp — see that type's
    /// doc and `docs/PE_EMISSION_PLAN.md`'s "FORMAT SPEC" section).
    ///
    /// # Panics
    /// Always, for now — Phase 2's actual heap/table serialization is not implemented by this
    /// interface-pinning stub.
    #[must_use]
    pub fn build(self) -> (Vec<u8>, PdbId) {
        todo!(
            "Phase 2: serialize the #Pdb stream + Document/MethodDebugInformation tables \
             (dotnet/runtime PortablePdb-Metadata.md); this stub only pins the call shape \
             pe.rs's debug-directory hook and body.rs's sequence-point collector build against."
        )
    }
}

/// The 20-byte Portable PDB content identifier (`PortablePdb-Metadata.md`'s "PDB Stream" `Id`
/// field): bytes `0..16` are a GUID, bytes `16..20` are a 4-byte "timestamp"/age-like value — but
/// per `docs/PE_EMISSION_PLAN.md`'s determinism constraint, THIS writer derives both deterministically
/// from content hashing (no `System.Guid.NewGuid()`/wall-clock — mirrors `tables::deterministic_mvid`'s
/// FNV-1a-of-content approach, `tables.rs:1436-1454`) rather than the reference `Microsoft.
/// CodeAnalysis` implementation's convention (a real random GUID + build timestamp). The same 20
/// bytes are what the PE's CodeView (type 2) Debug Directory entry embeds — GUID in its `Signature`
/// field, the low 4 bytes as `Age`-adjacent stamp bytes — so CoreCLR can match a loaded PDB against
/// its owning image (see [`DebugDirectoryEntry`]'s doc and the FORMAT SPEC section of the plan doc).
pub type PdbId = [u8; 20];

/// Derives a [`PdbId`] deterministically from a PDB's serialized content (everything except the id
/// field itself, mirroring how `deterministic_mvid` hashes the assembly name rather than embedding
/// a random UUID). Two independent FNV-1a passes over `content` fill the 16-byte GUID portion;
/// a third, differently-seeded pass fills the 4-byte stamp portion — all zero timestamps/randomness,
/// so re-exporting byte-identical IR always yields a byte-identical PDB id (required both for
/// reproducible builds and so [`DebugDirectoryEntry::from_pdb_id`] round-trips in tests without
/// faking a clock).
#[must_use]
pub fn deterministic_pdb_id(content: &[u8]) -> PdbId {
    fn fnv1a(seed: u64, data: &[u8]) -> u64 {
        let mut hash = seed;
        for &b in data {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
    let a = fnv1a(0xcbf2_9ce4_8422_2325, content);
    let b = fnv1a(0x9e37_79b9_7f4a_7c15, content);
    let c = fnv1a(0x8422_2325_cbf2_9ce4, content);
    let mut out = [0u8; 20];
    out[0..8].copy_from_slice(&a.to_le_bytes());
    out[8..16].copy_from_slice(&b.to_le_bytes());
    out[16..20].copy_from_slice(&c.to_le_bytes()[0..4]);
    out
}

/// The PE-side hook: a type-2 (`IMAGE_DEBUG_TYPE_CODEVIEW`) Debug Directory entry
/// (§II.25.3.1-adjacent — the Debug Directory isn't in ECMA-335 proper, it's a plain PE/COFF
/// concept CoreCLR repurposes for PDB association) carrying an "RSDS" payload: magic `b"RSDS"`,
/// 16-byte GUID, 4-byte age, then a NUL-terminated path to the PDB file. CoreCLR's loader looks
/// for `<assembly-stem>.pdb` next to the `.dll`/`.exe` first and only falls back to this embedded
/// path, but the entry must still be well-formed for `StackTrace(fNeedFileInfo: true)` to resolve
/// file:line (confirmed live by the Phase-0 probe against an ilasm-produced image; this type pins
/// the shape [`pe::write_pe`](super::pe::write_pe) will need to grow a parameter for).
///
/// This is a **stub signature only** — no writer code in `pe.rs` calls this yet; wiring it into
/// `write_pe`'s layout pass (a new small data directory entry pointing at a Debug Directory table
/// entry placed in `.text`, per this module's doc on `write_pe`'s existing layout-pass shape) is
/// follow-up work, not part of this interface-pinning task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugDirectoryEntry {
    /// Bytes `0..16` of the [`PdbId`] this entry's PDB was built with.
    pub guid: [u8; 16],
    /// The `Age` field (conventionally starts at 1 and increments per PDB rebuild for the same
    /// GUID under the reference toolchain; this writer has no incremental-rebuild concept, so it
    /// is always `1` — content changes produce a new GUID instead, via [`deterministic_pdb_id`]).
    pub age: u32,
    /// NUL-terminated (on write) path/filename CoreCLR's fallback resolver would use — conventionally
    /// just the PDB's bare filename (e.g. `"foo.pdb"`), matching `il_exporter`'s
    /// `pdb_file`/`{output_file_path}.pdb` convention in `cilly/src/bin/linker/main.rs:718-724`.
    pub pdb_path: String,
}

impl DebugDirectoryEntry {
    /// Builds the entry from a [`PdbId`] ([`PdbBuilder::build`]'s second return value) and the PDB
    /// file's on-disk name, splitting the 20 content-hash bytes into the CodeView GUID (bytes
    /// `0..16`) and folding bytes `16..20` into `age` (kept nonzero — `0` is a valid but unusual
    /// `Age`; XORing with `1` keeps this stub's placeholder derivation trivially distinguishable
    /// from "no debug directory" without claiming semantic meaning for the low bits it borrows).
    #[must_use]
    pub fn from_pdb_id(id: PdbId, pdb_path: String) -> Self {
        let mut guid = [0u8; 16];
        guid.copy_from_slice(&id[0..16]);
        let stamp = u32::from_le_bytes([id[16], id[17], id[18], id[19]]);
        Self {
            guid,
            age: stamp | 1,
            pdb_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_pdb_id_is_stable_across_calls() {
        let content = b"pretend-this-is-a-serialized-pdb-blob";
        assert_eq!(
            deterministic_pdb_id(content),
            deterministic_pdb_id(content),
            "same content must hash to the same PDB id every time"
        );
    }

    #[test]
    fn deterministic_pdb_id_differs_for_different_content() {
        assert_ne!(
            deterministic_pdb_id(b"content-a"),
            deterministic_pdb_id(b"content-b")
        );
    }

    #[test]
    fn deterministic_pdb_id_is_never_all_zero() {
        // A real GUID/stamp should be visibly content-derived, not an accidental zero id (which
        // would look identical to "id not set" bugs elsewhere).
        let id = deterministic_pdb_id(b"some assembly content");
        assert_ne!(id, [0u8; 20]);
    }

    #[test]
    fn debug_directory_entry_splits_guid_and_stamp() {
        let mut id = [0u8; 20];
        for (i, b) in id.iter_mut().enumerate() {
            *b = i as u8;
        }
        let entry = DebugDirectoryEntry::from_pdb_id(id, "foo.pdb".to_string());
        assert_eq!(entry.guid, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        assert_eq!(entry.pdb_path, "foo.pdb");
        // stamp bytes are [16,17,18,19] little-endian = 0x13121110; OR 1 keeps it nonzero (it
        // already is here) without masking any bit we care about asserting.
        assert_eq!(entry.age, 0x1312_1110 | 1);
    }

    #[test]
    fn pdb_builder_add_method_rejects_non_method_def_token() {
        let mut builder = PdbBuilder::new(TypeSystemRowCounts::default(), 4);
        let bogus = Token::new(Token::TABLE_TYPE_DEF, 1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            builder.add_method(bogus, MethodSequencePoints::default());
        }));
        assert!(result.is_err(), "expected a panic for a non-MethodDef token");
    }

    #[test]
    fn pdb_builder_add_method_accepts_in_range_method_def_rid() {
        let mut builder = PdbBuilder::new(TypeSystemRowCounts::default(), 2);
        let tok = Token::new(Token::TABLE_METHOD_DEF, 2);
        builder.add_method(
            tok,
            MethodSequencePoints {
                local_signature: None,
                points: vec![SequencePoint {
                    il_offset: 0,
                    document_path: "src/main.rs".to_string(),
                    line: 3,
                    col: 5,
                    end_line: 3,
                    end_col: 10,
                    is_hidden: false,
                }],
            },
        );
        assert_eq!(builder.methods[1].as_ref().unwrap().points.len(), 1);
    }
}
