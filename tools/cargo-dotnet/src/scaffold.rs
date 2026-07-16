//! `cargo dotnet new` — scaffold a ready-to-run interop project from a template.
//!
//! The onboarding keystone (ERGONOMICS_ROADMAP Theme-5 ⚑): zero-to-running in ONE
//! command instead of hand-assembling a `Cargo.toml` + `.csproj` + `RustDotnet.targets`
//! import. Product templates are modelled directly on shipped example crates or an external host's
//! documented project contract so the
//! scaffold is guaranteed to be the exact shape the native pipeline already builds:
//!
//!   * `--app`    — a Rust-on-.NET binary using `mycorrhiza::prelude` (models `cd_collections`).
//!                  `cargo dotnet run` builds + runs it.
//!   * `--lib`    — a Rust `cdylib` exporting via `export_rust_containers!()` PLUS a C#
//!                  consumer that references it through `RustDotnet.targets` (models
//!                  `cd_containers`). `dotnet run` in the `csharp/` dir builds both.
//!   * `--plugin` — the `#[dotnet_class]` variant of `--lib`: the Rust side defines a
//!                  managed class a C# host `new`s and calls (models `cd_typedef`).
//!   * `--excel`  — a Windows Excel-DNA add-in targeting `net10.0-windows`; ordinary attributed
//!                  C# worksheet functions call typed managed-Rust exports.
//!   * `--webapi` — an ASP.NET Core minimal API whose application logic is a schema-1 managed
//!                  Rust assembly. It builds and runs anywhere the supported CoreCLR profile does.
//!   * `--worker` — the same managed Rust contract hosted by a .NET worker service.
//!   * `--winui`  — an unpackaged WinUI 3 desktop app. The scaffold is Windows-only and remains
//!                  planned until its Windows runtime acceptance passes.
//!   * `--maui`   — a Windows-first MAUI app. Mobile TFMs are deliberately not generated until
//!                  their packaging and runtime gates exist.
//!
//! Templates are emitted from string constants (interpolating the crate name) — no
//! network, no example-crate copy at runtime. Every file the corresponding example
//! ships is reproduced, including the `.gitignore`s so a fresh scaffold is clean under
//! version control from the first commit.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::{NewArgs, Template};

/// Run `cargo dotnet new`.
pub fn run(args: &NewArgs) -> Result<i32> {
    let template = args.template()?;
    let name = resolve_name(args)?;
    let dir = target_dir(args, &name)?;

    if dir.exists()
        && dir
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        bail!(
            "target directory is not empty: {} (pass a fresh path, or remove it first)",
            dir.display()
        );
    }
    fs::create_dir_all(&dir)
        .with_context(|| format!("could not create target directory: {}", dir.display()))?;

    let files = render(template, &name, &args.dotnet);
    for f in &files {
        let path = dir.join(f.rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        fs::write(&path, &f.body).with_context(|| format!("could not write {}", path.display()))?;
    }

    print_next_steps(template, &name, &dir, &args.dotnet);
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
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
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
fn render(template: Template, name: &str, dotnet: &str) -> Vec<File> {
    match template {
        Template::App => app_files(name),
        Template::Lib => lib_files(name, dotnet),
        Template::Plugin => plugin_files(name, dotnet),
        Template::Excel => excel_files(name, dotnet),
        Template::Maui => maui_files(name, dotnet),
        Template::Winui => winui_files(name, dotnet),
        Template::WebApi => webapi_files(name, dotnet),
        Template::Worker => worker_files(name, dotnet),
        Template::Unity => unity_files(name),
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
                 mycorrhiza = \"0.0.0\"\n\
                 # NOTE: do NOT set the abort panic strategy — the native build-std has no\n\
                 # panic_abort crate; the default unwinding profile is what the backend expects.\n\
                 [workspace]\n",
            ),
        },
        File {
            rel: ".gitignore",
            body: GITIGNORE_RUST.to_string(),
        },
        File {
            rel: "src/main.rs",
            body: APP_MAIN.to_string(),
        },
    ]
}

// ---------------------------------------------------------------------------------
// --lib : a Rust cdylib exported to C# via export_rust_containers! (models cd_containers)
// ---------------------------------------------------------------------------------

fn lib_files(name: &str, dotnet: &str) -> Vec<File> {
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
                 mycorrhiza = \"0.0.0\"\n\
                 [workspace]\n\
                 \n\
                 [profile.release.build-override]\n\
                 codegen-units = 1\n",
            ),
        },
        File {
            rel: "rustlib/.gitignore",
            body: GITIGNORE_RUSTLIB.to_string(),
        },
        File {
            rel: "rustlib/src/lib.rs",
            body: LIB_RUST.to_string(),
        },
        File {
            rel: "csharp/Program.cs",
            body: LIB_CS_PROGRAM.replace("__CRATE__", name),
        },
        File {
            rel: &leak_str(format!("csharp/{cs_name}.csproj")),
            body: csproj_containers(&cs_name, dotnet),
        },
        File {
            rel: "csharp/.gitignore",
            body: GITIGNORE_CS.to_string(),
        },
    ]
}

// ---------------------------------------------------------------------------------
// --plugin : #[dotnet_class] managed type consumed by a C# host (models cd_typedef)
// ---------------------------------------------------------------------------------

fn plugin_files(name: &str, dotnet: &str) -> Vec<File> {
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
                 mycorrhiza = \"0.0.0\"\n\
                 dotnet_macros = \"0.1.0\"\n\
                 [workspace]\n\
                 \n\
                 [profile.release.build-override]\n\
                 codegen-units = 1\n",
            ),
        },
        File {
            rel: "rustlib/.gitignore",
            body: GITIGNORE_RUSTLIB.to_string(),
        },
        File {
            rel: "rustlib/src/lib.rs",
            body: PLUGIN_RUST.to_string(),
        },
        File {
            rel: "csharp/Program.cs",
            body: PLUGIN_CS_PROGRAM.to_string(),
        },
        File {
            rel: &leak_str(format!("csharp/{cs_name}.csproj")),
            body: csproj_plain(&cs_name, dotnet),
        },
        File {
            rel: "csharp/.gitignore",
            body: GITIGNORE_CS.to_string(),
        },
    ]
}

// ---------------------------------------------------------------------------------
// --excel : Excel-DNA worksheet functions backed by managed Rust
// ---------------------------------------------------------------------------------

fn excel_files(name: &str, dotnet: &str) -> Vec<File> {
    let assembly = format!("{name}_excel");
    let rust_crate = name.replace('-', "_");
    vec![
        File {
            rel: "rustlib/Cargo.toml",
            body: format!(
                "[package]\n\
                 name = \"{name}\"\n\
                 version = \"0.1.0\"\n\
                 edition = \"2024\"\n\
                 \n\
                 [lib]\n\
                 crate-type = [\"cdylib\"]\n\
                 \n\
                 [dependencies]\n\
                 mycorrhiza = \"0.0.0\"\n\
                 dotnet_macros = \"0.1.0\"\n\
                 [workspace]\n\
                 \n\
                 [profile.release.build-override]\n\
                 codegen-units = 1\n",
            ),
        },
        File {
            rel: "rustlib/.gitignore",
            body: GITIGNORE_RUSTLIB.to_string(),
        },
        File {
            rel: "rustlib/src/lib.rs",
            body: EXCEL_RUST.to_string(),
        },
        File {
            rel: "excel/Functions.cs",
            body: EXCEL_CS_FUNCTIONS.to_string(),
        },
        File {
            rel: &leak_str(format!("excel/{assembly}.csproj")),
            body: excel_csproj(&assembly, dotnet, &rust_crate),
        },
        File {
            rel: "excel/.gitignore",
            body: GITIGNORE_CS.to_string(),
        },
        File {
            rel: "README.md",
            body: EXCEL_README.replace("__NAME__", name),
        },
    ]
}

