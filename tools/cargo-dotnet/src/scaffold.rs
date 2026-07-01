//! `cargo dotnet new` — scaffold a ready-to-run interop project from a template.
//!
//! The onboarding keystone (ERGONOMICS_ROADMAP Theme-5 ⚑): zero-to-running in ONE
//! command instead of hand-assembling a `Cargo.toml` + `.csproj` + `RustDotnet.targets`
//! import. Three templates, each modelled directly on a shipped example crate so the
//! scaffold is guaranteed to be the exact shape the native pipeline already builds:
//!
//!   * `--app`    — a Rust-on-.NET binary using `mycorrhiza::prelude` (models `cd_collections`).
//!                  `cargo dotnet run` builds + runs it.
//!   * `--lib`    — a Rust `cdylib` exporting via `export_rust_containers!()` PLUS a C#
//!                  consumer that references it through `RustDotnet.targets` (models
//!                  `cd_containers`). `dotnet run` in the `csharp/` dir builds both.
//!   * `--plugin` — the `#[dotnet_class]` variant of `--lib`: the Rust side defines a
//!                  managed class a C# host `new`s and calls (models `cd_typedef`).
//!
//! Templates are emitted from string constants (interpolating the crate name) — no
//! network, no example-crate copy at runtime. Every file the corresponding example
//! ships is reproduced, including the `.gitignore`s so a fresh scaffold is clean under
//! version control from the first commit.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::cli::{NewArgs, Template};

/// Run `cargo dotnet new`.
pub fn run(args: &NewArgs) -> Result<i32> {
    let template = args.template()?;
    let name = resolve_name(args)?;
    let dir = target_dir(args, &name)?;

    if dir.exists() && dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false) {
        bail!(
            "target directory is not empty: {} (pass a fresh path, or remove it first)",
            dir.display()
        );
    }
    fs::create_dir_all(&dir)
        .with_context(|| format!("could not create target directory: {}", dir.display()))?;

    let files = render(template, &name);
    for f in &files {
        let path = dir.join(f.rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        fs::write(&path, &f.body).with_context(|| format!("could not write {}", path.display()))?;
    }

    print_next_steps(template, &name, &dir);
    Ok(0)
}

/// A scaffolded file: a repo-relative path + its rendered body.
struct File {
    rel: &'static str,
    body: String,
}

/// A valid crate name is what cargo accepts for a package: a non-empty run of
/// ASCII alphanumerics, `_` or `-`, not starting with a digit or `-`. We keep it
/// strict so the generated `Cargo.toml`/`.csproj` are always valid.
fn valid_crate_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Resolve the crate name: explicit `--name`, else the final component of the path.
fn resolve_name(args: &NewArgs) -> Result<String> {
    let name = if let Some(n) = &args.name {
        n.clone()
    } else {
        args.path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .context("could not derive a crate name from the path; pass --name")?
    };
    if !valid_crate_name(&name) {
        bail!(
            "invalid crate name {name:?}: use ASCII letters/digits/'_'/'-', not starting with a digit"
        );
    }
    Ok(name)
}

/// The absolute target directory for the scaffold.
fn target_dir(args: &NewArgs, name: &str) -> Result<PathBuf> {
    // If the caller passed an explicit path use it verbatim; otherwise the path IS the
    // name (cargo-new convention). Resolve against cwd without requiring it to exist.
    let raw = &args.path;
    let abs = if raw.is_absolute() {
        raw.clone()
    } else {
        std::env::current_dir()
            .context("could not read the current directory")?
            .join(raw)
    };
    // For the derive-name-from-last-component case this is already `<cwd>/<name>`.
    let _ = name;
    Ok(abs)
}

/// Render every file of the chosen template with the crate name interpolated.
fn render(template: Template, name: &str) -> Vec<File> {
    match template {
        Template::App => app_files(name),
        Template::Lib => lib_files(name),
        Template::Plugin => plugin_files(name),
    }
}

// ---------------------------------------------------------------------------------
// --app : a Rust-on-.NET binary (models cd_collections)
// ---------------------------------------------------------------------------------

fn app_files(name: &str) -> Vec<File> {
    vec![
        File {
            rel: "Cargo.toml",
            body: format!(
                "[package]\n\
                 name = \"{name}\"\n\
                 version = \"0.1.0\"\n\
                 edition = \"2021\"\n\
                 \n\
                 [dependencies]\n\
                 mycorrhiza = {{ path = \"{MYCORRHIZA_PATH}\" }}\n\
                 # NOTE: do NOT set the abort panic strategy — the native build-std has no\n\
                 # panic_abort crate; the default unwinding profile is what the backend expects.\n\
                 [workspace]\n",
                MYCORRHIZA_PATH = mycorrhiza_path_hint(),
            ),
        },
        File { rel: ".gitignore", body: GITIGNORE_RUST.to_string() },
        File { rel: "src/main.rs", body: APP_MAIN.to_string() },
    ]
}

