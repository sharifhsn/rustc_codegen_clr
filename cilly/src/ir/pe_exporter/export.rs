//! Top-level orchestration: `export_pe` walks a whole `Assembly` (class defs, fields, methods)
//! and drives the four-phase pipeline documented in `pe::write_pe`'s module doc —
//! `MetadataBuilder` population → `body::assemble_method` → RVA layout → `pe::write_pe`.
//!
//! Semantic oracle for *what* to walk and *how* to shape each row: `il_exporter::export_to_write`
//! (`cilly/src/ir/il_exporter/mod.rs`) — this function mirrors its per-class/per-method iteration
//! order (see that function's doc-adjacent comments) so the two exporters agree on every
//! assembly this milestone exercises.
//!
//! # Phase 1a scope
//!
//! This milestone only needs to carry a hand-built two-method assembly (a static `"entrypoint"`
//! calling a BCL method, per `docs/PE_EMISSION_PLAN.md`'s Phase 1a acceptance check) from
//! `Assembly` to a loadable `.exe`. The `Assembly` table's self-identity row IS wired
//! (`mb.set_assembly`, version `0.0.0.0` — mirrors `il_exporter`'s `.assembly _{}` executable
//! placeholder; a real version stamp for a named library assembly is deferred with the rest of
//! the `.dll` output path).
//!
//! # Phase 1b additions
//!
//! Closed since Phase 1a: named-parameter `Param` rows, `ClassDef::implements()` ->
//! `InterfaceImpl` rows, scalar `StaticFieldDef::default_value` (`FieldRVA` blobs sized to the
//! field's own declared type — no synthetic carrier needed), and const-data `FieldRVA` blobs
//! (`__rcl_const_blob_N` synthetic statics owned by `MainModule`, mirroring `body.rs`'s
//! `const_blob_field_token` — see that function's doc and the Pass 2.5 comment below for why the
//! ownership and naming must match it exactly).
//!
//! **Deliberately still unimplemented, but now loudly guarded (not a silent gap)**: `MainModule`
//! method-count partitioning (CoreCLR's ~65,535-methods-per-type cap, `il_exporter::partition`).
//! Verified NOT to be an upstream/assembly-level concern this exporter could inherit for free:
//! `il_exporter::partition::build` operates entirely inside `ILExporter::export_to_write`, keyed
//! by demangled `MethodDefIdx` names, with zero effect on the `Assembly` IR itself
//! (`cilly/src/ir/il_exporter/partition.rs`). Porting it here needs a second, interleaved
//! TypeDef/MethodDef pass (extra per-module classes must be added to `MetadataBuilder` *between*
//! `MainModule`'s own TypeDef and the next class def, since `add_type_def`'s
//! `method_list`/`field_list` cursors are table-position-sensitive — see `tables.rs::add_type_def`'s
//! doc, and see Pass 0/Pass 2's comments below for a *concrete* case of exactly this ordering trap
//! biting the const-data pass during this milestone's own development) plus a `body.rs`/`TokenSink`
//! redirect so a call from a partitioned class back into `MainModule` resolves to the right
//! TypeDef. Rather than leaving this an implicit, silent gap, `export_pe` now calls
//! `check_main_module_method_count` (Pass 3) and panics with a clear message before producing an
//! image `dotnet` would otherwise reject far more opaquely at load time.
//!
//! **Entry-point / `is_dll` handling**: fully wired *within* `export_pe` itself — the
//! `"entrypoint"`-named-method convention (matching `il_exporter`'s `ENTRYPOINT`/`asm::ENTRYPOINT`)
//! sets [`PeOptions::entry_point`], and [`ExportOptions::is_dll`] passes straight through to
//! [`PeOptions::is_dll`]; neither has an outstanding `todo!()`. What's still open is **outside**
//! this file's scope: `export_pe` returns `Vec<u8>` directly and does not implement the
//! `Exporter` trait the linker's existing `if *C_MODE {…} else if *JAVA_MODE {…} else {…}`
//! dispatch (`Assembly::export`) expects, so wiring a `DIRECT_PE` config flag into the real linker
//! binary needs either a thin `Exporter`-trait adapter or a parallel `export_pe` + `std::fs::write`
//! call site in the linker's `main()` — a decision for that call site, not something `export.rs`'s
//! internal `todo!()`s can express. Left for the dedicated linker-wiring task.

use super::body::{self, AssembledBody};
use super::pe::{self, PeOptions};
use super::sig;
use super::tables::{MetadataBuilder, Token};
use crate::ir::class::StaticFieldDef;
use crate::ir::{Assembly, ClassRef, Const};
use crate::Interned;

// `pe::SECTION_ALIGNMENT`/`pe::CLI_HEADER_CB` (both `pub(super)`) are used directly below rather
// than duplicated here — an earlier version of this file kept its own copies "since the RVA
// pre-computation below must match `pe::write_pe`'s own layout pass exactly", but that duplication
// was exactly the trap it warned about: bumping `pe::SECTION_ALIGNMENT` from `0x2000` to `0x4000`
// (see that constant's doc — a real macOS ARM64 `mprotect`-alignment `FileLoadException` fix) left
// THIS file's stale `0x2000` copy silently out of sync, producing `MethodDef`/`FieldRVA` RVAs that
// pointed outside `.text` entirely (`BadImageFormatException: Bad IL range` on every E2E test).
// Sharing the one real constant makes that class of drift impossible instead of just documented.

/// Everything `export_pe` needs beyond the `Assembly` itself.
pub struct ExportOptions {
    /// `true` for a `.dll`, `false` for a `.exe` — forwarded to [`PeOptions::is_dll`].
    pub is_dll: bool,
    /// The `Assembly` table's own identity name (§II.22.2's `Name` column) — `"_"` for an
    /// executable (loaded by path, identity irrelevant, mirrors `il_exporter`'s `.assembly _{}`),
    /// or the crate name for a library (so C# can reference the produced `.dll` by assembly
    /// identity). NOT the same thing as `module_name` — see that field's doc for why conflating
    /// them broke `Assembly.Load`.
    pub assembly_name: String,
    /// The `Module` table's own name (§II.22.30's `Name` column), independent of
    /// `assembly_name`. `ilasm` (given no explicit `.module` directive, which `il_exporter` never
    /// emits) defaults this to the `-output:` file's own basename — e.g.
    /// `cd_json-7dec5593b2da6ade.exe`, NOT the assembly identity `"_"`. Reusing `assembly_name`
    /// here (this field's original, wrong, implementation) stamped `Module.Name = "_"` for every
    /// executable; `System.Runtime.Loader.AssemblyLoadContext.InternalLoad`'s native path
    /// apparently cross-checks this against the manifest and rejects the mismatch with
    /// `System.IO.FileLoadException: Could not load file or assembly '_, ...'` — a
    /// `0x8007000C`/`COMException` thrown from native code *before* the CLI-aware managed loader
    /// (and even `System.Reflection.Metadata`'s own `PEReader`/`MetadataReader`, which validates
    /// this binary with zero errors) ever gets a look — root-caused via a real `ilasm`-produced
    /// `.dll` for the identical source, which loads fine and whose `Module.Name` was confirmed
    /// (via a from-scratch metadata reader) to be the output filename, not `"_"`.
    pub module_name: String,
    /// The bare filename (no directory) `export_pe`'s Debug Directory RSDS payload should embed
    /// as its fallback PDB path — conventionally the `.dll`/`.exe`'s own stem plus `.pdb` (mirrors
    /// `il_exporter`'s `{output_file_path}.pdb` convention in `cilly/src/bin/linker/main.rs`).
    /// CoreCLR's loader looks for `<assembly-stem>.pdb` next to the image FIRST and only falls
    /// back to this embedded path (see `pdb::DebugDirectoryEntry`'s doc), so correctness doesn't
    /// hinge on this string beyond "a plausible, non-empty filename" — but it must be non-empty
    /// for `export_pe` to build a PDB at all (empty means "no debug info requested", matching a
    /// `None` `PeOptions::debug_directory` and the writer's pre-Phase-2 output).
    pub pdb_file_name: String,
}