// ---------------------------------------------------------------------------------
// Product hosts: one schema-1 managed Rust backend, several ordinary .NET hosts.
// ---------------------------------------------------------------------------------

fn managed_stem(name: &str) -> String {
    let mut result = String::new();
    for part in name.split(['-', '_']).filter(|part| !part.is_empty()) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());
            result.extend(chars);
        }
    }
    if result.is_empty() {
        "RustApp".to_owned()
    } else {
        result
    }
}

fn product_rust_files(name: &str, compatibility_profile: &str) -> Vec<File> {
    let managed = managed_stem(name);
    let assembly = format!("{managed}.Rust");
    vec![
        File {
            rel: "rustlib/Cargo.toml",
            body: format!(
                "[package]\n\
                 name = \"{name}\"\n\
                 version = \"0.1.0\"\n\
                 edition = \"2024\"\n\
                 \n\
                 [lib]\n\
                 crate-type = [\"cdylib\"]\n\
                 \n\
                 [package.metadata.dotnet]\n\
                 identity-schema = 1\n\
                 package-id = \"{assembly}\"\n\
                 assembly-name = \"{assembly}\"\n\
                 root-namespace = \"{managed}\"\n\
                 module-type = \"Backend\"\n\
                 public-namespaces = [\"{managed}\"]\n\
                 compatibility-profile = \"{compatibility_profile}\"\n\
                 legacy-main-module = false\n\
                 \n\
                 [dependencies]\n\
                 mycorrhiza = \"0.0.0\"\n\
                 dotnet_macros = \"0.1.0\"\n\
                 [workspace]\n\
                 \n\
                 [profile.release.build-override]\n\
                 codegen-units = 1\n",
            ),
        },
        File {
            rel: "rustlib/src/lib.rs",
            body: PRODUCT_HOST_RUST.to_string(),
        },
        File {
            rel: "rustlib/.gitignore",
            body: GITIGNORE_RUSTLIB.to_string(),
        },
    ]
}

fn webapi_files(name: &str, dotnet: &str) -> Vec<File> {
    let managed = managed_stem(name);
    let mut files = product_rust_files(name, "net10-coreclr");
    files.extend([
        File {
            rel: "webapi/Program.cs",
            body: WEBAPI_PROGRAM.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: &leak_str(format!("webapi/{managed}.WebApi.csproj")),
            body: WEBAPI_CSPROJ_TEMPLATE
                .replace("__DOTNET__", dotnet)
                .replace("__ASSEMBLY__", &format!("{managed}.WebApi")),
        },
        File {
            rel: "webapi/.gitignore",
            body: GITIGNORE_CS.to_string(),
        },
        File {
            rel: "README.md",
            body: PRODUCT_README
                .replace("__HOST__", "ASP.NET Core Web API")
                .replace("__DIR__", "webapi")
                .replace("__COMMAND__", "dotnet run -c Release"),
        },
    ]);
    files
}

fn worker_files(name: &str, dotnet: &str) -> Vec<File> {
    let managed = managed_stem(name);
    let mut files = product_rust_files(name, "net10-coreclr");
    files.extend([
        File {
            rel: "worker/Program.cs",
            body: WORKER_PROGRAM.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: "worker/RustWorker.cs",
            body: WORKER_SERVICE.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: &leak_str(format!("worker/{managed}.Worker.csproj")),
            body: WORKER_CSPROJ_TEMPLATE
                .replace("__DOTNET__", dotnet)
                .replace("__ASSEMBLY__", &format!("{managed}.Worker")),
        },
        File {
            rel: "worker/.gitignore",
            body: GITIGNORE_CS.to_string(),
        },
        File {
            rel: "README.md",
            body: PRODUCT_README
                .replace("__HOST__", ".NET worker service")
                .replace("__DIR__", "worker")
                .replace("__COMMAND__", "dotnet run -c Release"),
        },
    ]);
    files
}

fn winui_files(name: &str, dotnet: &str) -> Vec<File> {
    let managed = managed_stem(name);
    let mut files = product_rust_files(name, "winui3-net10-windows");
    files.extend([
        File {
            rel: "winui/App.xaml",
            body: WINUI_APP_XAML.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: "winui/App.xaml.cs",
            body: WINUI_APP_CS.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: "winui/MainWindow.xaml",
            body: WINUI_MAIN_WINDOW_XAML.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: "winui/MainWindow.xaml.cs",
            body: WINUI_MAIN_WINDOW_CS.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: &leak_str(format!("winui/{managed}.WinUI.csproj")),
            body: WINUI_CSPROJ_TEMPLATE
                .replace("__DOTNET__", dotnet)
                .replace("__ASSEMBLY__", &format!("{managed}.WinUI")),
        },
        File {
            rel: "winui/.gitignore",
            body: GITIGNORE_CS.to_string(),
        },
        File {
            rel: "README.md",
            body: WINDOWS_HOST_README
                .replace("__HOST__", "WinUI 3")
                .replace("__DIR__", "winui"),
        },
    ]);
    files
}

fn maui_files(name: &str, dotnet: &str) -> Vec<File> {
    let managed = managed_stem(name);
    let mut files = product_rust_files(name, "maui-windows-net10");
    files.extend([
        File {
            rel: "maui/MauiProgram.cs",
            body: MAUI_PROGRAM.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: "maui/App.cs",
            body: MAUI_APP.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: "maui/Platforms/Windows/App.xaml",
            body: MAUI_WINDOWS_APP_XAML.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: "maui/Platforms/Windows/App.xaml.cs",
            body: MAUI_WINDOWS_APP_CS.replace("__NAMESPACE__", &managed),
        },
        File {
            rel: &leak_str(format!("maui/{managed}.Maui.csproj")),
            body: MAUI_CSPROJ_TEMPLATE
                .replace("__DOTNET__", dotnet)
                .replace("__ASSEMBLY__", &format!("{managed}.Maui"))
                .replace("__NAMESPACE__", &managed),
        },
        File {
            rel: "maui/.gitignore",
            body: GITIGNORE_CS.to_string(),
        },
        File {
            rel: "README.md",
            body: WINDOWS_HOST_README
                .replace("__HOST__", ".NET MAUI for Windows")
                .replace("__DIR__", "maui"),
        },
    ]);
    files
}

// ---------------------------------------------------------------------------------
// SDK crates are ordinary version dependencies in portable manifests. cargo-dotnet's
// private build config redirects them to the copies installed under CARGO_DOTNET_HOME.
// ---------------------------------------------------------------------------------
//
// Portable manifests use ordinary version dependencies. Setup copies the matching SDK crates into
// CARGO_DOTNET_HOME, and the build-local Cargo config patches those names to the installed sources;
// users never edit a path dependency and the checkout can be removed after setup.

// ---------------------------------------------------------------------------------
// csproj rendering.
// ---------------------------------------------------------------------------------

/// The C#-consumer csproj for `--lib`: opts into the shipped `RustVec<T>`/`RustBoxVec<T>`
/// wrappers and auto-builds the Rust crate via RustDotnet.targets.
fn csproj_containers(cs_name: &str, dotnet: &str) -> String {
    CSPROJ_TEMPLATE
        .replace("__ASSEMBLY__", cs_name)
        .replace("__DOTNET__", dotnet)
        .replace(
            "__CONTAINERS_PROP__",
            "\n    <UseRustDotnetContainers>true</UseRustDotnetContainers>",
        )
}

/// The C#-host csproj for `--plugin`: no containers, just auto-builds the Rust crate.
fn csproj_plain(cs_name: &str, dotnet: &str) -> String {
    CSPROJ_TEMPLATE
        .replace("__ASSEMBLY__", cs_name)
        .replace("__DOTNET__", dotnet)
        .replace("__CONTAINERS_PROP__", "")
}

