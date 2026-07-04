//! `run` the produced artifact — typed port of the bash run block (core 818-838).

use std::process::Command;

use anyhow::{Context as _, Result};

use crate::artifact::Artifact;
use crate::context::Context;

/// Run the produced artifact, forwarding program args + propagating the exit code.
/// A library is not runnable (exit 0 with a "reference the .dll" note); a missing
/// apphost is exit 3.
pub fn run(art: &Artifact, prog_args: &[String], ctx: &Context) -> Result<i32> {
    match art {
        Artifact::Executable(exe) => {
            if ctx.flags.verbose {
                eprintln!("== RUN {} ==", exe.display());
            }
            let mut cmd = Command::new(exe);
            cmd.args(prog_args);
            // The apphost is a native launcher that finds the .NET runtime; self-heal
            // dotnet onto PATH for it too, mirroring the build env.
            if let Some((path_add, dotnet_root)) = &ctx.dotnet_heal {
                let cur = std::env::var("PATH").unwrap_or_default();
                cmd.env("PATH", format!("{}:{}", path_add.display(), cur));
                cmd.env("DOTNET_ROOT", dotnet_root);
            }
            let status = cmd
                .status()
                .with_context(|| format!("failed to run apphost {}", exe.display()))?;
            let code = status.code().unwrap_or(1);
            if ctx.flags.verbose {
                eprintln!("run exit: {code}");
            }
            Ok(code)
        }
        Artifact::Library { dll, stem, .. } => {
            println!(
                "cargo dotnet run: '{stem}' is a LIBRARY (no entrypoint) — reference {} \
                 from a C# project (see docs/INTEROP_CSHARP.md)",
                dll.file_name().and_then(|s| s.to_str()).unwrap_or("the .dll")
            );
            Ok(0)
        }
        Artifact::None => {
            eprintln!("!! cargo dotnet run: no runnable apphost found (looked for an executable artifact)");
            Ok(3)
        }
    }
}