// ---------------------------------------------------------------------------------
// --lib : a Rust cdylib exported to C# via export_rust_containers! (models cd_containers)
// ---------------------------------------------------------------------------------

fn lib_files(name: &str) -> Vec<File> {
    let cs_name = format!("{name}_cs");
    vec![
        File {
            rel: "rustlib/Cargo.toml",
            body: format!(
                "[package]\n\
                 name = \"{name}\"\n\
                 version = \"0.1.0\"\n\
                 edition = \"2021\"\n\
                 \n\
                 [lib]\n\
                 # A cdylib: the produced .NET assembly exports `MainModule.rcl_vec_*`, which the C#\n\
                 # side's shipped RustVec<T>/RustBoxVec<T> call.\n\
                 crate-type = [\"cdylib\"]\n\
                 \n\
                 [dependencies]\n\
                 mycorrhiza = {{ path = \"{MYCORRHIZA_PATH}\" }}\n\
                 [workspace]\n\
                 \n\
                 [profile.release.build-override]\n\
                 codegen-units = 1\n",
                MYCORRHIZA_PATH = mycorrhiza_lib_path_hint(),
            ),
        },
        File { rel: "rustlib/.gitignore", body: GITIGNORE_RUSTLIB.to_string() },
        File { rel: "rustlib/src/lib.rs", body: LIB_RUST.to_string() },
        File {
            rel: "csharp/Program.cs",
            body: LIB_CS_PROGRAM.replace("__CRATE__", name),
        },
        File {
            rel: &leak_str(format!("csharp/{cs_name}.csproj")),
            body: csproj_containers(&cs_name),
        },
        File { rel: "csharp/.gitignore", body: GITIGNORE_CS.to_string() },
    ]
}

// ---------------------------------------------------------------------------------
// --plugin : #[dotnet_class] managed type consumed by a C# host (models cd_typedef)
// ---------------------------------------------------------------------------------

fn plugin_files(name: &str) -> Vec<File> {
    let cs_name = format!("{name}_cs");
    vec![
        File {
            rel: "rustlib/Cargo.toml",
            body: format!(
                "[package]\n\
                 name = \"{name}\"\n\
                 version = \"0.1.0\"\n\
                 edition = \"2021\"\n\
                 \n\
                 [lib]\n\
                 # A cdylib whose body defines a managed .NET class via #[dotnet_class]; the C# host\n\
                 # `new`s it and calls its accessors.\n\
                 crate-type = [\"cdylib\"]\n\
                 \n\
                 [dependencies]\n\
                 mycorrhiza = {{ path = \"{MYCORRHIZA_PATH}\" }}\n\
                 dotnet_macros = {{ path = \"{DOTNET_MACROS_PATH}\" }}\n\
                 [workspace]\n\
                 \n\
                 [profile.release.build-override]\n\
                 codegen-units = 1\n",
                MYCORRHIZA_PATH = mycorrhiza_lib_path_hint(),
                DOTNET_MACROS_PATH = dotnet_macros_path_hint(),
            ),
        },
        File { rel: "rustlib/.gitignore", body: GITIGNORE_RUSTLIB.to_string() },
        File { rel: "rustlib/src/lib.rs", body: PLUGIN_RUST.to_string() },
        File {
            rel: "csharp/Program.cs",
            body: PLUGIN_CS_PROGRAM.to_string(),
        },
        File {
            rel: &leak_str(format!("csharp/{cs_name}.csproj")),
            body: csproj_plain(&cs_name),
        },
        File { rel: "csharp/.gitignore", body: GITIGNORE_CS.to_string() },
    ]
}

// ---------------------------------------------------------------------------------
// Path hints for the mycorrhiza / dotnet_macros path dependencies.
// ---------------------------------------------------------------------------------
//
// A scaffolded project needs `mycorrhiza = { path = ... }`. Outside the repo there is
// no canonical published crate yet, so we emit a `<CARGO_DOTNET_HOME>/crates/...`-style
// relative hint the user edits, and print a clear note. Inside the repo (dev), the
// example crates use `../../mycorrhiza`; we cannot know the scaffold's depth, so we emit
// the env-anchored path and rely on the printed note.