fn excel_csproj(assembly: &str, dotnet: &str, rust_crate: &str) -> String {
    EXCEL_CSPROJ_TEMPLATE
        .replace("__ASSEMBLY__", assembly)
        .replace("__DOTNET__", dotnet)
        .replace("__RUST_CRATE__", rust_crate)
}

// ---------------------------------------------------------------------------------
// Post-scaffold guidance.
// ---------------------------------------------------------------------------------

fn print_next_steps(template: Template, name: &str, dir: &Path, dotnet: &str) {
    let d = dir.display();
    println!(
        "== scaffolded {} project '{name}' at {d} ==",
        template.label()
    );
    println!();
    match template {
        Template::App => {
            println!("Next:");
            println!("  cd {d}");
            println!("  cargo dotnet run --dotnet {dotnet}");
        }
        Template::Lib | Template::Plugin => {
            println!("Next:");
            println!("  cd {d}/csharp");
            println!(
                "  # ensure CARGO_DOTNET_HOME points at your install (or ~/.cargo-dotnet exists)"
            );
            println!("  dotnet run -c Release  # targets net{dotnet}.0");
        }
        Template::Excel => {
            println!("Next (Windows with desktop Excel installed):");
            println!("  cd {d}/excel");
            println!("  dotnet build -c Release  # targets net{dotnet}.0-windows");
            println!(
                "  # Open the generated *-packed.xll from bin/Release/net{dotnet}.0-windows/publish"
            );
        }
        Template::WebApi => {
            println!("Next:");
            println!("  cd {d}/webapi");
            println!("  dotnet run -c Release  # http://localhost:5000/health");
        }
        Template::Worker => {
            println!("Next:");
            println!("  cd {d}/worker");
            println!("  dotnet run -c Release");
        }
        Template::Winui => {
            println!("Next (Windows with Visual Studio and the Windows App SDK workload):");
            println!("  cd {d}/winui");
            println!("  dotnet run -c Release");
            println!("  # Profile winui3-net10-windows remains planned until runtime CI passes.");
        }
        Template::Maui => {
            println!("Next (Windows with the .NET MAUI workload installed):");
            println!("  cd {d}/maui");
            println!("  dotnet workload install maui-windows  # once per machine");
            println!("  dotnet run -c Release -f net{dotnet}.0-windows10.0.19041.0");
            println!("  # Android/iOS/Mac Catalyst are not generated or claimed yet.");
        }
        Template::Unity => {
            println!("Next (Unity 6.3 project with managed Rust):");
            println!("  cd {d}");
            println!("  cargo dotnet unity doctor");
            println!("  cargo dotnet unity build . rustlib");
            println!(
                "  cargo dotnet unity native . native --export rust_native_multiply  # optional macOS native kernel"
            );
            println!(
                "  # Open this directory in Unity Hub and press Play; the demo scene is generated automatically."
            );
        }
    }
}

