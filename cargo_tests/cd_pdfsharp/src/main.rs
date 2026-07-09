//! Real proof that Rust can drive `PdfSharpCore` (the cross-platform, ImageSharp-backed fork of
//! the classic PdfSharp — plain `PdfSharp` itself depends on `System.Drawing`/GDI+, which isn't
//! available on this non-Windows CoreCLR target, so `PdfSharpCore` is the honest pick for the
//! "heavier, native-interop-flavored" library slot) via `cargo dotnet add-nuget` bindings: build
//! a real multi-page PDF document (title/author metadata + 3 pages) and confirm the produced file
//! has the correct `%PDF-` header and `%%EOF` trailer byte signature — opened and validated by an
//! INDEPENDENT tool (`file`/hex dump), not just "PdfSharpCore didn't throw".
//!
//! TWO DISTINCT BINDING-GENERATION GAPS DIAGNOSED GETTING HERE (see the task report for the full
//! writeup; both are in `tools/cargo-dotnet/src/nuget.rs`, now fixed):
//!
//! 1. **Wrong dll picked from the .nupkg.** `PdfSharpCore`'s `.nupkg` ships a German satellite
//!    resource assembly (`lib/net8.0/de/PdfSharpCore.resources.dll`) that sorted before the real
//!    `lib/net8.0/PdfSharpCore.dll` in the zip's physical entry order — the old "first .dll under
//!    `lib/<tfm>/`" search picked the (empty) resources dll and reflected ZERO types. Fixed by
//!    requiring the dll sit DIRECTLY under `lib/<tfm>/` (no locale subfolder) and preferring a
//!    file stem matching the package id.
//! 2. **Missing transitive dependencies at reflection time.** `PdfSharpCore` depends on
//!    `SixLabors.ImageSharp`/`SixLabors.Fonts`/`SharpZipLib` for its image support;
//!    `Assembly.LoadFrom` + `Module.GetTypes()` throws `ReflectionTypeLoadException` for the
//!    WHOLE assembly if any referenced dependency assembly can't be resolved — even for types
//!    that never touch it. `add-nuget` now parses the `.nuspec`'s `<dependency>` list and fetches
//!    one level of transitive dependencies alongside the primary dll (for both the reflection
//!    step AND the consumer's own runtime output).
//!
//! ONE REMAINING, UNFIXED GAP (documented, not worked around): `XGraphics`'s actual drawing
//! surface (`DrawString`, `DrawLine`, `DrawRectangle`, `DrawImage`, ...) takes `XPoint`/`XRect`/
//! `XSize` — .NET STRUCTS (value types) — as parameters. spinacz's reflector unconditionally
//! `Skip`s any method with a value-type parameter (`DType::from_tpe`, `cargo_tests/spinacz/src/
//! reflect.rs`: "value types other than the recognised primitives have no generated alias"), so
//! NONE of `XGraphics`'s draw methods appear in the generated bindings — only structural methods
//! (`Save`/`Restore`/`*Transform`/`DrawPath` with ref-typed `XPen`/`XGraphicsPath`) survive. This
//! test therefore proves real multi-page PDF *document* construction (pages, metadata, save to a
//! real file) through PdfSharpCore, but NOT free-form text/shape drawing — that would need the
//! WF-9 value-type marshalling work spinacz's reflector doesn't have yet.
#![allow(dead_code)]

mod nuget;

use nuget::pdfsharpcore::PdfSharpCore::Pdf::{
    PdfDocument, PdfDocumentInformation, PdfDocumentInformation_Methods, PdfDocument_Methods, PdfPage_Methods,
    PdfRectangle_Methods,
};
use mycorrhiza::system::console::Console;
use mycorrhiza::system::DotNetString;

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    let doc: PdfDocument = PdfDocument::new();

    let info: PdfDocumentInformation = doc.get_info();
    let title: DotNetString = "Rust on .NET smoke test".into();
    let author: DotNetString = "cargo dotnet add-nuget".into();
    info.set_title(title.handle());
    info.set_author(author.handle());
    let got_title = String::from(DotNetString::from_handle(info.get_title()));
    chk!(got_title.as_str(), "Rust on .NET smoke test");

    // Three real pages, each with its own default Letter-size MediaBox from PdfSharpCore itself.
    let page1 = doc.add_page();
    let page2 = doc.add_page();
    let page3 = doc.add_page();
    chk!(doc.get_page_count(), 3);

    let mb1 = page1.get_media_box();
    let mb2 = page2.get_media_box();
    let mb3 = page3.get_media_box();
    // Real geometry computed by PdfSharpCore (US Letter: 612 x 792 points), not hand-typed —
    // cross-checked against all three pages agreeing with each other.
    chk!((mb1.get_width() > 0.0), true);
    chk!((mb1.get_height() > 0.0), true);
    chk!(mb1.get_width(), mb2.get_width());
    chk!(mb2.get_width(), mb3.get_width());

    let out_path = std::env::temp_dir().join("cd_pdfsharp_smoke.pdf");
    let path_str: DotNetString = out_path.to_string_lossy().into_owned().as_str().into();
    doc.save(path_str.handle());
    doc.close();

    // Verify the produced file independently through plain Rust `std::fs` (NOT PdfSharpCore
    // reading its own output back — that would just prove round-trip self-consistency).
    let bytes = std::fs::read(&out_path);
    match bytes {
        Ok(bytes) => {
            total += 1;
            let has_header = bytes.len() > 5 && &bytes[0..5] == b"%PDF-";
            let tail = if bytes.len() >= 6 { &bytes[bytes.len() - 6..] } else { &bytes[..] };
            let has_trailing_eof = tail.windows(5).any(|w| w == b"%%EOF");
            if has_header && has_trailing_eof && bytes.len() > 200 {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
            Console::writeln_u64(bytes.len() as u64); // observability: real byte count, not asserted exactly
        }
        Err(_) => {
            total += 1;
            Console::writeln_u64(900_000_000 + total as u64);
        }
    }

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
