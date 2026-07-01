// DotNetDecimal — the core numeric type of financial .NET code, used like a native number.
#![allow(dead_code)]
use mycorrhiza::prelude::*;
use mycorrhiza::system::console::Console;

fn main() -> std::process::ExitCode {
    let mut pass = 0u32; let mut total = 0u32;
    macro_rules! chk { ($g:expr,$w:expr) => {{ total+=1; if $g==$w {pass+=1;} else {Console::writeln_u64(900_000_000+total as u64);} }}; }

    let a = DotNetDecimal::parse("1234.56");
    let b = DotNetDecimal::parse("1000.44");
    // exact decimal arithmetic (no float error): 1234.56 + 1000.44 == 2235.00
    chk!((a + b).to_dotnet_string().to_rust_string(), "2235.00");
    chk!((a - b).to_dotnet_string().to_rust_string(), "234.12");
    // 0.1 + 0.2 == 0.3 EXACTLY (the classic float trap decimal avoids)
    let p1 = DotNetDecimal::parse("0.1"); let p2 = DotNetDecimal::parse("0.2");
    chk!((p1 + p2).to_dotnet_string().to_rust_string(), "0.3");
    chk!((p1 + p2 == DotNetDecimal::parse("0.3")), true);
    // comparison + from integers
    let ten = DotNetDecimal::from_i64(10);
    let three = DotNetDecimal::from_i32(3);
    chk!((ten > three), true);
    chk!((three < ten), true);
    chk!((ten == DotNetDecimal::from_i64(10)), true);
    chk!((ten * three).to_dotnet_string().to_rust_string(), "30");
    // division stays exact where representable: 10 / 4 == 2.5
    chk!((DotNetDecimal::from_i64(10) / DotNetDecimal::from_i64(4)).to_dotnet_string().to_rust_string(), "2.5");
    // to_f64
    chk!((DotNetDecimal::parse("2.5").to_f64() == 2.5), true);
    // Display
    chk!(format!("{}", DotNetDecimal::parse("42.00")), "42.00");

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
}