fn unity_files(name: &str) -> Vec<File> {
    let managed = managed_stem(name);
    let assembly = format!("{managed}.Rust");
    let native = format!("{}_native", name.replace('-', "_"));
    vec![
        File { rel: "rustlib/Cargo.toml", body: format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nmycorrhiza = \"0.0.0\"\ndotnet_macros = \"0.1.0\"\n\n[package.metadata.dotnet]\nidentity-schema = 1\npackage-id = \"{assembly}\"\nassembly-name = \"{assembly}\"\nroot-namespace = \"{managed}\"\nmodule-type = \"Exports\"\npublic-namespaces = [\"{managed}\"]\ncompatibility-profile = \"unity-netstandard2.1\"\nlegacy-main-module = false\n[workspace]\n") },
        File { rel: "rustlib/src/lib.rs", body: UNITY_RUST.to_string() },
        File {
            rel: "native/Cargo.toml",
            body: format!(
                "[package]\nname = \"{native}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[workspace]\n"
            ),
        },
        File {
            rel: "native/src/lib.rs",
            body: UNITY_NATIVE_RUST.to_string(),
        },
        File {
            rel: "Assets/Scripts/CargoDotnetUnity.cs",
            body: UNITY_RUNTIME
                .replace("__TYPE__", &format!("{managed}.Exports"))
                .replace("__NATIVE__", &native),
        },
        File { rel: "Assets/Scripts/CargoDotnetUnity.asmdef", body: format!(r#"{{ "name": "{name}.Runtime", "rootNamespace": "RustcCodegenClr.Unity" }}
"#) },
        File {
            rel: "Assets/Editor/CargoDotnetUnityBuild.cs",
            body: UNITY_EDITOR_BOOTSTRAP.to_string(),
        },
        File {
            rel: "Assets/Editor/CargoDotnetUnity.Editor.asmdef",
            body: format!(
                "{{\n  \"name\": \"{name}.Editor\",\n  \"references\": [\"{name}.Runtime\"],\n  \"includePlatforms\": [\"Editor\"],\n  \"autoReferenced\": true\n}}\n"
            ),
        },
        File {
            rel: "Assets/README.md",
            body: "# Unity assets\n\nRun `cargo dotnet unity build . rustlib` before opening or refreshing the project. The command atomically stages the managed Rust DLL, PDB, XML docs, helper closure, and `Assets/RustDotnetGenerated/link.xml`. On macOS, `cargo dotnet unity native . native --export rust_native_multiply` additionally verifies and stages the optional native Rust kernel. On first import the generated Editor bootstrap creates `Assets/Scenes/CargoDotnetUnity.unity`, attaches the typed adapter, and adds the scene to Build Settings. Open that scene and press Play; no hand wiring is required.\n".to_string(),
        },
        File {
            rel: ".gitignore",
            body: "Library/\nLogs/\nTemp/\nUserSettings/\nobj/\nrustlib/target/\nnative/target/\n**/Cargo.lock\nAssets/Plugins/Managed/\nAssets/Plugins/macOS/\nAssets/RustDotnetGenerated/\n".to_string(),
        },
        File { rel: "ProjectSettings/ProjectVersion.txt", body: "m_EditorVersion: 6000.3.19f1\nm_EditorVersionWithRevision: 6000.3.19f1 (unity)\n".to_string() },
        File { rel: "Packages/manifest.json", body: "{\n  \"dependencies\": {\n    \"com.unity.modules.physics\": \"1.0.0\"\n  }\n}\n".to_string() },
    ]
}

const UNITY_RUST: &str = r#"#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]
use dotnet_macros::dotnet_export;
#[dotnet_export(name = "RustEngineStatus")]
pub fn rust_engine_status() -> i32 { 1 }
"#;

const UNITY_NATIVE_RUST: &str = r#"#[unsafe(no_mangle)]
pub extern "C" fn rust_native_multiply(left: i32, right: i32) -> i32 {
    left * right
}
"#;

const UNITY_RUNTIME: &str = r#"using System.Runtime.InteropServices;
using UnityEngine;

namespace RustcCodegenClr.Unity
{
    /// Thin Unity edge: scene lifecycle stays in C#, durable game/domain logic stays in managed Rust.
    public sealed class CargoDotnetUnity : MonoBehaviour
    {
        [DllImport("__NATIVE__", EntryPoint = "rust_native_multiply")]
        private static extern int RustNativeMultiply(int left, int right);

        [SerializeField] private bool probeOnStart = true;

        public int ProbeManagedRust()
        {
            return __TYPE__.RustEngineStatus();
        }

        /// Optional high-performance native seam. Stage it first with
        /// `cargo dotnet unity native . native --export rust_native_multiply` on macOS.
        public int ProbeNativeRust(int left, int right) => RustNativeMultiply(left, right);

        private void Start()
        {
            if (probeOnStart)
            {
                var status = ProbeManagedRust();
                Debug.Log($"RUST_UNITY_READY={status}");
                if (Application.isBatchMode)
                    Application.Quit(status == 1 ? 0 : 1);
            }
        }
    }
}
"#;

const UNITY_EDITOR_BOOTSTRAP: &str = r#"#if UNITY_EDITOR
using System;
using System.Linq;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace RustcCodegenClr.Unity.Editor
{
    /// Generated, readable project automation for the scaffold's first scene and player builds.
    [InitializeOnLoad]
    public static class CargoDotnetUnityBuild
    {
        public const string ScenePath = "Assets/Scenes/CargoDotnetUnity.unity";

        static CargoDotnetUnityBuild() => EditorApplication.delayCall += EnsureDemoScene;

        [MenuItem("Rust/.NET/Prepare Demo Scene")]
        public static void EnsureDemoScene()
        {
            if (AssetDatabase.LoadAssetAtPath<SceneAsset>(ScenePath) == null)
            {
                if (!AssetDatabase.IsValidFolder("Assets/Scenes"))
                    AssetDatabase.CreateFolder("Assets", "Scenes");
                var scene = EditorSceneManager.NewScene(NewSceneSetup.DefaultGameObjects, NewSceneMode.Single);
                var host = new GameObject("Managed Rust Host");
                host.AddComponent<CargoDotnetUnity>();
                if (!EditorSceneManager.SaveScene(scene, ScenePath))
                    throw new InvalidOperationException($"Could not save generated scene {ScenePath}");
            }

            if (!EditorBuildSettings.scenes.Any(scene => scene.path == ScenePath && scene.enabled))
            {
                var scenes = EditorBuildSettings.scenes
                    .Where(scene => scene.path != ScenePath)
                    .Concat(new[] { new EditorBuildSettingsScene(ScenePath, true) })
                    .ToArray();
                EditorBuildSettings.scenes = scenes;
            }
        }

        public static void Mono() => Build("Builds/Mono.app", ScriptingImplementation.Mono2x);
        public static void IL2CPP() => Build("Builds/IL2CPP.app", ScriptingImplementation.IL2CPP);

        private static void Build(string output, ScriptingImplementation backend)
        {
            EnsureDemoScene();
            PlayerSettings.SetScriptingBackend(NamedBuildTarget.Standalone, backend);
            // The optional scaffolded native plug-in is built for the Apple-Silicon host.
            PlayerSettings.SetArchitecture(NamedBuildTarget.Standalone, 1);
            var report = BuildPipeline.BuildPlayer(
                new[] { ScenePath },
                output,
                BuildTarget.StandaloneOSX,
                BuildOptions.None);
            if (report.summary.result != BuildResult.Succeeded)
                throw new InvalidOperationException(report.summary.ToString());
        }
    }
}
#endif
"#;

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

/// `--lib` C# consumer: uses the shipped `RustVec<T>` wrapper against the Rust crate.
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
//! call `read_value()` / `read_step()` — no `#[unsafe(no_mangle)]`, no marshalling boilerplate.
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

/// `--excel` Rust library: application logic remains normal Rust and exports a typed managed seam.
/// Simple CLR-native UDFs can carry Excel-DNA metadata directly; the C# edge remains responsible
/// for Excel-specific range/error objects and asynchronous scheduling policy.
const EXCEL_RUST: &str = r#"#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::dotnet_export;
use mycorrhiza::cancellation::CancellationToken;

fn validate_portfolio_inputs(
    principal: f64,
    annual_rate_percent: f64,
    years: i32,
) -> Result<(), String> {
    if !principal.is_finite() || principal < 0.0 {
        return Err("principal must be a finite non-negative number".to_owned());
    }
    if !annual_rate_percent.is_finite() || annual_rate_percent <= -100.0 {
        return Err("annual rate must be finite and greater than -100%".to_owned());
    }
    if !(0..=200).contains(&years) {
        return Err("years must be between 0 and 200".to_owned());
    }
    Ok(())
}

/// Compound a principal at a fixed annual rate. Excel sees a normal static method returning double.
#[dotnet_export(name = "PortfolioFutureValue", error = "exception")]
pub fn portfolio_future_value(
    principal: f64,
    annual_rate_percent: f64,
    years: i32,
) -> Result<f64, String> {
    validate_portfolio_inputs(principal, annual_rate_percent, years)?;
    Ok(principal * (1.0 + annual_rate_percent / 100.0).powi(years))
}

/// Run a deterministic portfolio stress sweep. Excel calls this on a pool thread and supplies its
/// formula-lifetime CancellationToken, which is polled without involving the Excel object model.
#[dotnet_export(name = "PortfolioStressScore", error = "exception")]
pub fn portfolio_stress_score(
    cancellation: CancellationToken,
    principal: f64,
    annual_rate_percent: f64,
    years: i32,
    scenarios: i32,
) -> Result<f64, String> {
    validate_portfolio_inputs(principal, annual_rate_percent, years)?;
    if !(1..=10_000_000).contains(&scenarios) {
        return Err("scenarios must be between 1 and 10000000".to_owned());
    }

    let base_rate = annual_rate_percent / 100.0;
    let mut total = 0.0;
    for scenario in 0..scenarios {
        if scenario & 1023 == 0 {
            cancellation.throw_if_cancellation_requested();
        }
        // Deterministic +/-100 bp rate sweep: useful enough to replace with a real model while
        // keeping the generated project reproducible and allocation-free.
        let shock = ((scenario % 201) - 100) as f64 / 10_000.0;
        total += principal * (1.0 + base_rate + shock).powi(years);
    }
    cancellation.throw_if_cancellation_requested();
    Ok(total / scenarios as f64)
}

/// A string export demonstrates ordinary managed-string marshalling in an Excel cell.
#[dotnet_export(name = "RustEngineStatus")]
pub fn rust_engine_status() -> String {
    "managed Rust engine ready".to_owned()
}

/// A metadata-bearing Excel-DNA UDF proving safe external method and parameter attributes on a
/// backend-emitted managed Rust method. Rich Excel object/range/error policies stay in Functions.cs.
#[dotnet_export(
    name = "RustEngineInfo",
    attr(
        "[ExcelDna.Integration]ExcelDna.Integration.ExcelFunctionAttribute",
        fields(
            Name = "RUST.ENGINE_INFO",
            Description = "Queries the managed Rust engine without a C# forwarding method.",
            Category = "Rust on .NET",
            IsThreadSafe = true
        )
    ),
    param_attr(
        topic,
        "[ExcelDna.Integration]ExcelDna.Integration.ExcelArgumentAttribute",
        fields(Name = "topic", Description = "status, runtime, or profile")
    )
)]
pub fn rust_engine_info(topic: String) -> String {
    match topic.trim().to_ascii_lowercase().as_str() {
        "status" => "managed Rust engine ready".to_owned(),
        "runtime" => ".NET 10 CoreCLR".to_owned(),
        "profile" => "excel-dna-net10-windows preview".to_owned(),
        _ => "unknown topic; use status, runtime, or profile".to_owned(),
    }
}
"#;

/// `--excel` C# host: Excel-DNA owns Excel registration while worksheet calculations call the
/// managed Rust assembly directly. There is no `DllImport` and no pointer-shaped public API.
const EXCEL_CS_FUNCTIONS: &str = r##"using ExcelDna.Integration;
using System.Threading;
using System.Threading.Tasks;

public static class RustFunctions
{
    [ExcelFunction(
        Name = "RUST.PORTFOLIO_FV",
        Description = "Future value calculated by managed Rust.",
        Category = "Rust on .NET",
        IsThreadSafe = true)]
    public static object PortfolioFutureValue(
        [ExcelArgument(Name = "principal", Description = "Starting amount, zero or greater.")]
        double principal,
        [ExcelArgument(Name = "annualRatePercent", Description = "Annual percentage rate.")]
        double annualRatePercent,
        [ExcelArgument(Name = "years", Description = "Whole years from 0 through 200.")]
        int years)
    {
        try
        {
            return MainModule.PortfolioFutureValue(principal, annualRatePercent, years);
        }
        catch (Exception error)
        {
            // A Rust `Result::Err` is a managed exception. Excel UDFs should return an Excel value
            // rather than let an exception escape through the calculation engine.
            return $"#RUST! {error.Message}";
        }
    }

