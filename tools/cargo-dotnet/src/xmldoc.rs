//! Sidecar XML-doc generation for Rust-defined managed APIs.
//!
//! `dotnet_macros` scrapes `#[doc = "..."]` attributes from exports, classes, enums, methods,
//! properties, constructors, and interfaces at the consumer's compile time and appends one
//! newline-delimited-JSON entry per documented member to
//! `<crate_dir>/target/dotnet_xmldoc/<crate_name>.xmldoc.jsonl` (see `dotnet_macros/src/lib.rs`,
//! `emit_xmldoc_member`). This module reads that scratch file after a successful build and
//! assembles the standard ECMA-334 `<AssemblyName>.xml` sidecar doc file next to the built DLL —
//! the mechanism every .NET IDE/IntelliSense already knows how to pick up, with zero `cilly/src`
//! changes on the codegen side.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::context::{Context, ManagedIdentity};

/// Delete proc-macro scratch before one product build.
///
/// [`build_id`] is injected into every documentation-producing macro expansion, so Cargo
/// re-expands the crate after this removal even when the Rust source itself is unchanged. The
/// human-readable and JSON artifact-locator passes share one build ID; duplicate entries from
/// those passes are de-duplicated by member ID in [`generate`].
pub fn clear_scratch(ctx: &Context) {
    let _ = fs::remove_dir_all(ctx.crate_dir.join("target").join("dotnet_xmldoc"));
}

/// One token for all Cargo passes launched by this driver process.
///
/// Macro expansions include `option_env!("RCL_XMLDOC_BUILD_ID")`, so Cargo records this value as
/// an input and re-expands the crate after `clear_scratch`, even when no Rust source changed. The
/// normal build and JSON artifact-locator pass share the same token and therefore do not rebuild
/// each other.
pub fn build_id() -> &'static str {
    static BUILD_ID: OnceLock<String> = OnceLock::new();
    BUILD_ID.get_or_init(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{}-{nanos}", std::process::id())
    })
}

/// One scraped doc entry: an exact ECMA-334 member-ID plus a macro-generated, escaped XML body.
struct Entry {
    member: String,
    xml: String,
}

/// Parse the tiny escaped-string JSON-object-per-line format `dotnet_macros::emit_xmldoc_member`
/// writes: `{"member":"...","xml":"..."}`. Hand-rolled to avoid a `serde_json` dependency for
/// two fields with a fixed, macro-controlled shape; the legacy `summary` field remains readable.
fn parse_line(line: &str) -> Option<Entry> {
    let member = extract_field(line, "member")?;
    let xml = extract_field(line, "xml").or_else(|| {
        // Read scratch retained from SDK versions that only recorded a summary. This matters for
        // incremental builds where Cargo may not re-run the proc macro.
        extract_field(line, "summary")
            .map(|summary| format!("<summary>{}</summary>", xml_escape(&summary)))
    })?;
    Some(Entry { member, xml })
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

#[cfg(test)]
mod tests {
    use super::parse_line;

    #[test]
    fn parses_structured_xml_entries() {
        let entry = parse_line(
            r#"{"member":"M:MainModule.Compute(System.Int32)","xml":"<summary>Compute</summary>\n<param name=\"value\">Input</param>"}"#,
        )
        .unwrap();
        assert_eq!(entry.member, "M:MainModule.Compute(System.Int32)");
        assert!(entry.xml.contains("<param name=\"value\">Input</param>"));
    }

    #[test]
    fn upgrades_legacy_summary_entries_without_trusting_their_text_as_xml() {
        let entry = parse_line(r#"{"member":"M:MainModule.Legacy","summary":"uses <old> & text"}"#)
            .unwrap();
        assert_eq!(entry.xml, "<summary>uses &lt;old&gt; &amp; text</summary>");
    }
}

/// Read `<crate_dir>/target/dotnet_xmldoc/<crate_name>.xmldoc.jsonl` (if present) and write the
/// ECMA-334 sidecar `<dll_stem>.xml` beside `dll_path`. Crates without exported documentation get
/// a valid empty `<members>` inventory so release packages always carry a documentation contract.
pub fn generate(
    crate_dir: &Path,
    crate_name: &str,
    dll_path: &Path,
    managed_identity: Option<&ManagedIdentity>,
) -> Result<()> {
    let scratch = crate_dir
        .join("target")
        .join("dotnet_xmldoc")
        .join(format!("{crate_name}.xmldoc.jsonl"));
    let contents = fs::read_to_string(&scratch).unwrap_or_default();

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
        if let Some(existing) = entries
            .iter_mut()
            .find(|x: &&mut Entry| x.member == e.member)
        {
            *existing = e;
        } else {
            entries.push(e);
        }
    }
    let asm_name = dll_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(crate_name);

    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\"?>\n<doc>\n");
    xml.push_str(&format!(
        "<assembly>\n<name>{}</name>\n</assembly>\n",
        xml_escape(asm_name)
    ));
    xml.push_str("<members>\n");
    let public_type = managed_identity
        .and_then(ManagedIdentity::module_full_name)
        .unwrap_or_else(|| "MainModule".to_string());
    for e in &entries {
        let member = e.member.strip_prefix("M:MainModule.").map_or_else(
            || e.member.clone(),
            |suffix| format!("M:{public_type}.{suffix}"),
        );
        xml.push_str(&format!(
            "<member name=\"{}\">\n{}\n</member>\n",
            xml_escape(&member),
            e.xml
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