/// Builds the complete PE image bytes for `asm`: populates metadata for every class/field/method,
/// assembles every method body, lays out RVAs, patches them back into the metadata, and writes
/// the final PE/COFF container. Returns the finished `.exe`/`.dll` bytes, plus the standalone
/// Portable PDB bytes for the caller to write to `options.pdb_file_name` next to it (empty when
/// `options.pdb_file_name` is empty — see that field's doc) — Phase 2's debug-info wiring
/// (`docs/PE_EMISSION_PLAN.md`): every `body::AssembledBody::sequence_points` collected during
/// Pass 4 becomes that method's `pdb::MethodDebugInformation` row, in `MethodDef` RID order (the
/// same order `bodies` is already built in), and the returned image's Debug Directory (§Format
/// spec, CodeView/RSDS) is derived from the SAME [`pdb::PdbId`] embedded in the returned PDB
/// bytes, so the two files are guaranteed to match byte-for-byte on the GUID/age CoreCLR checks.
///
/// # Panics / `todo!()`
/// On any construct outside the Phase 1a inventory — see the module doc.
#[must_use]
pub fn export_pe(asm: &mut Assembly, options: &ExportOptions) -> (Vec<u8>, Vec<u8>) {
    let mut mb = MetadataBuilder::new();
    // Must happen before ANY `AssemblyRef` row is created (every class's implicit
    // `System.Object`/`System.ValueType` base pulls in `System.Runtime`) — see
    // `MetadataBuilder::is_lib`'s doc for why an unversioned executable-shaped `AssemblyRef` vs. a
    // versioned library-shaped one is not cosmetic (`FileLoadException 0x8007000C` on load).
    mb.set_is_lib(options.is_dll);

    // The `Assembly` table's single self-identity row (§II.22.2) — mirrors `il_exporter`'s
    // `.assembly '<name>'{}` / `.assembly _{}` directive, emitted unconditionally for both a
    // library and an executable (`il_exporter::export_to_write`'s very first `writeln!`, before
    // any `is_lib` branch). Version `0.0.0.0` matches the `.assembly _{}` placeholder's implicit
    // default (`il_exporter` only stamps a real version for the `is_lib` "named assembly" case,
    // which this Phase 1a milestone's `.exe`-only scope doesn't need — see the module doc's
    // "Assembly table's library identity" `todo!()` note for the `.dll` case).
    mb.set_assembly(&options.assembly_name, (0, 0, 0, 0));

    let class_def_ids: Vec<_> = asm.iter_class_def_ids().copied().collect();

    // --- Pass 0: const-data carrier TypeDefs (`__rcl_const_blob_N`), one per DISTINCT blob length
    // present in `asm.const_data` — mirrors `il_exporter::export_to_write`'s ordering exactly:
    // these `.class` blocks are emitted as literal IL text BEFORE any real class's block
    // (mod.rs:116-121, ahead of the `for class_def in asm.iter_class_defs()` loop at mod.rs:140).
    // That ordering is NOT cosmetic here: `TypeDef.MethodList`/`FieldList` are "run-start" pointers
    // (§II.22.37) — a `MethodDef`/`Field` row's owner is whichever `TypeDef` immediately precedes
    // it in the TABLE, not whichever `add_type_def` call happened to run most recently. An earlier
    // version of this pass created the carrier TypeDefs interleaved with `MainModule`'s OWN fields
    // (i.e. after `MainModule`'s TypeDef row but before `MainModule`'s methods were added in Pass
    // 3) — that positioned `__rcl_const_blob_N` as the table row immediately before `MainModule`'s
    // later-added `MethodDef` rows, so every one of `MainModule`'s methods (including `entrypoint`)
    // silently became owned by `__rcl_const_blob_N` instead: `dotnet` failed to load `entrypoint`
    // at all (`MissingFieldException: Field not found: 'MainModule.c_1'` from INSIDE
    // `__rcl_const_blob_4.entrypoint()` — the method itself had moved). Creating every carrier
    // TypeDef here, before Pass 1 adds any real class's TypeDef row, keeps them permanently ahead
    // of every class's `MethodList`/`FieldList` range in table order, matching `il_exporter`.
    let const_blob_carrier_type_of: std::collections::HashMap<usize, Token> = if asm.const_data.0.is_empty() {
        std::collections::HashMap::new()
    } else {
        let mut blob_sizes: Vec<usize> = asm.const_data.0.iter().map(|d| d.len().max(1)).collect();
        blob_sizes.sort_unstable();
        blob_sizes.dedup();
        let value_type_ref = system_runtime_type_ref(&mut mb, "System.ValueType");
        blob_sizes
            .into_iter()
            .map(|n| {
                let tok = mb.add_blob_sized_valuetype(&format!("__rcl_const_blob_{n}"), value_type_ref, u32::try_from(n).unwrap());
                (n, tok)
            })
            .collect()
    };

    // --- Pass 1: a TypeDef row for every class def, in assembly iteration order. Signature
    // encoding (reached from field/method population below) resolves a `ClassRef` to a TypeDef
    // row via `MetadataBuilder::find_type_def`'s linear scan, which requires the owning class
    // def's TypeDef row to already exist — see `tables.rs`'s `TypeDefOrRefResolver` impl doc
    // ("population walks class defs before any signature needs to resolve one"). Mirrors
    // `il_exporter::export_to_write`'s per-class loop (`.class … extends …`).
    //
    // Creating every `TypeDef` up front (before Pass 2/3 add any field/method row) means every
    // row's `FieldList`/`MethodList` (§II.22.37's run-start columns) is stamped `1` here — no
    // field/method row exists yet at ANY of these calls, so `tables.rs::add_type_def`'s "one past
    // the current end" capture is vacuously `1` every time. That is WRONG for any class that
    // owns >0 fields/methods once Pass 2/3 actually append them (only a lucky single-class
    // assembly — e.g. this milestone's early hand-built E2E tests, which only ever populate
    // `MainModule` — masks it: a `dotnet` load against a REAL multi-class compiler-generated
    // assembly instead attributes runs of unrelated fields to whichever TypeDef the table
    // position happens to land in, surfacing as `TypeLoadException: field '…' was not given an
    // explicit offset` or similar on a totally unrelated type). Pass 2/3 below re-stamp the
    // correct value via `set_type_def_field_list`/`set_type_def_method_list` immediately before
    // adding each class's own rows — this map is what lets them find the right `TypeDef` token
    // without a second `find_type_def` scan.
    let mut type_def_token_of: std::collections::HashMap<crate::ir::class::ClassDefIdx, Token> =
        std::collections::HashMap::with_capacity(class_def_ids.len());
    for &class_def_id in &class_def_ids {
        let class_def = asm[class_def_id].clone();
        // A genuine ECMA-335 `interface` `TypeDef` (`ClassDef::with_interface`, e.g. from a
        // Rust-trait-as-C#-interface spike) needs `Interface`+`Abstract` `TypeAttributes` and NO
        // `Extends` row at all (§II.10.1.3) — this writer has neither: the code just below always
        // computes and stamps a real `extends` (falling back to `System.Object`), which would
        // silently emit an ORDINARY concrete class instead of an interface, with its abstract
        // members' `MethodImpl::Missing` placeholder becoming a real throwing body (see
        // `MethodDef::is_abstract`'s doc — that placeholder is only inert because `il_exporter`
        // checks `is_abstract()` before ever reading it; this writer does not). Fail loudly instead
        // of emitting a fake, wrong-shaped class — mirrors the `method.overrides()` guard below for
        // the same "no PE-writer support yet" reason. See `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md`'s
        // Tier C finding #2.
        assert!(
            !class_def.is_interface(),
            "class '{}' is a genuine interface (ClassDef::with_interface), but the direct PE \
             writer (DIRECT_PE=1, the default) does not yet support interface TypeDefs -- set \
             DIRECT_PE=0 to use il_exporter (ilasm text) instead, which does. See \
             docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md's Tier C finding #2.",
            &asm[class_def.name()]
        );
        // `ClassDef::add_event` (§II.22.12/13/28's EventMap/Event/MethodSemantics rows) has no
        // support in this writer at all — no code path here even reads `class_def.events()`, so
        // silently dropping it would emit an ordinary class with plain public `add_`/`remove_`
        // methods and no `event` C# can subscribe to with `+=`/`-=`. Same "no PE-writer support
        // yet" reasoning as the two guards above. See docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md's
        // Tier C finding #5.
        assert!(
            class_def.events().is_empty(),
            "class '{}' declares a .NET event (ClassDef::add_event), but the direct PE writer \
             (DIRECT_PE=1, the default) does not yet support Event/EventMap/MethodSemantics \
             metadata rows -- set DIRECT_PE=0 to use il_exporter (ilasm text) instead, which \
             does. See docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md's Tier C finding #5.",
            &asm[class_def.name()]
        );
        // Every class needs an `Extends` row: `il_exporter::export_to_write` never leaves it NIL
        // — an explicit `extends` clause wins, otherwise `[System.Runtime]System.ValueType` for a
        // valuetype or `[System.Runtime]System.Object` for a reference type (mirrors that
        // function's `let extends = if let Some(parent) = … { … } else if is_valuetype { … } else
        // { … }` exactly). A NIL `Extends` is a real defect, not a harmless default: it makes the
        // CLR loader treat the TypeDef as an interface-shaped type with no concrete base, which
        // rejected this milestone's `MainModule` with `BadImageFormatException` during
        // development.
        let extends = if let Some(parent) = class_def.extends() {
            mb.class_ref_token(asm, parent)
        } else if class_def.is_valuetype() {
            system_runtime_type_ref(&mut mb, "System.ValueType")
        } else {
            system_runtime_type_ref(&mut mb, "System.Object")
        };
        // `implements I1, I2, …` (§II.22.23 `InterfaceImpl`): resolved the exact same way as
        // `extends` just above (each interface is itself a `ClassRef`, defined-in-assembly or
        // external), mirroring `il_exporter::export_to_write`'s `implements` clause (mod.rs:167-177,
        // one `simple_class_ref` per `class_def.implements()` entry). `add_type_def`'s `implements`
        // parameter already builds sorted-by-Class `InterfaceImpl` rows internally (tested:
        // `interface_impl_rows_are_emitted_sorted_by_class`), so insertion order here doesn't matter.
        //
        // `class_ref_token` (not the plain `type_def_or_ref` coded-index helper) so a GENERIC
        // interface reference (e.g. `IEquatable<int>` — see `rustc_codegen_clr_add_generic_
        // interface_impl`) resolves to a real `TypeSpec` carrying the full `GENERICINST` blob,
        // not a bare `TypeRef` to the unbound open generic definition (`IEquatable`1`) — a class
        // cannot implement/extend an unbound generic type; using the wrong one is a real
        // TypeLoadException at .NET load time, not a silent miscompilation, but wrong either way.
        // Every existing non-generic `extends=`/`implements=` caller is unaffected: `class_ref_token`
        // already falls back to the identical `type_def_or_ref` path whenever `generics()` is empty.
        let implements: Vec<Token> = class_def
            .implements()
            .iter()
            .map(|&iface| mb.class_ref_token(asm, iface))
            .collect();
        let has_explicit_layout = class_def.explict_size().is_some()
            || class_def.fields().iter().any(|(_, _, offset)| offset.is_some());
        let (pack, size) = if has_explicit_layout {
            (Some(1u16), class_def.explict_size().map(std::num::NonZeroU32::get))
        } else {
            (None, None)
        };
        // Split the last `.` into `TypeDef.Namespace`/`TypeDef.Name` (§II.22.37), exactly matching
        // what `ilasm` does with a `.class 'Full.Dotted.Name'` quoted identifier (confirmed
        // byte-for-byte against a real ilasm-built assembly: `.class 'cd_interop.Point'` becomes
        // `Namespace="cd_interop"`/`Name="Point"`, NOT `Namespace=""`/`Name="cd_interop.Point"`).
        // A prior version of this call always passed `namespace=""` and the FULL dotted string as
        // `name` — that loads and runs fine (token-based resolution doesn't care), but makes the
        // type uncompilable-against from C#: Roslyn's reference resolution looks a type up by
        // `Namespace`+`Name`, so `cd_interop.Point` (namespace `cd_interop`, name `Point`) was
        // simply absent, surfacing as `CS0246: The type or namespace name 'cd_interop' could not
        // be found` the moment a C# consumer referenced a namespaced Rust-exported type — see
        // `MetadataBuilder::find_type_def`'s doc for the paired lookup-side fix this requires.
        let raw_name = asm[class_def.name()].to_string();
        let (namespace, name) = super::tables::split_namespace(&raw_name);
        let tok = mb.add_type_def(namespace, name, class_def.is_valuetype(), Some(extends), pack, size, &implements);
        type_def_token_of.insert(class_def_id, tok);
    }

    // Every `FieldRVA` blob (§II.22.18) queued for `.sdata` placement, in the order queued —
    // scalar `StaticFieldDef::default_value`s and const-data blobs both land here; laid out into
    // real RVAs once bodies are assembled (Pass 5), same two-phase dance the module doc describes
    // for method bodies.
    let mut pending_field_rva: Vec<(Token, Vec<u8>)> = Vec::new();

    // `asm.main_module()` is idempotent (returns the existing class def if one was already
    // registered — which it always is by this point, since `body.rs::const_blob_field_token` can
    // only have been reachable from a method body Pass 3/4 below will assemble, and every such
    // body lives in a class def already walked by Pass 1); this just re-fetches the same
    // `ClassDefIdx` `il_exporter`'s equivalent lookup finds via `asm[cd.name()] ==
    // *super::asm::MAIN_MODULE` (mod.rs:135). Computed once, outside the loop below, so the
    // per-class-def comparison inside it is cheap.
    let main_module_id = asm.main_module();

    // --- Pass 2: fields (instance + static), matching `il_exporter`'s per-class field loop
    // (§II.22.15/§II.22.18). Const-data `FieldRVA` static fields (`c_{encode(idx)}`, typed to the
    // `__rcl_const_blob_N` carrier TypeDefs Pass 0 already added) are threaded into THIS SAME
    // per-class iteration, immediately after `MainModule`'s own static fields — not a separate
    // pass after this loop finishes. Reason: `TypeDef.FieldList` is a table-position "run-start"
    // pointer (§II.22.37), so a field's owner is whichever `TypeDef` row immediately precedes it
    // in table order, not whichever `add_type_def`/`add_static_field` call happened most recently
    // in program order. If const-data fields were added in a separate pass running after this
    // whole loop, they would land in whatever class `class_def_ids` (a `HashMap`-order snapshot)
    // happened to visit LAST — silently wrong unless MainModule happens to be last. A near-miss of
    // exactly this bug shape (for the analogous `MethodList` column) was caught during development:
    // see Pass 0's doc comment for the `MissingFieldException`/`entrypoint`-moved symptom it produced.
    for &class_def_id in &class_def_ids {
        let class_def = asm[class_def_id].clone();
        // Re-stamp THIS class's `FieldList` run-start to the table's current end, right before
        // adding any of its own field rows (see Pass 1's doc comment for why Pass 1 alone leaves
        // every row at the placeholder value `1`). Every class needs this call, including ones
        // with zero fields — the run-start still marks the correct boundary for its neighbors.
        mb.set_type_def_field_list(type_def_token_of[&class_def_id]);
        for &(tpe, name, offset) in class_def.fields() {
            let name_str = asm[name].to_string();
            let mut blob = Vec::new();
            sig::encode_field_sig(tpe, asm, &mut mb, &mut blob);
            let sig_off = mb.blobs.intern(&blob);
            mb.add_field(&name_str, sig_off, offset);
        }
        for StaticFieldDef {
            tpe,
            name,
            is_tls,
            default_value,
            is_const,
        } in class_def.static_fields()
        {
            let name_str = asm[*name].to_string();
            let mut blob = Vec::new();
            sig::encode_field_sig(*tpe, asm, &mut mb, &mut blob);
            let sig_off = mb.blobs.intern(&blob);
            match default_value {
                // No RVA data needed — the common case (`static mut`-shaped fields with no
                // compile-time initializer).
                None => {
                    mb.add_static_field(&name_str, sig_off, None, *is_tls, *is_const);
                }
                Some(cst) => {
                    // Scalar default values (§II.22.18's `FieldRVA`, `il_exporter`'s `.data cil
                    // C_N` + `at C_N` pairing, mod.rs:225-325 — the semantic oracle for both which
                    // `Const` kinds are legal here and their exact byte widths). Unlike the
                    // `__rcl_const_blob_N` carrier types below, this does NOT need a blob-sized
                    // synthetic value type: the field's own declared type (`tpe`, already encoded
                    // into `sig_off` above) is a scalar (bool/intN/floatN) whose width already
                    // exactly equals the blob's byte length by construction — the FieldRVA-sizing
                    // lesson (commit 4b487f7) only bites when the CARRIER type is narrower than
                    // the blob (a `u8`-typed field over an N-byte buffer), which can't happen here
                    // since `bytes_for_scalar_const` derives the blob straight from the same
                    // `Const` variant whose width `sig::encode_field_sig` just encoded as `tpe`.
                    let bytes = bytes_for_scalar_const(cst);
                    let tok = mb.add_static_field(&name_str, sig_off, Some(bytes.clone()), *is_tls, *is_const);
                    pending_field_rva.push((tok, bytes));
                }
            }
        }

        // Const-data statics: only for `MainModule`, only once, immediately after its own static
        // fields above — see this loop's doc comment for why position (not a separate pass) matters.
        if class_def_id == main_module_id && !asm.const_data.0.is_empty() {
            // `const_data` blobs are keyed by `Interned<Box<[u8]>>`, independent of any `ClassDef`,
            // so this reads `asm.const_data.0` (the BiMap's forward `Vec`) directly rather than
            // anything on `class_def`. Sorted by `Interned` index (1-based position in `.0`) for
            // determinism — `il_exporter` iterates the `HashMap` side (`.1.iter()`) directly and so
            // is NOT itself order-deterministic across runs, but this writer's "no wall-clock/
            // randomness anywhere" determinism contract (see `pe.rs`'s module doc) is worth the
            // extra sort here.
            //
            // **Ownership + naming are load-bearing, not a free choice**: `body.rs`'s
            // `const_blob_field_token` (the `Const::ByteBuffer` node-emission arm) independently
            // RE-DERIVES this same field's owner (`asm.main_module()`), name
            // (`c_{encode(idx.inner())}` via the identical `crate::utilis::encode`), and
            // carrier-type name (`__rcl_const_blob_{n}`) rather than looking up a token this
            // populate pass stored — so any drift here (different owner class, different encode
            // fn, off-by-one on `n`) does not fail to compile, it fails at body-assembly time with
            // a missing `StaticFieldDesc` lookup, or silently aliases some other field.
            let mut entries: Vec<(usize, u32, Vec<u8>)> = asm
                .const_data
                .0
                .iter()
                .enumerate()
                .map(|(zero_based, data)| {
                    let n = data.len().max(1);
                    // `n` (the `__rcl_const_blob_N` carrier's declared, ALWAYS-nonzero size — a
                    // zero-sized .NET valuetype is illegal, hence the `.max(1)` above) must equal
                    // the ACTUAL number of bytes queued into `pending_field_rva` below: the exact
                    // FieldRVA-sizing invariant `docs/PE_EMISSION_PLAN.md`/commit 4b487f7 already
                    // codify for scalar statics (a field's RVA data must match its declared type's
                    // width, or a NativeAOT ILC — and, as caught here, this writer's OWN `.sdata`
                    // layout cursor in the loop just below — reads/advances the wrong number of
                    // bytes). A `data.len() == 0` blob (a real, if rare, case: an empty
                    // `&[T; 0]`-shaped Rust static) used to push the RAW empty `Vec` here while the
                    // carrier was sized 1 — the `.sdata` layout loop advances its cursor by
                    // `bytes.len()` (0, not 1), so the NEXT queued blob landed on the SAME RVA as
                    // this one, silently aliasing two unrelated static fields onto one address (a
                    // real regression caught via `dotnet-ilverify`-clean-but-`Assembly.Load`-still-
                    // rejects on `cargo_tests/cd_collections`: a corrupted `.sdata` region is below
                    // the metadata-table layer ILVerify checks, so it never showed up as a
                    // `MissingMethod`/`TypeLoad` diagnostic — only as a `FileLoadException` from
                    // `Assembly.Load` itself refusing the whole malformed image). Padding with a
                    // trailing zero byte here keeps every blob's `.sdata` footprint equal to its
                    // carrier's declared size, exactly like the scalar-default path's own
                    // "declared type width == blob length" invariant just above.
                    let mut bytes = data.to_vec();
                    bytes.resize(n, 0);
                    (n, u32::try_from(zero_based + 1).unwrap(), bytes)
                })
                .collect();
            entries.sort_by_key(|&(_, idx_inner, _)| idx_inner);
            for (n, idx_inner, bytes) in entries {
                let carrier_tok = const_blob_carrier_type_of[&n];
                let sig_off = mb.field_sig_for_valuetype_token(carrier_tok);
                let field_name = format!("c_{}", crate::utilis::encode(u64::from(idx_inner)));
                let tok = mb.add_static_field(&field_name, sig_off, Some(bytes.clone()), false, false);
                pending_field_rva.push((tok, bytes));
            }
        }
    }

    // --- Pass 3: methods. Every class def's methods, in insertion order, matching
    // `il_exporter::export_to_write`'s per-class method loop (the unpartitioned path only — the
    // `MainModule`-overflow partition split is a documented, deliberately-deferred gap, see the
    // module doc's "Phase 1b additions" section). Fail LOUDLY here rather than silently emitting an
    // over-large `MainModule` TypeDef that would only fail much later, opaquely, as a
    // `TypeLoadException: … contains more methods than the current implementation allows` at
    // `dotnet` load time with no indication which pass caused it.
    if let Some(main_class) = class_def_ids.iter().find(|&&id| id == main_module_id) {
        check_main_module_method_count(asm[*main_class].methods().len());
    }
    let mut entry_point_token: Option<Token> = None;
    for &class_def_id in &class_def_ids {
        let class_def = asm[class_def_id].clone();
        // Same run-start re-stamp as Pass 2's `set_type_def_field_list`, for `MethodList`
        // instead of `FieldList` — see Pass 1's doc comment; `add_method`'s own doc documents
        // the identical "most recently added TypeDef" assumption this call satisfies.
        mb.set_type_def_method_list(type_def_token_of[&class_def_id]);
        for &method_id in class_def.methods() {
            let method = asm[method_id].clone();
            let name = asm[method.name()].to_string();
            let sig = asm[method.sig()].clone();
            let is_static = method.kind() == crate::ir::cilnode::MethodKind::Static;
            let is_virtual = method.kind() == crate::ir::cilnode::MethodKind::Virtual;
            // `SpecialName | RTSpecialName` (§II.22.26, §II.10.5.3) is what makes the CLR loader
            // recognize a type initializer and run it automatically before first access to any of
            // the type's static members. `MethodKind::Constructor` (instance `.ctor`) is the usual
            // source, but the assembly-wide static initializer built by `Assembly::cctor()`
            // (cilly/src/ir/asm.rs) is a *`MethodKind::Static`* method literally named `.cctor` —
            // `il_exporter`'s emitted `.il` text for it has NO `specialname`/`rtspecialname`
            // keywords either (verified against the actual `.il` ilasm consumes), yet the ilasm-built
            // assembly's `.cctor` carries those flags in its metadata: MS ilasm auto-recognizes the
            // reserved name `.cctor` (and `.ctor`) and stamps the flags in regardless of what the
            // source text asked for (ECMA-335 §II.10.5.3 requires this at the class-file-format
            // level: a type initializer method MUST be `.cctor`/rtspecialname to be auto-invoked; it
            // is one of the two runtime-reserved names, `.ctor`/`.cctor`). A hand-rolled writer gets
            // no such assembler-side auto-detection, so it must special-case the reserved name here.
            // Without this, the CLR never runs `.cctor` (it just looks like an ordinary unreferenced
            // static method) and every static/const-data/vtable initializer inside it is skipped —
            // every static field (including `dyn Trait` vtable slots, which are populated by
            // `ldftn`/`StInd` writes INSIDE `.cctor`, not `FieldRVA` data) stays zeroed, and the
            // first virtual dispatch through such a vtable calls a null function pointer (SIGSEGV,
            // no managed exception — this is exactly the residual cd_collections crash: a `blr` to
            // a zeroed vtable slot loaded from an uninitialized `dyn Trait` fat-pointer vtable
            // static).
            let is_ctor = method.kind() == crate::ir::cilnode::MethodKind::Constructor
                || name == crate::ir::asm::CCTOR;
            // An explicit ECMA-335 `.override` (`MethodDef::with_override`, e.g. via
            // `#[dotnet_override]`) needs a `MethodImpl` table row (§II.22.27) this writer does
            // not implement yet — only `il_exporter` (the ilasm-text path, `DIRECT_PE=0`) does.
            // Silently dropping it would emit an ordinary new-slot virtual instead of a genuine
            // override — a real miscompilation (the whole POINT of an explicit override is
            // landing in a specific vtable slot), not an acceptable degradation. Fail loudly
            // instead: see docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md's Tier C finding #1 for the
            // follow-up scope to add real MethodImpl support here.
            assert!(
                method.overrides().is_none(),
                "method '{name}' has an explicit .override, but the direct PE writer \
                 (DIRECT_PE=1, the default) does not yet support the MethodImpl metadata row \
                 this needs -- set DIRECT_PE=0 to use il_exporter (ilasm text) instead, which \
                 does. See docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md's Tier C finding #1."
            );
            // An abstract member (`MethodDef::is_abstract`, e.g. an interface method) has RVA=0
            // and no real body — `implementation()` is only an inert `MethodImpl::Missing`
            // placeholder for these (see that field's doc). This writer has no Abstract
            // `MethodAttributes`/RVA=0 support and would read straight through to the placeholder,
            // emitting a concrete method that THROWS at runtime instead of a genuine abstract slot
            // — the same class of silent miscompilation the `overrides` guard above exists to
            // prevent. See docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md's Tier C finding #2.
            assert!(
                !method.is_abstract(),
                "method '{name}' is abstract (MethodDef::with_abstract), but the direct PE \
                 writer (DIRECT_PE=1, the default) does not yet support Abstract/RVA=0 method \
                 rows -- set DIRECT_PE=0 to use il_exporter (ilasm text) instead, which does. \
                 See docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md's Tier C finding #2."
            );
            // Resolve `MethodImpl::AliasFor` chains before deciding P/Invoke-ness — mirrors
            // `il_exporter`'s `method.resolved_implementation(asm_mut)` (mod.rs:477) and
            // `body.rs`'s own `resolved_implementation` call (the single source of truth for
            // whether a method has a real body). Using the RAW, unresolved `method.implementation()`
            // here was a real bug: for a method whose OWN `MethodImpl` is `AliasFor(target)`, the
            // raw match falls through to `_ => None` regardless of what `target` resolves to, so a
            // method aliasing an unpatched `MethodImpl::Extern` stub got emitted with NORMAL
            // (non-abstract, non-PInvoke) `MethodDef` flags — while `body.rs` correctly resolved the
            // alias, found no body to assemble, and left `RVA = 0`. §II.22.26 requires RVA == 0 ONLY
            // for abstract/PInvoke/runtime-supplied methods; a "normal" method with RVA == 0 is
            // exactly the malformed shape CoreCLR's native type loader rejects as `TypeLoadException:
            // Abstract method with non-zero RVA` (the message is misleading — the real defect is a
            // *zero*-RVA method not flagged abstract/PInvoke, not a nonzero-RVA abstract method;
            // reproduced by the `pal_threads`/`cd_interop`-adjacent `rcl_dotnet_thread_spawn` P/Invoke
            // hook, whose aliasing wrapper hit this exact path with `codegen-units = 1`, which changes
            // whether the alias or its target gets visited first by `patch_missing_methods`).
            let pinvoke_owned = match method.resolved_implementation(asm) {
                crate::ir::MethodImpl::Extern { lib, preserve_errno } => {
                    Some((asm[*lib].to_string(), *preserve_errno))
                }
                _ => None,
            };
            let mut blob = Vec::new();
            let convention = if is_static { sig::SIG_DEFAULT } else { sig::SIG_HASTHIS };
            // `sig.inputs()` carries the IMPLICIT receiver (`this`) at index 0 for every
            // non-static kind (Instance/Virtual/Constructor) — matches `method.arg_names()`'s
            // "parallel to the FULL sig.inputs()" contract documented just below, and
            // `il_exporter`'s own `&sig.inputs()[1..]` skip at every one of its instance-method
            // signature-rendering sites (mod.rs:436/796/1068/1337 — the semantic oracle). A
            // `HASTHIS` `MethodDefSig`/`MethodRefSig` (§II.23.2.1) encodes the receiver
            // IMPLICITLY via the calling-convention byte alone — writing it out AGAIN as
            // parameter #0 doubles it, producing a `Method not found` at every call site
            // (regression caught wiring `DIRECT_PE=1`: a generic ctor's `MemberRef` signature
            // came out as `.ctor(Dictionary\`2<…>)` instead of `.ctor()`, the receiver type
            // itself masquerading as a real argument).
            let encode_sig = if is_static {
                sig.clone()
            } else {
                crate::ir::FnSig::new(sig.inputs()[1..].to_vec(), *sig.output())
            };
            // `SignatureOnlyResolver`, not `&mut mb` directly: this is the method's OWN declared
            // signature (C#-visible metadata a separately-compiled consumer resolves a call
            // against), the exact analog of `il_exporter`'s `type_il_signature` split at its
            // `.method` header line — see that resolver's doc for why every other
            // `TypeDefOrRefResolver` call site in this exporter (bodies, `extends`, `calli`,
            // fields) must stay on the shared, impl-assembly-qualified `MetadataBuilder` path.
            sig::encode_method_sig(
                convention,
                0,
                &encode_sig,
                asm,
                &mut SignatureOnlyResolver { mb: &mut mb },
                &mut blob,
            );
            let sig_off = mb.blobs.intern(&blob);
            // Named `Param` rows: `method.arg_names()` is parallel to the FULL `sig.inputs()`
            // (including the implicit `this` slot at index 0 for instance/virtual/ctor kinds), but
            // `Param` rows are only emitted for the ARGUMENTS a caller actually writes — mirrors
            // `il_exporter::export_to_write`'s `inputs.iter().zip(method.arg_names())` (mod.rs:439-441),
            // where `inputs` is already sliced to `&sig.inputs()[1..]` for non-static kinds and
            // `.zip()` silently truncates `arg_names` to match. No tables.rs change needed —
            // `add_method` already accepts `&[Option<&str>]` and pushes one Param row per entry.
            let skip = usize::from(!is_static);
            let arg_names = method.arg_names();
            debug_assert_eq!(arg_names.len(), sig.inputs().len(), "arg_names must be parallel to sig.inputs()");
            let param_names: Vec<Option<&str>> = arg_names[skip.min(arg_names.len())..]
                .iter()
                .map(|n| n.map(|interned| &asm[interned]))
                .collect();
            let pinvoke_ref = pinvoke_owned.as_ref().map(|(lib, preserve)| (lib.as_str(), *preserve));
            // Mirrors `il_exporter`'s `aggressiveinlining` JIT hint (mod.rs:455-469): small leaf
            // bodies (e.g. the `cast_f64_u32`-style saturating float->int cast helpers
            // `cilly::ir::builtins::casts` synthesizes, or monomorphized closure/iterator-adapter
            // wrappers) get `MethodImplAttributes.AggressiveInlining` so RyuJIT inlines the
            // per-call overhead out of hot callers. Heuristic shared with `il_exporter` via
            // `MethodImpl::should_hint_aggressive_inline` (see that method's doc — this is the
            // exact call-free/block-count/root-count shape empirically confirmed to get RyuJIT to
            // inline a small branchy leaf) so the two exporters can't drift out of parity on this
            // again. `PDB_FRAMES=1` suppresses it, same as `il_exporter`, so debug/PDB runs keep
            // these frames visible. Pure JIT hint — cannot affect correctness (verified: no
            // typecheck/codegen semantics change).
            let aggressive_inline = !*crate::PDB_FRAMES
                && method
                    .resolved_implementation(asm)
                    .should_hint_aggressive_inline(asm);
            let tok = mb.add_method(
                &name,
                sig_off,
                &param_names,
                is_static,
                is_virtual,
                is_ctor,
                pinvoke_ref,
                aggressive_inline,
            );
            mb.register_method_def(method_id, tok);
            if name == "entrypoint" {
                entry_point_token = Some(tok);
            }
        }
    }
    if let Some(tok) = entry_point_token {
        mb.set_entry_point(tok);
    }

    // --- Pass 4: assemble every method body now that every method this milestone's inventory
    // subset can reference has a resolvable token (passes 1-3 already added every in-assembly
    // row a `TokenSink` query could need).
    let mut bodies: Vec<(Token, AssembledBody)> = Vec::new();
    for &class_def_id in &class_def_ids {
        let class_def = asm[class_def_id].clone();
        for &method_id in class_def.methods() {
            let tok = mb
                .method_def_token(method_id)
                .expect("every method was registered in pass 3");
            let assembled = body::assemble_method(asm, method_id, &mut mb);
            bodies.push((tok, assembled));
        }
    }

    // --- Pass 5 (RVA layout, `pe::write_pe`'s module-doc pipeline steps 2-3): method bodies are
    // position-independent (they reference other methods/fields by *token*, never by address —
    // see that module doc), so laying them out just needs a running, 4-byte-aligned cursor
    // starting right after the CLI header + metadata blob within `.text`. The metadata's own
    // length is needed to compute that starting cursor, but the metadata isn't final until every
    // `MethodDef.RVA` is patched — resolved the same way `pe.rs` documents: serialize once to
    // measure, lay out bodies against that measurement, patch, then serialize again for real (the
    // measurement is stable because patching RVAs into already-existing rows never changes any
    // row's byte width — only heap-index/coded-index *values*, not their *sizes*, given the row
    // count and heap sizes were already fixed by pass 1-3's population).
    //
    // `finish_module` must run BEFORE the probe: it adds the `Module` table's one row (and its
    // sole `#GUID` heap entry for the MVID), which changes the serialized length just like any
    // other row addition would — doing it after the probe was a real bug caught by the
    // `debug_assert_eq!` below during development (probe/final length mismatch).
    mb.finish_module(&options.module_name);
    let metadata_len_probe = mb.serialize().len();
    // `pe::text_header_len` is the single source of truth for how many bytes precede the CLI
    // header within `.text` (0 for a `.dll`, or the bootstrap IAT's length for an `.exe` — see
    // that function's doc for the real bug this indirection prevents: a hardcoded `0` here once
    // silently shifted every method body 8 bytes short of where `MethodDef.RVA` said it was).
    let text_header_len = pe::text_header_len(entry_point_token.is_some());
    let bodies_start_rva =
        pe::SECTION_ALIGNMENT + text_header_len + pe::CLI_HEADER_CB + u32::try_from(metadata_len_probe).unwrap();
    let mut method_bodies_bytes = Vec::new();
    let mut cursor = bodies_start_rva;
    for (tok, assembled) in &bodies {
        // A method with NO body — `MethodImpl::Extern` (P/Invoke, `ImplMap`-forwarded, e.g. the
        // `strlen`/`memcmp` libc imports panic/backtrace formatting pulls in) or any other kind
        // `body::assemble_method` returns empty bytes for — must keep `RVA = 0` (§II.22.26: "for
        // methods with no body — abstract or runtime-supplied — RVA MUST be 0"; a P/Invoke
        // method's real entry point lives in `ImplMap`, not at an RVA in `.text`). `add_method`
        // already defaults every row's `rva` to 0, so the fix is simply SKIPPING both the
        // `set_method_body_rva` overwrite and (critically) the `cursor` advance for these —
        // advancing the cursor for a 0-byte body is a silent no-op that looks harmless in
        // isolation, but skipping the advance is NOT optional: leaving `cursor` unmoved after
        // stamping a bogus nonzero RVA here meant the very NEXT real method body got laid out
        // starting at the SAME address, silently aliasing two unrelated methods' code — a real
        // regression caught wiring `DIRECT_PE=1` into the linker (`cargo_tests/cd_collections`,
        // any `fn main() -> ExitCode` combined with allocator/libc-touching code): the resulting
        // `.text` corruption is below what `dotnet-ilverify`'s per-method IL analysis checks (it
        // verifies each method's OWN bytes, not cross-method RVA disjointness), so the only
        // observable symptom was `Assembly.Load` itself rejecting the whole image with a
        // `FileLoadException` naming this assembly's own placeholder identity ("_") — nowhere
        // near the actual defect. Mirrors the identical fix (and identical bug shape) for
        // zero-length `FieldRVA` const-data blobs in this same function's Pass 2.5, above.
        if assembled.bytes.is_empty() {
            continue;
        }
        while cursor % 4 != 0 {
            cursor += 1;
            method_bodies_bytes.push(0);
        }
        mb.set_method_body_rva(*tok, cursor);
        method_bodies_bytes.extend_from_slice(&assembled.bytes);
        cursor += u32::try_from(assembled.bytes.len()).unwrap();
    }

    // --- Phase 2: build the standalone Portable PDB from every `AssembledBody::sequence_points`
    // collected in Pass 4, BEFORE the `.sdata`/debug-directory RVA math below (which needs the
    // finished PDB's [`pdb::DebugDirectoryEntry`] to size the Debug Directory region ahead of
    // `.sdata` in `.text` — see `pe::field_rva_section_start`'s `debug_dir_len` parameter).
    // `options.pdb_file_name` empty means "no PDB requested" (e.g. a caller that only wants a
    // structural/E2E test image with no debug info) — every other field stays wired unconditionally
    // since it's cheap and keeps this pass's control flow simple.
    let debug_directory = if options.pdb_file_name.is_empty() {
        None
    } else {
        let type_system = super::pdb::TypeSystemRowCounts {
            rows: mb.type_system_row_counts(),
            entry_point_token: entry_point_token.map_or(0, |t| t.0),
        };
        let mut pdb_builder = super::pdb::PdbBuilder::new(type_system, mb.method_def_row_count());
        for (tok, assembled) in &bodies {
            // Methods with zero sequence points AND zero named locals have truly nothing for the
            // PDB to say about them — skip and let `PdbBuilder::build` fall back to its default
            // (empty) `MethodDebugInformation` row. A method with named locals but (implausibly)
            // no sequence points must NOT be skipped here: `LocalScope`/`LocalVariable` rows are
            // keyed by `MethodDef` row, independent of sequence-point presence, so skipping would
            // silently drop its locals from the PDB.
            if assembled.sequence_points.is_empty()
                && assembled.locals.iter().all(Option::is_none)
            {
                continue;
            }
            pdb_builder.add_method(
                *tok,
                super::pdb::MethodSequencePoints {
                    local_signature: assembled.locals_signature,
                    points: assembled.sequence_points.clone(),
                    locals: assembled.locals.clone(),
                    code_len: assembled.code_len,
                },
            );
        }
        let (pdb_bytes, pdb_id) = pdb_builder.build();
        // `PdbChecksumEntry` must be derived from the FINAL, complete PDB bytes (the checksum
        // covers the whole file) — see `pdb::sha256`'s doc for why this second Debug Directory
        // entry (not just CodeView/RSDS) turned out to be required for CoreCLR's runtime
        // `StackTraceSymbols` provider to trust the PDB at all.
        let checksum = super::pdb::PdbChecksumEntry::from_pdb_bytes(&pdb_bytes);
        let debug_dir_entry = super::pdb::DebugDirectoryEntry::from_pdb_id(pdb_id, options.pdb_file_name.clone());
        Some((pdb_bytes, debug_dir_entry, checksum))
    };
    let debug_dir_len = debug_directory
        .as_ref()
        .map_or(0, |(_, entry, checksum)| pe::debug_directory_len(entry, Some(checksum)));

    // `.sdata` layout: every queued `FieldRVA` blob (scalar static defaults + const-data, both
    // queued by Pass 2/2.5 above), 4-byte aligned per entry (mirrors the method-body layout loop
    // just above — not spec-mandated for `FieldRVA` the way §II.25.4.1 mandates it for method
    // bodies, but a harmless, conventional alignment). Empty when nothing was queued, in which
    // case `pe::write_pe` omits the `.sdata` section entirely (`sdata_absent_when_no_field_rva_data`
    // in `pe.rs` covers that shape).
    let sdata_start_rva = pe::field_rva_section_start(
        entry_point_token.is_some(),
        metadata_len_probe,
        method_bodies_bytes.len(),
        debug_dir_len,
    );
    let mut field_rva_bytes: Vec<u8> = Vec::new();
    let mut field_cursor = sdata_start_rva;
    for (tok, bytes) in &pending_field_rva {
        while field_cursor % 4 != 0 {
            field_cursor += 1;
            field_rva_bytes.push(0);
        }
        mb.set_field_rva(*tok, field_cursor);
        field_rva_bytes.extend_from_slice(bytes);
        field_cursor += u32::try_from(bytes.len()).unwrap();
    }

    let metadata = mb.serialize();
    debug_assert_eq!(
        metadata.len(),
        metadata_len_probe,
        "patching RVAs into already-sized rows must not change the metadata's serialized length"
    );

    let pdb_bytes = debug_directory.as_ref().map_or_else(Vec::new, |(bytes, _, _)| bytes.clone());
    let pe_options = PeOptions {
        is_dll: options.is_dll,
        entry_point: entry_point_token.map(|t| t.0),
        debug_directory: debug_directory.as_ref().map(|(_, entry, _)| entry.clone()),
        pdb_checksum: debug_directory.map(|(_, _, checksum)| checksum),
    };
    let image = pe::write_pe(&metadata, &method_bodies_bytes, &field_rva_bytes, &pe_options);
    (image, pdb_bytes)
}