    [ExcelFunction(
        Name = "RUST.STATUS",
        Description = "Reports whether the managed Rust assembly loaded.",
        Category = "Rust on .NET",
        IsThreadSafe = true)]
    public static string Status() => MainModule.RustEngineStatus();

    [ExcelFunction(
        Name = "RUST.PORTFOLIO_FV_TABLE",
        Description = "Calculates future value for rows of principal, annual rate, and years.",
        Category = "Rust on .NET",
        IsThreadSafe = true)]
    public static object[,] PortfolioFutureValueTable(
        [ExcelArgument(Name = "rows", Description = "Three columns: principal, annual rate %, years.")]
        object[,] rows)
    {
        int rowCount = rows.GetLength(0);
        if (rows.GetLength(1) != 3)
            return new object[,] { { "#RUST! expected exactly three columns" } };

        var results = new object[rowCount, 1];
        for (int row = 0; row < rowCount; row++)
        {
            try
            {
                // Excel-DNA owns Excel's object/error/empty-cell model. The public Rust seam stays
                // a normal typed .NET API, so the calculation engine is reusable outside Excel.
                double principal = Convert.ToDouble(rows[row, 0]);
                double rate = Convert.ToDouble(rows[row, 1]);
                int years = Convert.ToInt32(rows[row, 2]);
                results[row, 0] = MainModule.PortfolioFutureValue(principal, rate, years);
            }
            catch (Exception error)
            {
                results[row, 0] = $"#RUST! row {row + 1}: {error.Message}";
            }
        }
        return results;
    }

    [ExcelFunction(
        Name = "RUST.PORTFOLIO_STRESS_ASYNC",
        Description = "Runs a cancellable portfolio stress sweep in managed Rust without blocking Excel.",
        Category = "Rust on .NET")]
    public static async Task<object> PortfolioStressAsync(
        [ExcelArgument(Name = "principal", Description = "Starting amount, zero or greater.")]
        double principal,
        [ExcelArgument(Name = "annualRatePercent", Description = "Annual percentage rate.")]
        double annualRatePercent,
        [ExcelArgument(Name = "years", Description = "Whole years from 0 through 200.")]
        int years,
        [ExcelArgument(Name = "scenarios", Description = "Stress scenarios from 1 through 10000000.")]
        int scenarios,
        CancellationToken cancellationToken)
    {
        try
        {
            // Excel-DNA 1.9 treats a final CancellationToken as the lifetime of this formula and
            // cancels it when the formula is deleted. Only scalar copies cross the pool-thread
            // boundary: no Range, ExcelReference, C API call, or COM object is captured here.
            return await Task.Run(
                () => MainModule.PortfolioStressScore(
                    cancellationToken,
                    principal,
                    annualRatePercent,
                    years,
                    scenarios),
                cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            // Preserve cancellation so Excel-DNA retires the async topic instead of caching an
            // error-looking cell value.
            throw;
        }
        catch (Exception error)
        {
            return $"#RUST! {error.Message}";
        }
    }
}
"##;

const EXCEL_README: &str = r#"# __NAME__: managed Rust in Excel

This scaffold uses Excel-DNA 1.9.0 to package a `net10.0-windows` Excel add-in. Excel-DNA owns the
Excel-specific registration and deployment surface; the worksheet functions call ordinary typed
methods emitted from `rustlib` by `#[dotnet_export]`.

On Windows with 64-bit desktop Excel and the .NET 10 Desktop Runtime installed:

```powershell
cd excel
dotnet build -c Release
```

Open the generated 64-bit packed `.xll` under `excel/bin/Release/net10.0-windows/publish`, then use:

```text
=RUST.STATUS()
=RUST.PORTFOLIO_FV(1000, 7, 10)
=RUST.PORTFOLIO_FV_TABLE(A2:C20)
=RUST.PORTFOLIO_STRESS_ASYNC(1000, 7, 10, 250000)
```

The table function accepts an ordinary Excel range with columns `principal`, `annual rate %`, and
`years`, and spills one result per row. The C# edge owns Excel's special `object[,]` cell/error model;
each validated typed row crosses into reusable managed Rust business logic.

The asynchronous stress function returns Excel-DNA's preferred `Task<T>` shape. Excel-DNA supplies
a hidden final `CancellationToken` and cancels it if the formula is deleted. The C# edge copies only
scalar inputs into `Task.Run`; managed Rust polls the token during the CPU loop. Never capture a
`Range`, `ExcelReference`, `ExcelDnaUtil.Application`, or any other Excel COM/C-API object in that
worker. Code that intentionally changes Excel must instead schedule a command with
`ExcelAsyncUtil.QueueAsMacro`, which runs when Excel is ready on its main thread.

The generated C# host deliberately contains no P/Invoke. If a calculation needs a native Rust
kernel, declare that private C ABI in `rustlib`, add the binary with `cargo dotnet add-native` or
`add-native-file`, and keep Excel-facing signatures managed and typed.

The Rust assembly also emits `RustEngineInfo` directly with structured `ExcelFunctionAttribute`
and `ExcelArgumentAttribute` metadata. Repository acceptance reflects those fields from the packed
dependency. Its automatic worksheet discovery remains part of the real Windows Excel launch gate,
so the scaffold does not yet advertise a `RUST.ENGINE_INFO` formula. The C# functions intentionally
own Excel's range conversion, cell-error values, and async scheduling rules.

This template targets Windows desktop Excel. It is not a VSTO project and does not claim Office for
macOS support; cross-platform Office extensions use the Office web-add-in model with a managed-Rust
service or companion.
"#;

const EXCEL_CSPROJ_TEMPLATE: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net__DOTNET__.0-windows</TargetFramework>
    <RustDotnetVersion Condition="'$(RustDotnetVersion)'==''">__DOTNET__</RustDotnetVersion>
    <RustDotnetCompatibilityProfile>excel-dna-net10-windows</RustDotnetCompatibilityProfile>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <AssemblyName>__ASSEMBLY__</AssemblyName>
    <ExcelDnaCreate32BitAddIn>false</ExcelDnaCreate32BitAddIn>
    <ExcelDnaCreate64BitAddIn>true</ExcelDnaCreate64BitAddIn>
    <RunExcelDnaPack>true</RunExcelDnaPack>
    <ExcelAddInExplicitExports>true</ExcelAddInExplicitExports>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="ExcelDna.AddIn" Version="1.9.0" />
    <RustCrate Include="../rustlib" CrateName="__RUST_CRATE__" />
  </ItemGroup>

