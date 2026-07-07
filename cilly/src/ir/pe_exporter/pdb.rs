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
//! * **(a) missing frames** — **CLOSED** (fractal-rs perf investigation, 2026-07): `il_exporter`'s
//!   `aggressiveinlining` heuristic lets RyuJIT inline tiny leaf helpers (and, as a side effect,
//!   user `#[inline(never)]` frames) out of managed stack traces. This was confirmed NOT ported to
//!   the direct-PE path and, worse, a measurable perf gap: `MethodDefRow.impl_flags`
//!   (`tables.rs:153,745-758`) only ever set the pinvoke-impl bit (`0x80`), so under `DIRECT_PE=1`
//!   RyuJIT never got the `AggressiveInlining` (`0x100`) hint for e.g. the saturating
//!   float->int `cast_f64_*` helpers (`cilly::ir::builtins::casts::insert_casts`) a hot per-pixel
//!   Mandelbrot kernel calls 3x/pixel. `add_method` now takes an `aggressive_inline: bool`
//!   (`export.rs` computes it) and ORs `0x100` into `impl_flags` — see
//!   `add_method_aggressive_inline_*` in `tables.rs`'s test module. The heuristic itself now lives
//!   in one place, `MethodImpl::should_hint_aggressive_inline` (`ir/method.rs`), shared by BOTH
//!   exporters so they can't drift again, and was WIDENED beyond the original single-block
//!   requirement: it now also hints small, branchy-but-loop-free, call-free multi-block leaves
//!   (<=8 blocks, <=24 roots total, no handler, no internal Call/CallI anywhere) — exactly the
//!   shape the `cast_f64_*` helpers have (a NaN-check block + 3 single-`Ret` blocks), empirically
//!   confirmed to get RyuJIT to actually inline them (a standalone repro showed the helper's
//!   `fcvtzu` emitted inline at each call site instead of 3 `blr` indirect calls per escaping
//!   pixel). Still true: this reintroduces the same missing-frames tradeoff `il_exporter` already
//!   accepts (suppressed via `PDB_FRAMES=1`, wired identically here), so nothing about the
//!   PDB-quality bar itself changed — the parity gap is just closed, and the hint now reaches
//!   the actual hot-path helpers it was meant for.
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
    /// This method's locals in `LocalVarSig` declaration order (index == slot index), resolved to
    /// owned names (`None` = unnamed/compiler-generated temporary) — the source
    /// [`PdbBuilder::build`]'s `LocalScope`/`LocalVariable` (0x32/0x33) row emission reads from.
    /// Mirrors `body::AssembledBody::locals` (see that field's doc for why names are resolved to
    /// owned `String`s upstream rather than carried as raw `Interned<IString>` handles here — a
    /// `PdbBuilder` has no `Assembly` reference to resolve them with, exactly like
    /// [`SequencePoint::document_path`]).
    pub locals: Vec<Option<String>>,
    /// The method body's pure IL code length in bytes (`body::AssembledBody::code_len`) — used as
    /// [`PdbBuilder::build`]'s `LocalScope.Length`, covering the whole method as one flat scope.
    pub code_len: u32,
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
    ///
    /// `info.points` may contain multiple entries at the SAME `il_offset` — `body.rs`'s linearizer
    /// deliberately does not dedupe (parity bar: it visits every `CILRoot::SourceFileInfo` root
    /// exactly like `il_exporter`'s unconditional `.line` emission, and consecutive
    /// `SourceFileInfo` roots with no instruction between them — e.g. an inlined call's span
    /// immediately followed by another span before any code is emitted — are a legitimate MIR
    /// shape). The Portable PDB spec requires STRICTLY increasing IL offsets within one method's
    /// sequence-points blob (`encode_sequence_point_record` asserts this), so this method collapses
    /// any run of same-offset points down to the LAST one before storing — matching the intuitive
    /// "last `.line` directive before an instruction wins" semantics ilasm's own PDB writer applies
    /// silently, and keeping [`SequencePoint::il_offset`] strictly increasing is this module's
    /// responsibility to enforce, not `body.rs`'s (which stays a pure, undeduped mirror of the
    /// oracle exporter).
    pub fn add_method(&mut self, method_token: Token, mut info: MethodSequencePoints) {
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
        info.points = dedupe_same_offset_points(info.points);
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

        let mut strings = StringsHeap::default();
        let mut blobs = BlobHeap::default();
        let guids = GuidHeap::default();
        let user_strings = UserStringHeap::default();

        // Nil Language/HashAlgorithm/Hash: `PortablePdb-Metadata.md` defines C#/VB/F# language
        // GUIDs but no Rust one, and no hash algorithm is computed for a Document's source content
        // here. Empirically confirmed NOT to block runtime symbol resolution: the real Phase-2
        // acceptance blocker (see `pe.rs`'s `PORTABLE_CODEVIEW_MAJOR_VERSION`/
        // `DebugDirectoryEntry::stamp` docs) was the PE-side CodeView entry's version marker and
        // `TimeDateStamp` fields, not anything in this table — a from-scratch test that stamped a
        // real (C#) language GUID here resolved file:line identically to nil, isolating this as
        // genuinely optional in practice, matching the spec's own "optional" framing for these
        // three columns.
        let document_rows: Vec<DocumentRow> = documents
            .iter()
            .map(|path| DocumentRow {
                name: encode_document_name(&mut blobs, path),
                hash_algorithm: 0,
                hash: 0,
                language: 0,
            })
            .collect();

        // `LocalScope`/`LocalVariable` (0x32/0x33) are built in the SAME RID-order pass as
        // `MethodDebugInformation`, since both are keyed by `MethodDef` row and iterate
        // `self.methods` identically (see `PdbBuilder::build`'s doc on why a method with named
        // locals but no sequence points must still get a `LocalScope` row here). Built via an
        // explicit loop rather than `.map()`/`.collect()` because a `LocalScope` row's
        // `variable_list` must capture `local_variable_rows.len() + 1` (the owned-range
        // run-start, mirroring `tables::MetadataBuilder::add_type_def`'s `field_list`/
        // `method_list` pattern exactly) BEFORE this method's own `LocalVariable` rows are
        // pushed — an inherently stateful, order-dependent step `.map()` can't express cleanly.
        let mut method_rows: Vec<MethodDebugInformationRow> = Vec::with_capacity(self.methods.len());
        let mut local_scope_rows: Vec<LocalScopeRow> = Vec::new();
        let mut local_variable_rows: Vec<LocalVariableRow> = Vec::new();
        for (idx, method) in self.methods.into_iter().enumerate() {
            let rid = u32::try_from(idx + 1).expect("MethodDef rid exceeds u32");
            let Some(method) = method else {
                method_rows.push(MethodDebugInformationRow::default());
                continue;
            };

            if method.locals.iter().any(Option::is_some) {
                let variable_list = u32::try_from(local_variable_rows.len() + 1)
                    .expect("LocalVariable table exceeded u32 rows");
                local_scope_rows.push(LocalScopeRow {
                    method: rid,
                    import_scope: 0,
                    variable_list,
                    // `LocalConstant` (0x34) is never populated by this pass — always stamp the
                    // current (permanently empty) cursor, same "zero-owned-rows still stamps the
                    // run boundary" rule `add_type_def` documents for a class with zero fields.
                    constant_list: 1,
                    start_offset: 0,
                    length: method.code_len,
                });
                for (slot, name) in method.locals.iter().enumerate() {
                    let Some(name) = name else { continue };
                    local_variable_rows.push(LocalVariableRow {
                        attributes: 0,
                        index: u16::try_from(slot).expect("local slot index exceeds u16"),
                        name: strings.intern(name),
                    });
                }
            }

            if method.points.is_empty() {
                method_rows.push(MethodDebugInformationRow::default());
                continue;
            }
            let document = single_document_id(&method.points, &document_ids).unwrap_or(0);
            let sequence_points = encode_sequence_points(&method, &document_ids, document);
            method_rows.push(MethodDebugInformationRow {
                document,
                sequence_points: blobs.intern(&sequence_points),
            });
        }

        let method_def_row_count = self
            .type_system
            .rows
            .iter()
            .find(|&&(table, _)| table == Token::TABLE_METHOD_DEF)
            .map_or(0, |&(_, count)| count);
        let widths = PdbWidths::compute(
            &strings,
            &blobs,
            &guids,
            &user_strings,
            document_rows.len(),
            method_def_row_count,
            local_variable_rows.len(),
        );
        let tables_stream = pad4(&serialize_tables(
            &document_rows,
            &method_rows,
            &local_scope_rows,
            &local_variable_rows,
            &widths,
        ));
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

/// Collapses consecutive [`SequencePoint`]s sharing the same [`SequencePoint::il_offset`] down to
/// the LAST one in each run — see [`PdbBuilder::add_method`]'s doc for why this is necessary (the
/// Portable PDB spec requires strictly increasing offsets) and why "last wins" is the right rule
/// (mirrors the intuitive reading of ilasm's own `.line`-directive-per-offset collapsing). Assumes
/// `points` is already offset-sorted (non-decreasing) — true for every real caller, since
/// `body.rs`'s linearizer visits roots in code-offset order — but does not itself require strict
/// sortedness beyond non-decreasing runs; a later, smaller offset after a larger one is a caller
/// bug this function does not attempt to paper over (the subsequent strictly-increasing assert in
/// `encode_sequence_point_record` will catch it).
fn dedupe_same_offset_points(points: Vec<SequencePoint>) -> Vec<SequencePoint> {
    let mut out: Vec<SequencePoint> = Vec::with_capacity(points.len());
    for point in points {
        if let Some(last) = out.last_mut() {
            if last.il_offset == point.il_offset {
                *last = point;
                continue;
            }
        }
        out.push(point);
    }
    out
}

const TABLE_DOCUMENT: u32 = 0x30;
const TABLE_METHOD_DEBUG_INFORMATION: u32 = 0x31;
const TABLE_LOCAL_SCOPE: u32 = 0x32;
const TABLE_LOCAL_VARIABLE: u32 = 0x33;
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

/// `LocalScope` (0x32, `PortablePdb-Metadata.md`): a run of `LocalVariable`/`LocalConstant` rows
/// owned by one `MethodDef`, plus the IL range they're in scope for. This writer only ever emits
/// ONE scope per method (the whole body, `start_offset = 0` / `length = code_len`) — nested
/// block-scoped locals are a valid future refinement, not implemented here (see [`PdbBuilder::build`]'s
/// doc on this table).
#[derive(Debug, Clone)]
struct LocalScopeRow {
    /// Plain (non-coded) `MethodDef` RID — sized by the TYPE-SYSTEM assembly's own `MethodDef` row
    /// count (`PdbWidths::method_def_wide`), since `MethodDef` isn't a table this standalone PDB
    /// owns rows for itself.
    method: u32,
    /// Index into `ImportScope` (0x35) — always 0 (nil): this writer never populates that table.
    import_scope: u32,
    /// 1-based run-start index into [`LocalVariableRow`] — the owned-range pattern mirrors
    /// `tables::MetadataBuilder::add_type_def`'s `field_list`/`method_list` columns exactly (see
    /// that function's doc): the run's end is implicit, either the NEXT `LocalScope` row's
    /// `variable_list`, or the end of the `LocalVariable` table for the last row.
    variable_list: u32,
    /// 1-based run-start index into `LocalConstant` (0x34) — always `1` (an empty, degenerate-but-
    /// still-stamped range) since this pass never populates `LocalConstant`.
    constant_list: u32,
    /// IL offset this scope starts at — always `0` (the whole method is one flat scope).
    start_offset: u32,
    /// IL byte length this scope covers — the method body's total code length
    /// (`MethodSequencePoints::code_len`), i.e. the whole method.
    length: u32,
}

/// `LocalVariable` (0x33, `PortablePdb-Metadata.md`): one named local, keyed by its 0-based slot
/// index in the owning method's `LocalVarSig` (`cilly::ir::method::LocalDef`'s declaration order —
/// see [`PdbBuilder::build`]'s doc for why unnamed locals get NO row here, preserving index gaps).
#[derive(Debug, Clone)]
struct LocalVariableRow {
    /// `0` for every row this writer emits (no `DebuggerHidden` (bit `0x1`) support needed — every
    /// row here is a real, named Rust local by construction; see the skip rule above).
    attributes: u16,
    /// The local's 0-based slot index in the method's `LocalVarSig` — `LocalDef`'s position in the
    /// full (including-unnamed) locals list, NOT a compacted "Nth named local" counter.
    index: u16,
    /// `#Strings` heap index of the local's name (NOT `#Blob`/`#US` — see [`PdbWidths::strings_wide`]'s
    /// doc for why `#Strings` is the right heap here, matching how `Document`/`MethodDebugInformation`
    /// already use `#Blob` for path/sequence-point data but plain `#Strings` is the natural heap for
    /// a simple identifier).
    name: u32,
}

struct PdbWidths {
    heap_sizes: u8,
    blob_wide: bool,
    guid_wide: bool,
    document_wide: bool,
    /// Whether `LocalScope.Method` (a plain, uncoded `MethodDef` index) needs 4 bytes instead of
    /// 2 — sized by the TYPE-SYSTEM assembly's own `MethodDef` row count (from
    /// [`TypeSystemRowCounts::rows`]), NOT by any table this standalone PDB owns rows for itself
    /// (mirrors how `document_wide` is sized by `document_rows.len()`, but `MethodDef` isn't a
    /// row-count this builder maintains directly — it's read out of the type-system row counts
    /// [`PdbBuilder::new`] was constructed with).
    method_def_wide: bool,
    /// Whether `LocalVariable.Name` (a `#Strings` heap index) needs 4 bytes instead of 2 — same
    /// `> 0xFFFF` convention as `blob_wide`/`guid_wide`, just finally consuming the `strings` heap
    /// this module always constructed but never used before this table existed.
    strings_wide: bool,
    /// Whether `LocalScope.VariableList` (a row index into `LocalVariable`) needs 4 bytes instead
    /// of 2 — sized by the `LocalVariable` table's OWN row count, exactly like `document_wide` is
    /// sized by `document_rows.len()` (a plain table-index width, unrelated to any heap size).
    local_variable_wide: bool,
}

impl PdbWidths {
    fn compute(
        strings: &StringsHeap,
        blobs: &BlobHeap,
        guids: &GuidHeap,
        user_strings: &UserStringHeap,
        document_rows: usize,
        method_def_row_count: u32,
        local_variable_rows: usize,
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
            method_def_wide: method_def_row_count > 0xFFFF,
            strings_wide: strings.as_bytes().len() > 0xFFFF,
            local_variable_wide: local_variable_rows > 0xFFFF,
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

    let point = widen_degenerate_same_line_span(point);
    let point = &point;
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

/// Widens a degenerate same-line, zero-width-column `SequencePoint` (`end_col == col`) to a
/// 1-column span so it survives [`validate_visible_sequence_point`]'s `end_col > col` check
/// instead of panicking.
///
/// This shape is reachable from real (if rare) input: `span_source_info`
/// (`src/assembly.rs`) independently clamps `col_start`/`col_end` to `u16::MAX` before computing
/// `col_len = col_end - col_start`. A same-line span whose start column is already >= 65535
/// (e.g. from a source line >= 65534 columns wide — minified/generated `.rs`, a giant single-line
/// const array) clamps BOTH ends to 65535, collapsing `col_len` to 0 even though the ORIGINAL
/// (unclamped) span was non-empty. Prior to this fix that degenerate point either hit the
/// `end_col > col` assert (aborting the entire link for one bad span, no `catch_unwind` anywhere
/// upstream) or — had the assert simply been removed — would have been silently misencoded as a
/// HIDDEN point (delta_lines == 0 && delta_col == 0 is literally the hidden-point encoding, see
/// this function's siblings), which is worse: it would silently erase a real, resolvable
/// source location from the PDB instead of widening it. Widening by growing `end_col` (or, at the
/// `u16::MAX` ceiling, shrinking `col` instead) keeps the point visible and off-by-at-most-one
/// column, which is a far better debugging experience than either a full-link abort or a silently
/// hidden point.
fn widen_degenerate_same_line_span(point: &SequencePoint) -> SequencePoint {
    let mut widened = point.clone();
    if !widened.is_hidden && widened.end_line == widened.line && widened.end_col <= widened.col {
        if widened.col < u32::from(u16::MAX) {
            widened.end_col = widened.col + 1;
        } else {
            // `col` is already at the u16 ceiling (65535): widen backwards instead, since
            // `end_col` cannot exceed 65535 either (see `validate_visible_sequence_point`'s
            // column-range check).
            widened.col = widened.col.saturating_sub(1);
        }
    }
    widened
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
    local_scopes: &[LocalScopeRow],
    local_variables: &[LocalVariableRow],
    widths: &PdbWidths,
) -> Vec<u8> {
    let mut valid = 0u64;
    if !documents.is_empty() {
        valid |= 1u64 << TABLE_DOCUMENT;
    }
    if !methods.is_empty() {
        valid |= 1u64 << TABLE_METHOD_DEBUG_INFORMATION;
    }
    if !local_scopes.is_empty() {
        valid |= 1u64 << TABLE_LOCAL_SCOPE;
    }
    if !local_variables.is_empty() {
        valid |= 1u64 << TABLE_LOCAL_VARIABLE;
    }

    // `Sorted` bitmask (§II.24.2.6): `LocalScope` (0x32) is one of the tables the Portable PDB /
    // ECMA-335 metadata reader requires to be sorted by its primary key (`Method`) — CoreCLR's
    // `System.Reflection.Metadata` reader enforces this at load time (`LocalScopeTableReader`
    // throws `BadImageFormatException("Metadata table LocalScope not sorted")` if the bit isn't
    // set, confirmed empirically: a `LocalScope` table whose rows genuinely ARE in ascending
    // `Method` RID order — exactly what this builder produces, since it iterates `self.methods`
    // in RID order at row-construction time — was still REJECTED until this bit was set,
    // matching the reader's binary-search-by-Method assumption, which depends on the header
    // claiming sortedness, not just the rows happening to already be sorted). `Document`/
    // `MethodDebugInformation`/`LocalVariable` have no such requirement (no runtime binary-searches
    // them by a key column), so only bit `TABLE_LOCAL_SCOPE` is ever set here.
    let mut sorted = 0u64;
    if !local_scopes.is_empty() {
        sorted |= 1u64 << TABLE_LOCAL_SCOPE;
    }

    let mut out = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(2);
    out.push(0);
    out.push(widths.heap_sizes);
    out.push(1);
    out.extend_from_slice(&valid.to_le_bytes());
    out.extend_from_slice(&sorted.to_le_bytes());
    for table in 0..64u32 {
        if valid & (1u64 << table) != 0 {
            let count = match table {
                TABLE_DOCUMENT => documents.len(),
                TABLE_METHOD_DEBUG_INFORMATION => methods.len(),
                TABLE_LOCAL_SCOPE => local_scopes.len(),
                TABLE_LOCAL_VARIABLE => local_variables.len(),
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
    for row in local_scopes {
        write_heap_idx(&mut out, row.method, widths.method_def_wide);
        write_heap_idx(&mut out, row.import_scope, false);
        write_heap_idx(&mut out, row.variable_list, widths.local_variable_wide);
        write_heap_idx(&mut out, row.constant_list, false);
        out.extend_from_slice(&row.start_offset.to_le_bytes());
        out.extend_from_slice(&row.length.to_le_bytes());
    }
    for row in local_variables {
        out.extend_from_slice(&row.attributes.to_le_bytes());
        out.extend_from_slice(&row.index.to_le_bytes());
        write_heap_idx(&mut out, row.name, widths.strings_wide);
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

/// SHA-256 (FIPS 180-4) of `data`, dependency-free (the workspace has no `sha2` crate; this is
/// ~50 lines of a well-specified, unchanging algorithm — not worth a new dependency for one call
/// site). Used ONLY for [`PdbChecksumEntry`]'s payload: a `PdbChecksum` (type 19) PE Debug
/// Directory entry whose payload is `"SHA256\0"` + this hash of the WHOLE on-disk PDB file.
///
/// # Why this exists (root-caused during Phase-2 acceptance testing)
/// A from-scratch, otherwise spec-conformant Portable PDB (verified byte-for-byte against
/// `System.Reflection.Metadata`'s own reader — correct `#Pdb` stream row counts/entry-point token,
/// correct RSDS GUID/age matching the PE's CodeView entry, correct delta-encoded sequence points)
/// still produced NO file:line info from a live `Environment.StackTrace`/unhandled-exception trace
/// under CoreCLR 8.0.28 on this machine — even though the SAME mechanism resolved a Roslyn-built
/// assembly fine. Diffing the Debug Directory of a minimal Roslyn `.dll` against ours found Roslyn
/// emits THREE entries (`CodeView`, `PdbChecksum`, `Reproducible`), not just `CodeView` — CoreCLR's
/// runtime `StackTraceSymbols` provider (unlike the static SRM reader) apparently requires the
/// `PdbChecksum` entry before it will trust/load a portable PDB at runtime. Adding it (this
/// function + [`PdbChecksumEntry`] + `pe.rs`'s wiring) is what actually closed Phase 2's
/// acceptance gap — the GUID/age pairing alone, while spec-correct, was NOT sufficient in
/// practice.
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// A `PdbChecksum` (type 19) PE Debug Directory entry's payload (see [`sha256`]'s doc for why this
/// entry is required, not just the `CodeView`/RSDS one): `"SHA256\0"` (NUL-terminated algorithm
/// name) followed by the 32-byte SHA-256 digest of the complete on-disk PDB file bytes — matching
/// the exact payload shape a real Roslyn-produced `.dll`'s Debug Directory carries (confirmed via
/// `System.Reflection.PortableExecutable.PEReader.ReadDebugDirectory`/`GetSectionData` against a
/// live-built reference assembly during this investigation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdbChecksumEntry {
    pub algorithm_name: &'static str,
    pub checksum: [u8; 32],
}

impl PdbChecksumEntry {
    /// Computes the entry from the COMPLETE final PDB file bytes (i.e. [`PdbBuilder::build`]'s
    /// first return value) — must be called with the bytes exactly as written to disk, since the
    /// checksum covers the whole file, not just the `#Pdb` stream's own id field.
    #[must_use]
    pub fn from_pdb_bytes(pdb_bytes: &[u8]) -> Self {
        Self {
            algorithm_name: "SHA256",
            checksum: sha256(pdb_bytes),
        }
    }

    /// Serializes this entry's payload bytes: `"SHA256\0"` + the 32-byte digest, matching the
    /// reference Roslyn payload shape byte-for-byte (see this type's doc).
    #[must_use]
    pub fn payload_bytes(&self) -> Vec<u8> {
        let mut out = self.algorithm_name.as_bytes().to_vec();
        out.push(0);
        out.extend_from_slice(&self.checksum);
        out
    }
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
    /// The RSDS payload's `Age` field (conventionally starts at 1 and increments per PDB rebuild
    /// for the same GUID under the reference toolchain; this writer has no incremental-rebuild
    /// concept, so it is always `1` — content changes produce a new GUID instead, via
    /// [`deterministic_pdb_id`]).
    ///
    /// **NOT cosmetic to every consumer** — `System.Reflection.Metadata`'s own
    /// `PEReader.TryOpenAssociatedPortablePdb` never reads this field (only [`stamp`](Self::stamp)
    /// gates that API's match), but at least one real symbol-loading consumer hardcodes an exact
    /// `Age == 1` gate BEFORE it even compares GUID/stamp: `netcoredbg`'s managed `SymbolReader.
    /// TryOpenReaderFromCodeView` (`src/managed/SymbolReader.cs`) reads:
    /// ```csharp
    /// if (data.Age == 1 && new BlobContentId(reader.DebugMetadataHeader.Id)
    ///         == new BlobContentId(data.Guid, codeViewEntry.Stamp))
    /// ```
    /// Found empirically during this task's DAP/`netcoredbg` breakpoint-and-Locals-panel
    /// verification: a from-scratch DAP session against a `cd_pdb`-built `.dll`/`.pdb` pair loaded
    /// the module with `"symbolStatus": "Symbols not found."` and a pending breakpoint NEVER
    /// resolved, even though the SAME PDB parsed perfectly (all `Document`/`MethodDebugInformation`/
    /// `LocalScope`/`LocalVariable` rows, including the exact `mission_critical_value`/
    /// `another_named_local` locals this task added) under a standalone
    /// `System.Reflection.Metadata`-based reader harness and under `PEReader.
    /// TryOpenAssociatedPortablePdb` directly — isolating the failure to netcoredbg's stricter,
    /// GUID/stamp-first-only-if-`Age==1` gate. The previous `stamp | 1` value here produces some
    /// odd 32-bit number (content-hash-derived), essentially never `1`, silently failing that gate
    /// with no exception (the surrounding `try`/`catch` in `SymbolReader.cs` swallows everything).
    /// Roslyn always emits `Age = 1` for a freshly-built (non-incremental) PDB — matching that
    /// convention exactly, rather than deriving a "cosmetically nonzero" value from content, is
    /// what makes both consumers happy at once.
    pub age: u32,
    /// Bytes `16..20` of the [`PdbId`], written VERBATIM into the `IMAGE_DEBUG_DIRECTORY` row's own
    /// `TimeDateStamp` field (`pe.rs`'s `write_debug_directory`) — **this, not [`age`](Self::age),
    /// is what `PEReader.TryOpenCodeViewPortablePdb` actually compares against the opened PDB's own
    /// `#Pdb`-stream `Id` bytes `16..20`** (`new BlobContentId(codeViewDebugDirectoryData.Guid,
    /// codeViewEntry.Stamp)` in that decompiled source — the GUID half comes from the RSDS payload,
    /// but the 4-byte stamp half comes from the ROW, not the payload). A real bug found during
    /// Phase 2 acceptance testing: `pe.rs` previously wrote a hardcoded `0` into `TimeDateStamp`
    /// "for determinism" (a reasonable-looking but WRONG generalization from the COFF header's own
    /// `TimeDateStamp`, which genuinely should be `0`) — that silently mismatched this field
    /// whenever the real stamp was nonzero, making `TryOpenAssociatedPortablePdb` return `false`
    /// with no exception and no diagnostic, even though the PDB was otherwise byte-perfect.
    pub stamp: u32,
    /// NUL-terminated (on write) path/filename CoreCLR's fallback resolver would use — conventionally
    /// just the PDB's bare filename (e.g. `"foo.pdb"`), matching `il_exporter`'s
    /// `pdb_file`/`{output_file_path}.pdb` convention in `cilly/src/bin/linker/main.rs:718-724`.
    pub pdb_path: String,
}

impl DebugDirectoryEntry {
    /// Builds the entry from a [`PdbId`] ([`PdbBuilder::build`]'s second return value) and the PDB
    /// file's on-disk name, splitting the 20 content-hash bytes into the CodeView GUID (bytes
    /// `0..16`) and the row `TimeDateStamp` (bytes `16..20`, VERBATIM — see [`stamp`](Self::stamp)'s
    /// doc for why this exact value is the real match key for `System.Reflection.Metadata`-based
    /// consumers). `age` is always the literal `1` — see [`age`](Self::age)'s doc for why this
    /// writer, which never does incremental PDB rebuilds, must still emit the exact conventional
    /// value rather than any other nonzero placeholder: at least one real consumer (`netcoredbg`)
    /// gates its GUID/stamp comparison behind `Age == 1` and silently rejects everything else.
    #[must_use]
    pub fn from_pdb_id(id: PdbId, pdb_path: String) -> Self {
        let mut guid = [0u8; 16];
        guid.copy_from_slice(&id[0..16]);
        let stamp = u32::from_le_bytes([id[16], id[17], id[18], id[19]]);
        Self {
            guid,
            age: 1,
            stamp,
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
        sorted: u64,
        counts: Vec<(u32, usize)>,
        row_data_offset: usize,
    }

    #[derive(Debug, Clone, Copy)]
    struct TestMethodDebugInformationRow {
        document: u32,
        sequence_points: u32,
    }

    #[derive(Debug, Clone, Copy)]
    struct TestLocalScopeRow {
        method: u32,
        import_scope: u32,
        variable_list: u32,
        constant_list: u32,
        start_offset: u32,
        length: u32,
    }

    #[derive(Debug, Clone, Copy)]
    struct TestLocalVariableRow {
        attributes: u16,
        index: u16,
        name: u32,
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
            let sorted = u64::from_le_bytes(tables[16..24].try_into().unwrap());
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
                sorted,
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

        fn row_width(&self, table: u32, header: &TestTablesHeader) -> usize {
            let index = |wide| if wide { 4 } else { 2 };
            let blob = index(header.heap_sizes & 0x4 != 0);
            let guid = index(header.heap_sizes & 0x2 != 0);
            let strings = index(header.heap_sizes & 0x1 != 0);
            match table {
                TABLE_DOCUMENT => blob + guid + blob + guid,
                TABLE_METHOD_DEBUG_INFORMATION => {
                    let doc_rows = Self::table_row_count(header, TABLE_DOCUMENT);
                    index(doc_rows > 0xFFFF) + blob
                }
                TABLE_LOCAL_SCOPE => {
                    let (_, type_system_rows) = self.pdb_stream_rows();
                    let method_def_rows = type_system_rows
                        .iter()
                        .find(|&&(id, _)| id == Token::TABLE_METHOD_DEF)
                        .map_or(0, |&(_, count)| count);
                    let local_variable_rows = Self::table_row_count(header, TABLE_LOCAL_VARIABLE);
                    // Method(idx) + ImportScope(idx, always narrow, 0 rows) + VariableList(idx) +
                    // ConstantList(idx, always narrow, 0 rows) + StartOffset(4) + Length(4).
                    index(method_def_rows > 0xFFFF)
                        + 2
                        + index(local_variable_rows > 0xFFFF)
                        + 2
                        + 4
                        + 4
                }
                TABLE_LOCAL_VARIABLE => 2 + 2 + strings,
                other => panic!("unexpected test PDB table {other:#x}"),
            }
        }

        fn table_offset(&self, table: u32, header: &TestTablesHeader) -> usize {
            let mut offset = 0;
            for &(id, count) in &header.counts {
                if id == table {
                    return offset;
                }
                offset += count * self.row_width(id, header);
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

        fn local_scope_rows(&self) -> Vec<TestLocalScopeRow> {
            let header = self.tables_header();
            let count = Self::table_row_count(&header, TABLE_LOCAL_SCOPE);
            if count == 0 {
                return Vec::new();
            }
            let mut cursor = header.row_data_offset + self.table_offset(TABLE_LOCAL_SCOPE, &header);
            let tables = self.stream("#~");
            let (_, type_system_rows) = self.pdb_stream_rows();
            let method_def_rows = type_system_rows
                .iter()
                .find(|&&(id, _)| id == Token::TABLE_METHOD_DEF)
                .map_or(0, |&(_, count)| count);
            let method_wide = method_def_rows > 0xFFFF;
            let local_variable_wide = Self::table_row_count(&header, TABLE_LOCAL_VARIABLE) > 0xFFFF;
            let mut rows = Vec::new();
            for _ in 0..count {
                let method = Self::read_index(tables, &mut cursor, method_wide);
                let import_scope = Self::read_index(tables, &mut cursor, false);
                let variable_list = Self::read_index(tables, &mut cursor, local_variable_wide);
                let constant_list = Self::read_index(tables, &mut cursor, false);
                let start_offset = u32::from_le_bytes(tables[cursor..cursor + 4].try_into().unwrap());
                cursor += 4;
                let length = u32::from_le_bytes(tables[cursor..cursor + 4].try_into().unwrap());
                cursor += 4;
                rows.push(TestLocalScopeRow {
                    method,
                    import_scope,
                    variable_list,
                    constant_list,
                    start_offset,
                    length,
                });
            }
            rows
        }

        fn local_variable_rows(&self) -> Vec<TestLocalVariableRow> {
            let header = self.tables_header();
            let count = Self::table_row_count(&header, TABLE_LOCAL_VARIABLE);
            if count == 0 {
                return Vec::new();
            }
            let mut cursor =
                header.row_data_offset + self.table_offset(TABLE_LOCAL_VARIABLE, &header);
            let tables = self.stream("#~");
            let strings_wide = header.heap_sizes & 0x1 != 0;
            let mut rows = Vec::new();
            for _ in 0..count {
                let attributes = u16::from_le_bytes(tables[cursor..cursor + 2].try_into().unwrap());
                cursor += 2;
                let index = u16::from_le_bytes(tables[cursor..cursor + 2].try_into().unwrap());
                cursor += 2;
                let name = Self::read_index(tables, &mut cursor, strings_wide);
                rows.push(TestLocalVariableRow { attributes, index, name });
            }
            rows
        }

        /// Reads a plain, NUL-terminated `#Strings` heap entry — NOT the `#Blob`-compressed-length
        /// "document name" encoding [`Self::document_name`] uses (that's a `Document.Name`-specific
        /// convention; `LocalVariable.Name` is an ordinary `#Strings` index like any other metadata
        /// name column, e.g. `TypeDef.Name`).
        fn string_at(&self, offset: u32) -> String {
            let strings = self.stream("#Strings");
            let start = offset as usize;
            let end = strings[start..].iter().position(|&b| b == 0).unwrap() + start;
            std::str::from_utf8(&strings[start..end]).unwrap().to_string()
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
                ..Default::default()
            },
        );
        builder.add_method(
            Token::new(Token::TABLE_METHOD_DEF, 3),
            MethodSequencePoints {
                local_signature: None,
                points: method3.clone(),
                ..Default::default()
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
        // `age` is ALWAYS the literal Roslyn-convention `1`, regardless of PDB content — see
        // `DebugDirectoryEntry::age`'s doc for why a content-derived value (this module's earlier
        // `stamp | 1`) silently fails netcoredbg's `Age == 1` gate.
        assert_eq!(entry.age, 1);
        // `stamp` (the field the runtime ACTUALLY matches against, per `stamp`'s doc) must be the
        // RAW bytes[16..20] value — stamp bytes are [16,17,18,19] little-endian = 0x13121110.
        assert_eq!(entry.stamp, 0x1312_1110);
    }

    /// FIPS 180-4 / NIST published test vectors — the empty string and `"abc"` — the canonical
    /// sanity check for any from-scratch SHA-256 implementation.
    #[test]
    fn sha256_matches_nist_test_vectors() {
        assert_eq!(
            hex_string(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex_string(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    fn hex_string(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn pdb_checksum_entry_payload_matches_roslyn_shape() {
        let entry = PdbChecksumEntry::from_pdb_bytes(b"pretend pdb bytes");
        let payload = entry.payload_bytes();
        assert_eq!(&payload[0..7], b"SHA256\0", "NUL-terminated algorithm name");
        assert_eq!(payload.len(), 7 + 32, "algorithm name + 32-byte digest, no extra padding");
        assert_eq!(&payload[7..], &sha256(b"pretend pdb bytes"));
    }

    #[test]
    fn pdb_checksum_entry_changes_with_pdb_content() {
        let a = PdbChecksumEntry::from_pdb_bytes(b"content-a");
        let b = PdbChecksumEntry::from_pdb_bytes(b"content-b");
        assert_ne!(a.checksum, b.checksum);
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
                ..Default::default()
            },
        );
        assert_eq!(builder.methods[1].as_ref().unwrap().points.len(), 1);
    }

    /// Regression test for a real bug caught wiring `DIRECT_PE=1` end-to-end
    /// (`cargo_tests/cd_pdb`): `body.rs`'s linearizer can visit two-or-more `SourceFileInfo` roots
    /// with no instruction between them (offsets tie), which `encode_sequence_point_record`'s
    /// strictly-increasing assert would otherwise reject with "sequence-point IL offsets must be
    /// strictly increasing: N after N". `add_method` must collapse same-offset runs BEFORE that
    /// assert ever sees them, keeping the LAST point per offset (see `dedupe_same_offset_points`'s
    /// doc for why "last wins").
    #[test]
    fn add_method_collapses_same_il_offset_runs_keeping_the_last_point() {
        let mut builder = PdbBuilder::new(
            TypeSystemRowCounts { rows: vec![(Token::TABLE_METHOD_DEF, 1)], entry_point_token: 0 },
            1,
        );
        let tok = Token::new(Token::TABLE_METHOD_DEF, 1);
        builder.add_method(
            tok,
            MethodSequencePoints {
                local_signature: None,
                points: vec![
                    sp(0, "a.rs", 1, 1, 1, 2),
                    // Two more `SourceFileInfo` roots at the SAME il_offset (167 in the real bug) —
                    // no instruction was emitted between them.
                    sp(5, "a.rs", 2, 1, 2, 2),
                    sp(5, "a.rs", 3, 1, 3, 2),
                    sp(5, "a.rs", 4, 1, 4, 2),
                    sp(9, "a.rs", 5, 1, 5, 2),
                ],
                ..Default::default()
            },
        );
        let stored = builder.methods[0].as_ref().unwrap().points.clone();
        assert_eq!(
            stored,
            [sp(0, "a.rs", 1, 1, 1, 2), sp(5, "a.rs", 4, 1, 4, 2), sp(9, "a.rs", 5, 1, 5, 2)],
            "same-offset run at il_offset=5 collapses to its LAST entry (line 4), not the first"
        );

        // The whole pipeline (not just the stored Vec) must accept this without panicking, and the
        // encoded blob must decode back to exactly the deduped points.
        let (bytes, _) = builder.build();
        let reader = TestPdbReader::parse(&bytes);
        let documents = reader.documents();
        let rows = reader.method_rows();
        let (_, decoded) = reader.decode_sequence_points(rows[0], &documents);
        assert_eq!(decoded, stored);
    }

    /// Regression test for a real bug found in cross-model review: `span_source_info`
    /// (`src/assembly.rs`) independently clamps `col_start`/`col_end` to `u16::MAX` (65535) before
    /// computing `col_len = col_end - col_start`. For a same-line span whose start column is
    /// already >= 65535 (e.g. `cstart=65535, cend=65536`), BOTH clamp to 65535, so `col_len` becomes
    /// 0 and the resulting `SequencePoint` has `col == end_col` on the same line — exactly the
    /// degenerate shape `validate_visible_sequence_point` used to `assert!(end_col > col)` against,
    /// aborting the ENTIRE link with no PDB/PE output for a single degenerate span anywhere in the
    /// program. `add_method`/`build` must accept this shape (by widening it to a 1-column span, the
    /// least surprising fix that keeps the point visible instead of silently turning it hidden)
    /// rather than panicking.
    #[test]
    fn add_method_accepts_degenerate_same_line_zero_width_column_span() {
        let mut builder = PdbBuilder::new(
            TypeSystemRowCounts { rows: vec![(Token::TABLE_METHOD_DEF, 1)], entry_point_token: 0 },
            1,
        );
        let tok = Token::new(Token::TABLE_METHOD_DEF, 1);
        // col == end_col == 65535 on the same line: the exact shape produced by clamping
        // cstart=65535, cend=65536 to u16::MAX in `span_source_info`.
        let degenerate = sp(0, "a.rs", 10, 65535, 10, 65535);
        builder.add_method(
            tok,
            MethodSequencePoints {
                local_signature: None,
                points: vec![degenerate],
                ..Default::default()
            },
        );

        // Must not panic, and must still produce a well-formed, decodable blob.
        let (bytes, _) = builder.build();
        let reader = TestPdbReader::parse(&bytes);
        let documents = reader.documents();
        let rows = reader.method_rows();
        let (_, decoded) = reader.decode_sequence_points(rows[0], &documents);
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].line, 10);
        assert_eq!(decoded[0].end_line, 10);
        assert!(
            decoded[0].end_col > decoded[0].col,
            "widened to a non-empty column span instead of panicking or silently going hidden: {:?}",
            decoded[0]
        );
        assert!(!decoded[0].is_hidden, "a real span should stay visible, not collapse to hidden");
        // `col` was already at the u16 ceiling (65535), so the widen must shrink `col` backwards
        // (not grow `end_col`, which cannot legally exceed 65535 either).
        assert_eq!(decoded[0].col, 65534);
        assert_eq!(decoded[0].end_col, 65535);
    }

    /// Sibling of the above, covering the OTHER branch: when `col` is below the u16 ceiling, the
    /// widen must grow `end_col` forward instead of touching `col` (matches the span's true start
    /// position, which is more informative for a debugger than shifting it backwards).
    #[test]
    fn add_method_widens_degenerate_span_forward_when_col_is_not_at_the_ceiling() {
        let mut builder = PdbBuilder::new(
            TypeSystemRowCounts { rows: vec![(Token::TABLE_METHOD_DEF, 1)], entry_point_token: 0 },
            1,
        );
        let tok = Token::new(Token::TABLE_METHOD_DEF, 1);
        let degenerate = sp(0, "a.rs", 7, 42, 7, 42);
        builder.add_method(
            tok,
            MethodSequencePoints {
                local_signature: None,
                points: vec![degenerate],
                ..Default::default()
            },
        );

        let (bytes, _) = builder.build();
        let reader = TestPdbReader::parse(&bytes);
        let documents = reader.documents();
        let rows = reader.method_rows();
        let (_, decoded) = reader.decode_sequence_points(rows[0], &documents);
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].col, 42, "start column left untouched");
        assert_eq!(decoded[0].end_col, 43, "end column widened forward by one");
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
                ..Default::default()
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

    /// LocalScope (0x32) / LocalVariable (0x33) coverage: a method with 2 NAMED locals and 1
    /// UNNAMED (compiler-temp) local sandwiched between them must get exactly 2 `LocalVariable`
    /// rows, with `Index` reflecting the local's ABSOLUTE slot position in the method's
    /// `LocalVarSig` (i.e. the unnamed local's slot leaves a gap — index 0 and index 2, not 0 and
    /// 1), matching the recon brief's "iterate `enumerate()`, only push a row when `name.is_some()`"
    /// rule. Also checks the `LocalScope` row's `VariableList` owned-range start.
    #[test]
    fn add_method_with_locals_emits_local_variable_rows_preserving_slot_index_gaps() {
        let type_system = TypeSystemRowCounts {
            rows: vec![(Token::TABLE_METHOD_DEF, 1)],
            entry_point_token: 0,
        };
        let mut builder = PdbBuilder::new(type_system, 1);
        let tok = Token::new(Token::TABLE_METHOD_DEF, 1);
        builder.add_method(
            tok,
            MethodSequencePoints {
                local_signature: None,
                points: vec![sp(0, "src/main.rs", 1, 1, 1, 2)],
                locals: vec![
                    Some("mission_critical_value".to_string()),
                    None, // compiler-generated temporary — must get NO LocalVariable row.
                    Some("result".to_string()),
                ],
                code_len: 42,
            },
        );

        let (bytes, _) = builder.build();
        let reader = TestPdbReader::parse(&bytes);

        // Regression test for a real bug caught by the task's mandated empirical verification
        // (NOT by table-shape inspection alone): `System.Reflection.Metadata`'s reader REJECTS
        // the whole PDB with `BadImageFormatException("Metadata table LocalScope not sorted")`
        // unless the `#~` stream's `Sorted` bitmask (§II.24.2.6) has bit `TABLE_LOCAL_SCOPE` set —
        // even when the rows themselves are already in ascending `Method` order (as they always
        // are here, since `build()` iterates `self.methods` in RID order). A narrow structural/
        // byte-shape check would NOT have caught this; only loading the PDB through the real
        // runtime reader did.
        let header = reader.tables_header();
        assert_eq!(
            header.sorted & (1u64 << TABLE_LOCAL_SCOPE),
            1u64 << TABLE_LOCAL_SCOPE,
            "LocalScope must be declared Sorted or System.Reflection.Metadata rejects the PDB"
        );

        let scopes = reader.local_scope_rows();
        assert_eq!(scopes.len(), 1, "exactly one LocalScope row for the one method with locals");
        assert_eq!(scopes[0].method, 1, "MethodDef RID 1");
        assert_eq!(scopes[0].import_scope, 0, "no ImportScope support: always nil");
        assert_eq!(scopes[0].variable_list, 1, "1-based run start into LocalVariable");
        assert_eq!(scopes[0].constant_list, 1, "LocalConstant never populated: degenerate empty range");
        assert_eq!(scopes[0].start_offset, 0, "whole-method flat scope");
        assert_eq!(scopes[0].length, 42, "covers the method's full IL code length");

        let vars = reader.local_variable_rows();
        assert_eq!(vars.len(), 2, "only the 2 NAMED locals get rows; the unnamed temp gets none");
        assert_eq!(vars[0].index, 0, "first named local is slot 0");
        assert_eq!(vars[0].attributes, 0);
        assert_eq!(reader.string_at(vars[0].name), "mission_critical_value");
        assert_eq!(
            vars[1].index, 2,
            "second named local is slot 2 — the unnamed slot 1 leaves a gap, not compacted to 1"
        );
        assert_eq!(reader.string_at(vars[1].name), "result");
    }

    /// A method with ZERO named locals (either no locals at all, or locals that are all unnamed
    /// compiler temporaries) must get NO `LocalScope` row — matching how real compilers hide
    /// temporaries from the Locals panel entirely rather than emitting an empty scope for them.
    #[test]
    fn add_method_with_no_named_locals_gets_no_local_scope_row() {
        let type_system = TypeSystemRowCounts {
            rows: vec![(Token::TABLE_METHOD_DEF, 1)],
            entry_point_token: 0,
        };
        let mut builder = PdbBuilder::new(type_system, 1);
        let tok = Token::new(Token::TABLE_METHOD_DEF, 1);
        builder.add_method(
            tok,
            MethodSequencePoints {
                local_signature: None,
                points: vec![sp(0, "src/main.rs", 1, 1, 1, 2)],
                locals: vec![None, None], // only unnamed temporaries.
                code_len: 10,
            },
        );

        let (bytes, _) = builder.build();
        let reader = TestPdbReader::parse(&bytes);
        assert_eq!(reader.local_scope_rows().len(), 0);
        assert_eq!(reader.local_variable_rows().len(), 0);
        let header = reader.tables_header();
        assert_eq!(
            header.valid & (1u64 << TABLE_LOCAL_SCOPE),
            0,
            "LocalScope table must not even be marked Valid when it has zero rows"
        );
        assert_eq!(header.valid & (1u64 << TABLE_LOCAL_VARIABLE), 0);
    }

    /// A method with named locals but (implausibly) no sequence points must STILL get its
    /// `LocalScope`/`LocalVariable` rows — these tables are keyed by `MethodDef` row, independent
    /// of sequence-point presence (see `export.rs`'s `add_method` skip-guard, which only skips a
    /// method with BOTH zero points AND zero named locals).
    #[test]
    fn add_method_with_locals_but_no_sequence_points_still_gets_local_scope() {
        let type_system = TypeSystemRowCounts {
            rows: vec![(Token::TABLE_METHOD_DEF, 1)],
            entry_point_token: 0,
        };
        let mut builder = PdbBuilder::new(type_system, 1);
        let tok = Token::new(Token::TABLE_METHOD_DEF, 1);
        builder.add_method(
            tok,
            MethodSequencePoints {
                local_signature: None,
                points: Vec::new(),
                locals: vec![Some("x".to_string())],
                code_len: 7,
            },
        );

        let (bytes, _) = builder.build();
        let reader = TestPdbReader::parse(&bytes);
        let scopes = reader.local_scope_rows();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].length, 7);
        let vars = reader.local_variable_rows();
        assert_eq!(vars.len(), 1);
        assert_eq!(reader.string_at(vars[0].name), "x");
    }

    /// Multiple methods with locals: `LocalScope.VariableList` owned-range starts must be
    /// contiguous and non-overlapping across methods, in RID order — mirrors
    /// `tables::MetadataBuilder::add_type_def`'s `field_list`/`method_list` owned-range pattern
    /// (see [`PdbBuilder::build`]'s doc / the `LocalScopeRow` doc for the exact mirrored shape).
    #[test]
    fn local_variable_owned_ranges_are_contiguous_across_multiple_methods() {
        let type_system = TypeSystemRowCounts {
            rows: vec![(Token::TABLE_METHOD_DEF, 2)],
            entry_point_token: 0,
        };
        let mut builder = PdbBuilder::new(type_system, 2);
        builder.add_method(
            Token::new(Token::TABLE_METHOD_DEF, 1),
            MethodSequencePoints {
                local_signature: None,
                points: vec![sp(0, "a.rs", 1, 1, 1, 2)],
                locals: vec![Some("a0".to_string()), Some("a1".to_string())],
                code_len: 5,
            },
        );
        builder.add_method(
            Token::new(Token::TABLE_METHOD_DEF, 2),
            MethodSequencePoints {
                local_signature: None,
                points: vec![sp(0, "a.rs", 2, 1, 2, 2)],
                locals: vec![Some("b0".to_string())],
                code_len: 3,
            },
        );

        let (bytes, _) = builder.build();
        let reader = TestPdbReader::parse(&bytes);
        let scopes = reader.local_scope_rows();
        assert_eq!(scopes.len(), 2);
        assert_eq!(scopes[0].method, 1);
        assert_eq!(scopes[0].variable_list, 1, "method 1's variables start at row 1");
        assert_eq!(scopes[1].method, 2);
        assert_eq!(
            scopes[1].variable_list, 3,
            "method 2's variables start AFTER method 1's 2 locals, at row 3"
        );

        let vars = reader.local_variable_rows();
        assert_eq!(vars.len(), 3);
        assert_eq!(reader.string_at(vars[0].name), "a0");
        assert_eq!(reader.string_at(vars[1].name), "a1");
        assert_eq!(reader.string_at(vars[2].name), "b0");
    }

    /// Round-trip determinism, extended to cover the two new tables: re-building the identical
    /// fixture twice must produce byte-identical PDBs (including `LocalScope`/`LocalVariable`
    /// rows), and the tables read back must decode to the exact same named locals.
    #[test]
    fn pdb_build_is_deterministic_including_local_scope_and_local_variable_tables() {
        fn fixture() -> PdbBuilder {
            let type_system = TypeSystemRowCounts {
                rows: vec![(Token::TABLE_METHOD_DEF, 1)],
                entry_point_token: 0,
            };
            let mut builder = PdbBuilder::new(type_system, 1);
            builder.add_method(
                Token::new(Token::TABLE_METHOD_DEF, 1),
                MethodSequencePoints {
                    local_signature: None,
                    points: vec![sp(0, "a.rs", 1, 1, 1, 2)],
                    locals: vec![Some("mission_critical_value".to_string()), None],
                    code_len: 11,
                },
            );
            builder
        }

        let (bytes_a, id_a) = fixture().build();
        let (bytes_b, id_b) = fixture().build();
        assert_eq!(bytes_a, bytes_b);
        assert_eq!(id_a, id_b);

        let reader = TestPdbReader::parse(&bytes_a);
        let vars = reader.local_variable_rows();
        assert_eq!(vars.len(), 1);
        assert_eq!(reader.string_at(vars[0].name), "mission_critical_value");
        assert_eq!(vars[0].index, 0);
    }
}