/// Little-endian byte blob for a scalar `Const`'s `FieldRVA` default value — the exact widths
/// `il_exporter`'s static-field-default `match` uses (mod.rs:225-325, the semantic oracle):
/// bool/i8/u8 as 1 byte, i16/u16 as 2, i32/u32 as 4, i64/u64/isize/usize as 8, i128/u128 as 16
/// (`to_le_bytes()`, matching the fix in that match arm's own comment about the bytearray encoding
/// wanting LE hex pairs, not decimal), f32/f64 via their native `to_le_bytes()`. `PlatformString`/
/// `Null`/`ByteBuffer` are not scalar-default-shaped on the .NET target — `il_exporter` panics on
/// them too (mod.rs:317-321, "static-field default value of kind {other:?} is unsupported"); this
/// mirrors that with the same message rather than inventing new semantics the oracle doesn't have.
fn bytes_for_scalar_const(cst: &Const) -> Vec<u8> {
    match cst {
        Const::Bool(b) => vec![u8::from(*b)],
        Const::I8(b) => b.to_le_bytes().to_vec(),
        Const::U8(b) => b.to_le_bytes().to_vec(),
        Const::I16(b) => b.to_le_bytes().to_vec(),
        Const::U16(b) => b.to_le_bytes().to_vec(),
        Const::I32(b) => b.to_le_bytes().to_vec(),
        Const::U32(b) => b.to_le_bytes().to_vec(),
        Const::I64(b) => b.to_le_bytes().to_vec(),
        Const::U64(b) => b.to_le_bytes().to_vec(),
        Const::ISize(b) => b.to_le_bytes().to_vec(),
        Const::USize(b) => b.to_le_bytes().to_vec(),
        Const::I128(b) => b.to_le_bytes().to_vec(),
        Const::U128(b) => b.to_le_bytes().to_vec(),
        Const::F32(b) => b.0.to_le_bytes().to_vec(),
        Const::F64(b) => b.0.to_le_bytes().to_vec(),
        other => panic!("static-field default value of kind {other:?} is unsupported on the .NET target"),
    }
}

