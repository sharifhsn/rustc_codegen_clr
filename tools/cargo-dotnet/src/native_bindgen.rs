use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::BindgenArgs;

pub fn run(args: &BindgenArgs) -> Result<i32> {
    let crate_dir = crate_dir(args.path.as_deref())?;
    let header = resolve(&crate_dir, &args.header);
    let output = resolve(&crate_dir, &args.output);
    let report = rust_dotnet_bindgen::generate(&rust_dotnet_bindgen::BindgenConfig {
        header,
        library: args.library.clone(),
        output,
        allowlist_functions: args.allowlist_function.clone(),
        allowlist_types: args.allowlist_type.clone(),
        blocklist_items: args.blocklist_item.clone(),
        clang_args: args.clang_arg.clone(),
        derive_default: args.derive_default,
        layout_tests: args.layout_tests,
        check: args.check,
    })?;
    println!(
        "==> cargo dotnet: {} {} P/Invoke declaration block(s) -> {}",
        if report.changed {
            "generated"
        } else {
            "verified"
        },
        report.extern_blocks,
        report.output.display()
    );
    println!(
        "    add `mod {};` to your crate if it is not already declared",
        module_name(&crate_dir, &report.output)
    );
    Ok(0)
}

fn crate_dir(path: Option<&Path>) -> Result<PathBuf> {
    let path = path.unwrap_or_else(|| Path::new("."));
    let path = path
        .canonicalize()
        .with_context(|| format!("resolving consumer crate {}", path.display()))?;
    if !path.join("Cargo.toml").is_file() {
        bail!("consumer crate has no Cargo.toml: {}", path.display());
    }
    Ok(path)
}

fn resolve(crate_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        crate_dir.join(path)
    }
}

fn module_name(crate_dir: &Path, output: &Path) -> String {
    output
        .strip_prefix(crate_dir.join("src"))
        .ok()
        .and_then(|path| path.file_stem())
        .and_then(|name| name.to_str())
        .unwrap_or("native")
        .replace('-', "_")
}
