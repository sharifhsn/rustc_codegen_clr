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
    // Debug (same rendering as Display, exact)
    chk!(format!("{:?}", DotNetDecimal::parse("42.00")), "42.00");

    // Neg (op_UnaryNegation) is exact, not float-approximated
    chk!((-p1).to_dotnet_string().to_rust_string(), "-0.1");
    chk!((-(p1 + p2)).to_dotnet_string().to_rust_string(), "-0.3");
    chk!((-DotNetDecimal::from_i64(5) == DotNetDecimal::from_i64(-5)), true);

    // Default == Zero, and is the identity for +/-
    chk!((DotNetDecimal::default() == DotNetDecimal::zero()), true);
    chk!(((DotNetDecimal::default() + ten) == ten), true);
    chk!(format!("{}", DotNetDecimal::default()), "0");

    // from_u64 stays exact across the full u64 range (would misread as negative via i64)
    let big = u64::MAX;
    chk!(
        DotNetDecimal::from_u64(big).to_dotnet_string().to_rust_string(),
        big.to_string()
    );
    chk!((DotNetDecimal::from_u64(0) == DotNetDecimal::zero()), true);

    // from_f64 (explicit conversion): exact for exactly-representable values...
    chk!((DotNetDecimal::from_f64(2.5) == DotNetDecimal::parse("2.5")), true);
    // ...and the classic float trap: 0.1f64 + 0.2f64 != 0.3 in f64, but going through Decimal's own
    // conversion (not a home-grown approximation) still reflects real double-precision rounding,
    // i.e. this documents that from_f64 is NOT a magic fixer of float error, only an exact CLR
    // double->decimal conversion:
    chk!((DotNetDecimal::from_f64(0.1) == DotNetDecimal::parse("0.1")), true);

    // From<i64>/From<i32>/From<u64> ergonomic conversions match the from_* constructors exactly
    let via_from: DotNetDecimal = 10i64.into();
    chk!((via_from == DotNetDecimal::from_i64(10)), true);
    let via_from32: DotNetDecimal = 3i32.into();
    chk!((via_from32 == DotNetDecimal::from_i32(3)), true);
    let via_fromu64: DotNetDecimal = 42u64.into();
    chk!((via_fromu64 == DotNetDecimal::from_u64(42)), true);

    // Ord: real total order, usable with sort()
    let mut v = vec![
        DotNetDecimal::parse("3.5"),
        DotNetDecimal::parse("-1.2"),
        DotNetDecimal::parse("0"),
        DotNetDecimal::parse("2.25"),
    ];
    v.sort();
    let rendered: Vec<String> = v.iter().map(|d| d.to_dotnet_string().to_rust_string()).collect();
    chk!(rendered, vec!["-1.2", "0", "2.25", "3.5"]);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        println!("== cd_decimal done ==");
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
