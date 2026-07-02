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
//! # This module's scope
//!
//! This module serializes standalone Portable PDB bytes for collected method sequence points:
//! the `#Pdb` stream, PDB-only `#~` tables (`Document` and `MethodDebugInformation`), and the
//! independent debug heaps. Sequence-point collection in `body.rs` and PE Debug Directory wiring
//! in `pe.rs` remain separate follow-up integration points; the writer here is metadata-only and
//! does not change emitted IL or execution semantics.

use super::{
    heaps::{BlobHeap, GuidHeap, StringsHeap, UserStringHeap, write_compressed_u32},
    tables::Token,
};
use std::collections::{BTreeMap, HashMap};

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
    /// Panics if a local signature token is not a `StandAloneSig`, if sequence-point coordinates
    /// violate the Portable PDB ranges/order constraints, or if duplicate/invalid type-system row
    /// counts are supplied.
    #[must_use]
    pub fn build(self) -> (Vec<u8>, PdbId) {
        let mut documents = Vec::new();
        let mut document_ids = HashMap::new();
        for method in self.methods.iter().flatten() {
            for point in &method.points {
                intern_document_id(&mut documents, &mut document_ids, &point.document_path);
            }
        }

        let strings = StringsHeap::default();
        let mut blobs = BlobHeap::default();
        let guids = GuidHeap::default();
        let user_strings = UserStringHeap::default();

        let document_rows: Vec<DocumentRow> = documents
            .iter()
            .map(|path| DocumentRow {
                name: encode_document_name(&mut blobs, path),
                // PortablePdb-Metadata.md defines C#, VB, and F# language GUIDs, but no Rust GUID.
                // Emit nil Language/HashAlgorithm and an empty Hash so readers treat the values as
                // intentionally unspecified instead of falsely identifying the source language.
                hash_algorithm: 0,
                hash: 0,
                language: 0,
            })
            .collect();

        let method_rows: Vec<MethodDebugInformationRow> = self
            .methods
            .into_iter()
            .map(|method| {
                let Some(method) = method else {
                    return MethodDebugInformationRow::default();
                };
                if method.points.is_empty() {
                    return MethodDebugInformationRow::default();
                }
                let document = single_document_id(&method.points, &document_ids).unwrap_or(0);
                let sequence_points = encode_sequence_points(&method, &document_ids, document);
                MethodDebugInformationRow {
                    document,
                    sequence_points: blobs.intern(&sequence_points),
                }
            })
            .collect();

        let widths =
            PdbWidths::compute(&strings, &blobs, &guids, &user_strings, document_rows.len());
        let tables_stream = pad4(&serialize_tables(&document_rows, &method_rows, &widths));
        let strings_stream = pad4(strings.as_bytes());
        let user_strings_stream = pad4(user_strings.as_bytes());
        let guid_stream = pad4(guids.as_bytes());
        let blob_stream = pad4(blobs.as_bytes());

        let pdb_stream = pad4(&serialize_pdb_stream([0u8; 20], &self.type_system));
        let (mut bytes, pdb_id_offset) = serialize_standalone_pdb(
            &pdb_stream,
            &tables_stream,
            &strings_stream,
            &user_strings_stream,
            &guid_stream,
            &blob_stream,
        );
        let id = deterministic_pdb_id(&bytes);
        bytes[pdb_id_offset..pdb_id_offset + 20].copy_from_slice(&id);
        (bytes, id)
    }
}

const TABLE_DOCUMENT: u32 = 0x30;
const TABLE_METHOD_DEBUG_INFORMATION: u32 = 0x31;
const HIDDEN_LINE: u32 = 0x00FE_EFEE;

#[derive(Debug, Clone)]
struct DocumentRow {
    name: u32,
    hash_algorithm: u32,
    hash: u32,
    language: u32,
}

#[derive(Debug, Clone, Default)]
struct MethodDebugInformationRow {
    document: u32,
    sequence_points: u32,
}

struct PdbWidths {
    heap_sizes: u8,
    blob_wide: bool,
    guid_wide: bool,
    document_wide: bool,
}

