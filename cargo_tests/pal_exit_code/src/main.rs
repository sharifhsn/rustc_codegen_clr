//! P2-S2 regression: `std::process::exit(code)` must terminate the process with
//! that exact code on the dotnet PAL, matching native rustc — NOT drop the code
//! and abort.
//!
//! Before the fix, the injected `target_os = "dotnet"` arm of `sys::exit::exit`
//! was `let _ = code; crate::intrinsics::abort()`: it discarded the requested
//! exit code and threw the backend's "Called abort!" exception, so the process
//! died with SIGABRT (exit 134) instead of the requested code. The fix routes the
//! arm to `rcl_dotnet_exit(code)`, a PAL symbol the cilly linker maps to
//! `System.Environment.Exit((int)code)` — a clean managed process-exit carrying
//! the code.
//!
//! DIFFERENTIAL ORACLE (run BOTH on the same nightly, compare stdout + exit code):
//!   native : `cargo run --release`                 -> stdout "exit-code probe\n", exit 7
//!   backend: `cargo dotnet run <dir> --release`    -> stdout "exit-code probe\n", exit 7
//!
//! A regression reappears as exit 134 (abort) instead of 7.

fn main() {
    println!("exit-code probe");
    std::process::exit(7);
}