fn mycorrhiza_path_hint() -> &'static str {
    // For an --app scaffolded at <cwd>/<name>, the repo layout the examples use is two
    // levels up (cargo_tests/cd_*/ -> ../../mycorrhiza). We keep that as the DEFAULT and
    // tell the user to adjust if they scaffolded elsewhere.
    "../../mycorrhiza"
}

fn mycorrhiza_lib_path_hint() -> &'static str {
    // For --lib/--plugin the Rust crate lives one level deeper (<name>/rustlib), so the
    // examples use ../../../mycorrhiza.
    "../../../mycorrhiza"
}

fn dotnet_macros_path_hint() -> &'static str {
    "../../../dotnet_macros"
}

// ---------------------------------------------------------------------------------
// csproj rendering.
// ---------------------------------------------------------------------------------

/// The C#-consumer csproj for `--lib`: opts into the shipped RustVec<T>/RustBoxVec<T>
/// wrappers and auto-builds the Rust crate via RustDotnet.targets.
fn csproj_containers(cs_name: &str) -> String {
    CSPROJ_TEMPLATE
        .replace("__ASSEMBLY__", cs_name)
        .replace("__CONTAINERS_PROP__", "\n    <UseRustDotnetContainers>true</UseRustDotnetContainers>")
}

/// The C#-host csproj for `--plugin`: no containers, just auto-builds the Rust crate.
fn csproj_plain(cs_name: &str) -> String {
    CSPROJ_TEMPLATE
        .replace("__ASSEMBLY__", cs_name)
        .replace("__CONTAINERS_PROP__", "")
}

// ---------------------------------------------------------------------------------
// Post-scaffold guidance.
// ---------------------------------------------------------------------------------

fn print_next_steps(template: Template, name: &str, dir: &Path) {
    let d = dir.display();
    println!("== scaffolded {} project '{name}' at {d} ==", template.label());
    println!();
    match template {
        Template::App => {
            println!("Next:");
            println!("  cd {d}");
            println!("  # adjust the `mycorrhiza` path dependency in Cargo.toml if needed");
            println!("  cargo dotnet run");
        }
        Template::Lib | Template::Plugin => {
            println!("Next:");
            println!("  cd {d}/csharp");
            println!("  # adjust the `mycorrhiza` path dependency in ../rustlib/Cargo.toml if needed");
            println!("  # ensure CARGO_DOTNET_HOME points at your install (or ~/.cargo-dotnet exists)");
            println!("  dotnet run -c Release");
        }
    }
    println!();
    println!("(the `mycorrhiza` path deps default to the in-repo layout; run `cargo dotnet doctor` if a build fails)");
}

// ---------------------------------------------------------------------------------
// A `&'static str` from an owned String (rel paths are `&'static` in `File`). Scaffold
// runs once per process and exits, so the small, bounded leak is fine — it avoids
// threading lifetimes through the whole template table.
// ---------------------------------------------------------------------------------
fn leak_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

// =================================================================================
// TEMPLATE BODIES
// =================================================================================

const GITIGNORE_RUST: &str = "/target\nCargo.lock\n/.cargo/config.toml\n";
const GITIGNORE_RUSTLIB: &str = "/target\n/.cargo/config.toml\nCargo.lock\n";
const GITIGNORE_CS: &str = "bin/\nobj/\n";

/// `--app` main: mirrors the `cd_collections` chk! convention so the scaffold is a
/// runnable, self-checking proof out of the box.
const APP_MAIN: &str = r#"//! A Rust program that runs on .NET via rustc_codegen_clr.
//!
//! `mycorrhiza::prelude` brings the .NET generic collections + idiomatic wrappers into scope so this
//! reads like `std`. Build + run with:  `cargo dotnet run`
#![allow(dead_code)]

use mycorrhiza::prelude::*;
use mycorrhiza::system::console::Console;