  <Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
  <Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'=='' and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />
</Project>
"#;

/// The C# csproj, shared by `--lib` and `--plugin`. `__CONTAINERS_PROP__` toggles the
/// shipped-container opt-in; `__ASSEMBLY__` is the C# assembly name. The 3-way
/// RustDotnet.targets import mirrors the example crates: prefer `$CARGO_DOTNET_HOME`,
/// then `$HOME/.cargo-dotnet`, then the in-repo `msbuild/` as a dev fallback.
const CSPROJ_TEMPLATE: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <RustDotnetVersion Condition="'$(RustDotnetVersion)'==''">__DOTNET__</RustDotnetVersion>
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

const PRODUCT_HOST_RUST: &str = r#"#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::dotnet_export;

/// A small typed operation shared by every generated host. Replace its body with domain logic;
/// the managed signature remains an ordinary `static int Double(int)` method.
#[dotnet_export(name = "Double")]
pub fn double(value: i32) -> i32 {
    value.saturating_mul(2)
}

/// Managed strings cross directly; the host does not need P/Invoke or a generated C# shim.
#[dotnet_export(name = "Describe")]
pub fn describe(value: i32) -> String {
    format!("managed Rust processed {value} into {}", double(value))
}
"#;

const WEBAPI_PROGRAM: &str = r#"using __NAMESPACE__;

var builder = WebApplication.CreateBuilder(args);
var app = builder.Build();

app.MapGet("/health", () => new
{
    status = "ok",
    engine = Backend.Describe(21),
    answer = Backend.Double(21),
});

app.Run();
"#;

const WORKER_PROGRAM: &str = r#"using __NAMESPACE__.WorkerHost;

var builder = Host.CreateApplicationBuilder(args);
builder.Services.AddHostedService<RustWorker>();
await builder.Build().RunAsync();
"#;

const WORKER_SERVICE: &str = r#"using __NAMESPACE__;

namespace __NAMESPACE__.WorkerHost;

public sealed class RustWorker(
    ILogger<RustWorker> logger,
    IHostApplicationLifetime lifetime) : BackgroundService
{
    protected override Task ExecuteAsync(CancellationToken stoppingToken)
    {
        logger.LogInformation("{Message}; answer={Answer}",
            Backend.Describe(21), Backend.Double(21));
        lifetime.StopApplication();
        return Task.CompletedTask;
    }
}
"#;

const WEBAPI_CSPROJ_TEMPLATE: &str = r#"<Project Sdk="Microsoft.NET.Sdk.Web">
  <PropertyGroup>
    <TargetFramework>net__DOTNET__.0</TargetFramework>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <AssemblyName>__ASSEMBLY__</AssemblyName>
    <RustDotnetVersion Condition="'$(RustDotnetVersion)'==''">__DOTNET__</RustDotnetVersion>
    <RustDotnetCompatibilityProfile>net10-coreclr</RustDotnetCompatibilityProfile>
  </PropertyGroup>

  <ItemGroup>
    <RustCrate Include="../rustlib" />
  </ItemGroup>

  <Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
  <Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'=='' and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />
</Project>
"#;

const WORKER_CSPROJ_TEMPLATE: &str = r#"<Project Sdk="Microsoft.NET.Sdk.Worker">
  <PropertyGroup>
    <TargetFramework>net__DOTNET__.0</TargetFramework>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <AssemblyName>__ASSEMBLY__</AssemblyName>
    <RustDotnetVersion Condition="'$(RustDotnetVersion)'==''">__DOTNET__</RustDotnetVersion>
    <RustDotnetCompatibilityProfile>net10-coreclr</RustDotnetCompatibilityProfile>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.Extensions.Hosting" Version="10.0.0" />
    <RustCrate Include="../rustlib" />
  </ItemGroup>

  <Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
  <Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'=='' and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />
</Project>
"#;

const WINUI_APP_XAML: &str = r#"<Application
    x:Class="__NAMESPACE__.WinUI.App"
    xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
</Application>
"#;

const WINUI_APP_CS: &str = r#"using Microsoft.UI.Xaml;

namespace __NAMESPACE__.WinUI;

public partial class App : Application
{
    private Window? window;

    public App() => InitializeComponent();

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        window = new MainWindow();
        window.Activate();
    }
}
"#;

const WINUI_MAIN_WINDOW_XAML: &str = r#"<Window
    x:Class="__NAMESPACE__.WinUI.MainWindow"
    xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
  <Grid Padding="32">
    <StackPanel Spacing="12">
      <TextBlock FontSize="28" Text="Rust on .NET + WinUI 3" />
      <TextBlock x:Name="RustResult" TextWrapping="Wrap" />
    </StackPanel>
  </Grid>
</Window>
"#;

const WINUI_MAIN_WINDOW_CS: &str = r#"using Microsoft.UI.Xaml;
using __NAMESPACE__;

namespace __NAMESPACE__.WinUI;

public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
        RustResult.Text = Backend.Describe(21);
    }
}
"#;

const WINUI_CSPROJ_TEMPLATE: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>WinExe</OutputType>
    <TargetFramework>net__DOTNET__.0-windows10.0.19041.0</TargetFramework>
    <TargetPlatformMinVersion>10.0.17763.0</TargetPlatformMinVersion>
    <UseWinUI>true</UseWinUI>
    <WindowsPackageType>None</WindowsPackageType>
    <EnableWindowsTargeting>true</EnableWindowsTargeting>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <AssemblyName>__ASSEMBLY__</AssemblyName>
    <RustDotnetVersion Condition="'$(RustDotnetVersion)'==''">__DOTNET__</RustDotnetVersion>
    <RustDotnetCompatibilityProfile>winui3-net10-windows</RustDotnetCompatibilityProfile>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.WindowsAppSDK" Version="2.2.0" />
    <RustCrate Include="../rustlib" />
  </ItemGroup>

  <Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
  <Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'=='' and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />
</Project>
"#;

const MAUI_PROGRAM: &str = r#"using Microsoft.Maui.Hosting;

namespace __NAMESPACE__;

public static class MauiProgram
{
    public static MauiApp CreateMauiApp() =>
        MauiApp.CreateBuilder().UseMauiApp<App>().Build();
}
"#;

const MAUI_APP: &str = r#"using Microsoft.Maui;
using Microsoft.Maui.Controls;

namespace __NAMESPACE__;

public sealed class App : Application
{
    protected override Window CreateWindow(IActivationState? activationState)
    {
        var result = Backend.Describe(21);
        return new Window(new ContentPage
        {
            Title = "Rust on .NET",
            Content = new VerticalStackLayout
            {
                Padding = 32,
                Spacing = 12,
                Children =
                {
                    new Label { Text = "Managed Rust + .NET MAUI", FontSize = 28 },
                    new Label { Text = result },
                },
            },
        });
    }
}
"#;

const MAUI_WINDOWS_APP_XAML: &str = r#"<maui:MauiWinUIApplication
    x:Class="__NAMESPACE__.WinUI.App"
    xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
    xmlns:maui="using:Microsoft.Maui">
</maui:MauiWinUIApplication>
"#;

const MAUI_WINDOWS_APP_CS: &str = r#"using Microsoft.Maui;
using Microsoft.Maui.Hosting;

namespace __NAMESPACE__.WinUI;

public partial class App : MauiWinUIApplication
{
    public App() => InitializeComponent();

    protected override MauiApp CreateMauiApp() => MauiProgram.CreateMauiApp();
}
"#;

const MAUI_CSPROJ_TEMPLATE: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <!-- Windows is the only generated TFM until Android/iOS/Mac Catalyst runtime gates pass. -->
    <TargetFramework>net__DOTNET__.0-windows10.0.19041.0</TargetFramework>
    <OutputType>WinExe</OutputType>
    <RootNamespace>__NAMESPACE__</RootNamespace>
    <UseMaui>true</UseMaui>
    <SingleProject>true</SingleProject>
    <WindowsPackageType>None</WindowsPackageType>
    <EnableWindowsTargeting>true</EnableWindowsTargeting>
    <SupportedOSPlatformVersion>10.0.17763.0</SupportedOSPlatformVersion>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <AssemblyName>__ASSEMBLY__</AssemblyName>
    <RustDotnetVersion Condition="'$(RustDotnetVersion)'==''">__DOTNET__</RustDotnetVersion>
    <RustDotnetCompatibilityProfile>maui-windows-net10</RustDotnetCompatibilityProfile>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.Maui.Controls" Version="$(MauiVersion)" />
    <RustCrate Include="../rustlib" />
  </ItemGroup>

  <Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
  <Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'=='' and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />
</Project>
"#;

const PRODUCT_README: &str = r#"# Managed Rust + __HOST__