impl PdbWidths {
    fn compute(
        strings: &StringsHeap,
        blobs: &BlobHeap,
        guids: &GuidHeap,
        user_strings: &UserStringHeap,
        document_rows: usize,
    ) -> Self {
        let mut heap_sizes = 0u8;
        if strings.as_bytes().len() > 0xFFFF {
            heap_sizes |= 0x1;
        }
        if guids.as_bytes().len() > 0xFFFF {
            heap_sizes |= 0x2;
        }
        if blobs.as_bytes().len() > 0xFFFF {
            heap_sizes |= 0x4;
        }
        let _us_wide = user_strings.as_bytes().len() > 0xFFFF;
        Self {
            heap_sizes,
            blob_wide: blobs.as_bytes().len() > 0xFFFF,
            guid_wide: guids.as_bytes().len() > 0xFFFF,
            document_wide: document_rows > 0xFFFF,
        }
    }
}

fn intern_document_id(
    documents: &mut Vec<String>,
    ids: &mut HashMap<String, u32>,
    path: &str,
) -> u32 {
    if let Some(&rid) = ids.get(path) {
        return rid;
    }
    let rid = u32::try_from(documents.len() + 1).expect("Document table exceeded u32 rows");
    documents.push(path.to_string());
    ids.insert(path.to_string(), rid);
    rid
}

fn encode_document_name(blobs: &mut BlobHeap, path: &str) -> u32 {
    let part = if path.is_empty() {
        0
    } else {
        blobs.intern(path.as_bytes())
    };
    let mut name = Vec::new();
    // Separator 0 means "empty separator". A single UTF-8 part therefore round-trips every path
    // byte-for-byte without imposing a platform separator choice on mixed `/` and `\` inputs.
    name.push(0);
    write_compressed_u32(&mut name, part);
    blobs.intern(&name)
}

fn single_document_id(points: &[SequencePoint], documents: &HashMap<String, u32>) -> Option<u32> {
    let mut id = None;
    for point in points {
        let point_id = documents[&point.document_path];
        if id.is_some_and(|seen| seen != point_id) {
            return None;
        }
        id = Some(point_id);
    }
    id
}

fn encode_sequence_points(
    method: &MethodSequencePoints,
    documents: &HashMap<String, u32>,
    method_document: u32,
) -> Vec<u8> {
    let mut out = Vec::new();
    let local_signature = match method.local_signature {
        Some(token) => {
            assert_eq!(
                token.table(),
                Token::TABLE_STAND_ALONE_SIG,
                "sequence-point LocalSignature must be a StandAloneSig token"
            );
            token.rid()
        }
        None => 0,
    };
    write_compressed_u32(&mut out, local_signature);

    let first_document = documents[&method.points[0].document_path];
    let mut current_document = if method_document == 0 {
        write_compressed_u32(&mut out, first_document);
        first_document
    } else {
        method_document
    };

    let mut previous_il_offset = None;
    let mut previous_non_hidden = None;
    for point in &method.points {
        let point_document = documents[&point.document_path];
        if previous_il_offset.is_some() && point_document != current_document {
            write_compressed_u32(&mut out, 0);
            write_compressed_u32(&mut out, point_document);
            current_document = point_document;
        }
        encode_sequence_point_record(
            &mut out,
            point,
            &mut previous_il_offset,
            &mut previous_non_hidden,
        );
    }
    out
}

fn encode_sequence_point_record(
    out: &mut Vec<u8>,
    point: &SequencePoint,
    previous_il_offset: &mut Option<u32>,
    previous_non_hidden: &mut Option<(u32, u32)>,
) {
    assert!(
        point.il_offset < 0x2000_0000,
        "sequence-point IL offset out of Portable PDB range: {}",
        point.il_offset
    );
    let il_delta = match *previous_il_offset {
        Some(previous) => {
            assert!(
                point.il_offset > previous,
                "sequence-point IL offsets must be strictly increasing: {} after {}",
                point.il_offset,
                previous
            );
            point.il_offset - previous
        }
        None => point.il_offset,
    };
    write_compressed_u32(out, il_delta);
    *previous_il_offset = Some(point.il_offset);

    if point.is_hidden {
        write_compressed_u32(out, 0);
        write_compressed_u32(out, 0);
        return;
    }

    validate_visible_sequence_point(point);

    let delta_lines = point.end_line - point.line;
    write_compressed_u32(out, delta_lines);
    if delta_lines == 0 {
        write_compressed_u32(out, point.end_col - point.col);
    } else {
        write_compressed_i32(out, point.end_col as i32 - point.col as i32);
    }

    match *previous_non_hidden {
        Some((previous_line, previous_col)) => {
            write_compressed_i32(out, point.line as i32 - previous_line as i32);
            write_compressed_i32(out, point.col as i32 - previous_col as i32);
        }
        None => {
            write_compressed_u32(out, point.line);
            write_compressed_u32(out, point.col);
        }
    }
    *previous_non_hidden = Some((point.line, point.col));
}

