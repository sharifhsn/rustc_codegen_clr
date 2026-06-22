//! Standard cargo-flag passthrough (the P2/D4 fix) + the program-arg split.
//!
//! The bash front-end HARD-ERRORS on any unknown flag (`cargo-dotnet:574`). The Rust
//! binary instead forwards the clap-cargo standard groups (--features/-p/--manifest-
//! path/--workspace/--exclude) AND any extra trailing cargo flags verbatim to the
//! inner build-std cargo. clap splits the two streams for us:
//!   * `extra`     (trailing_var_arg) = cargo flags BEFORE `--`,
//!   * `prog_args` (last = true)      = program args AFTER `--` (run only).
//!
//! This module turns the modelled groups + `extra` into the final inner-cargo flag
//! list (the `CD_EXTRA_CARGO_FLAGS` env value); `prog_args` is forwarded as-is.

use crate::cli::BuildArgs;

/// Build the inner-cargo flag list from the parsed args. Does NOT include `--release`
/// (the core derives the profile from `CD_REL`); it carries only the standard groups
/// + the verbatim extras.
pub fn assemble_cargo_flags(args: &BuildArgs) -> Vec<String> {
    let mut cargo_flags: Vec<String> = Vec::new();

    // ---- clap-cargo Features ----
    for f in &args.features.features {
        cargo_flags.push("--features".to_string());
        cargo_flags.push(f.clone());
    }
    if args.features.all_features {
        cargo_flags.push("--all-features".to_string());
    }
    if args.features.no_default_features {
        cargo_flags.push("--no-default-features".to_string());
    }

    // ---- clap-cargo Manifest ----
    if let Some(path) = &args.manifest.manifest_path {
        cargo_flags.push("--manifest-path".to_string());
        cargo_flags.push(path.display().to_string());
    }

    // ---- clap-cargo Workspace ----
    for p in &args.workspace.package {
        cargo_flags.push("-p".to_string());
        cargo_flags.push(p.clone());
    }
    if args.workspace.workspace {
        cargo_flags.push("--workspace".to_string());
    }
    for e in &args.workspace.exclude {
        cargo_flags.push("--exclude".to_string());
        cargo_flags.push(e.clone());
    }

    // ---- verbatim extras (--locked/--offline/--frozen/--target-dir/--message-format/…) ----
    cargo_flags.extend(args.extra.iter().cloned());

    cargo_flags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> BuildArgs {
        BuildArgs {
            path: None,
            release: false,
            debug: false,
            clean: false,
            verbose: false,
            backend: None,
            dotnet: "8".to_string(),
            features: clap_cargo::Features::default(),
            manifest: clap_cargo::Manifest::default(),
            workspace: clap_cargo::Workspace::default(),
            extra: Vec::new(),
            prog_args: Vec::new(),
        }
    }

    #[test]
    fn extras_are_forwarded_verbatim() {
        let mut a = base_args();
        a.extra = vec!["--offline".into(), "--target-dir".into(), "/tmp/x".into()];
        let cargo = assemble_cargo_flags(&a);
        assert_eq!(cargo, vec!["--offline", "--target-dir", "/tmp/x"]);
    }

    #[test]
    fn features_and_packages_are_forwarded() {
        let mut a = base_args();
        a.features.features = vec!["foo".into(), "bar".into()];
        a.workspace.package = vec!["mycrate".into()];
        let cargo = assemble_cargo_flags(&a);
        assert_eq!(
            cargo,
            vec!["--features", "foo", "--features", "bar", "-p", "mycrate"]
        );
    }

    #[test]
    fn all_and_no_default_features() {
        let mut a = base_args();
        a.features.all_features = true;
        a.features.no_default_features = true;
        let cargo = assemble_cargo_flags(&a);
        assert!(cargo.contains(&"--all-features".to_string()));
        assert!(cargo.contains(&"--no-default-features".to_string()));
    }
}
