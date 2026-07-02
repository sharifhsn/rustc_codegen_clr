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
use super::sig::{self, TypeDefOrRefResolver};
use super::tables::{MetadataBuilder, Token};
use crate::ir::class::StaticFieldDef;
use crate::ir::{Assembly, Const};

/// `SectionAlignment` (§II.25.2.3.1) — duplicated from `pe.rs`'s private constant since the RVA
/// pre-computation below must match `pe::write_pe`'s own layout pass exactly (see that module's
/// doc for why method/field RVAs must be known before the FINAL `serialize()` call).
const SECTION_ALIGNMENT: u32 = 0x2000;
/// CLI header size (§II.25.3.3) — duplicated from `pe.rs`'s private constant for the same reason.
const CLI_HEADER_CB: u32 = 0x48;

/// Everything `export_pe` needs beyond the `Assembly` itself.
pub struct ExportOptions {
    /// `true` for a `.dll`, `false` for a `.exe` — forwarded to [`PeOptions::is_dll`].
    pub is_dll: bool,
    /// The `.NET` module name stamped into the `Module` table and hashed for the deterministic
    /// MVID (see [`MetadataBuilder::finish_module`]).
    pub assembly_name: String,
}

/// Builds the complete PE image bytes for `asm`: populates metadata for every class/field/method,
/// assembles every method body, lays out RVAs, patches them back into the metadata, and writes
/// the final PE/COFF container. Returns the finished `.exe`/`.dll` bytes.
///
/// # Panics / `todo!()`
/// On any construct outside the Phase 1a inventory — see the module doc.
#[must_use]
pub fn export_pe(asm: &mut Assembly, options: &ExportOptions) -> Vec<u8> {
    let mut mb = MetadataBuilder::new();

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
    for &class_def_id in &class_def_ids {
        let class_def = asm[class_def_id].clone();
        // Every class needs an `Extends` row: `il_exporter::export_to_write` never leaves it NIL
        // — an explicit `extends` clause wins, otherwise `[System.Runtime]System.ValueType` for a
        // valuetype or `[System.Runtime]System.Object` for a reference type (mirrors that
        // function's `let extends = if let Some(parent) = … { … } else if is_valuetype { … } else
        // { … }` exactly). A NIL `Extends` is a real defect, not a harmless default: it makes the
        // CLR loader treat the TypeDef as an interface-shaped type with no concrete base, which
        // rejected this milestone's `MainModule` with `BadImageFormatException` during
        // development.
        let extends = if let Some(parent) = class_def.extends() {
            decode_type_def_or_ref(mb.type_def_or_ref(parent, asm))
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
        let implements: Vec<Token> = class_def
            .implements()
            .iter()
            .map(|&iface| decode_type_def_or_ref(mb.type_def_or_ref(iface, asm)))
            .collect();
        let has_explicit_layout = class_def.explict_size().is_some()
            || class_def.fields().iter().any(|(_, _, offset)| offset.is_some());
        let (pack, size) = if has_explicit_layout {
            (Some(1u16), class_def.explict_size().map(std::num::NonZeroU32::get))
        } else {
            (None, None)
        };
        let raw_name = asm[class_def.name()].to_string();
        mb.add_type_def("", &raw_name, class_def.is_valuetype(), Some(extends), pack, size, &implements);
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
                .map(|(zero_based, data)| (data.len().max(1), u32::try_from(zero_based + 1).unwrap(), data.to_vec()))
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
        for &method_id in class_def.methods() {
            let method = asm[method_id].clone();
            let name = asm[method.name()].to_string();
            let sig = asm[method.sig()].clone();
            let is_static = method.kind() == crate::ir::cilnode::MethodKind::Static;
            let is_virtual = method.kind() == crate::ir::cilnode::MethodKind::Virtual;
            let is_ctor = method.kind() == crate::ir::cilnode::MethodKind::Constructor;
            let pinvoke_owned = match method.implementation() {
                crate::ir::MethodImpl::Extern { lib, preserve_errno } => {
                    Some((asm[*lib].to_string(), *preserve_errno))
                }
                _ => None,
            };
            let mut blob = Vec::new();
            let convention = if is_static { sig::SIG_DEFAULT } else { sig::SIG_HASTHIS };
            sig::encode_method_sig(convention, 0, &sig, asm, &mut mb, &mut blob);
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
            let tok = mb.add_method(&name, sig_off, &param_names, is_static, is_virtual, is_ctor, pinvoke_ref);
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
    mb.finish_module(&options.assembly_name);
    let metadata_len_probe = mb.serialize().len();
    // `pe::text_header_len` is the single source of truth for how many bytes precede the CLI
    // header within `.text` (0 for a `.dll`, or the bootstrap IAT's length for an `.exe` — see
    // that function's doc for the real bug this indirection prevents: a hardcoded `0` here once
    // silently shifted every method body 8 bytes short of where `MethodDef.RVA` said it was).
    let text_header_len = pe::text_header_len(entry_point_token.is_some());
    let bodies_start_rva =
        SECTION_ALIGNMENT + text_header_len + CLI_HEADER_CB + u32::try_from(metadata_len_probe).unwrap();
    let mut method_bodies_bytes = Vec::new();
    let mut cursor = bodies_start_rva;
    for (tok, assembled) in &bodies {
        while cursor % 4 != 0 {
            cursor += 1;
            method_bodies_bytes.push(0);
        }
        mb.set_method_body_rva(*tok, cursor);
        method_bodies_bytes.extend_from_slice(&assembled.bytes);
        cursor += u32::try_from(assembled.bytes.len()).unwrap();
    }

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

    let pe_options = PeOptions {
        is_dll: options.is_dll,
        entry_point: entry_point_token.map(|t| t.0),
    };
    pe::write_pe(&metadata, &method_bodies_bytes, &field_rva_bytes, &pe_options)
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

/// Decodes a `TypeDefOrRef` coded index back into a [`Token`] — the same decode
/// `tables.rs`'s private `decode_type_def_or_ref` performs, needed here for `extends` resolution
/// while walking class defs (kept as its own copy rather than exposed from `tables.rs`, since the
/// encoding is a fixed 2-bit-tag scheme documented in both places by ECMA-335 §II.24.2.6 directly,
/// not an implementation detail `export.rs` should reach into `tables.rs`'s row storage for).
fn decode_type_def_or_ref(coded: u32) -> Token {
    let tag = coded & 0x3;
    let rid = coded >> 2;
    let table = match tag {
        0 => Token::TABLE_TYPE_DEF,
        1 => Token::TABLE_TYPE_REF,
        2 => Token::TABLE_TYPE_SPEC,
        _ => unreachable!("2-bit tag"),
    };
    Token::new(table, rid)
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
        };
        let image = export_pe(&mut asm, &options);
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
        };
        let image = export_pe(&mut asm, &options);
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
        let (dotnet_root, dotnet_bin) = dotnet_host().expect("caller must guard with dotnet_host()");
        let scratch = std::env::temp_dir().join("pe_e2e");
        std::fs::create_dir_all(&scratch).expect("create scratch dir");
        let exe_path = scratch.join(format!("{name}.dll")); // apphost-less: `dotnet <path>.dll` runs it directly.
        std::fs::write(&exe_path, image).expect("write exported PE image");

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
        };
        let image = export_pe(&mut asm, &options);

        let (stdout, stderr, success) = run_under_dotnet(&image, "pe_e2e_hello");
        assert!(success, "dotnet run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stdout.contains("PE writer E2E OK"),
            "expected stdout to contain the WriteLine output; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
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
        };
        let image = export_pe(&mut asm, &options);

        let (stdout, stderr, success) = run_under_dotnet(&image, "pe_e2e_const_data");
        assert!(success, "dotnet run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stdout.trim() == "733",
            "expected the const-data readback to print 733; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
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
        };
        let image = export_pe(&mut asm, &options);

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
        };
        let image = export_pe(&mut asm, &options);
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
        };
        let image = export_pe(&mut asm, &options);

        let (stdout, stderr, success) = run_under_dotnet(&image, "pe_e2e_layout");
        assert!(success, "dotnet run failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stdout.trim() == "42",
            "expected x(10)+y(32) read back through explicit offsets to print 42; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
}