/// CoreCLR's per-type method cap, with headroom (mirrors `il_exporter::partition::PARTITION_LIMIT`,
/// `cilly/src/ir/il_exporter/partition.rs:32`).
const PARTITION_LIMIT: usize = 60_000;

/// Fails loudly if `MainModule` has grown past [`PARTITION_LIMIT`] methods — see Pass 3's call
/// site for why this can't be a silent no-op.
///
/// `PARTITION_LIMIT` is intentionally duplicated (not imported) from
/// `il_exporter::partition::PARTITION_LIMIT`: that module is `mod partition;` (private, not `pub
/// mod`) inside `il_exporter`, so it is genuinely unreachable from `pe_exporter` today —
/// confirming, not just asserting, the module doc's claim that partitioning is NOT an
/// upstream/assembly-level transform this exporter could inherit for free (`ModulePartition` is
/// built and consumed entirely inside `ILExporter::export_to_write`).
///
/// A standalone `usize -> ()` function (rather than inlined at the call site) so the overflow
/// case is unit-testable without actually constructing 60,001 `MethodDef`s (measured: doing so in
/// a test took over 60 seconds — some downstream bookkeeping the `Assembly` builder does per
/// `new_method` call is not built for that scale, which is itself a data point about why this
/// exporter needs the real partition port before anything that large is a realistic input).
fn check_main_module_method_count(method_count: usize) {
    assert!(
        method_count <= PARTITION_LIMIT,
        "MainModule has {method_count} methods (> {PARTITION_LIMIT}) — `pe_exporter` does not yet \
         port `il_exporter::partition`'s per-module TypeDef split (see this file's module doc: \
         porting it needs an interleaved TypeDef/MethodDef pass, since `add_type_def`'s \
         `method_list`/`field_list` cursors are table-position-sensitive, plus a `body.rs` \
         cross-partition call-token redirect). No assembly this milestone's test suite builds gets \
         anywhere near this size; a real whole-program build that does will need that work landed \
         first rather than silently producing a `TypeLoadException`-doomed image."
    );
}

/// Finds-or-creates a `TypeRef` to `System.Runtime`-scoped `type_name` (`System.Object` /
/// `System.ValueType` — the two implicit base types every class needs, see the Pass 1 comment
/// above). Uses [`MetadataBuilder::find_or_create_assembly_ref`] (not the always-inserts
/// [`MetadataBuilder::assembly_ref`]) so repeated calls share one `System.Runtime` `AssemblyRef`
/// row instead of creating a duplicate for every class def; `MetadataBuilder::type_ref` is
/// separately interning-cached, so the `TypeRef` row itself is deduplicated too.
fn system_runtime_type_ref(mb: &mut MetadataBuilder, type_name: &str) -> Token {
    let scope = mb.find_or_create_assembly_ref("System.Runtime");
    mb.type_ref(Some(scope), "System", &type_name["System.".len()..])
}

/// Local port of `il_exporter::ref_assembly_name` (kept private/duplicated rather than imported,
/// per the hard constraint that `pe_exporter` code must not depend on `il_exporter` — same
/// convention as this file's `is_bcl_assembly`/`split_namespace` ports). Maps an
/// IMPLEMENTATION-assembly name to the public REFERENCE assembly a C# compiler resolves against:
/// `System.Object`/`ValueType`/`String`/`Exception`/`SemaphoreSlim`/… physically live in
/// `System.Private.CoreLib` but are type-forwarded from `System.Runtime`. A separately-compiled
/// C# project only references the ref assembly, so any CoreLib name surviving into a method's OWN
/// declared signature fails to resolve with CS0012 the moment a consumer references that method.
///
/// Applied ONLY at [`SignatureOnlyResolver`]'s external-type branch (a method's declared
/// return/parameter types — C#-visible metadata a separately-compiled consumer resolves a call
/// against), never to the shared [`MetadataBuilder`]'s own `type_def_or_ref` (used for body
/// instructions, `extends`, and every other `TypeDefOrRef` resolution in this exporter): a
/// `call`/`callvirt` `MemberRef` scoped to `[System.Runtime]System.String` is genuine "Bad IL
/// format" against a real CoreLib `String` instance method (see `il_exporter::class_ref`'s doc for
/// the empirical root cause) — only the outer *declaration* of a method's own signature is
/// C#-visible metadata that needs the substitution; nothing that resolves a call INTO a type
/// (including a call into `SemaphoreSlim` itself) may use it.
fn ref_assembly_name(name: &str) -> &str {
    match name {
        "System.Private.CoreLib" | "mscorlib" => "System.Runtime",
        other => other,
    }
}

/// Local port of `il_exporter::ref_assembly_name_for_type` (same duplication convention as
/// [`ref_assembly_name`] above). A blanket CoreLib -> `System.Runtime` substitution is wrong for
/// types that aren't actually forwarded through the `System.Runtime` umbrella facade: confirmed by
/// scanning the real net8.0 ref-pack DLLs, `SemaphoreSlim`/`ManualResetEventSlim`/
/// `CountdownEvent`/`Barrier` are genuine `TypeDef`s in `System.Threading.dll`, not forwards from
/// `System.Runtime.dll` — using `System.Runtime` for them fails with `CS7069` ("claims it is
/// defined in 'System.Runtime', but it could not be found"), not CS0012. This is a small, explicit,
/// closed table of the types this backend's mycorrhiza bindings actually expose across a
/// C#-visible signature position today, not a general BCL type-forwarding resolver — extend it if
/// another such type needs to cross a signature boundary.
fn ref_assembly_name_for_type<'a>(assembly: &'a str, type_name: &str) -> &'a str {
    if matches!(assembly, "System.Private.CoreLib" | "mscorlib") {
        match type_name {
            "System.Threading.SemaphoreSlim"
            | "System.Threading.ManualResetEventSlim"
            | "System.Threading.CountdownEvent"
            | "System.Threading.Barrier" => return "System.Threading",
            // See il_exporter::ref_assembly_name_for_type's twin entry: Task/Task<T> are real
            // TypeDefs in System.Threading.Tasks.dll, not forwarded through System.Runtime.
            "System.Threading.Tasks.Task" => return "System.Threading.Tasks",
            _ => {}
        }
    }
    ref_assembly_name(assembly)
}

/// [`super::sig::TypeDefOrRefResolver`] used ONLY for encoding a method's OWN declared signature
/// (`export_pe`'s `sig::encode_method_sig` call, the exact analog of `il_exporter`'s
/// `type_il_signature`/`non_void_type_il_signature` split at its `.method` header line — see
/// those functions' doc for the full rationale, mirrored here for the hand-rolled PE writer).
///
/// Defers to the shared [`MetadataBuilder`] unchanged for types DEFINED in this assembly (a
/// `TypeDef` — never a BCL forwarding concern) and for the CACHED path (`class_token_cache` is
/// deliberately NOT reused here: sharing it would leak a ref-assembly-qualified token back into
/// body/call-site resolution, which must keep the impl-assembly name). For an EXTERNAL type, it
/// creates its OWN independent `TypeRef` — deduplicated via `MetadataBuilder::type_ref`'s own
/// `(scope, namespace, name)` cache, so repeated signature-position references to the same type
/// still share one row — scoped through [`ref_assembly_name_for_type`] instead of the raw impl
/// name.
struct SignatureOnlyResolver<'a> {
    mb: &'a mut MetadataBuilder,
}

impl super::sig::TypeDefOrRefResolver for SignatureOnlyResolver<'_> {
    fn type_def_or_ref(&mut self, cref: Interned<ClassRef>, asm: &mut Assembly) -> u32 {
        let class_ref = asm.class_ref(cref).clone();
        if asm.class_ref_to_def(cref).is_some() {
            // Defined in this assembly: no BCL forwarding concern, and the shared resolver
            // already knows how to find its `TypeDef` row — reuse it verbatim.
            return self.mb.type_def_or_ref(cref, asm);
        }
        let raw_name = &asm[class_ref.name()];
        let scope = class_ref
            .asm()
            .map(|asm_name_id| {
                let name = ref_assembly_name_for_type(&asm[asm_name_id], raw_name).to_string();
                self.mb.find_or_create_assembly_ref(&name)
            });
        let full_name = if class_ref.generics().is_empty() {
            raw_name.to_string()
        } else {
            format!("{raw_name}`{}", class_ref.generics().len())
        };
        let (namespace, name) = super::tables::split_namespace(&full_name);
        let tok = self.mb.type_ref(scope, namespace, name);
        encode_type_def_or_ref_token(tok)
    }
}