fn validate_visible_sequence_point(point: &SequencePoint) {
    assert!(
        point.line < 0x2000_0000 && point.end_line < 0x2000_0000,
        "sequence-point lines out of Portable PDB range: {}..{}",
        point.line,
        point.end_line
    );
    assert!(
        point.line != HIDDEN_LINE && point.end_line != HIDDEN_LINE,
        "visible sequence point uses the Portable PDB hidden line marker"
    );
    assert!(
        point.col < 0x1_0000 && point.end_col < 0x1_0000,
        "sequence-point columns out of Portable PDB range: {}..{}",
        point.col,
        point.end_col
    );
    assert!(
        point.end_line >= point.line,
        "sequence-point end line precedes start line: {} < {}",
        point.end_line,
        point.line
    );
    if point.end_line == point.line {
        assert!(
            point.end_col > point.col,
            "same-line sequence point must have end column greater than start column"
        );
    }
}

fn write_compressed_i32(out: &mut Vec<u8>, value: i32) {
    assert!(
        (-(1 << 28)..=(1 << 28) - 1).contains(&value),
        "signed compressed integer out of range: {value}"
    );
    let sign = u32::from(value < 0);
    if (-64..=63).contains(&value) {
        let encoded = (((value as u32) & 0x3F) << 1) | sign;
        out.push(encoded as u8);
    } else if (-8192..=8191).contains(&value) {
        let encoded = (((value as u32) & 0x1FFF) << 1) | sign;
        out.extend_from_slice(&[(0x80 | (encoded >> 8)) as u8, encoded as u8]);
    } else {
        let encoded = (((value as u32) & 0x0FFF_FFFF) << 1) | sign;
        out.extend_from_slice(&[
            (0xC0 | (encoded >> 24)) as u8,
            (encoded >> 16) as u8,
            (encoded >> 8) as u8,
            encoded as u8,
        ]);
    }
}

fn serialize_pdb_stream(id: PdbId, type_system: &TypeSystemRowCounts) -> Vec<u8> {
    let rows = normalized_type_system_rows(type_system);
    let mut mask = 0u64;
    for &(table, _) in &rows {
        assert!(
            table < 64,
            "type-system table id {table:#x} does not fit the #Pdb mask"
        );
        mask |= 1u64 << table;
    }

    let mut out = Vec::with_capacity(32 + rows.len() * 4);
    out.extend_from_slice(&id);
    out.extend_from_slice(&type_system.entry_point_token.to_le_bytes());
    out.extend_from_slice(&mask.to_le_bytes());
    for &(_, count) in &rows {
        out.extend_from_slice(&count.to_le_bytes());
    }
    out
}

fn normalized_type_system_rows(type_system: &TypeSystemRowCounts) -> Vec<(u32, u32)> {
    let mut rows = BTreeMap::new();
    for &(table, count) in &type_system.rows {
        if count == 0 {
            continue;
        }
        assert!(
            rows.insert(table, count).is_none(),
            "duplicate type-system row count for table {table:#x}"
        );
    }
    rows.into_iter().collect()
}