`rustlib` is compiled into a normal schema-1 .NET assembly. The host references it through the
shipped `RustDotnet.targets`; no hand-written P/Invoke or separate Rust build command is required.

```bash
cd __DIR__
__COMMAND__
```

Edit `rustlib/src/lib.rs` for application logic. Its public CLR contract is the namespace and
`Backend` type declared in `[package.metadata.dotnet]`.
"#;

const WINDOWS_HOST_README: &str = r#"# Managed Rust + __HOST__

This is an honest Windows-first scaffold. Build it on Windows with the relevant Visual Studio/.NET
workload installed:

```powershell
cd __DIR__
dotnet run -c Release
```

The C# project imports `RustDotnet.targets`, so build/debug automatically rebuilds and references
`rustlib`. This host profile remains **planned**, not supported, until the repository's Windows
runtime acceptance executes the generated app. Mobile TFMs are not silently enabled.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_cargo_manifests_parse(files: &[File]) {
        for file in files.iter().filter(|file| file.rel.ends_with("Cargo.toml")) {
            toml::from_str::<toml::Value>(&file.body)
                .unwrap_or_else(|error| panic!("generated {} is invalid TOML: {error}", file.rel));
        }
    }

    #[test]
    fn every_template_emits_valid_cargo_manifests() {
        assert_cargo_manifests_parse(&app_files("demo"));
        assert_cargo_manifests_parse(&lib_files("demo", "10"));
        assert_cargo_manifests_parse(&plugin_files("demo", "10"));
        assert_cargo_manifests_parse(&excel_files("demo", "10"));
        assert_cargo_manifests_parse(&maui_files("demo", "10"));
        assert_cargo_manifests_parse(&winui_files("demo", "10"));
        assert_cargo_manifests_parse(&webapi_files("demo", "10"));
        assert_cargo_manifests_parse(&worker_files("demo", "10"));
        assert_cargo_manifests_parse(&unity_files("demo"));
    }

    #[test]
    fn unity_scaffold_is_a_project_with_buildable_rust_subcrate() {
        let files = unity_files("demo");
        let rels: Vec<&str> = files.iter().map(|f| f.rel).collect();
        assert!(rels.contains(&"ProjectSettings/ProjectVersion.txt"));
        assert!(rels.contains(&"Packages/manifest.json"));
        assert!(rels.contains(&"Assets/README.md"));
        assert!(rels.contains(&"Assets/Scripts/CargoDotnetUnity.cs"));
        assert!(rels.contains(&"Assets/Editor/CargoDotnetUnityBuild.cs"));
        assert!(rels.contains(&"Assets/Editor/CargoDotnetUnity.Editor.asmdef"));
        assert!(rels.contains(&"rustlib/Cargo.toml"));
        assert!(rels.contains(&"rustlib/src/lib.rs"));
        assert!(rels.contains(&"native/Cargo.toml"));
        assert!(rels.contains(&"native/src/lib.rs"));
        assert!(rels.contains(&"Assets/Scripts/CargoDotnetUnity.asmdef"));
        assert!(rels.contains(&".gitignore"));
        assert!(!rels.iter().any(|rel| rel.ends_with("package.json")));
        assert!(!rels.iter().any(|rel| rel.starts_with("Runtime/")));

        let manifest = files
            .iter()
            .find(|file| file.rel == "rustlib/Cargo.toml")
            .expect("Unity scaffold must include a Rust manifest");
        let parsed: toml::Value = toml::from_str(&manifest.body).unwrap();
        assert_eq!(parsed["package"]["name"].as_str(), Some("demo"));
        assert_eq!(parsed["lib"]["crate-type"][0].as_str(), Some("cdylib"));
        assert_eq!(
            parsed["package"]["metadata"]["dotnet"]["compatibility-profile"].as_str(),
            Some("unity-netstandard2.1")
        );
        assert_eq!(
            parsed["package"]["metadata"]["dotnet"]["assembly-name"].as_str(),
            Some("Demo.Rust")
        );
        let native_manifest = files
            .iter()
            .find(|file| file.rel == "native/Cargo.toml")
            .expect("Unity scaffold must include its optional native kernel");
        let native_parsed: toml::Value = toml::from_str(&native_manifest.body).unwrap();
        assert_eq!(
            native_parsed["package"]["name"].as_str(),
            Some("demo_native")
        );
        assert_eq!(
            native_parsed["lib"]["crate-type"][0].as_str(),
            Some("cdylib")
        );

        let project_version = files
            .iter()
            .find(|file| file.rel == "ProjectSettings/ProjectVersion.txt")
            .expect("Unity scaffold must pin a known editor version");
        assert!(
            project_version
                .body
                .contains("m_EditorVersion: 6000.3.19f1")
        );

        let runtime = files
            .iter()
            .find(|file| file.rel == "Assets/Scripts/CargoDotnetUnity.cs")
            .expect("Unity scaffold must include its managed bridge");
        assert!(runtime.body.contains("Demo.Exports"));
        assert!(runtime.body.contains("demo_native"));
        assert!(runtime.body.contains("rust_native_multiply"));
        assert!(runtime.body.contains("ProbeNativeRust"));
        assert!(runtime.body.contains("RUST_UNITY_READY="));
        assert!(runtime.body.contains("Application.Quit"));
        assert!(runtime.body.contains("Demo.Exports.RustEngineStatus()"));
        assert!(!runtime.body.contains("System.Reflection"));

        let editor = files
            .iter()
            .find(|file| file.rel == "Assets/Editor/CargoDotnetUnityBuild.cs")
            .expect("Unity scaffold must generate its demo scene and player automation");
        assert!(editor.body.contains("EnsureDemoScene"));
        assert!(
            editor
                .body
                .contains("host.AddComponent<CargoDotnetUnity>()")
        );
        assert!(editor.body.contains("ScriptingImplementation.IL2CPP"));

        let asmdef = files
            .iter()
            .find(|file| file.rel == "Assets/Scripts/CargoDotnetUnity.asmdef")
            .expect("Unity scaffold must define a runtime assembly");
        let asmdef_json: serde_json::Value =
            serde_json::from_str(&asmdef.body).expect("generated asmdef must be valid JSON");
        assert_eq!(asmdef_json["name"].as_str(), Some("demo.Runtime"));
    }

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
        let files = lib_files("mylib", "10");
        let rels: Vec<&str> = files.iter().map(|f| f.rel).collect();
        assert!(rels.contains(&"rustlib/Cargo.toml"));
        assert!(rels.contains(&"rustlib/src/lib.rs"));
        assert!(
            rels.iter()
                .any(|r| r.starts_with("csharp/") && r.ends_with(".csproj"))
        );
        assert!(rels.contains(&"csharp/Program.cs"));
        // csproj name derives from the crate name.
        let csproj = files.iter().find(|f| f.rel.ends_with(".csproj")).unwrap();
        assert_eq!(csproj.rel, "csharp/mylib_cs.csproj");
        assert!(
            csproj
                .body
                .contains("<AssemblyName>mylib_cs</AssemblyName>")
        );
        // containers opt-in present for --lib.
        assert!(csproj.body.contains("UseRustDotnetContainers"));
        assert!(csproj.body.contains(
            "<RustDotnetVersion Condition=\"'$(RustDotnetVersion)'==''\">10</RustDotnetVersion>"
        ));
        // Rust side exports the container core.
        let lib = &files
            .iter()
            .find(|f| f.rel == "rustlib/src/lib.rs")
            .unwrap()
            .body;
        assert!(lib.contains("export_rust_containers!()"));
        // Program.cs got the crate name interpolated into its banner.
        let prog = &files
            .iter()
            .find(|f| f.rel == "csharp/Program.cs")
            .unwrap()
            .body;
        assert!(prog.contains("mylib:"));
        assert!(!prog.contains("__CRATE__"));
    }

    #[test]
    fn plugin_scaffold_has_expected_files() {
        let files = plugin_files("myplug", "10");
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
        let lib = &files
            .iter()
            .find(|f| f.rel == "rustlib/src/lib.rs")
            .unwrap()
            .body;
        assert!(lib.contains("#[dotnet_class]"));
        // plugin csproj does NOT opt into containers.
        let csproj = files.iter().find(|f| f.rel.ends_with(".csproj")).unwrap();
        assert!(!csproj.body.contains("UseRustDotnetContainers"));
        assert!(csproj.body.contains(
            "<RustDotnetVersion Condition=\"'$(RustDotnetVersion)'==''\">10</RustDotnetVersion>"
        ));
    }

    #[test]
    fn excel_scaffold_is_a_managed_rust_backed_excel_dna_addin() {
        let files = excel_files("risk_engine", "10");
        let rels: Vec<&str> = files.iter().map(|file| file.rel).collect();
        assert!(rels.contains(&"rustlib/Cargo.toml"));
        assert!(rels.contains(&"rustlib/src/lib.rs"));
        assert!(rels.contains(&"excel/Functions.cs"));
        assert!(rels.contains(&"excel/risk_engine_excel.csproj"));
        assert!(rels.contains(&"README.md"));

        let csproj = &files
            .iter()
            .find(|file| file.rel.ends_with(".csproj"))
            .unwrap()
            .body;
        assert!(csproj.contains("<TargetFramework>net10.0-windows</TargetFramework>"));
        assert!(csproj.contains("ExcelDna.AddIn\" Version=\"1.9.0"));
        assert!(csproj.contains("<RustCrate Include=\"../rustlib\" CrateName=\"risk_engine\" />"));
        assert!(csproj.contains("<ExcelDnaCreate32BitAddIn>false"));
        assert!(csproj.contains("<ExcelDnaCreate64BitAddIn>true"));
        assert!(csproj.contains("<ExcelAddInExplicitExports>true"));

        let rust = &files
            .iter()
            .find(|file| file.rel == "rustlib/src/lib.rs")
            .unwrap()
            .body;
        assert!(rust.contains("#[dotnet_export(name = \"PortfolioFutureValue\""));
        assert!(rust.contains("#[dotnet_export(name = \"PortfolioStressScore\""));
        assert!(rust.contains("ExcelDna.Integration.ExcelFunctionAttribute"));
        assert!(rust.contains("ExcelDna.Integration.ExcelArgumentAttribute"));
        assert!(rust.contains("RUST.ENGINE_INFO"));
        assert!(rust.contains("CancellationToken"));
        let csharp = &files
            .iter()
            .find(|file| file.rel == "excel/Functions.cs")
            .unwrap()
            .body;
        assert!(csharp.contains("[ExcelFunction("));
        assert!(csharp.contains("MainModule.PortfolioFutureValue"));
        assert!(csharp.contains("PortfolioFutureValueTable"));
        assert!(csharp.contains("object[,] rows"));
        assert!(csharp.contains("new object[rowCount, 1]"));
        assert!(csharp.contains("Task<object> PortfolioStressAsync"));
        assert!(csharp.contains("CancellationToken cancellationToken"));
        assert!(csharp.contains("Task.Run("));
        assert!(csharp.contains("catch (OperationCanceledException)"));
        assert!(csharp.contains("IsThreadSafe = true"));
        assert!(!csharp.contains("ExcelDnaUtil.Application"));
        assert!(!csharp.contains("DllImport"));
    }

    #[test]
    fn product_hosts_share_one_schema_one_managed_backend_contract() {
        for (files, profile) in [
            (webapi_files("risk-engine", "10"), "net10-coreclr"),
            (worker_files("risk-engine", "10"), "net10-coreclr"),
            (winui_files("risk-engine", "10"), "winui3-net10-windows"),
            (maui_files("risk-engine", "10"), "maui-windows-net10"),
        ] {
            let cargo = &files
                .iter()
                .find(|file| file.rel == "rustlib/Cargo.toml")
                .unwrap()
                .body;
            assert!(cargo.contains("identity-schema = 1"));
            assert!(cargo.contains("assembly-name = \"RiskEngine.Rust\""));
            assert!(cargo.contains("root-namespace = \"RiskEngine\""));
            assert!(cargo.contains("module-type = \"Backend\""));
            assert!(cargo.contains(&format!("compatibility-profile = \"{profile}\"")));
            assert!(cargo.contains("legacy-main-module = false"));

            let csproj = files
                .iter()
                .find(|file| file.rel.ends_with(".csproj"))
                .unwrap();
            assert!(csproj.body.contains("<RustCrate Include=\"../rustlib\" />"));
            assert!(csproj.body.contains("RustDotnet.targets"));
        }
    }

    #[test]
    fn webapi_and_worker_are_cross_platform_net10_hosts() {
        let webapi = webapi_files("service", "10");
        let webapi_project = webapi
            .iter()
            .find(|file| file.rel.ends_with(".csproj"))
            .unwrap();
        assert!(webapi_project.body.contains("Microsoft.NET.Sdk.Web"));
        assert!(
            webapi_project
                .body
                .contains("<TargetFramework>net10.0</TargetFramework>")
        );
        let program = &webapi
            .iter()
            .find(|file| file.rel == "webapi/Program.cs")
            .unwrap()
            .body;
        assert!(program.contains("Backend.Describe(21)"));
        assert!(program.contains("MapGet(\"/health\""));

        let worker = worker_files("service", "10");
        let worker_project = worker
            .iter()
            .find(|file| file.rel.ends_with(".csproj"))
            .unwrap();
        assert!(worker_project.body.contains("Microsoft.NET.Sdk.Worker"));
        let service = &worker
            .iter()
            .find(|file| file.rel == "worker/RustWorker.cs")
            .unwrap()
            .body;
        assert!(service.contains("Backend.Describe(21)"));
        assert!(service.contains("StopApplication()"));
    }

    #[test]
    fn windows_hosts_are_explicitly_windows_only() {
        let winui = winui_files("desktop", "10");
        let winui_project = winui
            .iter()
            .find(|file| file.rel.ends_with(".csproj"))
            .unwrap();
        assert!(winui_project.body.contains("<UseWinUI>true</UseWinUI>"));
        assert!(
            winui_project
                .body
                .contains("Microsoft.WindowsAppSDK\" Version=\"2.2.0")
        );

        let maui = maui_files("desktop", "10");
        let maui_project = maui
            .iter()
            .find(|file| file.rel.ends_with(".csproj"))
            .unwrap();
        assert!(maui_project.body.contains("<UseMaui>true</UseMaui>"));
        assert!(
            maui_project
                .body
                .contains("<TargetFramework>net10.0-windows10.0.19041.0")
        );
        assert!(!maui_project.body.contains("-android"));
        assert!(!maui_project.body.contains("-ios"));
        assert!(!maui_project.body.contains("-maccatalyst"));
    }

    #[test]
    fn no_template_placeholders_leak() {
        for name in ["a", "b_c", "Xyz"] {
            for files in [
                app_files(name),
                lib_files(name, "10"),
                plugin_files(name, "10"),
                excel_files(name, "10"),
                maui_files(name, "10"),
                winui_files(name, "10"),
                webapi_files(name, "10"),
                worker_files(name, "10"),
            ] {
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
                        !f.body.contains("__DOTNET__"),
                        "unresolved __DOTNET__ in {}",
                        f.rel
                    );
                    assert!(
                        !f.body.contains("__RUST_CRATE__"),
                        "unresolved __RUST_CRATE__ in {}",
                        f.rel
                    );
                    assert!(
                        !f.body.contains("__NAMESPACE__"),
                        "unresolved __NAMESPACE__ in {}",
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
