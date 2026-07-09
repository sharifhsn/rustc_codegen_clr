//! Sidecar XML-doc generation for `#[dotnet_export]`.
//!
//! `dotnet_macros`'s `#[dotnet_export]` expansion scrapes `#[doc = "..."]` attrs off each exported
//! fn at the consumer's compile time and appends one newline-delimited-JSON entry per fn to
//! `<crate_dir>/target/dotnet_xmldoc/<crate_name>.xmldoc.jsonl` (see `dotnet_macros/src/lib.rs`,
//! `emit_xmldoc_entry`). This module reads that scratch file after a successful build and
//! assembles the standard ECMA-334 `<AssemblyName>.xml` sidecar doc file next to the built DLL —
//! the mechanism every .NET IDE/IntelliSense already knows how to pick up, with zero `cilly/src`
//! changes on the codegen side.
//!
//! Design reference: `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md`, "Tier C research findings", item 4.
//!
//! Known limitation (documented, not solved, in this first slice): the member-ID this module
//! writes assumes `#[dotnet_export]` always emits directly onto `MainModule` with no
//! namespace/enclosing-type nesting and no name-shortening — true for the entire marshalling
//! surface `dotnet_macros` supports today. If `MainModule` is ever partitioned across per-module
//! classes for size (`cilly/src/ir/il_exporter/partition.rs`) or exported fns gain a
//! namespace/nesting option, the member-ID derivation in `dotnet_macros::emit_xmldoc_entry` would
//! need to track that to keep matching the real emitted metadata name exactly.

use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::context::Context;

/// Delete the whole `<crate_dir>/target/dotnet_xmldoc/` scratch dir before a build.
///
/// `dotnet_macros::emit_xmldoc_entry` can only ever APPEND (a proc-macro invocation has no way to
/// know about, or clear, entries from a previous compiler run). Clearing here means the scratch
/// dir always reflects exactly the fns that get (re-)expanded on THIS build; entries never
/// accumulate duplicates across repeated builds. Best-effort: a missing dir is not an error, and a
/// failure to remove it is only reported, never fatal (this must not block an actual build).
pub fn clear_scratch(ctx: &Context) {
    let dir = ctx.crate_dir.join("target").join("dotnet_xmldoc");
    if dir.is_dir() {
        if let Err(e) = fs::remove_dir_all(&dir) {
            eprintln!("== xml docs: could not clear stale scratch dir {}: {e} ==", dir.display());
        }
    }
}

/// One scraped doc entry: an exact ECMA-334 member-ID plus its doc-comment body.
struct Entry {
    member: String,
    summary: String,
}

/// Parse the tiny escaped-string JSON-object-per-line format `dotnet_macros::emit_xmldoc_entry`
/// writes: `{"member":"...","summary":"..."}`. Hand-rolled to avoid a `serde_json` dependency for
/// two fields with a fixed, macro-controlled shape.
fn parse_line(line: &str) -> Option<Entry> {
    let member = extract_field(line, "member")?;
    let summary = extract_field(line, "summary")?;
    Some(Entry { member, summary })
}

fn extract_field(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = (&mut chars).take(4).collect();
                    let cp = u32::from_str_radix(&hex, 16).ok()?;
                    out.push(char::from_u32(cp)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// XML-escape doc text for embedding inside an element body.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Read `<crate_dir>/target/dotnet_xmldoc/<crate_name>.xmldoc.jsonl` (if present) and write the
/// ECMA-334 sidecar `<dll_stem>.xml` beside `dll_path`. No-op (not an error) if no scratch file
/// exists — most crates don't use `#[dotnet_export]` doc comments, and this must never fail a
/// build over doc generation.
pub fn generate(crate_dir: &Path, crate_name: &str, dll_path: &Path) -> Result<()> {
    let scratch = crate_dir
        .join("target")
        .join("dotnet_xmldoc")
        .join(format!("{crate_name}.xmldoc.jsonl"));
    let Ok(contents) = fs::read_to_string(&scratch) else {
        return Ok(());
    };

    // De-duplicate by member-ID, keeping the LAST occurrence. `cargo-dotnet`'s own pipeline runs
    // the inner `cargo build` twice per invocation (a human-readable pass, then a
    // `--message-format=json` pass for the artifact locator) — if cargo actually re-executes
    // rustc for either pass (e.g. after `--clean`, or a flag mismatch that busts the fingerprint
    // cache), `#[dotnet_export]`'s proc-macro expansion re-runs and appends the SAME entries
    // again, since a proc-macro invocation has no way to see prior appends. Deduping here keeps
    // the sidecar's `<members>` list well-formed (repeated `<member name="...">` for one ID is
    // invalid/undefined per ECMA-334) regardless of how many times the compiler actually invoked
    // the macro for a given fn.
    let mut entries: Vec<Entry> = Vec::new();
    for e in contents.lines().filter_map(parse_line) {
        if let Some(existing) = entries.iter_mut().find(|x: &&mut Entry| x.member == e.member) {
            *existing = e;
        } else {
            entries.push(e);
        }
    }
    if entries.is_empty() {
        return Ok(());
    }

    let asm_name = dll_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(crate_name);

    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\"?>\n<doc>\n");
    xml.push_str(&format!("<assembly>\n<name>{}</name>\n</assembly>\n", xml_escape(asm_name)));
    xml.push_str("<members>\n");
    for e in &entries {
        xml.push_str(&format!(
            "<member name=\"{}\">\n<summary>{}</summary>\n</member>\n",
            xml_escape(&e.member),
            xml_escape(&e.summary)
        ));
    }
    xml.push_str("</members>\n</doc>\n");

    let xml_path = dll_path.with_extension("xml");
    fs::write(&xml_path, xml)?;
    eprintln!(
        "== xml docs: {} ({} member{}) ==",
        xml_path.display(),
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    );
    Ok(())
}
