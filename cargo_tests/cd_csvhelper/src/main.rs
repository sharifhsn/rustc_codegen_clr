//! Real proof that Rust can drive `CsvHelper` (a third-party NuGet package) via
//! `cargo dotnet add-nuget` bindings: parse a small CSV string with a header row and read back
//! field values, using CsvHelper's real parsing/quoting logic (not a hand-rolled split).
#![allow(dead_code)]

mod nuget;

use nuget::csvhelper::CsvHelper::{CsvReader, CsvReader_Methods};
use mycorrhiza::system::console::Console;
use mycorrhiza::system::DotNetString;
use mycorrhiza::System::Globalization::CultureInfo;
use mycorrhiza::System::IO::StringReader;

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

    // CSV with a quoted field containing a comma, to actually exercise CsvHelper's real parser
    // (a naive split(',') would mis-parse this).
    let csv = "Name,City,Age\r\n\"Doe, John\",Springfield,42\r\nJane Smith,\"Metropolis\",37\r\n";
    let text: DotNetString = csv.into();
    let reader = StringReader::new(text.handle());
    let culture = CultureInfo::get_invariant_culture();
    let csv_reader: CsvReader = CsvReader::new(reader.into(), culture, false);

    let has_header = csv_reader.read();
    chk!(has_header, true);
    let header_ok = csv_reader.read_header();
    chk!(header_ok, true);

    // Row 1: "Doe, John", Springfield, 42
    let row1 = csv_reader.read();
    chk!(row1, true);
    let name1 = String::from(DotNetString::from_handle(csv_reader.get_field(0)));
    let city1 = String::from(DotNetString::from_handle(csv_reader.get_field(1)));
    let age1 = String::from(DotNetString::from_handle(csv_reader.get_field(2)));
    chk!(name1.as_str(), "Doe, John");
    chk!(city1.as_str(), "Springfield");
    chk!(age1.as_str(), "42");

    // Row 2: Jane Smith, Metropolis, 37
    let row2 = csv_reader.read();
    chk!(row2, true);
    let name2 = String::from(DotNetString::from_handle(csv_reader.get_field(0)));
    let city2 = String::from(DotNetString::from_handle(csv_reader.get_field(1)));
    let age2 = String::from(DotNetString::from_handle(csv_reader.get_field(2)));
    chk!(name2.as_str(), "Jane Smith");
    chk!(city2.as_str(), "Metropolis");
    chk!(age2.as_str(), "37");

    // No more rows.
    let row3 = csv_reader.read();
    chk!(row3, false);

    csv_reader.dispose();

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