fn serialize_tables(
    documents: &[DocumentRow],
    methods: &[MethodDebugInformationRow],
    widths: &PdbWidths,
) -> Vec<u8> {
    let mut valid = 0u64;
    if !documents.is_empty() {
        valid |= 1u64 << TABLE_DOCUMENT;
    }
    if !methods.is_empty() {
        valid |= 1u64 << TABLE_METHOD_DEBUG_INFORMATION;
    }

    let mut out = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(2);
    out.push(0);
    out.push(widths.heap_sizes);
    out.push(1);
    out.extend_from_slice(&valid.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    for table in 0..64u32 {
        if valid & (1u64 << table) != 0 {
            let count = match table {
                TABLE_DOCUMENT => documents.len(),
                TABLE_METHOD_DEBUG_INFORMATION => methods.len(),
                _ => unreachable!(),
            };
            out.extend_from_slice(&(count as u32).to_le_bytes());
        }
    }

    for row in documents {
        write_heap_idx(&mut out, row.name, widths.blob_wide);
        write_heap_idx(&mut out, row.hash_algorithm, widths.guid_wide);
        write_heap_idx(&mut out, row.hash, widths.blob_wide);
        write_heap_idx(&mut out, row.language, widths.guid_wide);
    }
    for row in methods {
        write_heap_idx(&mut out, row.document, widths.document_wide);
        write_heap_idx(&mut out, row.sequence_points, widths.blob_wide);
    }
    out
}

fn write_heap_idx(out: &mut Vec<u8>, value: u32, wide: bool) {
    if wide {
        out.extend_from_slice(&value.to_le_bytes());
    } else {
        out.extend_from_slice(&(value as u16).to_le_bytes());
    }
}

fn serialize_standalone_pdb(
    pdb_stream: &[u8],
    tables_stream: &[u8],
    strings_stream: &[u8],
    user_strings_stream: &[u8],
    guid_stream: &[u8],
    blob_stream: &[u8],
) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    out.extend_from_slice(b"BSJB");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    const VERSION: &str = "v4.0.30319";
    let mut version_bytes = VERSION.as_bytes().to_vec();
    version_bytes.push(0);
    while version_bytes.len() % 4 != 0 {
        version_bytes.push(0);
    }
    out.extend_from_slice(&(version_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&version_bytes);

    let streams = [
        ("#Pdb", pdb_stream),
        ("#~", tables_stream),
        ("#Strings", strings_stream),
        ("#US", user_strings_stream),
        ("#GUID", guid_stream),
        ("#Blob", blob_stream),
    ];
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(streams.len() as u16).to_le_bytes());

    let header_names: Vec<Vec<u8>> = streams
        .iter()
        .map(|(name, _)| {
            let mut bytes = name.as_bytes().to_vec();
            bytes.push(0);
            while bytes.len() % 4 != 0 {
                bytes.push(0);
            }
            bytes
        })
        .collect();
    let header_total_len: usize = header_names.iter().map(|name| 8 + name.len()).sum();
    let mut running = out.len() + header_total_len;
    let mut pdb_id_offset = None;
    for ((name, bytes), name_bytes) in streams.iter().zip(&header_names) {
        if *name == "#Pdb" {
            pdb_id_offset = Some(running);
        }
        out.extend_from_slice(&(running as u32).to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(name_bytes);
        running += bytes.len();
    }
    for (_, bytes) in &streams {
        out.extend_from_slice(bytes);
    }
    (out, pdb_id_offset.expect("#Pdb stream is always present"))
}

fn pad4(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
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
    use std::collections::HashMap;

    #[derive(Debug)]
    struct TestPdbReader<'a> {
        bytes: &'a [u8],
        streams: HashMap<String, (usize, usize)>,
    }

    #[derive(Debug)]
    struct TestTablesHeader {
        heap_sizes: u8,
        valid: u64,
        counts: Vec<(u32, usize)>,
        row_data_offset: usize,
    }

    #[derive(Debug, Clone, Copy)]
    struct TestMethodDebugInformationRow {
        document: u32,
        sequence_points: u32,
    }

    impl<'a> TestPdbReader<'a> {
        fn parse(bytes: &'a [u8]) -> Self {
            assert_eq!(&bytes[0..4], b"BSJB");
            assert_eq!(u16::from_le_bytes(bytes[4..6].try_into().unwrap()), 1);
            assert_eq!(u16::from_le_bytes(bytes[6..8].try_into().unwrap()), 1);
            let version_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            let version = &bytes[16..16 + version_len];
            let nul = version.iter().position(|&b| b == 0).unwrap();
            assert_eq!(&version[..nul], b"v4.0.30319");

            let mut cursor = 16 + version_len;
            assert_eq!(
                u16::from_le_bytes(bytes[cursor..cursor + 2].try_into().unwrap()),
                0
            );
            cursor += 2;
            let stream_count = u16::from_le_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
            cursor += 2;

            let mut streams = HashMap::new();
            for _ in 0..stream_count {
                let offset =
                    u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
                let size =
                    u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
                cursor += 8;
                let name_start = cursor;
                let name_end =
                    bytes[name_start..].iter().position(|&b| b == 0).unwrap() + name_start;
                let name = std::str::from_utf8(&bytes[name_start..name_end])
                    .unwrap()
                    .to_string();
                let mut name_len = name_end - name_start + 1;
                while name_len % 4 != 0 {
                    name_len += 1;
                }
                cursor = name_start + name_len;
                assert_eq!(offset % 4, 0, "stream {name} offset must be 4-byte aligned");
                assert_eq!(size % 4, 0, "stream {name} size must be padded to 4 bytes");
                streams.insert(name, (offset, size));
            }
            Self { bytes, streams }
        }

        fn stream(&self, name: &str) -> &'a [u8] {
            let (offset, size) = self.streams[name];
            &self.bytes[offset..offset + size]
        }

        fn stream_offset(&self, name: &str) -> usize {
            self.streams[name].0
        }

        fn pdb_id(&self) -> PdbId {
            self.stream("#Pdb")[0..20].try_into().unwrap()
        }

        fn pdb_stream_rows(&self) -> (u32, Vec<(u32, u32)>) {
            let pdb = self.stream("#Pdb");
            let entry_point = u32::from_le_bytes(pdb[20..24].try_into().unwrap());
            let mask = u64::from_le_bytes(pdb[24..32].try_into().unwrap());
            let mut cursor = 32;
            let mut rows = Vec::new();
            for table in 0..64u32 {
                if mask & (1u64 << table) != 0 {
                    let count = u32::from_le_bytes(pdb[cursor..cursor + 4].try_into().unwrap());
                    rows.push((table, count));
                    cursor += 4;
                }
            }
            (entry_point, rows)
        }

        fn tables_header(&self) -> TestTablesHeader {
            let tables = self.stream("#~");
            assert_eq!(u32::from_le_bytes(tables[0..4].try_into().unwrap()), 0);
            assert_eq!(tables[4], 2);
            assert_eq!(tables[5], 0);
            assert_eq!(tables[7], 1);
            let heap_sizes = tables[6];
            let valid = u64::from_le_bytes(tables[8..16].try_into().unwrap());
            let mut cursor = 24;
            let mut counts = Vec::new();
            for table in 0..64u32 {
                if valid & (1u64 << table) != 0 {
                    let count =
                        u32::from_le_bytes(tables[cursor..cursor + 4].try_into().unwrap()) as usize;
                    counts.push((table, count));
                    cursor += 4;
                }
            }
            TestTablesHeader {
                heap_sizes,
                valid,
                counts,
                row_data_offset: cursor,
            }
        }

        fn table_row_count(header: &TestTablesHeader, table: u32) -> usize {
            header
                .counts
                .iter()
                .find(|&&(id, _)| id == table)
                .map_or(0, |&(_, count)| count)
        }

        fn row_width(table: u32, header: &TestTablesHeader) -> usize {
            let index = |wide| if wide { 4 } else { 2 };
            let blob = index(header.heap_sizes & 0x4 != 0);
            let guid = index(header.heap_sizes & 0x2 != 0);
            match table {
                TABLE_DOCUMENT => blob + guid + blob + guid,
                TABLE_METHOD_DEBUG_INFORMATION => {
                    let doc_rows = Self::table_row_count(header, TABLE_DOCUMENT);
                    index(doc_rows > 0xFFFF) + blob
                }
                other => panic!("unexpected test PDB table {other:#x}"),
            }
        }

        fn table_offset(&self, table: u32, header: &TestTablesHeader) -> usize {
            let mut offset = 0;
            for &(id, count) in &header.counts {
                if id == table {
                    return offset;
                }
                offset += count * Self::row_width(id, header);
            }
            panic!("table {table:#x} is not present");
        }

        fn read_index(bytes: &[u8], cursor: &mut usize, wide: bool) -> u32 {
            if wide {
                let value = u32::from_le_bytes(bytes[*cursor..*cursor + 4].try_into().unwrap());
                *cursor += 4;
                value
            } else {
                let value = u16::from_le_bytes(bytes[*cursor..*cursor + 2].try_into().unwrap());
                *cursor += 2;
                u32::from(value)
            }
        }

        fn blob_at(&self, offset: u32) -> &'a [u8] {
            assert_ne!(offset, 0, "offset 0 is the nil/empty blob");
            let blob = self.stream("#Blob");
            let mut cursor = offset as usize;
            let len = read_compressed_u32_from(blob, &mut cursor) as usize;
            &blob[cursor..cursor + len]
        }

        fn documents(&self) -> Vec<String> {
            let header = self.tables_header();
            let count = Self::table_row_count(&header, TABLE_DOCUMENT);
            let mut cursor = header.row_data_offset + self.table_offset(TABLE_DOCUMENT, &header);
            let tables = self.stream("#~");
            let blob_wide = header.heap_sizes & 0x4 != 0;
            let guid_wide = header.heap_sizes & 0x2 != 0;
            let mut docs = Vec::new();
            for _ in 0..count {
                let name = Self::read_index(tables, &mut cursor, blob_wide);
                let _hash_algorithm = Self::read_index(tables, &mut cursor, guid_wide);
                let _hash = Self::read_index(tables, &mut cursor, blob_wide);
                let _language = Self::read_index(tables, &mut cursor, guid_wide);
                docs.push(self.document_name(name));
            }
            docs
        }

        fn document_name(&self, name_offset: u32) -> String {
            let name = self.blob_at(name_offset);
            let mut cursor = 0;
            let separator = match name[0] {
                0 => {
                    cursor += 1;
                    String::new()
                }
                _ => {
                    let text = std::str::from_utf8(name).unwrap();
                    let ch = text.chars().next().unwrap();
                    cursor += ch.len_utf8();
                    ch.to_string()
                }
            };
            let mut parts = Vec::new();
            while cursor < name.len() {
                let part_offset = read_compressed_u32_from(name, &mut cursor);
                if part_offset == 0 {
                    parts.push(String::new());
                } else {
                    parts.push(
                        std::str::from_utf8(self.blob_at(part_offset))
                            .unwrap()
                            .to_string(),
                    );
                }
            }
            parts.join(&separator)
        }

        fn method_rows(&self) -> Vec<TestMethodDebugInformationRow> {
            let header = self.tables_header();
            let count = Self::table_row_count(&header, TABLE_METHOD_DEBUG_INFORMATION);
            let mut cursor =
                header.row_data_offset + self.table_offset(TABLE_METHOD_DEBUG_INFORMATION, &header);
            let tables = self.stream("#~");
            let document_wide = Self::table_row_count(&header, TABLE_DOCUMENT) > 0xFFFF;
            let blob_wide = header.heap_sizes & 0x4 != 0;
            let mut rows = Vec::new();
            for _ in 0..count {
                rows.push(TestMethodDebugInformationRow {
                    document: Self::read_index(tables, &mut cursor, document_wide),
                    sequence_points: Self::read_index(tables, &mut cursor, blob_wide),
                });
            }
            rows
        }

        fn decode_sequence_points(
            &self,
            row: TestMethodDebugInformationRow,
            documents: &[String],
        ) -> (u32, Vec<SequencePoint>) {
            if row.sequence_points == 0 {
                return (0, Vec::new());
            }
            let blob = self.blob_at(row.sequence_points);
            let mut cursor = 0;
            let local_signature = read_compressed_u32_from(blob, &mut cursor);
            let mut current_document = if row.document == 0 {
                read_compressed_u32_from(blob, &mut cursor)
            } else {
                row.document
            };
            let mut first_sequence_record = true;
            let mut previous_il = None;
            let mut previous_non_hidden = None;
            let mut points = Vec::new();
            while cursor < blob.len() {
                let il_delta = read_compressed_u32_from(blob, &mut cursor);
                if !first_sequence_record && il_delta == 0 {
                    current_document = read_compressed_u32_from(blob, &mut cursor);
                    continue;
                }
                let il_offset = match previous_il {
                    Some(previous) => previous + il_delta,
                    None => il_delta,
                };
                previous_il = Some(il_offset);
                first_sequence_record = false;

                let delta_lines = read_compressed_u32_from(blob, &mut cursor);
                let delta_columns_or_hidden = if delta_lines == 0 {
                    read_compressed_u32_from(blob, &mut cursor) as i32
                } else {
                    read_compressed_i32_from(blob, &mut cursor)
                };

                let document_path = documents[(current_document - 1) as usize].clone();
                if delta_lines == 0 && delta_columns_or_hidden == 0 {
                    points.push(SequencePoint {
                        il_offset,
                        document_path,
                        line: HIDDEN_LINE,
                        col: 0,
                        end_line: HIDDEN_LINE,
                        end_col: 0,
                        is_hidden: true,
                    });
                    continue;
                }

                let (line, col) = match previous_non_hidden {
                    Some((previous_line, previous_col)) => {
                        let line_delta = read_compressed_i32_from(blob, &mut cursor);
                        let col_delta = read_compressed_i32_from(blob, &mut cursor);
                        (
                            (previous_line as i32 + line_delta) as u32,
                            (previous_col as i32 + col_delta) as u32,
                        )
                    }
                    None => (
                        read_compressed_u32_from(blob, &mut cursor),
                        read_compressed_u32_from(blob, &mut cursor),
                    ),
                };
                let end_line = line + delta_lines;
                let end_col = if delta_lines == 0 {
                    col + delta_columns_or_hidden as u32
                } else {
                    (col as i32 + delta_columns_or_hidden) as u32
                };
                previous_non_hidden = Some((line, col));
                points.push(SequencePoint {
                    il_offset,
                    document_path,
                    line,
                    col,
                    end_line,
                    end_col,
                    is_hidden: false,
                });
            }
            (local_signature, points)
        }
    }

    fn read_compressed_u32_from(bytes: &[u8], cursor: &mut usize) -> u32 {
        let first = bytes[*cursor];
        *cursor += 1;
        if first & 0x80 == 0 {
            return u32::from(first);
        }
        if first & 0xC0 == 0x80 {
            let second = bytes[*cursor];
            *cursor += 1;
            return (u32::from(first & 0x3F) << 8) | u32::from(second);
        }
        assert_eq!(first & 0xE0, 0xC0);
        let b1 = bytes[*cursor];
        let b2 = bytes[*cursor + 1];
        let b3 = bytes[*cursor + 2];
        *cursor += 3;
        (u32::from(first & 0x1F) << 24)
            | (u32::from(b1) << 16)
            | (u32::from(b2) << 8)
            | u32::from(b3)
    }

    fn read_compressed_i32_from(bytes: &[u8], cursor: &mut usize) -> i32 {
        let first = bytes[*cursor];
        let payload_bits = if first & 0x80 == 0 {
            6
        } else if first & 0xC0 == 0x80 {
            13
        } else {
            28
        };
        let raw = read_compressed_u32_from(bytes, cursor);
        let payload = (raw >> 1) as i32;
        if raw & 1 == 0 {
            payload
        } else {
            payload - (1 << payload_bits)
        }
    }

    fn sp(
        il_offset: u32,
        document_path: &str,
        line: u32,
        col: u32,
        end_line: u32,
        end_col: u32,
    ) -> SequencePoint {
        SequencePoint {
            il_offset,
            document_path: document_path.to_string(),
            line,
            col,
            end_line,
            end_col,
            is_hidden: false,
        }
    }

    fn hidden(il_offset: u32, document_path: &str) -> SequencePoint {
        SequencePoint {
            il_offset,
            document_path: document_path.to_string(),
            line: HIDDEN_LINE,
            col: 0,
            end_line: HIDDEN_LINE,
            end_col: 0,
            is_hidden: true,
        }
    }

    fn two_doc_three_method_fixture() -> (PdbBuilder, Vec<Vec<SequencePoint>>) {
        let type_system = TypeSystemRowCounts {
            rows: vec![
                (Token::TABLE_METHOD_DEF, 3),
                (Token::TABLE_STAND_ALONE_SIG, 1),
            ],
            entry_point_token: Token::new(Token::TABLE_METHOD_DEF, 1).0,
        };
        let mut builder = PdbBuilder::new(type_system, 3);
        let method1 = vec![
            sp(0, "src/main.rs", 10, 2, 10, 7),
            sp(4, "src/main.rs", 11, 1, 12, 3),
            hidden(8, "src/lib.rs"),
            sp(12, "src/lib.rs", 20, 4, 20, 9),
        ];
        let method3 = vec![sp(0, "src/lib.rs", 30, 1, 30, 5)];
        builder.add_method(
            Token::new(Token::TABLE_METHOD_DEF, 1),
            MethodSequencePoints {
                local_signature: Some(Token::new(Token::TABLE_STAND_ALONE_SIG, 1)),
                points: method1.clone(),
            },
        );
        builder.add_method(
            Token::new(Token::TABLE_METHOD_DEF, 3),
            MethodSequencePoints {
                local_signature: None,
                points: method3.clone(),
            },
        );
        (builder, vec![method1, Vec::new(), method3])
    }

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
        assert_eq!(
            entry.guid,
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
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
        assert!(
            result.is_err(),
            "expected a panic for a non-MethodDef token"
        );
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

    #[test]
    fn build_serializes_two_documents_and_three_method_debug_rows() {
        let (builder, expected) = two_doc_three_method_fixture();
        let (bytes, id) = builder.build();
        let reader = TestPdbReader::parse(&bytes);

        assert_eq!(reader.pdb_id(), id);
        let (entry_point, type_system_rows) = reader.pdb_stream_rows();
        assert_eq!(entry_point, Token::new(Token::TABLE_METHOD_DEF, 1).0);
        assert_eq!(
            type_system_rows,
            vec![
                (Token::TABLE_METHOD_DEF, 3),
                (Token::TABLE_STAND_ALONE_SIG, 1),
            ]
        );

        let header = reader.tables_header();
        assert_eq!(
            header.valid,
            (1u64 << TABLE_DOCUMENT) | (1u64 << TABLE_METHOD_DEBUG_INFORMATION)
        );
        let documents = reader.documents();
        assert_eq!(
            documents,
            vec!["src/main.rs".to_string(), "src/lib.rs".to_string()]
        );

        let method_rows = reader.method_rows();
        assert_eq!(method_rows.len(), 3);
        assert_eq!(
            method_rows[0].document, 0,
            "multi-document method uses InitialDocument"
        );
        assert_eq!(
            method_rows[1].sequence_points, 0,
            "method without points uses the empty blob"
        );
        assert_eq!(
            method_rows[2].document, 2,
            "single-document method stores its Document row id"
        );

        let (local_sig1, points1) = reader.decode_sequence_points(method_rows[0], &documents);
        assert_eq!(local_sig1, 1);
        assert_eq!(points1, expected[0]);
        assert_eq!(
            reader.decode_sequence_points(method_rows[1], &documents).1,
            expected[1]
        );
        let (local_sig3, points3) = reader.decode_sequence_points(method_rows[2], &documents);
        assert_eq!(local_sig3, 0);
        assert_eq!(points3, expected[2]);
    }

    #[test]
    fn sequence_point_delta_encoding_handles_boundaries_and_document_switches() {
        assert_eq!(compressed_i32_bytes(-64), [0x01]);
        assert_eq!(compressed_i32_bytes(-1), [0x7F]);
        assert_eq!(compressed_i32_bytes(0), [0x00]);
        assert_eq!(compressed_i32_bytes(63), [0x7E]);
        assert_eq!(compressed_i32_bytes(64), [0x80, 0x80]);
        assert_eq!(compressed_i32_bytes(-65), [0xBF, 0x7F]);
        assert_eq!(compressed_i32_bytes(-8192), [0x80, 0x01]);

        let points = vec![
            sp(0, "a.rs", 1, 1, 1, 2),
            sp(1, "a.rs", 1, 2, 1, 3),
            hidden(2, "a.rs"),
            sp(3, "b.rs", 2, 10, 3, 4),
            sp(4, "a.rs", 1, 8, 2, 5),
        ];
        let mut builder = PdbBuilder::new(
            TypeSystemRowCounts {
                rows: vec![(Token::TABLE_METHOD_DEF, 1)],
                entry_point_token: 0,
            },
            1,
        );
        builder.add_method(
            Token::new(Token::TABLE_METHOD_DEF, 1),
            MethodSequencePoints {
                local_signature: None,
                points: points.clone(),
            },
        );
        let (bytes, _) = builder.build();
        let reader = TestPdbReader::parse(&bytes);
        let documents = reader.documents();
        assert_eq!(documents, vec!["a.rs".to_string(), "b.rs".to_string()]);
        let rows = reader.method_rows();
        assert_eq!(
            rows[0].document, 0,
            "document switches force MethodDebugInformation.Document nil"
        );
        let (_, decoded) = reader.decode_sequence_points(rows[0], &documents);
        assert_eq!(decoded, points);
    }

    fn compressed_i32_bytes(value: i32) -> Vec<u8> {
        let mut out = Vec::new();
        write_compressed_i32(&mut out, value);
        let mut cursor = 0;
        assert_eq!(read_compressed_i32_from(&out, &mut cursor), value);
        assert_eq!(cursor, out.len());
        out
    }

    #[test]
    fn pdb_build_is_deterministic_including_pdb_id() {
        let (builder_a, _) = two_doc_three_method_fixture();
        let (bytes_a, id_a) = builder_a.build();
        let (builder_b, _) = two_doc_three_method_fixture();
        let (bytes_b, id_b) = builder_b.build();

        assert_eq!(id_a, id_b);
        assert_eq!(bytes_a, bytes_b);
        let reader = TestPdbReader::parse(&bytes_a);
        assert_eq!(reader.pdb_id(), id_a);
        assert_eq!(
            &bytes_a[reader.stream_offset("#Pdb")..reader.stream_offset("#Pdb") + 20],
            &id_a
        );
    }
}
