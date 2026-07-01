// Capturing closures as .NET delegates: a `move` closure over local state becomes a managed
// Action/Func whose Invoke drives the Rust closure with its captured environment intact.
use mycorrhiza::prelude::*;
use mycorrhiza::system::console::Console;

static mut SUM: i32 = 0;

fn main() -> std::process::ExitCode {
    let mut pass = 0u32; let mut total = 0u32;
    macro_rules! chk { ($g:expr,$w:expr) => {{ total+=1; if $g==$w {pass+=1;} else {Console::writeln_u64(900_000_000+total as u64);} }}; }

    // Action1 capturing `factor`: .NET Invoke -> trampoline -> closure, factor rides along.
    let factor = 10;
    let acc = Action1::<i32>::from_closure(move |x| unsafe { SUM += x * factor });
    acc.invoke(5);            // SUM += 5*10
    chk!(unsafe { SUM }, 50);
    acc.invoke(3);            // SUM += 3*10
    chk!(unsafe { SUM }, 80);

    // Func1 capturing `base`: returns a value derived from the captured state.
    let base = 100;
    let f = Func1::<i32, i32>::from_closure(move |x| x + base);
    chk!(f.invoke(7), 107);
    chk!(f.invoke(42), 142);

    // A second closure with a DIFFERENT capture — distinct env, no interference.
    let base2 = 1000;
    let g = Func1::<i32, i32>::from_closure(move |x| x * 2 + base2);
    chk!(g.invoke(5), 1010);
    chk!(f.invoke(1), 101);   // f still uses its own base

    // Two-arg Func capturing state.
    let bias = 7;
    let h = Func2::<i32, i32, i32>::from_closure(move |a, b| a - b + bias);
    chk!(h.invoke(10, 3), 14); // 10-3+7

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
}