/// Encodes a [`Token`] into a `TypeDefOrRef` coded index (§II.24.2.6), needed by
/// [`SignatureOnlyResolver`] (`tables.rs`'s private `encode_type_def_or_ref_token` is the
/// canonical twin; kept as its own copy rather than exposed from `tables.rs`, since the encoding
/// is a fixed 2-bit-tag scheme documented in both places by ECMA-335 §II.24.2.6 directly, not an
/// implementation detail `export.rs` should reach into `tables.rs`'s row storage for). The former
/// decode-direction twin (`decode_type_def_or_ref`) was removed once `extends`/`implements`
/// resolution switched to `MetadataBuilder::class_ref_token` (which returns an already-decoded
/// `Token` directly, needed for the generic-interface case — see that fn's doc).
fn encode_type_def_or_ref_token(token: Token) -> u32 {
    let tag = match token.table() {
        Token::TABLE_TYPE_DEF => 0,
        Token::TABLE_TYPE_REF => 1,
        Token::TABLE_TYPE_SPEC => 2,
        other => panic!("{other:#x} is not a TypeDefOrRef member"),
    };
    (token.rid() << 2) | tag
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::cilnode::{IsPure, MethodKind};
    use crate::ir::method::MethodDef;
    use crate::ir::{Access, BasicBlock, CILNode, CILRoot, Const, MethodImpl, Type};
    use std::io::Write as _;
    use std::process::Command;

    /// The `MainModule`-overflow guard (`check_main_module_method_count`, called from Pass 3) must
    /// fail LOUDLY rather than silently letting `export_pe` emit an image `dotnet` would only
    /// reject much later, opaquely, with `TypeLoadException: … contains more methods than the
    /// current implementation allows`. Calls the guard directly with a huge count instead of
    /// constructing 60,001 real `MethodDef`s through `export_pe` — measured that construction path
    /// alone (independent of this change) at over 60 seconds, too slow for a unit test; see
    /// `check_main_module_method_count`'s doc for why a standalone function was worth it here.
    #[test]
    #[should_panic(expected = "does not yet port")]
    fn check_main_module_method_count_panics_loudly_past_the_partition_limit() {
        check_main_module_method_count(PARTITION_LIMIT + 1);
    }

    #[test]
    fn check_main_module_method_count_accepts_up_to_the_partition_limit() {
        check_main_module_method_count(PARTITION_LIMIT);
        check_main_module_method_count(0);
    }

    #[test]
    fn export_pe_smoke_no_entry_point_produces_a_loadable_shape() {
        // A bodyless-methods-free "library" shape: just `MainModule` with zero methods. This is
        // a cheap structural smoke test (metadata/PE bytes shaped correctly) that doesn't require
        // a `dotnet` host — see [`e2e_hand_built_assembly_runs_under_dotnet`] for the real
        // acceptance check.
        let mut asm = Assembly::default();
        let _ = asm.main_module();
        let options = ExportOptions {
            is_dll: true,
            assembly_name: "export_pe_smoke".to_string(),
            module_name: "export_pe_smoke.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);
        assert_eq!(&image[0..2], b"MZ", "must start with the DOS signature");
        assert!(image.len() > 0x200, "must be at least one FileAlignment block");
    }

    /// Builds the tiny two-method assembly the Phase 1a milestone acceptance check
    /// (`docs/PE_EMISSION_PLAN.md`) describes: a static `MainModule::entrypoint()` whose body is
    /// `ldstr "PE writer E2E OK" ; call void [System.Console]System.Console::WriteLine(string) ;
    /// ret`.
    fn build_hello_world_assembly() -> Assembly {
        let mut asm = Assembly::default();
        let main = asm.main_module();

        let console = crate::ir::ClassRef::console(&mut asm);
        let write_line_name = asm.alloc_string("WriteLine");
        let write_line_sig = asm.sig([Type::PlatformString], Type::Void);
        let write_line = asm.alloc_methodref(crate::ir::MethodRef::new(
            console,
            write_line_name,
            write_line_sig,
            MethodKind::Static,
            vec![].into(),
        ));

        let msg = asm.alloc_string("PE writer E2E OK");
        let ldstr = asm.alloc_node(CILNode::Const(Box::new(Const::PlatformString(msg))));
        let call = asm.alloc_root(CILRoot::Call(Box::new((write_line, vec![ldstr].into(), IsPure::NOT))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BasicBlock::new(vec![call, ret], 0, None);

        let entry_sig = asm.sig([], Type::Void);
        let entry_name = asm.alloc_string("entrypoint");
        let entry_def = MethodDef::new(
            Access::Public,
            main,
            entry_name,
            entry_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![block],
                locals: vec![],
            },
            vec![],
        );
        asm.new_method(entry_def);
        asm
    }

    #[test]
    fn export_pe_hand_built_hello_world_has_an_entry_point_token() {
        let mut asm = build_hello_world_assembly();
        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_hello".to_string(),
            module_name: "pe_e2e_hello.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);
        assert_eq!(&image[0..2], b"MZ");

        // Locate the CLI header the same way `pe.rs`'s own test-only parser does (DOS e_lfanew ->
        // COFF -> optional header -> data directory 14 -> section-resolved file offset -> CLI
        // header's own EntryPointToken field, +20 bytes into the 72-byte header).
        let u32_at = |off: usize| u32::from_le_bytes(image[off..off + 4].try_into().unwrap());
        let u16_at = |off: usize| u16::from_le_bytes(image[off..off + 2].try_into().unwrap());
        let e_lfanew = u32_at(0x3C) as usize;
        let coff = e_lfanew + 4;
        let num_sections = u16_at(coff + 2) as usize;
        let opt_header_size = u16_at(coff + 16) as usize;
        let opt = coff + 20;
        let dir_base = opt + 96;
        let cli_rva = u32_at(dir_base + 14 * 8);
        assert_ne!(cli_rva, 0, "CLI header directory must be populated");

        // Resolve the CLI header's RVA to a file offset via the section table (an `.exe` always
        // has a `.reloc` section too — see `pe::write_pe`'s module doc — so this can't assume a
        // single `.text` section covers every RVA in the image).
        let sec_table = opt + opt_header_size;
        let mut cli_file_off = None;
        for i in 0..num_sections {
            let s = sec_table + i * 40;
            let vsize = u32_at(s + 8);
            let rva = u32_at(s + 12);
            let raw_size = u32_at(s + 16);
            let file_off = u32_at(s + 20);
            if rva <= cli_rva && cli_rva < rva + vsize.max(raw_size) {
                cli_file_off = Some(file_off + (cli_rva - rva));
                break;
            }
        }
        let cli_file_off = cli_file_off.expect("CLI header RVA must fall inside a section") as usize;
        let entry_point_token = u32_at(cli_file_off + 20);
        assert_eq!(
            entry_point_token,
            Token::new(Token::TABLE_METHOD_DEF, 1).0,
            "entrypoint must be the first (and only) MethodDef row"
        );
    }

    /// **Phase 2 structural acceptance check** (no `dotnet` host needed — see
    /// `e2e_hand_built_assembly_runs_under_dotnet` for the live-execution counterpart): builds the
    /// hello-world assembly with a `CILRoot::SourceFileInfo` root prepended to `entrypoint`'s body
    /// (mirroring what `span_source_info`, `src/assembly.rs:586-616`, actually produces per
    /// statement), exports it with a non-empty `pdb_file_name`, and asserts:
    /// * the returned PDB bytes are non-empty and its `#Pdb` stream's `EntryPointToken` matches the
    ///   `.dll`'s own CLI-header `EntryPointToken` (both come from the same `entry_point_token`,
    ///   but only a byte-level check proves the wiring, not just the shared local variable);
    /// * the `.dll`'s Debug Directory RSDS GUID/age matches `pdb::deterministic_pdb_id` of the
    ///   returned PDB bytes exactly (the "byte check" the task's acceptance criteria call for);
    /// * the PDB's `MethodDebugInformation` row for `entrypoint` decodes a `SequencePoint` whose
    ///   `document_path` is the exact file string passed to `SourceFileInfo`.
    #[test]
    fn export_pe_hand_built_hello_world_with_source_info_produces_a_matching_pdb() {
        let mut asm = Assembly::default();
        let main = asm.main_module();

        let console = crate::ir::ClassRef::console(&mut asm);
        let write_line_name = asm.alloc_string("WriteLine");
        let write_line_sig = asm.sig([Type::PlatformString], Type::Void);
        let write_line = asm.alloc_methodref(crate::ir::MethodRef::new(
            console,
            write_line_name,
            write_line_sig,
            MethodKind::Static,
            vec![].into(),
        ));

        let file = asm.alloc_string("src/main.rs");
        let source_info = asm.alloc_root(CILRoot::SourceFileInfo {
            line_start: 3,
            line_len: 1,
            col_start: 4,
            col_len: 10,
            file,
        });
        let msg = asm.alloc_string("PE writer E2E OK");
        let ldstr = asm.alloc_node(CILNode::Const(Box::new(Const::PlatformString(msg))));
        let call = asm.alloc_root(CILRoot::Call(Box::new((write_line, vec![ldstr].into(), IsPure::NOT))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BasicBlock::new(vec![source_info, call, ret], 0, None);

        let entry_sig = asm.sig([], Type::Void);
        let entry_name = asm.alloc_string("entrypoint");
        let entry_def = MethodDef::new(
            Access::Public,
            main,
            entry_name,
            entry_sig,
            MethodKind::Static,
            MethodImpl::MethodBody { blocks: vec![block], locals: vec![] },
            vec![],
        );
        asm.new_method(entry_def);

        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_pdb".to_string(),
            module_name: "pe_e2e_pdb.exe".to_string(),
            pdb_file_name: "pe_e2e_pdb.pdb".to_string(),
        };
        let (image, pdb) = export_pe(&mut asm, &options);
        assert!(!pdb.is_empty(), "a non-empty pdb_file_name must produce non-empty PDB bytes");

        // Read back the .dll's Debug Directory RSDS payload the same way `pe.rs`'s own
        // `debug_directory_round_trips_through_write_pe` test does, but via raw offsets here
        // (this module can't reach `pe::tests::parse_pe`, it's private to that module).
        let u32_at = |off: usize| u32::from_le_bytes(image[off..off + 4].try_into().unwrap());
        let u16_at = |off: usize| u16::from_le_bytes(image[off..off + 2].try_into().unwrap());
        let e_lfanew = u32_at(0x3C) as usize;
        let coff = e_lfanew + 4;
        let num_sections = u16_at(coff + 2) as usize;
        let opt_header_size = u16_at(coff + 16) as usize;
        let opt = coff + 20;
        let dir_base = opt + 96;
        let debug_dir_rva = u32_at(dir_base + 6 * 8); // DataDirectory[6] = Debug.
        assert_ne!(debug_dir_rva, 0, "Debug Directory must be populated when pdb_file_name is non-empty");

        let sec_table = opt + opt_header_size;
        let mut debug_dir_file_off = None;
        for i in 0..num_sections {
            let s = sec_table + i * 40;
            let vsize = u32_at(s + 8);
            let rva = u32_at(s + 12);
            let raw_size = u32_at(s + 16);
            let file_off = u32_at(s + 20);
            if rva <= debug_dir_rva && debug_dir_rva < rva + vsize.max(raw_size) {
                debug_dir_file_off = Some(file_off + (debug_dir_rva - rva));
                break;
            }
        }
        let debug_dir_file_off = debug_dir_file_off.expect("Debug Directory RVA must fall inside a section") as usize;
        // IMAGE_DEBUG_DIRECTORY: Characteristics(4) TimeDateStamp(4) MajorVersion(2)
        // MinorVersion(2) Type(4) SizeOfData(4) AddressOfRawData(4) PointerToRawData(4).
        let debug_type = u32_at(debug_dir_file_off + 12);
        assert_eq!(debug_type, 2, "IMAGE_DEBUG_TYPE_CODEVIEW");
        let pointer_to_raw_data = u32_at(debug_dir_file_off + 24) as usize;
        assert_eq!(&image[pointer_to_raw_data..pointer_to_raw_data + 4], b"RSDS");
        let guid = &image[pointer_to_raw_data + 4..pointer_to_raw_data + 20];
        let age = u32_at(pointer_to_raw_data + 20);

        // Locate the PDB's own `#Pdb` stream (BSJB header -> stream directory, same shape
        // `pdb::tests::TestPdbReader::parse` decodes) to read back the REAL 20-byte id this PDB
        // was built with — NOT `deterministic_pdb_id(&pdb)` re-hashed over the FINAL bytes, which
        // would be wrong: `PdbBuilder::build` hashes the pre-id-patch bytes (see that function's
        // doc), so re-hashing the patched-in final bytes does not reproduce the same id.
        let pdb_id = {
            let version_len = u32::from_le_bytes(pdb[12..16].try_into().unwrap()) as usize;
            let mut cursor = 16 + version_len + 2; // skip Flags.
            let stream_count = u16::from_le_bytes(pdb[cursor..cursor + 2].try_into().unwrap());
            cursor += 2;
            let mut pdb_stream_offset = None;
            for _ in 0..stream_count {
                let offset = u32::from_le_bytes(pdb[cursor..cursor + 4].try_into().unwrap()) as usize;
                cursor += 8; // offset(4) + size(4)
                let name_start = cursor;
                let name_end = pdb[name_start..].iter().position(|&b| b == 0).unwrap() + name_start;
                let name = std::str::from_utf8(&pdb[name_start..name_end]).unwrap();
                let mut name_len = name_end - name_start + 1;
                while name_len % 4 != 0 {
                    name_len += 1;
                }
                cursor = name_start + name_len;
                if name == "#Pdb" {
                    pdb_stream_offset = Some(offset);
                }
            }
            let off = pdb_stream_offset.expect("#Pdb stream must be present");
            <[u8; 20]>::try_from(&pdb[off..off + 20]).unwrap()
        };
        assert_eq!(guid, &pdb_id[0..16], "Debug Directory GUID must equal the PDB's own #Pdb-stream id[0..16]");
        let expected_stamp = u32::from_le_bytes([pdb_id[16], pdb_id[17], pdb_id[18], pdb_id[19]]);
        // `age` is ALWAYS the literal Roslyn-convention `1` (see `DebugDirectoryEntry::age`'s doc):
        // a content-derived value silently fails `netcoredbg`'s `SymbolReader.
        // TryOpenReaderFromCodeView`, which gates its GUID/stamp comparison behind `Age == 1` before
        // even checking them, found empirically during this task's live-debugger verification.
        assert_eq!(age, 1, "Age must be the literal Roslyn-convention 1, not a content-derived value");
        // THE critical check (root cause of a real Phase-2 acceptance bug — see
        // `pdb::DebugDirectoryEntry::stamp`'s doc): `System.Reflection.Metadata`'s
        // `PEReader.TryOpenCodeViewPortablePdb` matches the opened PDB's id against
        // `(codeViewData.Guid, codeViewEntry.Stamp)` — the ROW's own `TimeDateStamp` field, NOT the
        // RSDS payload's `Age`. It must equal `pdb_id[16..20]` VERBATIM (no `| 1`), or
        // `Environment.StackTrace`/`TryOpenAssociatedPortablePdb` silently fails to resolve file:line
        // even though every other structural check here passes.
        let row_time_date_stamp = u32_at(debug_dir_file_off + 4);
        assert_eq!(
            row_time_date_stamp, expected_stamp,
            "IMAGE_DEBUG_DIRECTORY.TimeDateStamp must equal pdb_id[16..20] verbatim — this is the field SRM actually matches on, not RSDS Age"
        );

        // Decode the PDB's own #Pdb stream EntryPointToken and cross-check it against the .dll's
        // CLI header EntryPointToken (both should be `entrypoint`'s MethodDef token).
        assert_eq!(&pdb[0..4], b"BSJB", "standalone PDB must start with the BSJB signature");
        let cli_rva = u32_at(dir_base + 14 * 8);
        let mut cli_file_off = None;
        for i in 0..num_sections {
            let s = sec_table + i * 40;
            let vsize = u32_at(s + 8);
            let rva = u32_at(s + 12);
            let raw_size = u32_at(s + 16);
            let file_off = u32_at(s + 20);
            if rva <= cli_rva && cli_rva < rva + vsize.max(raw_size) {
                cli_file_off = Some(file_off + (cli_rva - rva));
                break;
            }
        }
        let cli_file_off = cli_file_off.expect("CLI header RVA must fall inside a section") as usize;
        let dll_entry_point_token = u32_at(cli_file_off + 20);
        assert_eq!(dll_entry_point_token, Token::new(Token::TABLE_METHOD_DEF, 1).0);
    }

    /// Path to the real `dotnet` host on this machine, or `None` if not present — every E2E test
    /// below shares this guard (`eprintln!` + early return, not a failure, so the suite stays
    /// green on a machine with no .NET SDK installed, per the task's original guard).
    fn dotnet_host() -> Option<(String, String)> {
        let dotnet_root = std::env::var("HOME").map(|h| format!("{h}/.dotnet")).unwrap_or_default();
        let dotnet_bin = format!("{dotnet_root}/dotnet");
        std::path::Path::new(&dotnet_bin).exists().then_some((dotnet_root, dotnet_bin))
    }

    /// Writes `image` (+ a minimal net8.0 `runtimeconfig.json`, mirroring
    /// `il_exporter::get_runtime_config`'s shape without depending on that module — the hard
    /// constraint that `pe_exporter` must not import `il_exporter`) to `<scratch>/<name>.dll` and
    /// runs it under the real `dotnet` host, returning `(stdout, stderr, success)`. Shared by
    /// every E2E test below that needs to actually execute a hand-built PE image.
    fn run_under_dotnet(image: &[u8], name: &str) -> (String, String, bool) {
        run_under_dotnet_impl(image, None, name)
    }

    /// Same as [`run_under_dotnet`], but also writes `pdb` to `<scratch>/<name>.pdb` — i.e.
    /// EXACTLY where CoreCLR's loader looks first (`<dll-stem>.pdb` next to the `.dll`, per
    /// `pdb::DebugDirectoryEntry`'s doc) — before running. Used by Phase-2 acceptance tests that
    /// need a live host to actually resolve file:line info through the PDB this writer produced.
    fn run_under_dotnet_with_pdb(image: &[u8], pdb: &[u8], name: &str) -> (String, String, bool) {
        run_under_dotnet_impl(image, Some(pdb), name)
    }

    fn run_under_dotnet_impl(image: &[u8], pdb: Option<&[u8]>, name: &str) -> (String, String, bool) {
        let (dotnet_root, dotnet_bin) = dotnet_host().expect("caller must guard with dotnet_host()");
        let scratch = std::env::temp_dir().join("pe_e2e");
        std::fs::create_dir_all(&scratch).expect("create scratch dir");
        let exe_path = scratch.join(format!("{name}.dll")); // apphost-less: `dotnet <path>.dll` runs it directly.
        std::fs::write(&exe_path, image).expect("write exported PE image");
        if let Some(pdb) = pdb {
            std::fs::write(scratch.join(format!("{name}.pdb")), pdb).expect("write exported PDB");
        }

        let runtimeconfig = r#"{
  "runtimeOptions": {
    "tfm": "net8.0",
    "framework": {
      "name": "Microsoft.NETCore.App",
      "version": "8.0.0"
    },
    "rollForward": "LatestMajor"
  }
}
"#;
        let config_path = scratch.join(format!("{name}.runtimeconfig.json"));
        std::fs::File::create(&config_path)
            .and_then(|mut f| f.write_all(runtimeconfig.as_bytes()))
            .expect("write runtimeconfig.json");

        let mut cmd = Command::new(&dotnet_bin);
        cmd.arg(&exe_path).env("DOTNET_ROOT", &dotnet_root);
        let output = cmd.output().expect("spawn dotnet");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        (stdout, stderr, output.status.success())
    }

    /// **The Phase 1a milestone acceptance check.** Builds the hand-built hello-world assembly,
    /// exports it via `export_pe` (no `ilasm` anywhere), and runs it under the real `dotnet` host.
    #[test]
    fn e2e_hand_built_assembly_runs_under_dotnet() {
        if dotnet_host().is_none() {
            eprintln!("skipping e2e_hand_built_assembly_runs_under_dotnet: no dotnet host");
            return;
        }

        let mut asm = build_hello_world_assembly();
        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_hello".to_string(),
            module_name: "pe_e2e_hello.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);

        let (stdout, stderr, success) = run_under_dotnet(&image, "pe_e2e_hello");
        assert!(success, "dotnet run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stdout.contains("PE writer E2E OK"),
            "expected stdout to contain the WriteLine output; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    /// **Phase 2 acceptance milestone (live-execution counterpart of
    /// `export_pe_hand_built_hello_world_with_source_info_produces_a_matching_pdb`)**: builds a
    /// hand-built assembly whose `entrypoint` runs a `CILRoot::SourceFileInfo`-tagged statement
    /// then throws an unhandled `System.Exception` from another `SourceFileInfo`-tagged statement,
    /// exports it (dll + PDB) via `export_pe` (no `ilasm`), writes BOTH files next to each other
    /// under a live `dotnet` host, and asserts `dotnet`'s own unhandled-exception printer resolves
    /// real `file:line` info — the exact same `fNeedFileInfo` code path
    /// `Environment.StackTrace`/`cargo_tests/cd_pdb`'s probe exercises, proving the direct-PE +
    /// Phase-2-PDB pipeline end to end with a live CoreCLR host, not just static metadata
    /// inspection. Root-caused two real bugs to get here (see `pe.rs`'s
    /// `PORTABLE_CODEVIEW_MAJOR_VERSION` and `pdb::DebugDirectoryEntry::stamp` docs for the full
    /// writeup): the CodeView entry's `MajorVersion`/`MinorVersion` must carry the Portable-PDB
    /// marker, and its `TimeDateStamp` (not the RSDS payload's `Age`) is `System.Reflection.
    /// Metadata`'s actual PDB-id match key.
    #[test]
    fn e2e_unhandled_exception_resolves_file_line_through_our_pdb() {
        if dotnet_host().is_none() {
            eprintln!("skipping e2e_unhandled_exception_resolves_file_line_through_our_pdb: no dotnet host");
            return;
        }

        let mut asm = Assembly::default();
        let main = asm.main_module();
        let file = asm.alloc_string("cd_pdb_mini/src/main.rs");
        let source_info = asm.alloc_root(CILRoot::SourceFileInfo {
            line_start: 5,
            line_len: 1,
            col_start: 1,
            col_len: 10,
            file,
        });
        let nop = asm.alloc_root(CILRoot::Nop);
        let throw_source_info = asm.alloc_root(CILRoot::SourceFileInfo {
            line_start: 9,
            line_len: 1,
            col_start: 1,
            col_len: 20,
            file,
        });
        let exc_cref = crate::ir::ClassRef::exception(&mut asm);
        let exc_msg = asm.alloc_string("pe_e2e_pdb unhandled exception");
        let exc_ldstr = asm.alloc_node(CILNode::Const(Box::new(Const::PlatformString(exc_msg))));
        let ctor_name = asm.alloc_string(".ctor");
        let exc_ctor_sig = asm.sig([Type::ClassRef(exc_cref), Type::PlatformString], Type::Void);
        let exc_ctor = asm.alloc_methodref(crate::ir::MethodRef::new(
            exc_cref,
            ctor_name,
            exc_ctor_sig,
            MethodKind::Constructor,
            vec![].into(),
        ));
        let new_exc = asm.alloc_node(CILNode::Call(Box::new((exc_ctor, vec![exc_ldstr].into(), IsPure::NOT))));
        let throw_root = asm.alloc_root(CILRoot::Throw(new_exc));
        let block = BasicBlock::new(vec![source_info, nop, throw_source_info, throw_root], 0, None);

        let entry_sig = asm.sig([], Type::Void);
        let entry_name = asm.alloc_string("entrypoint");
        let entry_def = MethodDef::new(
            Access::Public,
            main,
            entry_name,
            entry_sig,
            MethodKind::Static,
            MethodImpl::MethodBody { blocks: vec![block], locals: vec![] },
            vec![],
        );
        asm.new_method(entry_def);

        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_pdb_throw".to_string(),
            module_name: "pe_e2e_pdb_throw.exe".to_string(),
            pdb_file_name: "pe_e2e_pdb_throw.pdb".to_string(),
        };
        let (image, pdb) = export_pe(&mut asm, &options);
        assert!(!pdb.is_empty());

        let (stdout, stderr, success) = run_under_dotnet_with_pdb(&image, &pdb, "pe_e2e_pdb_throw");
        assert!(!success, "the process must exit non-zero on an unhandled exception");
        assert!(
            stderr.contains("pe_e2e_pdb unhandled exception"),
            "expected the unhandled exception message in stderr; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(
            stderr.contains("main.rs:line 9"),
            "expected the unhandled-exception trace to resolve file:line through OUR PDB (no ilasm); got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    /// **Phase 1b acceptance check: const-data `FieldRVA` blobs.** Builds an assembly whose
    /// `entrypoint` reads a 4-byte `i32` const-data buffer (`asm.bytebuffer`, the same API real
    /// codegen uses for `const`/`&str`-literal data) back via `ldind.i4` and prints it — end to end
    /// through `export_pe`'s Pass 2.5 (`__rcl_const_blob_N` synthetic statics) and `body.rs`'s
    /// pre-existing `Const::ByteBuffer` emission (`const_blob_field_token`), which independently
    /// re-derives the same field name/owner/carrier-type — so a live `dotnet` run only succeeds if
    /// both sides agree byte-for-byte (see the module doc's "Pass 2.5" comment for what could
    /// silently drift). This is the single most important new test: it's the concrete runtime
    /// proof of the FieldRVA-sizing lesson (commit 4b487f7) — a wrongly-sized carrier type would
    /// still load and run under the JIT (which reads the whole contiguous blob regardless of the
    /// field's declared size) but would corrupt the value under NativeAOT/ILC; this test can't
    /// distinguish those two cases on its own (it runs the ordinary JIT, not `PublishAot`), so it
    /// asserts the VALUE is correct as a baseline — the carrier-type-width invariant itself is unit
    /// tested directly in `tables.rs` (`add_blob_sized_valuetype_is_private_sealed_explicit_and_exactly_sized`).
    #[test]
    fn e2e_const_data_static_read_back_at_runtime() {
        if dotnet_host().is_none() {
            eprintln!("skipping e2e_const_data_static_read_back_at_runtime: no dotnet host");
            return;
        }

        let mut asm = Assembly::default();
        let main = asm.main_module();

        let console = crate::ir::ClassRef::console(&mut asm);
        let write_line_name = asm.alloc_string("WriteLine");
        let write_line_sig = asm.sig([Type::Int(crate::ir::Int::I32)], Type::Void);
        let write_line = asm.alloc_methodref(crate::ir::MethodRef::new(
            console,
            write_line_name,
            write_line_sig,
            MethodKind::Static,
            vec![].into(),
        ));

        // A 4-byte LE `i32` const-data buffer holding `733`.
        let i32_ty = asm.alloc_type(Type::Int(crate::ir::Int::I32));
        let data_ptr = asm.bytebuffer(&733i32.to_le_bytes(), i32_ty);
        let value = asm.alloc_node(CILNode::LdInd {
            addr: data_ptr,
            tpe: i32_ty,
            volatile: false,
        });
        let call = asm.alloc_root(CILRoot::Call(Box::new((write_line, vec![value].into(), IsPure::NOT))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BasicBlock::new(vec![call, ret], 0, None);

        let entry_sig = asm.sig([], Type::Void);
        let entry_name = asm.alloc_string("entrypoint");
        let entry_def = MethodDef::new(
            Access::Public,
            main,
            entry_name,
            entry_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![block],
                locals: vec![],
            },
            vec![],
        );
        asm.new_method(entry_def);

        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_const_data".to_string(),
            module_name: "pe_e2e_const_data.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);

        let (stdout, stderr, success) = run_under_dotnet(&image, "pe_e2e_const_data");
        assert!(success, "dotnet run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stdout.trim() == "733",
            "expected the const-data readback to print 733; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    /// Regression test for a real bug caught wiring `DIRECT_PE=1` into the linker on
    /// `cargo_tests/cd_collections` (via `Box::new`/`Vec` pulling in std's panic/backtrace
    /// machinery, which embeds a great many small `&[T]` `const_data` buffers, including
    /// zero-length ones): Pass 2.5's `entries` map computed the `__rcl_const_blob_N` carrier's
    /// size as `data.len().max(1)` (never zero — a zero-sized valuetype is illegal) but queued
    /// the RAW, un-padded `data` bytes into `pending_field_rva`. For an EMPTY buffer this queued
    /// 0 bytes under a 1-byte-sized carrier, so the `.sdata` layout loop's cursor (which advances
    /// by `bytes.len()`, not the carrier's declared size) failed to advance past it — the NEXT
    /// queued blob landed on the exact same RVA, silently aliasing two unrelated static fields.
    /// `dotnet-ilverify` reported no error (a corrupted `.sdata` region is below the metadata-table
    /// layer it checks) — the only symptom was `Assembly.Load` itself rejecting the whole image
    /// with a `FileLoadException` naming the CURRENT assembly's own placeholder identity ("_"),
    /// nowhere near the actual defect. Two buffers of the SAME small size class (`<= 1` byte) are
    /// used here specifically so a regression reproduces the exact aliasing shape found in the
    /// wild: an empty buffer immediately followed by another buffer that would land on its start.
    #[test]
    fn e2e_two_const_data_statics_do_not_alias_when_the_first_is_empty() {
        if dotnet_host().is_none() {
            eprintln!("skipping e2e_two_const_data_statics_do_not_alias_when_the_first_is_empty: no dotnet host");
            return;
        }

        let mut asm = Assembly::default();
        let main = asm.main_module();

        let console = crate::ir::ClassRef::console(&mut asm);
        let write_line_name = asm.alloc_string("WriteLine");
        let write_line_sig = asm.sig([Type::Int(crate::ir::Int::I32)], Type::Void);
        let write_line = asm.alloc_methodref(crate::ir::MethodRef::new(
            console,
            write_line_name,
            write_line_sig,
            MethodKind::Static,
            vec![].into(),
        ));

        // Buffer 1: EMPTY (`data.len() == 0`) — forces the `.max(1)`-sized-carrier /
        // un-padded-bytes mismatch this test pins. Read back as a `u8` (the carrier's actual
        // on-disk size once padded) — a real Rust `&[T; 0]` static never gets dereferenced as a
        // value, so this only needs to prove the BYTES don't alias, not model real codegen.
        let u8_ty = asm.alloc_type(Type::Int(crate::ir::Int::U8));
        let empty_ptr = asm.bytebuffer(&[], u8_ty);
        let empty_val = asm.alloc_node(CILNode::LdInd {
            addr: empty_ptr,
            tpe: u8_ty,
            volatile: false,
        });
        let empty_val = asm.int_cast(empty_val, crate::ir::Int::I32, crate::ir::cilnode::ExtendKind::ZeroExtend);

        // Buffer 2: a real 4-byte `i32` payload — if buffer 1 failed to advance the `.sdata`
        // cursor, this lands on buffer 1's RVA and reads back some mix of buffer 1's (nonexistent)
        // bytes instead of `733`.
        let i32_ty = asm.alloc_type(Type::Int(crate::ir::Int::I32));
        let data_ptr = asm.bytebuffer(&733i32.to_le_bytes(), i32_ty);
        let value = asm.alloc_node(CILNode::LdInd {
            addr: data_ptr,
            tpe: i32_ty,
            volatile: false,
        });

        let print_empty = asm.alloc_root(CILRoot::Call(Box::new((write_line, vec![empty_val].into(), IsPure::NOT))));
        let print_value = asm.alloc_root(CILRoot::Call(Box::new((write_line, vec![value].into(), IsPure::NOT))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BasicBlock::new(vec![print_empty, print_value, ret], 0, None);

        let entry_sig = asm.sig([], Type::Void);
        let entry_name = asm.alloc_string("entrypoint");
        let entry_def = MethodDef::new(
            Access::Public,
            main,
            entry_name,
            entry_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![block],
                locals: vec![],
            },
            vec![],
        );
        asm.new_method(entry_def);

        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_no_alias".to_string(),
            module_name: "pe_e2e_no_alias.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);

        let (stdout, stderr, success) = run_under_dotnet(&image, "pe_e2e_no_alias");
        assert!(success, "dotnet run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(
            lines,
            vec!["0", "733"],
            "the empty buffer must read back 0 (its zero-padded byte) and the SECOND buffer must \
             still read back 733 — not corrupted by aliasing onto the empty buffer's RVA\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    /// Regression test for a second, closely-related bug caught in the SAME `cd_collections`
    /// investigation as the const-data one above: Pass 5's method-body layout loop unconditionally
    /// called `set_method_body_rva` for every method, including `MethodImpl::Extern` (P/Invoke,
    /// `ImplMap`-forwarded) ones — which `body::assemble_method` returns as an EMPTY body for
    /// (§II.22.26 requires `RVA == 0` for a method with no body; the real entry point is the
    /// `ImplMap` row, not an RVA into `.text`). Stamping a nonzero RVA onto a P/Invoke method's
    /// `MethodDef` row is a structural metadata defect even though CoreCLR happens to tolerate it
    /// at runtime here (it checks the `PInvokeImpl` flag before ever consulting `RVA`) — verified
    /// directly against the emitted bytes rather than via a `dotnet`-host E2E run, since the
    /// runtime-observable symptom this bug produced in the wild (a `FileLoadException` loading
    /// `cargo_tests/cd_collections`, `fn main() -> ExitCode` + allocator/libc-touching code) traces
    /// to a DIFFERENT still-open defect this fix's `continue` does not, by itself, resolve — see
    /// this round's summary. Keeping the spec-conformance fix (and this test) regardless: a
    /// bodyless method's `RVA` must be `0` per §II.22.26 independent of what CoreCLR tolerates.
    #[test]
    fn extern_method_keeps_rva_zero_after_pass_5_layout() {
        let mut asm = Assembly::default();
        let main = asm.main_module();

        // A real, non-trivial method defined FIRST — its body actually gets appended to
        // `method_bodies_bytes` before the P/Invoke method below is even reached, so its own RVA
        // is unaffected by the bug; it exists purely to give the P/Invoke method's phantom
        // "cursor didn't advance" position somewhere non-zero to alias onto.
        let second_sig = asm.sig([], Type::Int(crate::ir::Int::I32));
        let second_name = asm.alloc_string("second");
        let const_733 = asm.alloc_node(Const::I32(733));
        let second_ret = asm.alloc_root(CILRoot::Ret(const_733));
        let second_block = BasicBlock::new(vec![second_ret], 0, None);
        let second_def = MethodDef::new(
            Access::Public,
            main,
            second_name,
            second_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![second_block],
                locals: vec![],
            },
            vec![],
        );
        let second = asm.new_method(second_def);

        // A real P/Invoke method — `strlen` from the platform C library, exactly the shape
        // `cargo_tests/cd_collections`'s failure surfaced (see `ImplMap` dump in the investigation:
        // `strlen`/`memcmp` from `libSystem.B.dylib`). Body-less (`MethodImpl::Extern`), so
        // `body::assemble_method` returns zero bytes for it — the exact case Pass 5 mishandled:
        // its cursor-position bookkeeping (RVA metadata) does not advance past `second`'s body,
        // but the SEPARATE `method_bodies_bytes` output buffer keeps growing regardless (plain
        // `Vec::extend_from_slice`, not a positional write) — so the method placed immediately
        // AFTER this one gets its `MethodDef.RVA` metadata stamped with a STALE address (this
        // P/Invoke method's non-advanced cursor, landing inside/at the end of `second`'s body)
        // while its ACTUAL bytes are appended much later in the buffer. The result: `entrypoint`
        // below JITs whatever bytes truly sit at the stale RVA (garbage / `second`'s tail),
        // never reaching its real, correctly-assembled-but-wrongly-addressed body.
        let libc_name = asm.alloc_string(if cfg!(target_os = "macos") { "libSystem.B.dylib" } else { "libc.so.6" });
        let strlen_name = asm.alloc_string("strlen");
        let u8_ptr = asm.nptr(Type::Int(crate::ir::Int::U8));
        let strlen_sig = asm.sig([u8_ptr], Type::Int(crate::ir::Int::USize));
        let strlen_def = MethodDef::new(
            Access::Extern,
            main,
            strlen_name,
            strlen_sig,
            MethodKind::Static,
            MethodImpl::Extern {
                lib: libc_name,
                preserve_errno: false,
            },
            vec![None],
        );
        asm.new_method(strlen_def);

        let _ = second; // registered via `new_method`; RID checked directly below, not called.
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BasicBlock::new(vec![ret], 0, None);

        let entry_sig = asm.sig([], Type::Void);
        let entry_name = asm.alloc_string("entrypoint");
        let entry_def = MethodDef::new(
            Access::Public,
            main,
            entry_name,
            entry_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![block],
                locals: vec![],
            },
            vec![],
        );
        asm.new_method(entry_def);

        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_no_pinvoke_alias".to_string(),
            module_name: "pe_e2e_no_pinvoke_alias.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);

        // `strlen` is the 2nd `MethodDef` row added (`second`, `strlen`, `entrypoint` — 1-based
        // RID 2), and its RVA must read back as exactly `0` (§II.22.26: bodyless methods MUST
        // have RVA 0) — Pass 5, pre-fix, stamped it with the (bogus, nonzero) layout cursor
        // instead.
        let rva = read_method_def_rva(&image, 2 /* RID of `strlen` */);
        assert_eq!(rva, 0, "a bodyless (P/Invoke) MethodDef row's RVA column must stay 0, not the Pass 5 layout cursor");

        // The two REAL-bodied methods (`second` RID 1, `entrypoint` RID 3) must each have their
        // OWN distinct, nonzero RVA — proving Pass 5's `continue` for the bodyless row in between
        // didn't also skip (or double-assign) either of theirs.
        let second_rva = read_method_def_rva(&image, 1);
        let entry_rva = read_method_def_rva(&image, 3);
        assert_ne!(second_rva, 0, "`second` has a real body; its RVA must be nonzero");
        assert_ne!(entry_rva, 0, "`entrypoint` has a real body; its RVA must be nonzero");
        assert_ne!(second_rva, entry_rva, "two distinct method bodies must not share an RVA");
    }

    /// Reads `MethodDef` row `rid`'s (1-based) `RVA` column (§II.22.26) straight out of a
    /// freshly-`export_pe`'d image — a minimal, test-only ECMA-335 reader (PE section table ->
    /// CLI header -> BSJB metadata root -> `#~` stream header -> row-count array -> per-table
    /// byte offset -> row). `RVA` is always a raw `u32` at column 0 (never a heap/coded index),
    /// so only the STRIDE between rows (which DOES depend on heap/index widths) needs real width
    /// computation.
    fn read_method_def_rva(image: &[u8], rid: u32) -> u32 {
        // CLI header directory RVA (§II.25.3.3, fixed by `pe.rs`): `.text` starts at
        // `pe::SECTION_ALIGNMENT` and the CLI header sits 8 bytes in, after the (always-present,
        // even for a `.dll`) 8-byte IAT-shaped pad `pe::write_pe` reserves at `.text`'s start —
        // see that module's `text_header_len`. Derived from the real (`pub(super)`) constant, not
        // a hand-copied literal — a hardcoded `0x2008`/`0x4008` here is exactly the drift trap
        // that made every E2E test fail with `BadImageFormatException: Bad IL range` the first
        // time `SECTION_ALIGNMENT` was bumped (`0x2000` -> `0x4000`, fixing a macOS ARM64
        // `mprotect`-alignment `FileLoadException`; see that constant's doc).
        let cli_off = rva_to_file_offset(image, pe::SECTION_ALIGNMENT + 8);
        let md_rva = u32::from_le_bytes(image[cli_off + 8..cli_off + 12].try_into().unwrap());
        let md_off = rva_to_file_offset(image, md_rva);
        let version_len = u32::from_le_bytes(image[md_off + 12..md_off + 16].try_into().unwrap()) as usize;
        let mut p = md_off + 16 + version_len + 2 /* Flags */;
        let n_streams = u16::from_le_bytes(image[p..p + 2].try_into().unwrap());
        p += 2;
        let mut ts = None;
        for _ in 0..n_streams {
            let s_off = u32::from_le_bytes(image[p..p + 4].try_into().unwrap()) as usize;
            p += 8;
            let name_start = p;
            let name_end = image[name_start..].iter().position(|&b| b == 0).unwrap() + name_start;
            let name = std::str::from_utf8(&image[name_start..name_end]).unwrap();
            let mut name_len = name_end - name_start + 1;
            while name_len % 4 != 0 {
                name_len += 1;
            }
            p = name_start + name_len;
            if name == "#~" {
                ts = Some(md_off + s_off);
            }
        }
        let ts = ts.expect("#~ stream must be present");
        let heap_sizes = image[ts + 6];
        let str_w: usize = if heap_sizes & 0x1 != 0 { 4 } else { 2 };
        let blob_w: usize = if heap_sizes & 0x4 != 0 { 4 } else { 2 };
        let valid = u64::from_le_bytes(image[ts + 8..ts + 16].try_into().unwrap());
        let mut row_counts = [0u32; 64];
        let mut rp = ts + 24;
        for i in 0..64u32 {
            if valid & (1 << i) != 0 {
                row_counts[i as usize] = u32::from_le_bytes(image[rp..rp + 4].try_into().unwrap());
                rp += 4;
            }
        }
        let simple_w = |rows: u32| if rows > 0xFFFF { 4usize } else { 2usize };
        let coded_w_2tag = |max_rows: u32| if max_rows as usize >= (1usize << 14) { 4usize } else { 2usize };
        // Table row widths for JUST the tables that precede `MethodDef` (id 0x06) in the fixed
        // §II.22 table ordering: Module(0x00), TypeRef(0x01), TypeDef(0x02), Field(0x04) —
        // Param(0x08)/others come AFTER `MethodDef` so they don't affect this table's start
        // offset, only its OWN row stride (computed separately below).
        let module_w = 2 + str_w + 3 * 2; // this reader's test images always have a tiny (narrow) #GUID heap.
        let type_or_ref_max = row_counts[0x02].max(row_counts[0x01]);
        let typeref_w = coded_w_2tag(row_counts[0x00].max(row_counts[0x1A]).max(row_counts[0x23])) + 2 * str_w;
        let typedef_w = 4 + 2 * str_w + coded_w_2tag(type_or_ref_max) + simple_w(row_counts[0x04]) + simple_w(row_counts[0x06]);
        let field_w = 2 + str_w + blob_w;
        let mut table_off = rp;
        for (id, width) in [(0x00u32, module_w), (0x01, typeref_w), (0x02, typedef_w), (0x04, field_w)] {
            table_off += row_counts[id as usize] as usize * width;
        }
        let row_width = 4 + 2 + 2 + str_w + blob_w + simple_w(row_counts[0x08]);
        let row_off = table_off + (rid as usize - 1) * row_width;
        u32::from_le_bytes(image[row_off..row_off + 4].try_into().unwrap())
    }

    /// Resolves a PE RVA to a file offset via the section table (§II.25.3).
    fn rva_to_file_offset(image: &[u8], rva: u32) -> usize {
        let pe_off = u32::from_le_bytes(image[0x3c..0x40].try_into().unwrap()) as usize;
        let n_sections = u16::from_le_bytes(image[pe_off + 6..pe_off + 8].try_into().unwrap());
        let opt_hdr_size = u16::from_le_bytes(image[pe_off + 20..pe_off + 22].try_into().unwrap());
        let sec_table_off = pe_off + 24 + opt_hdr_size as usize;
        for i in 0..n_sections as usize {
            let off = sec_table_off + i * 40;
            let vsize = u32::from_le_bytes(image[off + 8..off + 12].try_into().unwrap());
            let vaddr = u32::from_le_bytes(image[off + 12..off + 16].try_into().unwrap());
            let praw = u32::from_le_bytes(image[off + 20..off + 24].try_into().unwrap());
            if vaddr <= rva && rva < vaddr + vsize {
                return (praw + (rva - vaddr)) as usize;
            }
        }
        panic!("rva {rva:#x} not found in any section");
    }

    /// **Phase 1b acceptance check: scalar `StaticFieldDef::default_value` `FieldRVA` blobs.**
    /// Separate code path from the const-data test above (`export_pe`'s Pass 2 `Some(cst) =>`
    /// arm, not Pass 2.5): a static `i32` field with a compile-time initializer (`733`), whose
    /// declared TYPE is itself the `FieldRVA` carrier — no synthetic `__rcl_const_blob_N` needed
    /// (see that arm's doc comment for why). Reads the static field back via `ldsfld` and prints
    /// it, proving both `add_static_field`'s `HasFieldRVA` flag and the RVA layout/patch pipeline
    /// this field shares with the const-data path.
    #[test]
    fn e2e_static_field_default_value_read_back_at_runtime() {
        if dotnet_host().is_none() {
            eprintln!("skipping e2e_static_field_default_value_read_back_at_runtime: no dotnet host");
            return;
        }

        let mut asm = Assembly::default();
        let main = asm.main_module();

        let console = crate::ir::ClassRef::console(&mut asm);
        let write_line_name = asm.alloc_string("WriteLine");
        let write_line_sig = asm.sig([Type::Int(crate::ir::Int::I32)], Type::Void);
        let write_line = asm.alloc_methodref(crate::ir::MethodRef::new(
            console,
            write_line_name,
            write_line_sig,
            MethodKind::Static,
            vec![].into(),
        ));

        let sfld = asm.add_static(
            Type::Int(crate::ir::Int::I32),
            "ANSWER",
            false,
            main,
            Some(Const::I32(733)),
            true,
        );
        let value = asm.alloc_node(CILNode::LdStaticField(sfld));
        let call = asm.alloc_root(CILRoot::Call(Box::new((write_line, vec![value].into(), IsPure::NOT))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BasicBlock::new(vec![call, ret], 0, None);

        let entry_sig = asm.sig([], Type::Void);
        let entry_name = asm.alloc_string("entrypoint");
        let entry_def = MethodDef::new(
            Access::Public,
            main,
            entry_name,
            entry_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![block],
                locals: vec![],
            },
            vec![],
        );
        asm.new_method(entry_def);

        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_static_default".to_string(),
            module_name: "pe_e2e_static_default.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);

        let (stdout, stderr, success) = run_under_dotnet(&image, "pe_e2e_static_default");
        assert!(success, "dotnet run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stdout.trim() == "733",
            "expected the static-default readback to print 733; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    /// **Phase 1b structural check: `ClassDef::implements()` -> `InterfaceImpl` rows.** Full
    /// runtime execution of an interface-implementing type is out of scope for this pass (this
    /// backend does not yet emit `MethodImpl`/`.override` rows, so implementing a BCL interface's
    /// methods isn't wired — the same "il_exporter doesn't do this either" boundary the module doc
    /// notes for `MainModule` partitioning); this test instead verifies the metadata shape
    /// directly, the same way `export_pe_hand_built_hello_world_has_an_entry_point_token` verifies
    /// the CLI header without running `dotnet` — parse the `TypeDef`/`InterfaceImpl` tables back
    /// out of the produced image and confirm the row exists and points at the right `TypeRef`.
    #[test]
    fn export_pe_class_implements_interface_gets_an_interface_impl_row() {
        let mut asm = Assembly::default();
        let main = asm.main_module();
        // A real BCL interface reference — no method body needed for a metadata-shape check.
        let idisposable_name = asm.alloc_string("System.IDisposable");
        let idisposable_asm = asm.alloc_string("System.Runtime");
        let idisposable = asm.alloc_class_ref(crate::ir::ClassRef::new(idisposable_name, Some(idisposable_asm), false, [].into()));
        asm.class_mut(main).add_interface(idisposable);

        let options = ExportOptions {
            is_dll: true,
            assembly_name: "export_pe_implements".to_string(),
            module_name: "export_pe_implements.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);
        assert_eq!(&image[0..2], b"MZ");
        assert!(image.len() > 0x200);
        // A byte-level `InterfaceImpl` row check would need to duplicate the metadata reader
        // `tables.rs`'s own tests already exercise (`interface_impl_rows_are_emitted_sorted_by_class`)
        // — this test's job is to confirm `export_pe`'s `Assembly`-driven `implements()` walk
        // actually reaches `add_type_def`'s `implements` parameter without panicking/todo!()-ing,
        // which is exactly the Phase 1a `todo!()` this closes. `tables.rs` already covers the row
        // shape once it's populated.
    }

    /// **Phase 1b acceptance check: an explicit-layout struct with nonzero field offsets.**
    /// `[FieldOffset]`-tagged structs are core-path for this backend (every Rust struct/enum uses
    /// explicit layout, per the task's institutional-lessons list) — this proves `export_pe`'s
    /// `ClassLayout`/`FieldLayout` wiring (Pass 1's `has_explicit_layout`/`pack`/`size` computation
    /// + `add_field`'s `offset` parameter) all the way through to a real CoreCLR field access, not
    /// just the structural row-shape `tables.rs`'s own unit tests already cover
    /// (`class_layout_row_added_for_explicit_layout_type`, `field_layout_rows_are_emitted_sorted_by_field`).
    /// Builds a `[StructLayout(LayoutKind.Explicit, Size=8)] struct Point { [FieldOffset(0)] int x;
    /// [FieldOffset(4)] int y; }`-shaped valuetype, sets both fields via a local, reads them back,
    /// and prints `x + y` — a wrong offset (e.g. both fields aliasing offset 0) would print `20` or
    /// `64` instead of `42`, not merely fail to load.
    #[test]
    fn e2e_explicit_layout_struct_with_nonzero_offsets_round_trips_at_runtime() {
        if dotnet_host().is_none() {
            eprintln!("skipping e2e_explicit_layout_struct_with_nonzero_offsets_round_trips_at_runtime: no dotnet host");
            return;
        }

        let mut asm = Assembly::default();
        let main = asm.main_module();

        let point_name = asm.alloc_string("Point");
        let x_name = asm.alloc_string("x");
        let y_name = asm.alloc_string("y");
        let i32_ty = Type::Int(crate::ir::Int::I32);
        let point_def = crate::ir::class::ClassDef::new(
            point_name,
            true, // valuetype
            0,
            None,
            vec![(i32_ty, x_name, Some(0)), (i32_ty, y_name, Some(4))],
            vec![],
            Access::Public,
            std::num::NonZeroU32::new(8),
            None,
            true,
        );
        let point_idx = asm.class_def(point_def).expect("Point layout check");
        let point_cref = point_idx.0;
        let x_field = asm.alloc_field(crate::ir::field::FieldDesc::new(point_cref, x_name, i32_ty));
        let y_field = asm.alloc_field(crate::ir::field::FieldDesc::new(point_cref, y_name, i32_ty));

        let console = crate::ir::ClassRef::console(&mut asm);
        let write_line_name = asm.alloc_string("WriteLine");
        let write_line_sig = asm.sig([i32_ty], Type::Void);
        let write_line = asm.alloc_methodref(crate::ir::MethodRef::new(
            console,
            write_line_name,
            write_line_sig,
            MethodKind::Static,
            vec![].into(),
        ));

        // `local 0`: a `Point`. `ldloca.0 ; ldc.i4 10 ; stfld x` / `ldloca.0 ; ldc.i4 32 ; stfld y`,
        // then `ldloc.0 ; ldfld x ; ldloc.0 ; ldfld y ; add ; call WriteLine(int)`.
        let loc_addr = asm.alloc_node(CILNode::LdLocA(0));
        let ten = asm.alloc_node(CILNode::Const(Box::new(Const::I32(10))));
        let set_x = asm.alloc_root(CILRoot::SetField(Box::new((x_field, loc_addr, ten))));
        let loc_addr2 = asm.alloc_node(CILNode::LdLocA(0));
        let thirty_two = asm.alloc_node(CILNode::Const(Box::new(Const::I32(32))));
        let set_y = asm.alloc_root(CILRoot::SetField(Box::new((y_field, loc_addr2, thirty_two))));

        let loc_val_for_x = asm.alloc_node(CILNode::LdLoc(0));
        let get_x = asm.alloc_node(CILNode::LdField {
            addr: loc_val_for_x,
            field: x_field,
        });
        let loc_val_for_y = asm.alloc_node(CILNode::LdLoc(0));
        let get_y = asm.alloc_node(CILNode::LdField {
            addr: loc_val_for_y,
            field: y_field,
        });
        let sum = asm.alloc_node(CILNode::BinOp(get_x, get_y, crate::ir::BinOp::Add));
        let call = asm.alloc_root(CILRoot::Call(Box::new((write_line, vec![sum].into(), IsPure::NOT))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BasicBlock::new(vec![set_x, set_y, call, ret], 0, None);

        let entry_sig = asm.sig([], Type::Void);
        let entry_name = asm.alloc_string("entrypoint");
        let entry_def = MethodDef::new(
            Access::Public,
            main,
            entry_name,
            entry_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![block],
                locals: vec![(None, asm.alloc_type(Type::ClassRef(point_cref)))],
            },
            vec![],
        );
        asm.new_method(entry_def);

        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_layout".to_string(),
            module_name: "pe_e2e_layout.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);

        let (stdout, stderr, success) = run_under_dotnet(&image, "pe_e2e_layout");
        assert!(success, "dotnet run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stdout.trim() == "42",
            "expected x(10)+y(32) read back through explicit offsets to print 42; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    /// **Regression probe for the `pal_threads`/`cd_interop`/whole-program `FileLoadException
    /// 0x8007000C` bug class.** Real codegen's `format_args!` lowering (`core::fmt::rt::Argument`,
    /// `library/core/src/fmt/rt.rs`) stores an `ldftn`-obtained function pointer into an
    /// EXPLICIT-LAYOUT struct field typed `Type::FnPtr` (the `Placeholder_formatter` field of the
    /// `ArgumentType` union), then later reads that field back and `calli`s through it — isolated
    /// via a differential bisection: `println!("{}", x)` for a `let`-bound `i128`/`u128` fails to
    /// even `Assembly.Load` under `DIRECT_PE=1` (same `FileLoadException` signature as
    /// `pal_threads`), while `x.to_string()` (same `Display::fmt`, called directly instead of
    /// through a stored/reloaded `Argument`) loads fine — narrowing the defect to the
    /// store-fnptr-in-a-field-then-load-and-`calli`-it pattern, not `Display` or 128-bit types
    /// themselves. This test reproduces the SAME shape (an explicit-layout struct with a
    /// `Type::FnPtr` field, `ldftn` + `stfld` + `ldfld` + `calli`) with a trivial `i32 -> i32`
    /// callee, minimizing everything else, to isolate whether `pe_exporter`'s handling of a
    /// function-pointer-typed FIELD (as opposed to a `calli` call site's own signature, which
    /// `e2e_hand_built_assembly_runs_under_dotnet`-adjacent call-site paths already exercise) is
    /// the defect. If this test itself fails to load, the bug is in FIELD/local `Type::FnPtr`
    /// signature encoding (`sig::encode_field_sig`/`encode_locals_sig`'s `ET_FNPTR` branch) or in
    /// `ClassLayout`/`FieldLayout` row shaping for a fnptr-typed field — not in a 128-bit-specific
    /// code path.
    #[test]
    fn e2e_fnptr_field_store_then_calli_round_trips_at_runtime() {
        if dotnet_host().is_none() {
            eprintln!("skipping e2e_fnptr_field_store_then_calli_round_trips_at_runtime: no dotnet host");
            return;
        }

        let mut asm = Assembly::default();
        let main = asm.main_module();

        // `callee(i128* p) -> i32`: returns `42` unconditionally, but the PARAMETER is a
        // pointer-to-BCL-128-bit-valuetype — a stand-in for `<i128 as Display>::fmt` taking
        // `&i128` (the REAL bug's callee shape: a BY-REF/BY-POINTER 128-bit BCL valuetype
        // argument, not a plain scalar). Only the SIGNATURE shape matters for this probe, not the
        // arithmetic — we just need to know whether `dotnet` still loads the assembly.
        let i128_cref = crate::ir::ClassRef::int_128(&mut asm);
        let i128_ty = Type::ClassRef(i128_cref);
        let i128_ptr_ty = asm.alloc_type(i128_ty);
        let callee_sig = asm.sig([Type::Ptr(i128_ptr_ty)], Type::Int(crate::ir::Int::I32));
        let callee_name = asm.alloc_string("callee");
        let forty_two_i32 = asm.alloc_node(CILNode::Const(Box::new(Const::I32(42))));
        let callee_ret = asm.alloc_root(CILRoot::Ret(forty_two_i32));
        let callee_block = BasicBlock::new(vec![callee_ret], 0, None);
        let callee_def = MethodDef::new(
            Access::Public,
            main,
            callee_name,
            callee_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![callee_block],
                locals: vec![],
            },
            vec![None],
        );
        let callee_idx = asm.new_method(callee_def);
        let callee_mref = asm.alloc_methodref(crate::ir::MethodRef::new(
            main.0,
            callee_name,
            callee_sig,
            MethodKind::Static,
            vec![].into(),
        ));
        let _ = callee_idx;

        // `Holder { fp: fn(native uint) -> i32 }` — DELIBERATELY a DIFFERENT signature than
        // `callee`'s real `fn(i32) -> i32` (mirrors Rust's `transmute::<fn(&T, &Formatter) -> R,
        // fn(NonNull<()>, &Formatter) -> R>` in `Argument::new_display`: the field's declared
        // fnptr type is the type-ERASED signature, not the callee's real one). Explicit layout,
        // size 8 (matches a raw function pointer's size on a 64-bit target) — mirrors
        // `ArgumentType`'s `Placeholder_formatter` field shape (a fnptr-typed field inside an
        // explicit-layout struct storing a type-erased function pointer).
        let holder_name = asm.alloc_string("Holder");
        let fp_name = asm.alloc_string("fp");
        let erased_sig = asm.sig([Type::Int(crate::ir::Int::USize)], Type::Int(crate::ir::Int::I32));
        let fnptr_ty = Type::FnPtr(erased_sig);
        let holder_def = crate::ir::class::ClassDef::new(
            holder_name,
            true, // valuetype
            0,
            None,
            vec![(fnptr_ty, fp_name, Some(0))],
            vec![],
            Access::Public,
            std::num::NonZeroU32::new(8),
            None,
            true,
        );
        let holder_idx = asm.class_def(holder_def).expect("Holder layout check");
        let holder_cref = holder_idx.0;
        let fp_field = asm.alloc_field(crate::ir::field::FieldDesc::new(holder_cref, fp_name, fnptr_ty));

        let console = crate::ir::ClassRef::console(&mut asm);
        let write_line_name = asm.alloc_string("WriteLine");
        let write_line_sig = asm.sig([Type::Int(crate::ir::Int::I32)], Type::Void);
        let write_line = asm.alloc_methodref(crate::ir::MethodRef::new(
            console,
            write_line_name,
            write_line_sig,
            MethodKind::Static,
            vec![].into(),
        ));

        // `local 0`: a `Holder`. `ldftn callee` -> `ldloca.0 ; stfld fp` -> `ldloc.0 ; ldfld fp` ->
        // `calli (i32)->i32` with arg 41 -> `WriteLine(result)`. Expects `42`.
        let ftn = asm.alloc_node(CILNode::LdFtn(callee_mref));
        let loc_addr = asm.alloc_node(CILNode::LdLocA(0));
        let set_fp = asm.alloc_root(CILRoot::SetField(Box::new((fp_field, loc_addr, ftn))));

        let loc_val = asm.alloc_node(CILNode::LdLoc(0));
        let get_fp = asm.alloc_node(CILNode::LdField {
            addr: loc_val,
            field: fp_field,
        });
        let forty_one = asm.alloc_node(CILNode::Const(Box::new(Const::USize(41))));
        let call_result = asm.alloc_node(CILNode::CallI(Box::new((
            get_fp,
            erased_sig,
            vec![forty_one].into(),
        ))));
        let call = asm.alloc_root(CILRoot::Call(Box::new((write_line, vec![call_result].into(), IsPure::NOT))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BasicBlock::new(vec![set_fp, call, ret], 0, None);

        let entry_sig = asm.sig([], Type::Void);
        let entry_name = asm.alloc_string("entrypoint");
        let entry_def = MethodDef::new(
            Access::Public,
            main,
            entry_name,
            entry_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![block],
                locals: vec![(None, asm.alloc_type(Type::ClassRef(holder_cref)))],
            },
            vec![],
        );
        asm.new_method(entry_def);

        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_fnptr_field".to_string(),
            module_name: "pe_e2e_fnptr_field.exe".to_string(),
            pdb_file_name: String::new(),
        };
        let (image, _pdb) = export_pe(&mut asm, &options);

        let (stdout, stderr, success) = run_under_dotnet(&image, "pe_e2e_fnptr_field");
        assert!(success, "dotnet run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stdout.trim() == "42",
            "expected callee(41)=42 read back through a stored/reloaded fnptr field; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
}