fn main() -> std::process::ExitCode {
    // A tiny in-Rust check harness: tallies pass/total, prints a 9000000xx marker on any failure,
    // then prints `pass` and `total`. Returns non-zero if anything mismatched.
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

    // Use a .NET List<T> exactly like a Vec.
    let mut xs = List::<i32>::new();
    for i in 0..5i32 {
        xs.push(i * 10); // 0,10,20,30,40
    }
    chk!(xs.len(), 5);
    chk!(xs.get(0), Some(0));
    chk!(xs.get(4), Some(40));
    let mut sum = 0i32;
    for v in xs.iter() {
        sum += v;
    }
    chk!(sum, 100);

    // Use a .NET Dictionary<K,V> like a HashMap.
    let mut m = Dictionary::<i32, i64>::new();
    m.insert(1, 100);
    m.insert(2, 200);
    chk!(m.get(1), Some(100));
    chk!(m.get(99), None);

    // A managed string, printed through System.Console.
    let greeting = DotNetString::from("hello from Rust on .NET");
    Console::writeln_string(greeting.handle());

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
"#;

/// `--lib` Rust: the whole crate body is one macro call emitting the reusable container core.
const LIB_RUST: &str = r#"//! A Rust cdylib whose entire body exports the reusable C#->Rust generic container core.
//!
//! `export_rust_containers!()` emits `MainModule.rcl_vec_*` into the produced .NET assembly. The C#
//! consumer's shipped `RustDotnet.RustVec<T>` / `RustBoxVec<T>` wrappers (auto-included by
//! RustDotnet.targets because <UseRustDotnetContainers>true</UseRustDotnetContainers>) call those.
mycorrhiza::export_rust_containers!();
"#;

/// `--lib` C# consumer: uses the shipped RustVec<T> wrapper against the Rust crate.
const LIB_CS_PROGRAM: &str = r#"// Consuming the Rust cdylib from C# with ZERO hand-written interop: the Rust side is one
// `export_rust_containers!()` line; this side uses the shipped RustDotnet.RustVec<T>.
using System;
using RustDotnet;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        using (var vi = RustVec<int>.New())
        {
            vi.Push(10);
            vi.Push(20);
            vi.Push(30);
            Check("Count", vi.Count, 3, ref pass, ref total);
            Check("Get(2)", vi.Get(2), 30, ref pass, ref total);
            vi.Set(0, 99);
            Check("Get(0) after Set", vi.Get(0), 99, ref pass, ref total);
        }

        Console.WriteLine($"__CRATE__: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok) pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
"#;

/// `--plugin` Rust: define a managed .NET class via `#[dotnet_class]`.
const PLUGIN_RUST: &str = r#"//! A Rust cdylib that DEFINES a managed .NET class via the `#[dotnet_class]` proc-macro.
//!
//! The annotated struct becomes a real managed reference type with a parameterized primary
//! constructor and a `read_<field>()` accessor per field. A C# host can `new Counter(5, 100)` and
//! call `read_value()` / `read_step()` — no `#[no_mangle]`, no marshalling boilerplate.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use dotnet_macros::dotnet_class;

/// A managed `Counter : System.Object` with fields `value: int32`, `step: int64`, a ctor
/// `Counter(int32, int64)`, and `read_value()` / `read_step()`.
#[dotnet_class]
pub struct Counter {
    value: i32,
    step: i64,
}
"#;

/// `--plugin` C# host: constructs the Rust-defined managed class and reads it back.
const PLUGIN_CS_PROGRAM: &str = r#"// Using a Rust-DEFINED managed .NET class from C#. The `Counter` type below is produced by the Rust
// crate's `#[dotnet_class]` struct — RustDotnet.targets built the Rust crate and referenced its
// assembly, so `Counter`, its ctor, and its accessors are all available here.
using System;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        var c = new Counter(5, 100);
        Check("read_value()", c.read_value(), 5, ref pass, ref total);
        Check("read_step()", c.read_step(), 100L, ref pass, ref total);

        Console.WriteLine($"plugin: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok) pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
"#;

/// The C# csproj, shared by `--lib` and `--plugin`. `__CONTAINERS_PROP__` toggles the
/// shipped-container opt-in; `__ASSEMBLY__` is the C# assembly name. The 3-way
/// RustDotnet.targets import mirrors the example crates: prefer `$CARGO_DOTNET_HOME`,
/// then `$HOME/.cargo-dotnet`, then the in-repo `msbuild/` as a dev fallback.
const CSPROJ_TEMPLATE: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <RustDotnetVersion Condition="'$(RustDotnetVersion)'==''">8</RustDotnetVersion>
    <TargetFramework>net$(RustDotnetVersion).0</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <Nullable>disable</Nullable>
    <ImplicitUsings>disable</ImplicitUsings>
    <AssemblyName>__ASSEMBLY__</AssemblyName>__CONTAINERS_PROP__
  </PropertyGroup>

  <!-- Auto-build the Rust crate + reference its assembly (RustDotnet.targets). -->
  <Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
  <Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'=='' and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />

  <ItemGroup>
    <RustCrate Include="../rustlib" />
  </ItemGroup>
</Project>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_validation() {
        assert!(valid_crate_name("my_app"));
        assert!(valid_crate_name("my-app"));
        assert!(valid_crate_name("App123"));
        assert!(valid_crate_name("_x"));
        assert!(!valid_crate_name(""));
        assert!(!valid_crate_name("1abc"));
        assert!(!valid_crate_name("-abc"));
        assert!(!valid_crate_name("has space"));
        assert!(!valid_crate_name("dot.name"));
    }

    #[test]
    fn app_scaffold_has_expected_files() {
        let files = app_files("demo");
        let rels: Vec<&str> = files.iter().map(|f| f.rel).collect();
        assert!(rels.contains(&"Cargo.toml"));
        assert!(rels.contains(&"src/main.rs"));
        assert!(rels.contains(&".gitignore"));
        // Name interpolated into Cargo.toml.
        let cargo = &files.iter().find(|f| f.rel == "Cargo.toml").unwrap().body;
        assert!(cargo.contains("name = \"demo\""));
        assert!(cargo.contains("mycorrhiza"));
        // No panic=abort (native build-std has no panic_abort).
        assert!(!cargo.contains("panic = \"abort\""));
        // main.rs uses the prelude and a chk! harness.
        let main = &files.iter().find(|f| f.rel == "src/main.rs").unwrap().body;
        assert!(main.contains("use mycorrhiza::prelude::*;"));
        assert!(main.contains("macro_rules! chk"));
    }

    #[test]
    fn lib_scaffold_has_expected_files() {
        let files = lib_files("mylib");
        let rels: Vec<&str> = files.iter().map(|f| f.rel).collect();
        assert!(rels.contains(&"rustlib/Cargo.toml"));
        assert!(rels.contains(&"rustlib/src/lib.rs"));
        assert!(rels.iter().any(|r| r.starts_with("csharp/") && r.ends_with(".csproj")));
        assert!(rels.contains(&"csharp/Program.cs"));
        // csproj name derives from the crate name.
        let csproj = files
            .iter()
            .find(|f| f.rel.ends_with(".csproj"))
            .unwrap();
        assert_eq!(csproj.rel, "csharp/mylib_cs.csproj");
        assert!(csproj.body.contains("<AssemblyName>mylib_cs</AssemblyName>"));
        // containers opt-in present for --lib.
        assert!(csproj.body.contains("UseRustDotnetContainers"));
        // Rust side exports the container core.
        let lib = &files.iter().find(|f| f.rel == "rustlib/src/lib.rs").unwrap().body;
        assert!(lib.contains("export_rust_containers!()"));
        // Program.cs got the crate name interpolated into its banner.
        let prog = &files.iter().find(|f| f.rel == "csharp/Program.cs").unwrap().body;
        assert!(prog.contains("mylib:"));
        assert!(!prog.contains("__CRATE__"));
    }

    #[test]
    fn plugin_scaffold_has_expected_files() {
        let files = plugin_files("myplug");
        let rels: Vec<&str> = files.iter().map(|f| f.rel).collect();
        assert!(rels.contains(&"rustlib/Cargo.toml"));
        assert!(rels.contains(&"rustlib/src/lib.rs"));
        assert!(rels.contains(&"csharp/Program.cs"));
        // plugin Cargo.toml pulls in dotnet_macros.
        let cargo = &files
            .iter()
            .find(|f| f.rel == "rustlib/Cargo.toml")
            .unwrap()
            .body;
        assert!(cargo.contains("dotnet_macros"));
        // Rust side uses #[dotnet_class].
        let lib = &files.iter().find(|f| f.rel == "rustlib/src/lib.rs").unwrap().body;
        assert!(lib.contains("#[dotnet_class]"));
        // plugin csproj does NOT opt into containers.
        let csproj = files.iter().find(|f| f.rel.ends_with(".csproj")).unwrap();
        assert!(!csproj.body.contains("UseRustDotnetContainers"));
    }

    #[test]
    fn no_template_placeholders_leak() {
        for name in ["a", "b_c", "Xyz"] {
            for files in [app_files(name), lib_files(name), plugin_files(name)] {
                for f in files {
                    assert!(
                        !f.body.contains("__ASSEMBLY__"),
                        "unresolved __ASSEMBLY__ in {}",
                        f.rel
                    );
                    assert!(
                        !f.body.contains("__CONTAINERS_PROP__"),
                        "unresolved __CONTAINERS_PROP__ in {}",
                        f.rel
                    );
                    assert!(
                        !f.body.contains("__CRATE__"),
                        "unresolved __CRATE__ in {}",
                        f.rel
                    );
                    assert!(
                        !f.body.contains("MYCORRHIZA_PATH"),
                        "unresolved MYCORRHIZA_PATH token in {}",
                        f.rel
                    );
                }
            }
        }
    }
}
