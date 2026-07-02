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
//! `Assembly` to a loadable `.exe`. Deliberately **not** wired yet (loud `todo!()`s below, per the
//! plan doc's "anything `il_exporter` doesn't emit, we don't build" scope fence — and anything it
//! *does* emit that this milestone's test doesn't reach stays a `todo!()` rather than a silently
//! wrong row): const-data `FieldRVA` blobs (`__rcl_const_blob_N` synthetic statics), non-
//! `ByteBuffer` static-field defaults, and `MainModule` method-count partitioning. The `Assembly`
//! table's self-identity row IS wired (`mb.set_assembly`, version `0.0.0.0` — mirrors
//! `il_exporter`'s `.assembly _{}` executable placeholder; a real version stamp for a named
//! library assembly is deferred with the rest of the `.dll` output path).

use super::body::{self, AssembledBody};
use super::pe::{self, PeOptions};
use super::sig::{self, TypeDefOrRefResolver};
use super::tables::{MetadataBuilder, Token};
use crate::ir::class::StaticFieldDef;
use crate::ir::Assembly;

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
        if !class_def.implements().is_empty() {
            todo!("Phase 1b: ClassDef::implements() -> InterfaceImpl rows in export_pe (not exercised by the Phase 1a milestone test)");
        }
        let has_explicit_layout = class_def.explict_size().is_some()
            || class_def.fields().iter().any(|(_, _, offset)| offset.is_some());
        let (pack, size) = if has_explicit_layout {
            (Some(1u16), class_def.explict_size().map(std::num::NonZeroU32::get))
        } else {
            (None, None)
        };
        let raw_name = asm[class_def.name()].to_string();
        mb.add_type_def("", &raw_name, class_def.is_valuetype(), Some(extends), pack, size, &[]);
    }

    // --- Pass 2: fields (instance + static), matching `il_exporter`'s per-class field loop
    // (§II.22.15/§II.22.18). Only fieldless classes (this milestone's `MainModule`) and the
    // `None`-default-value static-field shape are exercised; anything else is a loud `todo!()`.
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
            if default_value.is_some() {
                todo!(
                    "Phase 1b: StaticFieldDef::default_value (FieldRVA-blob / metadata-Constant \
                     static field initializers) in export_pe — not exercised by the Phase 1a \
                     milestone test, which defines no static data fields"
                );
            }
            let name_str = asm[*name].to_string();
            let mut blob = Vec::new();
            sig::encode_field_sig(*tpe, asm, &mut mb, &mut blob);
            let sig_off = mb.blobs.intern(&blob);
            mb.add_static_field(&name_str, sig_off, None, *is_tls, *is_const);
        }
    }

    if !asm.const_data.0.is_empty() {
        todo!(
            "Phase 1b: const-data FieldRVA blobs (__rcl_const_blob_N synthetic statics) in \
             export_pe — not exercised by the Phase 1a milestone test, which allocates no \
             const-data buffers"
        );
    }

    // --- Pass 3: methods. Every class def's methods, in insertion order, matching
    // `il_exporter::export_to_write`'s per-class method loop (the unpartitioned path only —
    // Phase 1a doesn't need the `MainModule`-overflow partition split, see the module doc).
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
            if method.arg_names().iter().any(Option::is_some) {
                todo!(
                    "Phase 1b: named-parameter Param rows in export_pe — not exercised by the \
                     Phase 1a milestone test, whose methods take no named arguments"
                );
            }
            let param_names: Vec<Option<&str>> = vec![None; sig.inputs().len().saturating_sub(usize::from(!is_static))];
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

    // No FieldRVA data this milestone (see the `const_data_is_empty`/`default_value` `todo!()`s
    // above), so `.sdata` is always empty — `pe::write_pe` omits the section entirely in that
    // case, exactly like the `sdata_absent_when_no_field_rva_data` unit test in `pe.rs` verifies.
    let field_rva_bytes: Vec<u8> = Vec::new();

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

    /// **The Phase 1a milestone acceptance check.** Builds the hand-built hello-world assembly,
    /// exports it via `export_pe` (no `ilasm` anywhere), writes it plus a minimal
    /// `net8.0` `runtimeconfig.json` (mirrors `il_exporter::get_runtime_config`'s shape) to a
    /// scratch directory, and runs it under the real `dotnet` host. Skips (with an `eprintln!`,
    /// not a failure) when no `dotnet` host is available, per the task's guard — but on the
    /// machine this was developed on, `dotnet` is present, so this test actually executes the
    /// produced `.exe` and asserts its output.
    #[test]
    fn e2e_hand_built_assembly_runs_under_dotnet() {
        let dotnet_root = std::env::var("HOME").map(|h| format!("{h}/.dotnet")).unwrap_or_default();
        let dotnet_bin = format!("{dotnet_root}/dotnet");
        if !std::path::Path::new(&dotnet_bin).exists() {
            eprintln!("skipping e2e_hand_built_assembly_runs_under_dotnet: no dotnet host at {dotnet_bin}");
            return;
        }

        let mut asm = build_hello_world_assembly();
        let options = ExportOptions {
            is_dll: false,
            assembly_name: "pe_e2e_hello".to_string(),
        };
        let image = export_pe(&mut asm, &options);

        let scratch = std::env::temp_dir().join("pe_e2e");
        std::fs::create_dir_all(&scratch).expect("create scratch dir");
        let exe_path = scratch.join("pe_e2e_hello.dll"); // apphost-less: `dotnet <path>.dll` runs it directly.
        std::fs::write(&exe_path, &image).expect("write exported PE image");

        // Minimal net8.0 runtimeconfig.json — mirrors the shape
        // `il_exporter::get_runtime_config` (~line 1905) produces, without depending on that
        // module (hard constraint: `pe_exporter` must not import `il_exporter`). `rollForward:
        // LatestMinor` lets a net8.0-targeted TFM run on the newer host installed on this
        // machine (9.0.17 here), matching how `il_exporter`'s version scrapes the live host.
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
        let config_path = scratch.join("pe_e2e_hello.runtimeconfig.json");
        std::fs::File::create(&config_path)
            .and_then(|mut f| f.write_all(runtimeconfig.as_bytes()))
            .expect("write runtimeconfig.json");

        let mut cmd = Command::new(&dotnet_bin);
        cmd.arg(&exe_path).env("DOTNET_ROOT", &dotnet_root);
        let output = cmd.output().expect("spawn dotnet");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            output.status.success(),
            "dotnet exited with {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status.code()
        );
        assert!(
            stdout.contains("PE writer E2E OK"),
            "expected stdout to contain the WriteLine output; got:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
}
