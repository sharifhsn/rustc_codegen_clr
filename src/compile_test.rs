use std::path::PathBuf;

// The standalone regression corpus predates Rust 2024 and intentionally tests
// codegen rather than source-edition migration. Cargo crates carry their own
// edition in their manifests; keep these direct-rustc fixtures on the newest
// edition their checked-in source actually satisfies.
const STANDALONE_TEST_EDITION: &str = "2024";

#[cfg(test)]
fn assert_compile_succeeded(command: &str, output: &std::process::Output) {
    assert!(
        output.status.success(),
        "compiler command failed: {command}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[must_use]
pub fn test_dotnet_executable(file_path: &str, test_dir: &str) -> String {
    use std::io::Write;
    if crate::config::current().dry_run() {
        return String::new();
    }
    #[cfg(not(target_os = "windows"))]
    assert!(
        (*IS_DOTNET_PRESENT || *IS_MONO_PRESENT),
        "You must have the dotnet runtime installed to run tests."
    );
    let exec_path = &format!("{file_path}.exe");
    #[cfg(target_os = "windows")]
    let exec_path = &std::fs::canonicalize(format!("{test_dir}//{exec_path}")).unwrap();
    let mut stdout = String::new();
    //println!("exec_path:{exec_path:?}");
    if *IS_DOTNET_PRESENT {
        let config_path = if file_path.contains(test_dir) {
            format!("{file_path}.runtimeconfig.json")
        } else if cfg!(target_os = "windows") {
            format!("{test_dir}\\{file_path}.runtimeconfig.json")
        } else {
            format!("{test_dir}/{file_path}.runtimeconfig.json")
        };

        let mut file = std::fs::File::create(&config_path).unwrap_or_else(|err| {
            panic!("Could not create runtime config file at {config_path:?} due to {err:?}")
        });
        let runtime = crate::config::current().artifact_abi().dotnet_runtime();
        let runtime_config = format!(
            "{{\n  \"runtimeOptions\": {{\n    \"tfm\": \"{}\",\n    \"framework\": {{\n      \"name\": \"Microsoft.NETCore.App\",\n      \"version\": \"{}\"\n    }},\n    \"rollForward\": \"LatestMajor\"\n  }}\n}}\n",
            runtime.tfm(),
            runtime.framework_version(),
        );
        file.write_all(runtime_config.as_bytes())
            .expect("Could not write runtime config");
        //RUNTIME_CONFIG
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let mut cmd = {
            let mut cmd = std::process::Command::new("timeout");
            cmd.arg("-v");
            cmd.arg("5");
            cmd.arg("dotnet");
            cmd.arg(exec_path);
            cmd
        };
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let cmd = std::process::Command::new(exec_path);
            cmd
        };
        cmd.current_dir(test_dir);

        #[cfg(target_family = "unix")]
        with_stack_size(&mut cmd, 1024 * 80);
        let out = cmd.output().expect("failed to run test assebmly!");

        let stderr = String::from_utf8(out.stderr).expect("Stdout is not UTF8 String!");
        assert!(
            out.status.success(),
            "Test program exited with status {}. stdout:\n{}\nstderr:\n{}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            stderr,
        );
        assert!(
            stderr.is_empty(),
            "Test program failed with message {stderr:}"
        );
        stdout = String::from_utf8_lossy(&out.stdout).to_string();
    }
    if *IS_MONO_PRESENT && crate::config::current().test_with_mono() {
        // Execute the test assembly
        let out = std::process::Command::new("mono")
            .current_dir(test_dir)
            .args([exec_path])
            .output()
            .expect("failed to run test assebmly!");
        let stderr = String::from_utf8(out.stderr).expect("Stdout is not UTF8 String!");
        assert!(
            stderr.is_empty(),
            "Test program failed with message {stderr:}"
        );
    } else {
        #[cfg(not(target_os = "windows"))]
        assert!(
            *IS_DOTNET_PRESENT,
            "Only mono runtime present. Mono does not support all the features required to get Rust code working."
        );
    }

    stdout
}
#[cfg(test)]
fn test_lib(args: &[&str], test_name: &str) {
    // Ensures the test directory is present
    std::fs::create_dir_all("./test/out").expect("Could not setup the test env");
    // Builds the backend if neceasry
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    // Compiles the test project
    let mut command = std::process::Command::new("rustc");
    command.arg("-Ctarget-feature=+x87+sse");
    let command = command
        .current_dir("./test/out")
        //.env("RUST_TARGET_PATH","../../")
        .args(args);

    let command = if *IS_MONO_PRESENT {
        // Tell the linker to test AOT
        command.args(["-C", "link-arg=--aot-mode,mono-full"])
    } else {
        command
    };
    let out = command.output().expect("failed to execute process");
    if String::from_utf8(out.stderr.clone())
        .unwrap()
        .contains("error:")
    {
        let stdout =
            String::from_utf8(out.stdout).expect("rustc error contained non-UTF8 characters.");
        let stderr =
            String::from_utf8(out.stderr).expect("rustc error contained non-UTF8 characters.");
        panic!("stdout:\n{stdout}\nstderr:\n{stderr}");
    }
    let test_dll = format!("./{test_name}.dll");
    let out = std::process::Command::new(RUSTC_CODEGEN_CLR_LINKER.display().to_string())
        .current_dir("./test/out")
        .arg("-o")
        .arg(test_dll)
        .arg(format!("./{test_name}.rlib"))
        .output()
        .unwrap();
    //super::peverify(test_dll, "./test/out");
    // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
    if !out.stderr.is_empty() {
        let stdout =
            String::from_utf8(out.stdout).expect("rustc error contained non-UTF8 characters.");
        let stderr =
            String::from_utf8(out.stderr).expect("rustc error contained non-UTF8 characters.");
        panic!("stdout:\n{stdout}\nstderr:\n{stderr}");
    }
}
/// Compiles `$test_name` with both the backend and native rustc, runs both binaries, and
/// byte-diffs stdout. The only macro here that actually proves output-correctness against
/// native Rust — `run_test!`/`cargo_test!` do not compare output at all.
macro_rules! compare_tests {
    ($prefix:ident,$test_name:ident,$is_stable:ident) => {
        mod $test_name {
            mod $is_stable {
                #[cfg(test)]
                #[cfg(test)]
                static COMPILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
                #[test]
                fn release() {
                    let lock = COMPILE_LOCK.lock();
                    let mut should_panic = false;
                    #[cfg(target_os = "windows")]
                    let test_dir = concat!(".\\test\\", stringify!($prefix), "\\");
                    #[cfg(not(target_os = "windows"))]
                    let test_dir = concat!("./test/", stringify!($prefix), "/");
                    // Ensures the test directory is present
                    std::fs::create_dir_all(test_dir).expect("Could not setup the test env");
                    // Builds the backend if neceasry
                    super::super::RUSTC_BUILD_STATUS
                        .as_ref()
                        .expect("Could not build rustc!");
                    // Compiles the test project
                    let mut cmd = super::super::compiler(stringify!($test_name), test_dir, true);
                    let copy = format!("{cmd:?}");
                    let out = cmd.output().expect("failed to execute process");
                    super::super::assert_compile_succeeded(&copy, &out);
                    // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                    if String::from_utf8(out.stderr.clone())
                        .unwrap()
                        .contains("error:")
                    {
                        let stdout = String::from_utf8(out.stdout)
                            .expect("rustc error contained non-UTF8 characters.");
                        let stderr = String::from_utf8(out.stderr)
                            .expect("rustc error contained non-UTF8 characters.");
                        eprintln!("cmd:{copy}\nstdout:\n{stdout}\nstderr:\n{stderr}");
                        if stderr.contains("error") {
                            should_panic = true;
                        }
                    }
                    if crate::config::current().dry_run() {
                        return;
                    }
                    #[cfg(not(target_os = "windows"))]
                    let exec_path = concat!("./", stringify!($test_name));
                    #[cfg(target_os = "windows")]
                    let exec_path = concat!(".\\", stringify!($test_name));
                    drop(lock);
                    //super::peverify(exec_path, test_dir);
                    eprintln!("Prepating to test with .NET");
                    let dotnet_out = super::super::test_dotnet_executable(exec_path, test_dir);
                    // Compiles the project with native rust
                    let mut cmd = std::process::Command::new("rustc");
                    //.env("RUST_TARGET_PATH","../../")
                    cmd.current_dir(test_dir).args([
                        "-O",
                        concat!("./", stringify!($test_name), ".rs"),
                        "-o",
                        concat!("./", stringify!($test_name), ".a"),
                        "--edition",
                        super::super::STANDALONE_TEST_EDITION,
                        "-Ctarget-feature=+x87+sse",
                    ]);
                    let copy = format!("{cmd:?}");
                    let out = cmd.output().expect("failed to execute process");
                    super::super::assert_compile_succeeded(&copy, &out);
                    // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                    if String::from_utf8(out.stderr.clone())
                        .unwrap()
                        .contains("error:")
                    {
                        let stdout = String::from_utf8(out.stdout)
                            .expect("rustc error contained non-UTF8 characters.");
                        let stderr = String::from_utf8(out.stderr)
                            .expect("rustc error contained non-UTF8 characters.");
                        if stderr.contains("error") || stderr.matches("thread 'rustc'").count() > 1
                        {
                            should_panic = true;
                        }
                        eprintln!("cmd:{copy}\nstdout:\n{stdout}\nstderr:\n{stderr}");
                    }
                    let rust_out =
                        std::process::Command::new(concat!("./", stringify!($test_name), ".a"))
                            .current_dir(test_dir)
                            .output()
                            .expect("failed to execute process");
                    let rust_out = String::from_utf8(rust_out.stdout)
                        .expect("rust error contained non-UTF8 characters.");
                    if rust_out != dotnet_out {
                        panic!("rust_out:\n{rust_out}\n\ndotnet_out:\n{dotnet_out}");
                    }

                    if should_panic {
                        panic!("{rust_out}{dotnet_out}");
                    }
                }
                #[test]
                fn debug() {
                    let lock = COMPILE_LOCK.lock();
                    let mut should_panic = false;
                    #[cfg(target_os = "windows")]
                    let test_dir = concat!(".\\test\\", stringify!($prefix), "\\");
                    #[cfg(not(target_os = "windows"))]
                    let test_dir = concat!("./test/", stringify!($prefix), "/");
                    // Ensures the test directory is present
                    std::fs::create_dir_all(test_dir).expect("Could not setup the test env");
                    // Builds the backend if neceasry
                    super::super::RUSTC_BUILD_STATUS
                        .as_ref()
                        .expect("Could not build rustc!");
                    let mut cmd = super::super::compiler(stringify!($test_name), test_dir, true);
                    let copy = format!("{cmd:?}");
                    let out = cmd.output().expect("failed to execute process");
                    super::super::assert_compile_succeeded(&copy, &out);
                    // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                    if String::from_utf8(out.stderr.clone())
                        .unwrap()
                        .contains("error:")
                    {
                        let stdout = String::from_utf8(out.stdout)
                            .expect("rustc error contained non-UTF8 characters.");
                        let stderr = String::from_utf8(out.stderr)
                            .expect("rustc error contained non-UTF8 characters.");
                        eprintln!("stdout:\n{stdout}\nstderr:\n{stderr}");
                        if stderr.contains("error") {
                            should_panic = true;
                        }
                    }
                    #[cfg(not(target_os = "windows"))]
                    let exec_path = concat!("./", stringify!($test_name));
                    #[cfg(target_os = "windows")]
                    let exec_path = concat!(".\\", stringify!($test_name));
                    drop(lock);
                    //super::peverify(exec_path, test_dir);
                    eprintln!("Prepating to test with .NET");
                    if crate::config::current().dry_run() {
                        return;
                    }
                    let dotnet_out = super::super::test_dotnet_executable(exec_path, test_dir);
                    // Compiles the project with native rust
                    let mut cmd = std::process::Command::new("rustc");
                    //.env("RUST_TARGET_PATH","../../")
                    cmd.current_dir(test_dir).args([
                        "-O",
                        concat!("./", stringify!($test_name), ".rs"),
                        "-o",
                        concat!("./", stringify!($test_name), ".a"),
                        "--edition",
                        super::super::STANDALONE_TEST_EDITION,
                        "-Ctarget-feature=+x87+sse",
                    ]);
                    let copy = format!("{cmd:?}");
                    let out = cmd.output().expect("failed to execute process");
                    super::super::assert_compile_succeeded(&copy, &out);
                    // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                    if String::from_utf8(out.stderr.clone())
                        .unwrap()
                        .contains("error:")
                    {
                        let stdout = String::from_utf8(out.stdout)
                            .expect("rustc error contained non-UTF8 characters.");
                        let stderr = String::from_utf8(out.stderr)
                            .expect("rustc error contained non-UTF8 characters.");
                        if stderr.contains("error") || stderr.matches("thread 'rustc'").count() > 1
                        {
                            should_panic = true;
                        }
                        eprintln!("stdout:\n{stdout}\nstderr:\n{stderr}");
                    }
                    let rust_out =
                        std::process::Command::new(concat!("./", stringify!($test_name), ".a"))
                            .current_dir(test_dir)
                            .output()
                            .expect("failed to execute process");
                    let rust_out = String::from_utf8(rust_out.stdout)
                        .expect("rust error contained non-UTF8 characters.");
                    if rust_out != dotnet_out {
                        panic!("rust_out:\n{rust_out}\n\ndotnet_out:\n{dotnet_out}");
                    }

                    if should_panic {
                        panic!("{rust_out}{dotnet_out}");
                    }
                }
            }
        }
    };
}

/// Compiles `$test_name` with `--crate-type=lib` via the backend and never runs it — only
/// proves the crate compiles, not that any code in it behaves correctly.
macro_rules! test_lib {
    // Inner arm: emits one test fn `$fname`. `$($opt:literal,)*` is the leading
    // optimization flag (`"-O",` for release, empty for debug); the rest of the rustc
    // argument array is shared, so both variants expand to identical bodies modulo `-O`.
    (@body $test_name:ident, $fname:ident, [$($opt:literal,)*]) => {
        #[test]
        fn $fname() {
            // Ensures no two compilations run at the same time.
            let lock = COMPILE_LOCK.lock();
            super::super::test_lib(
                &[
                    $($opt,)*
                    "--crate-type=lib",
                    "-Z",
                    &super::super::backend_path(),
                    "-C",
                    &format!(
                        "linker={}",
                        super::super::RUSTC_CODEGEN_CLR_LINKER.display()
                    ),
                    concat!("../", stringify!($test_name), ".rs"),
                    "-o",
                    concat!("./", stringify!($test_name), ".rlib"),
                    //"--target",
                    // "clr64-unknown-clr"
                ],
                stringify!($test_name),
            );
            drop(lock);
        }
    };
    ($test_name:ident,$is_stable:ident) => {
        mod $test_name {
            mod $is_stable {
                #[cfg(test)]
                static COMPILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
                test_lib! {@body $test_name, release, ["-O",]}
                test_lib! {@body $test_name, debug, []}
            }
        }
    };
}
#[cfg(test)]
fn compiler(test_name: &str, test_dir: &str, release: bool) -> std::process::Command {
    // Compiles the test project
    let mut cmd = std::process::Command::new("rustc");
    //.env("RUST_TARGET_PATH","../../")
    if release {
        cmd.arg("-O");
    }
    cmd.current_dir(test_dir)
        .args(rustc_args().iter())
        .args([format!("./{test_name}.rs"), "-o".to_owned()]);
    if release {
        cmd.arg(format!("./{test_name}.exe"));
    } else {
        cmd.arg(format!("./debug_{test_name}.exe"));
    }
    if crate::config::current().dry_run() {
        cmd.args(["-Z", "no-codegen"]);
    }
    cmd.arg("-Ctarget-feature=+x87+sse");
    cmd
}
/// Compiles `$test_name` with the backend and runs the .NET (or C) output, asserting only that
/// the process exits without producing stderr — it does NOT compare output against native
/// rustc, so a pass here does not prove correctness (use `compare_tests!` for that).
macro_rules! run_test {
    ($prefix:ident,$test_name:ident,$is_stable:ident) => {
        mod $test_name {
            mod $is_stable {
                #[cfg(test)]
                static COMPILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
                #[test]
                fn release() {
                    let lock = COMPILE_LOCK.lock();
                    #[cfg(target_os = "windows")]
                    let test_dir = concat!(".\\test\\", stringify!($prefix), "\\");
                    #[cfg(not(target_os = "windows"))]
                    let test_dir = concat!("./test/", stringify!($prefix), "/");
                    // Ensures the test directory is present
                    std::fs::create_dir_all(test_dir).expect("Could not setup the test env");
                    // Builds the backend if neceasry
                    super::super::RUSTC_BUILD_STATUS
                        .as_ref()
                        .expect("Could not build rustc!");
                    let mut cmd = super::super::compiler(stringify!($test_name), test_dir, true);

                    eprintln!("Command: {cmd:?}");
                    let out = cmd.output().expect("failed to execute process");
                    // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                    if String::from_utf8(out.stderr.clone())
                        .unwrap()
                        .contains("error:")
                    {
                        let stdout = String::from_utf8(out.stdout)
                            .expect("rustc error contained non-UTF8 characters.");
                        let stderr = String::from_utf8(out.stderr)
                            .expect("rustc error contained non-UTF8 characters.");
                        panic!("stdout:\n{stdout}\nstderr:\n{stderr}");
                    }

                    #[cfg(not(target_os = "windows"))]
                    let exec_path = concat!("./", stringify!($test_name));
                    #[cfg(target_os = "windows")]
                    let exec_path = concat!(".\\", stringify!($test_name));
                    drop(lock);
                    let _ = super::super::test_dotnet_executable(exec_path, test_dir);
                }
                #[test]
                fn debug() {
                    let lock = COMPILE_LOCK.lock();
                    #[cfg(target_os = "windows")]
                    let test_dir = concat!(".\\test\\", stringify!($prefix), "\\");
                    #[cfg(not(target_os = "windows"))]
                    let test_dir = concat!("./test/", stringify!($prefix), "/");
                    // Ensures the test directory is present
                    std::fs::create_dir_all(test_dir).expect("Could not setup the test env");
                    // Builds the backend if neceasry
                    super::super::RUSTC_BUILD_STATUS
                        .as_ref()
                        .expect("Could not build rustc!");
                    let test_name = concat!("debug_", stringify!($test_name));
                    let mut cmd = super::super::compiler(stringify!($test_name), test_dir, false);
                    // /eprintln!("out:{out:?}");
                    eprintln!("test_name:{test_name:?}");
                    let out = cmd.output().expect("failed to execute process");
                    // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                    if String::from_utf8(out.stderr.clone())
                        .unwrap()
                        .contains("error:")
                    {
                        let stdout = String::from_utf8(out.stdout)
                            .expect("rustc error contained non-UTF8 characters.");
                        let stderr = String::from_utf8(out.stderr)
                            .expect("rustc error contained non-UTF8 characters.");
                        panic!("stdout:\n{stdout}\nstderr:\n{stderr}");
                    }
                    #[cfg(not(target_os = "windows"))]
                    let exec_path = format!("./{test_name}");
                    #[cfg(target_os = "windows")]
                    let exec_path = format!(".\\{test_name}");

                    drop(lock);

                    let _ = super::super::test_dotnet_executable(&exec_path, test_dir);
                }
            }
        }
    };
}
/// Runs `cargo build` (debug and release) for a `cargo_tests/` crate through the backend —
/// never executes the resulting binary, so this only proves the crate compiles, not that it
/// runs or produces correct output.
macro_rules! cargo_test {
    ($test_name:ident,$is_stable:ident) => {
        mod $test_name { mod $is_stable{

            #[cfg(test)]
            static COMPILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
            #[test]
            fn cargo_debug() {
                let lock = COMPILE_LOCK.lock();
                #[cfg(target_os = "windows")]
                let test_dir = concat!(".\\cargo_tests\\", stringify!($prefix), "\\");
                #[cfg(not(target_os = "windows"))]
                let test_dir = concat!("./cargo_tests/", stringify!($test_name), "/");

                // Ensures the test directory is present
                std::fs::create_dir_all(test_dir).expect("Could not setup the test env");
                // Builds the backend if neceasry
                let rustflags = super::super::cargo_build_env();
                // Compiles the test project
                let out = std::process::Command::new("cargo")
                    .env("RUSTFLAGS", &rustflags)
                    .current_dir(test_dir)
                    .args(["-Zjson-target-spec", "build"])
                    .output()
                    .expect("failed to execute process");
                // panic!("out:{out:?}");
                // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                if !out.stderr.is_empty() {
                    let stderr = String::from_utf8(out.stderr.clone())
                        .expect("rustc error contained non-UTF8 characters.");

                    if !stderr.contains("Finished") {
                        let stdout = String::from_utf8(out.stdout)
                            .expect("rustc error contained non-UTF8 characters.");
                        let stderr = String::from_utf8(out.stderr)
                            .expect("rustc error contained non-UTF8 characters.");
                        if !stderr.contains("Finished") {
                            panic!("stdout:\n{stdout}\nstderr:\n{stderr}");
                        }
                    }
                }
                drop(lock);
            }
            #[test]
            fn cargo_release() {
                let lock = COMPILE_LOCK.lock();
                #[cfg(target_os = "windows")]
                let test_dir = concat!(".\\cargo_tests\\", stringify!($prefix), "\\");
                #[cfg(not(target_os = "windows"))]
                let test_dir = concat!("./cargo_tests/", stringify!($test_name), "/");
                // Ensures the test directory is present
                std::fs::create_dir_all(test_dir).expect("Could not setup the test env");
                // Builds the backend if neceasry
                let rustflags = super::super::cargo_build_env();
                // Compiles the test project
                let mut command = std::process::Command::new("cargo");
                command
                    .env("RUSTFLAGS", &rustflags)
                    .current_dir(test_dir)
                    .args([
                        "-Zjson-target-spec",
                        "build",
                        "--release", //"--target",
                                     //"clr64-unknown-clr"
                    ]);
                let out = command.output().expect("failed to execute process");

                // panic!("out:{out:?}");
                // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                if !out.stderr.is_empty() {
                    let stdout = String::from_utf8(out.stdout)
                        .expect("rustc error contained non-UTF8 characters.");
                    let stderr = String::from_utf8(out.stderr)
                        .expect("rustc error contained non-UTF8 characters.");
                    if !stderr.contains("Finished") {
                        panic!(
                            "command:{command:?} failed. \n stdout:\n{stdout}\nstderr:\n{stderr}"
                        );
                    }
                }
                drop(lock);
            }
        }}
    };
}
/// Same as `cargo_test!` (build-only, no execution) but `#[ignore]`d — for crates too slow or
/// unreliable to run in the default test pass.
macro_rules! cargo_test_ignored {
    ($test_name:ident) => {
        mod $test_name {

            #[cfg(test)]
            static COMPILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
            #[ignore]
            #[test]
            fn cargo_debug() {
                let lock = COMPILE_LOCK.lock();
                #[cfg(target_os = "windows")]
                let test_dir = concat!(".\\cargo_tests\\", stringify!($prefix), "\\");
                #[cfg(not(target_os = "windows"))]
                let test_dir = concat!("./cargo_tests/", stringify!($prefix), "/");
                // Ensures the test directory is present
                std::fs::create_dir_all(test_dir).expect("Could not setup the test env");

                let rustflags = super::cargo_build_env();
                // Compiles the test project
                let out = std::process::Command::new("cargo")
                    .env("RUSTFLAGS", &rustflags)
                    .current_dir(test_dir)
                    .args(["-Zjson-target-spec", "build"])
                    .output()
                    .expect("failed to execute process");
                // panic!("out:{out:?}");
                // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                if !out.stderr.is_empty() {
                    let stderr = String::from_utf8(out.stderr.clone())
                        .expect("rustc error contained non-UTF8 characters.");

                    if !stderr.contains("Finished") {
                        let stdout = String::from_utf8(out.stdout)
                            .expect("rustc error contained non-UTF8 characters.");
                        let stderr = String::from_utf8(out.stderr)
                            .expect("rustc error contained non-UTF8 characters.");
                        if !stderr.contains("Finished") {
                            panic!("stdout:\n{stdout}\nstderr:\n{stderr}");
                        }
                    }
                }
                drop(lock);
            }
            #[ignore]
            #[test]
            fn cargo_release() {
                let lock = COMPILE_LOCK.lock();
                #[cfg(target_os = "windows")]
                let test_dir = concat!(".\\cargo_tests\\", stringify!($prefix), "\\");
                #[cfg(not(target_os = "windows"))]
                let test_dir = concat!("./cargo_tests/", stringify!($prefix), "/");
                // Ensures the test directory is present
                std::fs::create_dir_all(test_dir).expect("Could not setup the test env");

                let rustflags = super::cargo_build_env();
                // Compiles the test project
                let mut command = std::process::Command::new("cargo");
                command
                    .env("RUSTFLAGS", &rustflags)
                    .current_dir(test_dir)
                    .args([
                        "-Zjson-target-spec",
                        "build",
                        "--release", //"--target",
                                     //"clr64-unknown-clr"
                    ]);
                let out = command.output().expect("failed to execute process");
                // panic!("out:{out:?}");
                // If stderr is not empty, then something went wrong, so print the stdout and stderr for debuging.
                if !out.stderr.is_empty() {
                    let stdout = String::from_utf8(out.stdout)
                        .expect("rustc error contained non-UTF8 characters.");
                    let stderr = String::from_utf8(out.stderr)
                        .expect("rustc error contained non-UTF8 characters.");
                    if !stderr.contains("Finished") || true {
                        panic!(
                            "command:{command:?} failed. \n stdout:\n{stdout}\nstderr:\n{stderr}"
                        );
                    }
                }
                drop(lock);
            }
        }
    };
}
#[cfg(debug_assertions)]
fn build_backend() -> Result<(), String> {
    let _out = std::process::Command::new("cargo")
        .args(["build", "--lib"])
        .output()
        .map_err(|err| err.to_string())?;
    let _out = std::process::Command::new("cargo")
        .args(["build", "--lib", "--release"])
        .output()
        .map_err(|err| err.to_string())?;
    let _out = std::process::Command::new("cargo")
        .current_dir("cilly")
        .args(["build", "--bin", "linker"])
        .output()
        .expect("could not build the backend");
    let _out = std::process::Command::new("cargo")
        .current_dir("cilly")
        .args(["build", "--bin", "linker", "--release"])
        .output()
        .expect("could not build the backend");
    Ok(())
}
#[cfg(not(debug_assertions))]
fn build_backend() -> Result<(), String> {
    std::process::Command::new("cargo")
        .args(["build", "--release", "--lib"])
        .output()
        .expect("could not build the backend");
    std::process::Command::new("cargo")
        .args(["build", "--release", "--bin", "linker"])
        .output()
        .expect("could not build the backend");
    Ok(())
}
/// Absolute path to the codegen backend shared library.
#[must_use]
pub fn absolute_backend_path() -> PathBuf {
    if cfg!(debug_assertions) {
        if cfg!(target_os = "linux") {
            std::fs::canonicalize("target/debug/librustc_codegen_clr.so").unwrap()
        } else if cfg!(target_os = "windows") {
            std::fs::canonicalize("target/debug/rustc_codegen_clr.dll").unwrap()
        } else if cfg!(target_os = "macos") {
            std::fs::canonicalize("target/debug/librustc_codegen_clr.dylib").unwrap()
        } else {
            panic!("Unsupported target OS");
        }
    } else if cfg!(target_os = "linux") {
        std::fs::canonicalize("target/release/librustc_codegen_clr.so").unwrap()
    } else if cfg!(target_os = "windows") {
        std::fs::canonicalize("target/release/rustc_codegen_clr.dll").unwrap()
    } else if cfg!(target_os = "macos") {
        std::fs::canonicalize("target/release/librustc_codegen_clr.dylib").unwrap()
    } else {
        panic!("Unsupported target OS");
    }
}
#[cfg(target_family = "unix")]
fn with_stack_size(cmd: &mut std::process::Command, limit_kb: u64) {
    use libc::{RLIMIT_STACK, rlimit, setrlimit};
    use std::os::unix::process::CommandExt;

    unsafe {
        cmd.pre_exec(move || {
            setrlimit(
                RLIMIT_STACK,
                &rlimit {
                    rlim_cur: limit_kb * 1024,
                    rlim_max: limit_kb * 1024,
                },
            );
            Ok(())
        })
    };
}
fn backend_path() -> String {
    format!("codegen-backend={}", absolute_backend_path().display())
}
test_lib! {assign,stable}
test_lib! {binops,stable}
test_lib! {branches,stable}
test_lib! {calls,stable}
test_lib! {casts,stable}
test_lib! {closure,stable}
test_lib! {identity,stable}
test_lib! {types,stable}
test_lib! {autodiff,unstable}
test_lib! {references,stable}
//test_lib! {structs}
test_lib! {empty_string_slice,stable}

test_lib! {recursive,stable}
test_lib! {fn_ptr,stable}
test_lib! {tuple,stable}

run_test! {bench,iter,stable}
run_test! {alloc,abox,stable}
run_test! {alloc,raw_vec,stable}
run_test! {alloc,slice_to_owned,stable}
run_test! {arthm,add,stable}
run_test! {arthm,ctlz,unstable}
run_test! {arthm,ptr,stable}
run_test! {arthm,qsrt,stable}
run_test! {arthm,cmp,stable}
run_test! {arthm,greater_than,stable}
run_test! {arthm,max,stable}
run_test! {arthm,mul,stable}
run_test! {arthm,not,stable}
run_test! {arthm,num_test,stable}
run_test! {arthm,shift,stable}
run_test! {arthm,sub,stable}
run_test! {arthm,xor,stable}
run_test! {cast,i8_to_u64,stable}
run_test! {cast,i16_to_u64,stable}
run_test! {cast,i32_to_u64,stable}
run_test! {cast,i32_to_usize,stable}
run_test! {cast,coerce_unsized,unstable}
run_test! {control_flow,cf_for,stable}
run_test! {control_flow,drop,stable}
run_test! {fuzz,test0,stable}
run_test! {fuzz,test1,stable}
run_test! {intrinsics,addr_of,stable}
run_test! {intrinsics,alloc,stable}
run_test! {intrinsics,arith_offset,stable}
run_test! {intrinsics,arithmetic_misc,stable}
run_test! {intrinsics,assert,stable}
run_test! {intrinsics,float_minmax,stable}
run_test! {intrinsics,atomics,stable}
compare_tests! {intrinsics,atomic_u16_i16_rmw,stable}

run_test! {intrinsics,bswap,stable}
run_test! {intrinsics,caller_location,stable}
run_test! {intrinsics,catch,stable}
run_test! {intrinsics,cmp_bytes,stable}
run_test! {intrinsics,copy_nonoverlaping,stable}
run_test! {intrinsics,cpuid,stable}
run_test! {intrinsics,ctpop,stable}
run_test! {intrinsics,malloc,stable}
run_test! {intrinsics,offset_of,stable}
run_test! {intrinsics,overflow_ops,stable}
run_test! {intrinsics,pow_sqrt,stable}
run_test! {intrinsics,printf,stable}
run_test! {intrinsics,ptr_offset_from_unsigned,stable}
run_test! {intrinsics,round,stable}
run_test! {intrinsics,simd,stable}
run_test! {intrinsics,size_of_val,stable}
run_test! {intrinsics,transmute,stable}
run_test! {intrinsics,trigonometry,stable}
run_test! {intrinsics,type_id,stable}
run_test! {intrinsics,wrapping_ops,stable}
run_test! {iter,fold,stable}
run_test! {iter,array_byval,stable}
run_test! {statics,thread_local,stable}
run_test! {std,arg_test,stable}
run_test! {std,getopt,stable}
run_test! {std,const_error,stable}
run_test! {std,cell_test,unstable}
run_test! {std,cstr,unstable}
run_test! {std,format,unstable}
run_test! {std,futex_test,stable}
run_test! {std,futexrw_test,unstable}
run_test! {std,main,stable}
run_test! {std,sort,stable}
run_test! {std,mutithreading,stable}
run_test! {std,once_lock_test,stable}
run_test! {std,tlocal_key_test,stable}
run_test! {std,uninit_fill,stable}

run_test! {core,ascii_align,unstable}
run_test! {core,floatfmt,unstable}
run_test! {core,flt2dec,unstable}
run_test! {core,from_raw_parts,unstable}
run_test! {core,tuple_ord,stable}
run_test! {core,zst_iter,stable}
run_test! {core,adt_name_collision,stable}
run_test! {core,fixed_array_layout_identity,stable}

run_test! {types,adt_enum,stable}
run_test! {types,f128,stable}
run_test! {types,f16,stable}
run_test! {types,aligned,stable}
run_test! {types,any,stable}
run_test! {types,arr,stable}
run_test! {types,async_types,stable}
run_test! {types,dst,stable}
run_test! {types,dyns,stable}
run_test! {types,enums,stable}
run_test! {types,int128,stable}
run_test! {types,interop,stable}
compare_tests! {types,lowering_boundaries,stable}
compare_tests! {types,generated_ctor_authority,stable}
compare_tests! {types,generic_fn_ptr_abi,stable}
compare_tests! {types,track_caller_abi,stable}
compare_tests! {types,vtable_identity,stable}
run_test! {types,interop_typedef,unstable}
run_test! {types,maybeuninit,stable}
run_test! {types,nbody,stable}
run_test! {types,ref_deref,stable}
run_test! {types,self_referential_statics,stable}
run_test! {types,slice,stable}
run_test! {types,slice_from_end,stable}
run_test! {types,slice_index_ref,stable}
run_test! {types,slice_ptr_cast,stable}
run_test! {types,statics,stable}
run_test! {types,string_slice,stable}
run_test! {types,structs,stable}
run_test! {types,subslice,stable}
run_test! {types,tuple_enum,stable}
run_test! {types,tuple_structs,stable}
run_test! {types,vec,stable}

compare_tests! {fuzz,fuzz0,stable}
compare_tests! {fuzz,fuzz1,stable}
compare_tests! {fuzz,fuzz2,stable}
compare_tests! {fuzz,fuzz3,stable}
compare_tests! {fuzz,fuzz4,stable}
compare_tests! {fuzz,fuzz5,stable}
compare_tests! {fuzz,fuzz6,stable}
compare_tests! {fuzz,fuzz7,stable}
compare_tests! {fuzz,fuzz8,stable}
compare_tests! {fuzz,fuzz9,stable}

compare_tests! {fuzz,fuzz10,stable}
compare_tests! {fuzz,fuzz11,stable}
compare_tests! {fuzz,fuzz12,stable}
compare_tests! {fuzz,fuzz13,stable}
compare_tests! {fuzz,fuzz14,stable}
compare_tests! {fuzz,fuzz15,stable}
compare_tests! {fuzz,fuzz16,stable}
compare_tests! {fuzz,fuzz17,stable}
compare_tests! {fuzz,fuzz18,stable}
compare_tests! {fuzz,fuzz19,stable}

compare_tests! {fuzz,fuzz20,stable}
compare_tests! {fuzz,fuzz21,stable}
compare_tests! {fuzz,fuzz22,stable}
compare_tests! {fuzz,fuzz23,stable}
compare_tests! {fuzz,fuzz24,stable}
compare_tests! {fuzz,fuzz25,stable}
compare_tests! {fuzz,fuzz26,stable}
compare_tests! {fuzz,fuzz27,stable}
compare_tests! {fuzz,fuzz28,stable}
compare_tests! {fuzz,fuzz29,stable}

compare_tests! {fuzz,fuzz30,stable}
compare_tests! {fuzz,fuzz31,stable}
compare_tests! {fuzz,fuzz32,stable}
compare_tests! {fuzz,fuzz33,stable}
compare_tests! {fuzz,fuzz34,stable}
compare_tests! {fuzz,fuzz35,stable}
compare_tests! {fuzz,fuzz36,stable}
compare_tests! {fuzz,fuzz37,stable}
compare_tests! {fuzz,fuzz38,stable}
compare_tests! {fuzz,fuzz39,stable}

compare_tests! {fuzz,fuzz40,stable}
compare_tests! {fuzz,fuzz41,stable}
compare_tests! {fuzz,fuzz42,stable}
compare_tests! {fuzz,fuzz43,stable}
compare_tests! {fuzz,fuzz44,stable}
compare_tests! {fuzz,fuzz45,stable}
compare_tests! {fuzz,fuzz46,stable}
compare_tests! {fuzz,fuzz47,stable}
compare_tests! {fuzz,fuzz48,stable}
compare_tests! {fuzz,fuzz49,stable}

compare_tests! {fuzz,fuzz50,stable}
compare_tests! {fuzz,fuzz51,stable}
compare_tests! {fuzz,fuzz52,stable}
compare_tests! {fuzz,fuzz53,stable}
compare_tests! {fuzz,fuzz54,stable}
compare_tests! {fuzz,fuzz55,stable}
compare_tests! {fuzz,fuzz56,stable}
compare_tests! {fuzz,fuzz57,stable}
compare_tests! {fuzz,fuzz58,stable}
compare_tests! {fuzz,fuzz59,stable}

compare_tests! {fuzz,fuzz60,stable}
compare_tests! {fuzz,fuzz61,stable}
compare_tests! {fuzz,fuzz62,stable}
compare_tests! {fuzz,fuzz63,stable}
compare_tests! {fuzz,fuzz64,stable}
compare_tests! {fuzz,fuzz65,stable}
compare_tests! {fuzz,fuzz66,stable}
compare_tests! {fuzz,fuzz67,stable}
compare_tests! {fuzz,fuzz68,stable}
compare_tests! {fuzz,fuzz69,stable}

compare_tests! {fuzz,fuzz70,stable}
compare_tests! {fuzz,fuzz71,stable}
compare_tests! {fuzz,fuzz72,stable}
compare_tests! {fuzz,fuzz73,stable}
compare_tests! {fuzz,fuzz74,stable}
compare_tests! {fuzz,fuzz75,stable}
compare_tests! {fuzz,fuzz76,stable}
compare_tests! {fuzz,fuzz77,stable}
compare_tests! {fuzz,fuzz78,stable}
compare_tests! {fuzz,fuzz79,stable}

compare_tests! {fuzz,fuzz80,stable}
compare_tests! {fuzz,fuzz81,stable}
compare_tests! {fuzz,fuzz82,stable}
compare_tests! {fuzz,fuzz83,stable}
compare_tests! {fuzz,fuzz84,stable}
compare_tests! {fuzz,fuzz85,stable}
compare_tests! {fuzz,fuzz86,stable}
compare_tests! {fuzz,fuzz87,stable}
compare_tests! {fuzz,fuzz88,stable}
compare_tests! {fuzz,fuzz89,stable}

compare_tests! {fuzz,fuzz90,stable}
compare_tests! {fuzz,fuzz91,stable}
compare_tests! {fuzz,fuzz92,stable}
compare_tests! {fuzz,fuzz93,stable}
compare_tests! {fuzz,fuzz94,stable}
compare_tests! {fuzz,fuzz95,stable}
compare_tests! {fuzz,fuzz96,stable}
compare_tests! {fuzz,fuzz97,stable}
compare_tests! {fuzz,fuzz98,stable}
compare_tests! {fuzz,fuzz99,stable}
compare_tests! {fuzz,fuzz100,stable}
// Found later using an integrated version of Rustlantis
compare_tests! {fuzz,fuzz159,stable}

compare_tests! {fuzz,fuzz333,stable}
compare_tests! {fuzz,fuzz580,stable}

// Assembler issue:fuzz952
compare_tests! {fuzz,fuzz952,stable}
//compare_tests! {fuzz,fuzz4433,stable}

run_test! {fuzz,fail0,stable}
run_test! {fuzz,fail1,stable}
compare_tests! {fuzz,fail3,stable}
compare_tests! {fuzz,fail4,stable}
compare_tests! {fuzz,fail5,stable}
compare_tests! {fuzz,fail6,stable}
compare_tests! {fuzz,fail7,stable}
compare_tests! {fuzz,fail8,stable}

compare_tests! {fuzz,fail9,stable}
// TODO: fix this test. It is a NaN issue, so it is a very low prioity, but it should still get fixed or something.
compare_tests! {fuzz,fail10,stable}
compare_tests! {fuzz,fail11,stable}

cargo_test! {hello_world,stable}
cargo_test! {std_hello_world,stable}
cargo_test_ignored! {build_core}
cargo_test_ignored! {build_alloc}
cargo_test_ignored! {build_std}
cargo_test! {benchmarks,bench}
// TODO: This trips up some post-link sanity checks, investigate.
cargo_test! {glam_test,unstable}
cargo_test! {fastrand_test,stable}

#[cfg(target_os = "windows")]
const IS_DOTNET_PRESENT: &bool = &true;

#[cfg(not(target_os = "windows"))]
static IS_DOTNET_PRESENT: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::process::Command::new("dotnet").output().is_ok());
static IS_MONO_PRESENT: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::process::Command::new("mono").output().is_ok());

static RUSTC_BUILD_STATUS: std::sync::LazyLock<Result<(), String>> =
    std::sync::LazyLock::new(build_backend);

#[test]
fn emitted_compiler_object_is_repeatable_across_output_paths() {
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    let source = std::fs::canonicalize("test/types/deterministic_identities.rs")
        .expect("deterministic identity fixture is missing");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock predates the Unix epoch")
        .as_nanos();
    let output_dir = std::env::temp_dir().join(format!(
        "rustc_codegen_clr_identity_{}_{}",
        std::process::id(),
        nonce
    ));
    std::fs::create_dir(&output_dir).expect("could not create identity test directory");

    let compile = |stem: &str, perturb: bool| {
        let requested_output = output_dir.join(format!("{stem}.o"));
        let mut command = std::process::Command::new("rustc");
        command
            .arg(format!(
                "-Zcodegen-backend={}",
                absolute_backend_path().display()
            ))
            .args([
                "--edition",
                STANDALONE_TEST_EDITION,
                "--crate-name",
                "deterministic_identities",
                "--crate-type",
                "lib",
                "--emit",
                "obj",
                "-O",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&requested_output);
        if perturb {
            command.args(["--cfg", "identity_perturb"]);
        }
        let display = format!("{command:?}");
        let output = command
            .output()
            .expect("failed to compile deterministic identity fixture");
        assert_compile_succeeded(&display, &output);

        // Backend object emission is a serialized cilly assembly. rustc names it from the
        // requested output stem, so compare its contents rather than the deliberately different
        // archive member/output names.
        let emitted_object = output_dir.join(format!("{stem}..rcgu.bc"));
        std::fs::read(&emitted_object).unwrap_or_else(|err| {
            panic!(
                "could not read compiler object at {}: {err}",
                emitted_object.display()
            )
        })
    };

    let first = compile("first", false);
    let second = compile("second", false);
    assert_eq!(
        first, second,
        "compiler-emitted identities changed between identical builds"
    );

    let static_names = |bytes: &[u8]| {
        let artifact = cilly::decode_assembly_artifact(bytes)
            .expect("compiler object is not a current cilly assembly artifact");
        let assembly = artifact.assembly();
        assembly
            .class_defs()
            .values()
            .flat_map(|class| class.static_fields())
            .map(|field| assembly[field.name].to_string())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let baseline_names = static_names(&first);
    let baseline_methods = {
        let artifact = cilly::decode_assembly_artifact(&first)
            .expect("compiler object is not a current cilly assembly artifact");
        let assembly = artifact.assembly();
        assembly
            .method_defs()
            .values()
            .map(|method| assembly[method.name()].to_string())
            .collect::<std::collections::BTreeSet<_>>()
    };
    for exported in ["deterministic_identity_probe", "mutable_counter_probe"] {
        assert!(
            baseline_methods.contains(exported),
            "canonical instance naming did not preserve explicit #[no_mangle] symbol {exported:?}: {baseline_methods:#?}"
        );
    }
    let perturbed = compile("perturbed", true);
    let perturbed_names = static_names(&perturbed);

    let baseline_counter: Vec<_> = baseline_names
        .iter()
        .filter(|name| name.contains("MUTABLE_COUNTER"))
        .collect();
    assert_eq!(
        baseline_counter.len(),
        1,
        "the named mutable static must have exactly one backing field: {baseline_names:#?}"
    );
    assert!(
        perturbed_names.contains(baseline_counter[0]),
        "an unrelated promotion changed the mutable static's emitted identity"
    );
    assert!(
        !baseline_names.iter().any(|name| name.starts_with("mut_")),
        "a named `static mut` was incorrectly reified as an anonymous allocation: {baseline_names:#?}"
    );
    for name in baseline_names
        .iter()
        .filter(|name| name.starts_with("ro_") || name.starts_with("mut_"))
    {
        assert!(
            perturbed_names.contains(name),
            "unrelated promotion changed stable allocation field {name:?}"
        );
    }
    assert!(
        perturbed_names
            .iter()
            .filter(|name| name.starts_with("ro_"))
            .any(|name| !baseline_names.contains(name)),
        "the perturbation did not materialize its independent promoted allocation"
    );
    std::fs::remove_dir_all(&output_dir).expect("could not remove identity test directory");
}

#[test]
fn upstream_inline_instances_ignore_the_instantiating_crate_suffix() {
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    let support = std::fs::canonicalize("test/types/instance_identity_support.rs")
        .expect("instance identity support fixture is missing");
    let consumer = std::fs::canonicalize("test/types/instance_identity_consumer.rs")
        .expect("instance identity consumer fixture is missing");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock predates the Unix epoch")
        .as_nanos();
    let output_dir = std::env::temp_dir().join(format!(
        "rustc_codegen_clr_cross_crate_identity_{}_{}",
        std::process::id(),
        nonce
    ));
    std::fs::create_dir(&output_dir).expect("could not create instance identity test directory");

    let compile =
        |source: &std::path::Path, crate_name: &str, extern_crate: Option<&std::path::Path>| {
            let mut command = std::process::Command::new("rustc");
            command
                .arg(format!(
                    "-Zcodegen-backend={}",
                    absolute_backend_path().display()
                ))
                .args([
                    "--edition",
                    STANDALONE_TEST_EDITION,
                    "--crate-name",
                    crate_name,
                    "--crate-type",
                    "lib",
                    "--emit",
                    "metadata,obj",
                    "-O",
                ])
                .arg(source)
                .arg("--out-dir")
                .arg(&output_dir);
            if let Some(metadata) = extern_crate {
                command
                    .arg("--extern")
                    .arg(format!("instance_identity_support={}", metadata.display()));
            }
            let display = format!("{command:?}");
            let output = command
                .output()
                .expect("failed to compile cross-crate instance identity fixture");
            assert_compile_succeeded(&display, &output);
            std::fs::read(output_dir.join(format!("{crate_name}..rcgu.bc")))
                .expect("compiler did not emit the expected cilly object")
        };

    let defining = compile(&support, "instance_identity_support", None);
    let metadata = output_dir.join("libinstance_identity_support.rmeta");
    let consuming = compile(&consumer, "instance_identity_consumer", Some(&metadata));
    let probe_names = |bytes: &[u8]| {
        let artifact = cilly::decode_assembly_artifact(bytes)
            .expect("compiler object is not a current cilly assembly artifact");
        let assembly = artifact.assembly();
        assembly
            .method_defs()
            .values()
            .map(|method| assembly[method.name()].to_string())
            .filter(|name| {
                name.contains("instance_suffix_probe")
                    || name.contains("instance_unreachable_probe")
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    let defining_names = probe_names(&defining);
    let consuming_names = probe_names(&consuming);
    assert_eq!(
        defining_names.len(),
        2,
        "defining crate did not emit both inline probes: {defining_names:#?}"
    );
    assert_eq!(
        consuming_names, defining_names,
        "the same semantic upstream Instance acquired an instantiating-crate-specific managed name"
    );

    let (_, mut defining_assembly) = cilly::decode_assembly_artifact(&defining)
        .expect("defining object is not a current cilly artifact")
        .into_parts();
    let (_, consuming_assembly) = cilly::decode_assembly_artifact(&consuming)
        .expect("consuming object is not a current cilly artifact")
        .into_parts();
    defining_assembly
        .try_link_in_place(consuming_assembly)
        .expect("identical upstream method bodies must merge across defining/consuming crates");

    std::fs::remove_dir_all(&output_dir)
        .expect("could not remove instance identity test directory");
}

#[test]
fn global_asm_is_a_hard_codegen_error() {
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    let source = std::fs::canonicalize("test/compile_fail/global_asm.rs")
        .expect("global_asm compile-fail fixture is missing");
    let output_path = std::env::temp_dir().join(format!(
        "rustc_codegen_clr_global_asm_{}.rlib",
        std::process::id()
    ));
    let output = std::process::Command::new("rustc")
        // Global assembly has no sound throwing-stub replacement, so it must remain fatal even in
        // the explicitly requested exploratory recovery mode.
        .env("ABORT_ON_ERROR", "0")
        .arg(format!(
            "-Zcodegen-backend={}",
            absolute_backend_path().display()
        ))
        .args(["--edition", STANDALONE_TEST_EDITION, "--crate-type", "lib"])
        .arg(source)
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("failed to run global_asm compile-fail fixture");
    let _ = std::fs::remove_file(output_path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "global_asm unexpectedly compiled successfully"
    );
    assert!(
        stderr.contains("UnsupportedFeature") && stderr.contains("global_asm"),
        "global_asm failed without the structured unsupported diagnostic:\n{stderr}"
    );
}

#[test]
fn indirect_c_variadic_fn_pointer_is_rejected_before_calli() {
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    let source = std::fs::canonicalize("test/compile_fail/indirect_c_variadic.rs")
        .expect("indirect C-variadic compile-fail fixture is missing");
    let output_path = std::env::temp_dir().join(format!(
        "rustc_codegen_clr_indirect_c_variadic_{}.rlib",
        std::process::id()
    ));
    let output = std::process::Command::new("rustc")
        .env("ABORT_ON_ERROR", "1")
        .arg(format!(
            "-Zcodegen-backend={}",
            absolute_backend_path().display()
        ))
        .args([
            "--edition",
            STANDALONE_TEST_EDITION,
            "--crate-name",
            "indirect_c_variadic",
            "--crate-type",
            "lib",
        ])
        .arg(source)
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("failed to run indirect C-variadic compile-fail fixture");
    let _ = std::fs::remove_file(output_path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "indirect C-variadic function-pointer call unexpectedly compiled"
    );
    assert!(
        stderr.contains("UnsupportedFeature(indirect_c_variadic_fn_pointer)")
            && stderr.contains("requires a CIL vararg call-site signature"),
        "indirect C-variadic call failed without the exact structured diagnostic:\n{stderr}"
    );
    assert!(
        !stderr.contains("internal compiler error") && !stderr.contains("thread 'rustc' panicked"),
        "indirect C-variadic rejection became an ICE:\n{stderr}"
    );
}

#[test]
fn unsupported_target_layouts_are_rejected_at_codegen_entry() {
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    let source = std::fs::canonicalize("test/compile_fail/unsupported_target_layout.rs")
        .expect("unsupported-target compile-fail fixture is missing");

    for (target, expected_facts) in [
        ("i686-unknown-linux-gnu", "32-bit little-endian"),
        ("s390x-unknown-linux-gnu", "64-bit big-endian"),
    ] {
        let output_path = std::env::temp_dir().join(format!(
            "rustc_codegen_clr_unsupported_target_{}_{}.rlib",
            target,
            std::process::id()
        ));
        let output = std::process::Command::new("rustc")
            .arg(format!(
                "-Zcodegen-backend={}",
                absolute_backend_path().display()
            ))
            .args([
                "--edition",
                STANDALONE_TEST_EDITION,
                "--crate-name",
                "unsupported_target_layout",
                "--crate-type",
                "lib",
                "--target",
                target,
            ])
            .arg(&source)
            .arg("-o")
            .arg(&output_path)
            .output()
            .unwrap_or_else(|error| panic!("failed to compile for target {target}: {error}"));
        let _ = std::fs::remove_file(output_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "unsupported target {target} unexpectedly reached codegen"
        );
        assert!(
            stderr.contains("UnsupportedFeature(target_layout)")
                && stderr.contains("requires a 64-bit little-endian Rust target")
                && stderr.contains(expected_facts),
            "target {target} failed without the exact backend target-layout diagnostic:\n{stderr}"
        );
        assert!(
            !stderr.contains("internal compiler error")
                && !stderr.contains("thread 'rustc' panicked"),
            "target-layout rejection for {target} became an ICE:\n{stderr}"
        );
    }
}

#[test]
fn managed_references_are_rejected_from_rust_byte_storage() {
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    let support = std::fs::canonicalize("test/compile_fail/support/managed_storage_mycorrhiza.rs")
        .expect("managed-storage support crate is missing");
    let source = std::fs::canonicalize("test/compile_fail/managed_storage.rs")
        .expect("managed-storage compile-fail fixture is missing");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock predates the Unix epoch")
        .as_nanos();
    let output_dir = std::env::temp_dir().join(format!(
        "rustc_codegen_clr_managed_storage_{}_{}",
        std::process::id(),
        nonce
    ));
    std::fs::create_dir(&output_dir).expect("could not create managed-storage test directory");
    let support_rlib = output_dir.join("libmycorrhiza.rlib");
    let support_output = std::process::Command::new("rustc")
        .args([
            "--edition",
            STANDALONE_TEST_EDITION,
            "--crate-name",
            "mycorrhiza",
            "--crate-type",
            "rlib",
        ])
        .arg(&support)
        .arg("-o")
        .arg(&support_rlib)
        .output()
        .expect("failed to compile managed-storage support crate");
    assert_compile_succeeded("rustc managed_storage_mycorrhiza.rs", &support_output);

    let compile_case = |case: &str, support_rlib: &std::path::Path| {
        let output_path = output_dir.join(format!("{case}.rlib"));
        // Definition-boundary escape checks apply only to actual native-facing library exports.
        // Keep these cases product-shaped; a private `extern "C"` function in an rlib can instead
        // be a managed-only delegate trampoline.
        let crate_type = if matches!(
            case,
            "external_argument"
                | "external_return"
                | "external_safe_managed_marker"
                | "external_managed_marker"
                | "external_unsafe_no_managed_marker"
                | "external_marked_nonunwind"
        ) {
            "cdylib"
        } else {
            "lib"
        };
        let mut command = std::process::Command::new("rustc");
        command
            .env("ABORT_ON_ERROR", "1")
            .arg(format!(
                "-Zcodegen-backend={}",
                absolute_backend_path().display()
            ))
            .args([
                "--edition",
                STANDALONE_TEST_EDITION,
                "--crate-name",
                "managed_storage",
                "--crate-type",
                crate_type,
            ])
            .arg("--extern")
            .arg(format!("mycorrhiza={}", support_rlib.display()))
            .arg("--cfg")
            .arg(format!("managed_case=\"{case}\""))
            .arg(&source)
            .arg("-o")
            .arg(output_path);
        // A successful cdylib backend compile produces a serialized cilly object, not a native
        // Mach-O/ELF input. Stop after codegen for the positive marked-export case; the product
        // fixtures exercise the real managed linker and runtime path.
        if case == "external_managed_marker" {
            command.args(["--emit", "obj"]);
        }
        let display = format!("{command:?}");
        let output = command.output().unwrap_or_else(|error| {
            panic!("failed to compile managed-storage case {case}: {error}")
        });
        (display, output)
    };

    for case in [
        "array",
        "managed_struct_storage",
        "aggregate",
        "recursive_raw",
        "enum",
        "coroutine",
        "coroutine_closure",
        "closure",
        "box",
        "vec",
        "rc",
        "arc",
        "vecdeque",
        "static",
        "static_ref",
        "static_slice",
        "static_wrapped_ref",
        "anonymous_promotion",
        "copy",
        "write_bytes",
        "volatile_load",
        "volatile_store",
        "typed_swap",
        "indirect_write",
        "indirect_read",
        "transmute",
        "raw_pointer_formation",
        "reference_storage",
        "pointer_return",
        "external_argument",
        "external_return",
        "external_safe_managed_marker",
        "external_unsafe_no_managed_marker",
        "external_marked_nonunwind",
        "external_fn_pointer",
        "external_call_return",
        "rust_call_tuple_persist",
        "rust_call_tuple_borrow",
        "rust_call_tuple_copy",
        "try_managed_naked_capture",
        "try_managed_naked_result",
        "forged_root",
        "conditional_capability_raw",
        "lifetime_capability",
        "reference_capability",
        "container_reference_capability",
        "unsafe_binder",
    ] {
        let (_display, output) = compile_case(case, &support_rlib);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "managed-storage case {case} unexpectedly compiled successfully"
        );
        assert!(
            stderr.contains("managed_reference_storage")
                && (stderr.contains("UnsupportedFeature")
                    || stderr.contains("error: managed_reference_storage")),
            "managed-storage case {case} failed without the structured diagnostic:\n{stderr}"
        );
        assert!(
            !stderr.contains("internal compiler error")
                && !stderr.contains("unexpected panic")
                && !stderr.contains("thread 'rustc' panicked"),
            "managed-storage case {case} turned a structured rejection into an ICE:\n{stderr}"
        );
        let operation_fragment = match case {
            "copy" => Some("CopyNonOverlapping"),
            "write_bytes" => Some("intrinsic `write_bytes`"),
            "volatile_load" => Some("intrinsic `volatile_load`"),
            "volatile_store" => Some("intrinsic `volatile_store`"),
            "typed_swap" => Some("intrinsic `typed_swap_nonoverlapping`"),
            "indirect_write" | "indirect_read" => Some("indirect MIR place access"),
            "transmute" => Some("transmute source"),
            "raw_pointer_formation" => Some("raw pointer formation"),
            "pointer_return" => Some("function return pointer/reference escape"),
            "external_argument" => Some("external `\"C\"` function argument"),
            "external_return" => Some("external `\"C\"` function return"),
            "external_safe_managed_marker" => Some("external `\"C-unwind\"` function return"),
            "external_unsafe_no_managed_marker" => Some("external `\"C-unwind\"` function return"),
            "external_marked_nonunwind" => Some("external `\"C\"` function return"),
            "external_call_return" => Some("external `\"C\"` call return"),
            _ => None,
        };
        if let Some(operation_fragment) = operation_fragment {
            assert!(
                stderr.contains(operation_fragment),
                "managed-storage case {case} missed its operation-specific gate `{operation_fragment}`:\n{stderr}"
            );
        }
    }

    for case in [
        "transient",
        "rooted",
        "rooted_registration",
        "recursive_safe",
        "zero_array",
        "pointer_phantom",
        "safe_helper_near_miss",
        "unsafe_binder_primitive",
        "internal_fn_pointer",
        "private_c_abi_definition",
        "external_managed_marker",
        "rust_call_tuple",
        "try_managed_rooted_state",
        "value_type_receiver",
        "conditional_capability",
    ] {
        let (display, output) = compile_case(case, &support_rlib);
        assert_compile_succeeded(&display, &output);
    }

    // Compilation alone cannot distinguish the safe same-name helper from an accidentally
    // substituted stack transmute. Execute the exact crate/module/name/doc collision and compare
    // its observable `41 -> 42` result.
    if !crate::config::current().dry_run() {
        let helper_base = output_dir.join("safe_helper_near_miss");
        let helper_exe = helper_base.with_extension("exe");
        let mut command = std::process::Command::new("rustc");
        command
            .args(rustc_args().iter())
            .args([
                "-O",
                "--crate-name",
                "mycorrhiza",
                "--cfg",
                "safe_helper_binary",
            ])
            .arg(&support)
            .arg("-o")
            .arg(&helper_exe);
        let display = format!("{command:?}");
        let output = command
            .output()
            .expect("failed to compile executable safe-helper near miss");
        assert_compile_succeeded(&display, &output);
        let helper_base = helper_base
            .to_str()
            .expect("safe-helper output path is not UTF-8");
        let output_dir_str = output_dir
            .to_str()
            .expect("managed-storage output directory is not UTF-8");
        let stdout = test_dotnet_executable(helper_base, output_dir_str);
        assert!(
            stdout.is_empty(),
            "safe-helper oracle unexpectedly wrote output: {stdout:?}"
        );
    }

    // `ManagedRef::copy_raw` must be a non-consuming peek. Prove the exact lowered CIL shape:
    // both helpers recover the target through `handle_to_obj`, but only Take releases the root.
    let managed_box_source = std::fs::canonicalize("test/types/managed_box_get.rs")
        .expect("managed-box lowering fixture is missing");
    let requested_object = output_dir.join("managed_box_get.o");
    let mut command = std::process::Command::new("rustc");
    command
        .arg(format!(
            "-Zcodegen-backend={}",
            absolute_backend_path().display()
        ))
        .args([
            "--edition",
            STANDALONE_TEST_EDITION,
            "--crate-name",
            "managed_box_get",
            "--crate-type",
            "lib",
            "--emit",
            "obj",
        ])
        .arg("--extern")
        .arg(format!("mycorrhiza={}", support_rlib.display()))
        .arg(&managed_box_source)
        .arg("-o")
        .arg(&requested_object);
    let display = format!("{command:?}");
    let output = command
        .output()
        .expect("failed to compile managed-box lowering fixture");
    assert_compile_succeeded(&display, &output);
    let emitted_object = output_dir.join("managed_box_get..rcgu.bc");
    let bytes = std::fs::read(&emitted_object).unwrap_or_else(|error| {
        panic!(
            "could not read managed-box compiler object at {}: {error}",
            emitted_object.display()
        )
    });
    let artifact = cilly::decode_assembly_artifact(&bytes)
        .expect("managed-box fixture did not emit a current cilly assembly artifact");
    let assembly = artifact.assembly();
    let calls_in = |method_name: &str| {
        let method = assembly
            .method_defs()
            .values()
            .find(|method| &assembly[method.name()] == method_name)
            .unwrap_or_else(|| panic!("managed-box fixture omitted method {method_name}"));
        method
            .iter_cil(assembly)
            .expect("managed-box probe has no CIL body")
            .filter_map(|element| match element {
                cilly::CILIterElem::Node(cilly::CILNode::Call(call))
                | cilly::CILIterElem::Root(cilly::CILRoot::Call(call)) => Some(call.0),
                _ => None,
            })
            .map(|method_ref| assembly[assembly[method_ref].name()].to_string())
            .collect::<Vec<_>>()
    };
    let peek_calls = calls_in("managed_box_peek_probe");
    assert_eq!(
        peek_calls
            .iter()
            .filter(|name| *name == "handle_to_obj")
            .count(),
        1,
        "ManagedBoxGet must recover the rooted target exactly once: {peek_calls:?}"
    );
    assert_eq!(
        peek_calls
            .iter()
            .filter(|name| *name == "handle_free")
            .count(),
        0,
        "ManagedBoxGet must leave the GCHandle live: {peek_calls:?}"
    );
    let take_calls = calls_in("managed_box_take_probe");
    assert_eq!(
        take_calls
            .iter()
            .filter(|name| *name == "handle_to_obj")
            .count(),
        1,
        "ManagedBoxTake must recover the rooted target exactly once: {take_calls:?}"
    );
    assert_eq!(
        take_calls
            .iter()
            .filter(|name| *name == "handle_free")
            .count(),
        1,
        "ManagedBoxTake must release the GCHandle exactly once: {take_calls:?}"
    );
    let free_calls = calls_in("managed_box_free_probe");
    assert_eq!(
        free_calls
            .iter()
            .filter(|name| *name == "handle_to_obj")
            .count(),
        0,
        "ManagedBoxFree must not materialize the managed target: {free_calls:?}"
    );
    assert_eq!(
        free_calls
            .iter()
            .filter(|name| *name == "handle_free")
            .count(),
        1,
        "ManagedBoxFree must release the GCHandle exactly once: {free_calls:?}"
    );

    let nodes_in = |method_name: &str| {
        let method = assembly
            .method_defs()
            .values()
            .find(|method| &assembly[method.name()] == method_name)
            .unwrap_or_else(|| panic!("managed-box fixture omitted method {method_name}"));
        method
            .iter_cil(assembly)
            .expect("managed-box probe has no CIL body")
            .filter_map(cilly::CILIterElem::as_node)
            .collect::<Vec<_>>()
    };
    let array_new_nodes = nodes_in("managed_box_array_new_probe");
    assert!(
        !array_new_nodes
            .iter()
            .any(|node| matches!(node, cilly::CILNode::Box { .. })),
        "ManagedBoxNew must not box an already-managed CLR array: {array_new_nodes:?}"
    );
    let array_peek_nodes = nodes_in("managed_box_array_peek_probe");
    assert!(
        !array_peek_nodes
            .iter()
            .any(|node| matches!(node, cilly::CILNode::UnboxAny { .. })),
        "ManagedBoxGet must not unbox.any an already-managed CLR array: {array_peek_nodes:?}"
    );
    assert_eq!(
        array_peek_nodes
            .iter()
            .filter(|node| {
                matches!(
                    node,
                    cilly::CILNode::CheckedCast(_, target)
                        if matches!(assembly[*target], cilly::Type::PlatformArray { .. })
                )
            })
            .count(),
        1,
        "ManagedBoxGet must cast the rooted object back to its CLR array type: {array_peek_nodes:?}"
    );
    let value_new_nodes = nodes_in("managed_box_value_new_probe");
    assert_eq!(
        value_new_nodes
            .iter()
            .filter(|node| matches!(node, cilly::CILNode::Box { .. }))
            .count(),
        1,
        "ManagedBoxNew must still box CLR value types: {value_new_nodes:?}"
    );
    let value_peek_nodes = nodes_in("managed_box_value_peek_probe");
    assert_eq!(
        value_peek_nodes
            .iter()
            .filter(|node| matches!(node, cilly::CILNode::UnboxAny { .. }))
            .count(),
        1,
        "ManagedBoxGet must still unbox CLR value types: {value_peek_nodes:?}"
    );

    // A safe trait with the right diagnostic-item name is not an unsafe storage contract.
    let safe_trait_rlib = output_dir.join("libmycorrhiza_safe_trait.rlib");
    let safe_trait_support_output = std::process::Command::new("rustc")
        .args([
            "--edition",
            STANDALONE_TEST_EDITION,
            "--crate-name",
            "mycorrhiza",
            "--crate-type",
            "rlib",
            "--cfg",
            "forged_safe_capability",
        ])
        .arg(&support)
        .arg("-o")
        .arg(&safe_trait_rlib)
        .output()
        .expect("failed to compile safe-trait capability support crate");
    assert_compile_succeeded(
        "rustc managed_storage_mycorrhiza.rs --cfg forged_safe_capability",
        &safe_trait_support_output,
    );
    let (_display, output) = compile_case("forged_root", &safe_trait_rlib);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() && stderr.contains("managed_reference_storage"),
        "safe capability trait unexpectedly bypassed managed storage:\n{stderr}"
    );

    // Likewise, a safe foundational raw-type trait does not authorize CLR ABI interpretation.
    // The array case is a decisive oracle: it compiles only when the copied marker remains an
    // ordinary Rust struct.
    let safe_raw_identity_rlib = output_dir.join("libmycorrhiza_safe_raw_identity.rlib");
    let safe_raw_identity_output = std::process::Command::new("rustc")
        .args([
            "--edition",
            STANDALONE_TEST_EDITION,
            "--crate-name",
            "mycorrhiza",
            "--crate-type",
            "rlib",
            "--cfg",
            "forged_raw_identity",
        ])
        .arg(&support)
        .arg("-o")
        .arg(&safe_raw_identity_rlib)
        .output()
        .expect("failed to compile safe raw-identity support crate");
    assert_compile_succeeded(
        "rustc managed_storage_mycorrhiza.rs --cfg forged_raw_identity",
        &safe_raw_identity_output,
    );
    let (display, output) = compile_case("raw_identity_near_miss", &safe_raw_identity_rlib);
    assert_compile_succeeded(&display, &output);

    // A diagnostic item is only an identifier, not proof that its definition has the expected
    // kind. A replacement crate that binds the name to a struct must be rejected structurally,
    // never passed to `tcx.trait_def` (which would ICE on a non-trait DefId).
    let non_trait_rlib = output_dir.join("libmycorrhiza_non_trait.rlib");
    let non_trait_support_output = std::process::Command::new("rustc")
        .args([
            "--edition",
            STANDALONE_TEST_EDITION,
            "--crate-name",
            "mycorrhiza",
            "--crate-type",
            "rlib",
            "--cfg",
            "forged_capability_item",
        ])
        .arg(&support)
        .arg("-o")
        .arg(&non_trait_rlib)
        .output()
        .expect("failed to compile non-trait capability support crate");
    assert_compile_succeeded(
        "rustc managed_storage_mycorrhiza.rs --cfg forged_capability_item",
        &non_trait_support_output,
    );
    let (_display, output) = compile_case("forged_root", &non_trait_rlib);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "non-trait capability diagnostic item unexpectedly bypassed managed storage"
    );
    assert!(
        stderr.contains("managed_reference_storage")
            && (stderr.contains("UnsupportedFeature")
                || stderr.contains("error: managed_reference_storage")),
        "non-trait capability item failed without the structured diagnostic:\n{stderr}"
    );
    std::fs::remove_dir_all(output_dir).expect("could not remove managed-storage test directory");
}

/// Public metadata must preserve the exact marker on magic functions instantiated through an
/// external `mycorrhiza` rlib. A private declaration loses its attributes at this boundary and
/// silently lowers as a call to the aborting Rust placeholder instead of the managed built-in.
#[test]
fn cross_crate_magic_intrinsics_use_managed_builtins() {
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    let mycorrhiza =
        std::fs::canonicalize("mycorrhiza/src/lib.rs").expect("mycorrhiza crate root is missing");
    let source = std::fs::canonicalize("test/types/try_catch_cross_crate.rs")
        .expect("cross-crate try/catch fixture is missing");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock predates the Unix epoch")
        .as_nanos();
    let output_dir = std::env::temp_dir().join(format!(
        "rustc_codegen_clr_try_catch_metadata_{}_{}",
        std::process::id(),
        nonce
    ));
    std::fs::create_dir(&output_dir).expect("could not create try/catch metadata test directory");
    let mycorrhiza_rlib = output_dir.join("libmycorrhiza.rlib");
    let output = std::process::Command::new("rustc")
        .args([
            "--edition",
            STANDALONE_TEST_EDITION,
            "--crate-name",
            "mycorrhiza",
            "--crate-type",
            "rlib",
        ])
        .arg(&mycorrhiza)
        .arg("-o")
        .arg(&mycorrhiza_rlib)
        .output()
        .expect("failed to compile the real mycorrhiza rlib");
    assert_compile_succeeded("rustc mycorrhiza/src/lib.rs", &output);

    let requested_object = output_dir.join("try_catch_cross_crate.o");
    let mut command = std::process::Command::new("rustc");
    command
        .arg(format!(
            "-Zcodegen-backend={}",
            absolute_backend_path().display()
        ))
        .args([
            "--edition",
            STANDALONE_TEST_EDITION,
            "--crate-name",
            "try_catch_cross_crate",
            "--crate-type",
            "lib",
            "--emit",
            "obj",
        ])
        .arg("--extern")
        .arg(format!("mycorrhiza={}", mycorrhiza_rlib.display()))
        .arg(&source)
        .arg("-o")
        .arg(&requested_object);
    let display = format!("{command:?}");
    let output = command
        .output()
        .expect("failed to compile cross-crate try/catch fixture");
    assert_compile_succeeded(&display, &output);

    let emitted_object = output_dir.join("try_catch_cross_crate..rcgu.bc");
    let bytes = std::fs::read(&emitted_object).unwrap_or_else(|error| {
        panic!(
            "could not read cross-crate try/catch object at {}: {error}",
            emitted_object.display()
        )
    });
    let artifact = cilly::decode_assembly_artifact(&bytes)
        .expect("cross-crate try/catch fixture did not emit a current cilly artifact");
    let assembly = artifact.assembly();
    let method_names = assembly
        .method_refs()
        .iter_keys()
        .map(|method| assembly[assembly[method].name()].to_string())
        .collect::<Vec<_>>();
    assert!(
        method_names.iter().any(|name| name == "interop_try_catch"),
        "cross-crate try_managed did not lower to the managed try/catch built-in: {method_names:?}"
    );
    for placeholder in [
        "rustc_clr_interop_try_catch",
        "rustc_clr_interop_managed_box_new",
        "rustc_clr_interop_managed_box_get",
        "rustc_clr_interop_managed_box_take",
        "rustc_clr_interop_managed_box_free",
    ] {
        assert!(
            method_names.iter().all(|name| !name.contains(placeholder)),
            "cross-crate lowering retained aborting Rust placeholder `{placeholder}`: {method_names:?}"
        );
    }
    std::fs::remove_dir_all(output_dir)
        .expect("could not remove try/catch metadata test directory");
}

/// The codegen backend owns rustc's `cfg(target_feature)` result. Keep that frontend contract
/// aligned with the x86-64 ABI and prove that explicit feature settings are parsed rather than
/// silently discarded. This catches representation-splitting failures in crates such as `wide`
/// before they surface as ecosystem build errors.
#[test]
fn target_feature_cfg_contract() {
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    let backend = absolute_backend_path();
    let target = std::fs::canonicalize("x86_64-unknown-dotnet.json")
        .expect("x86_64 dotnet target spec is missing");

    let query = |feature: Option<&str>| {
        let mut command = std::process::Command::new("rustc");
        command
            .arg(format!("-Zcodegen-backend={}", backend.display()))
            .args(["-Zunstable-options", "--print", "cfg", "--target"])
            .arg(&target);
        if let Some(feature) = feature {
            command.arg(format!("-Ctarget-feature={feature}"));
        }
        let output = command
            .output()
            .expect("failed to query backend target cfg");
        assert!(
            output.status.success(),
            "target cfg query failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        (
            String::from_utf8(output.stdout).expect("target cfg stdout was not UTF-8"),
            String::from_utf8(output.stderr).expect("target cfg stderr was not UTF-8"),
        )
    };

    let (baseline, baseline_stderr) = query(None);
    for feature in ["fxsr", "sse", "sse2", "x87"] {
        assert!(
            baseline.contains(&format!("target_feature=\"{feature}\"")),
            "mandatory x86-64 feature `{feature}` missing from:\n{baseline}"
        );
    }
    assert!(
        !baseline_stderr.contains("must be enabled to ensure that the ABI"),
        "backend published an invalid baseline ABI:\n{baseline_stderr}"
    );

    let (with_avx2, _) = query(Some("+avx2"));
    assert!(
        with_avx2.contains("target_feature=\"avx2\""),
        "explicitly enabled target feature was ignored:\n{with_avx2}"
    );

    let (without_sse2, disabled_stderr) = query(Some("-sse2"));
    assert!(
        !without_sse2.contains("target_feature=\"sse2\""),
        "explicitly disabled target feature was ignored:\n{without_sse2}"
    );
    assert!(
        disabled_stderr.contains("must be enabled to ensure that the ABI"),
        "disabling mandatory SSE2 did not produce rustc's ABI diagnostic:\n{disabled_stderr}"
    );

    let target_json = std::fs::read_to_string(target).expect("could not read target spec");
    assert!(
        !target_json.contains("\"stack-probes\""),
        ".NET must not inherit native x86 inline stack probes"
    );
}

static RUSTC_CODEGEN_CLR_LINKER: std::sync::LazyLock<PathBuf> = std::sync::LazyLock::new(|| {
    let _ = *RUSTC_BUILD_STATUS;
    if cfg!(debug_assertions) {
        // `linker` is a bin of the `cilly` package, so `-p cilly` is required — a bare
        // `cargo build --bin linker` from the workspace root does not reliably produce it on a
        // clean target. Fail loudly if the build doesn't succeed (it used to fail silently).
        let out = std::process::Command::new("cargo")
            .args(["build", "-p", "cilly", "--bin", "linker"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "failed to build the `linker` bin:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        //TODO: Fix this for other platforms
        if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
            std::fs::canonicalize("target/debug/linker").unwrap()
        } else if cfg!(target_os = "windows") {
            std::fs::canonicalize("target/debug/linker.exe").unwrap()
        } else {
            panic!("Unsupported target OS");
        }
    } else {
        // `linker` is a bin of the `cilly` package, so `-p cilly` is required — a bare
        // `cargo build --bin linker` from the workspace root does not reliably produce it on a
        // clean target. Fail loudly if the build doesn't succeed (it used to fail silently).
        let out = std::process::Command::new("cargo")
            .args(["build", "-p", "cilly", "--bin", "linker", "--release"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "failed to build the `linker` bin:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        //TODO: Fix this for other platforms
        if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
            std::fs::canonicalize("target/release/linker").unwrap()
        } else if cfg!(target_os = "windows") {
            std::fs::canonicalize("target/release/linker.exe").unwrap()
        } else {
            panic!("Unsupported target OS");
        }
    }
});
/// A list of arguments needed for invoking `rustc` with this backend included.
#[must_use]
pub fn rustc_args() -> Box<[String]> {
    if crate::config::current().randomize_layout() {
        [
            "-Z".to_owned(),
            backend_path(),
            "-C".to_owned(),
            format!("linker={}", RUSTC_CODEGEN_CLR_LINKER.display()),
            "-Z".to_owned(),
            "randomize-layout".to_owned(),
            "--edition".to_owned(),
            STANDALONE_TEST_EDITION.to_owned(),
        ]
        .into()
    } else {
        [
            "-Z".to_owned(),
            backend_path(),
            "-C".to_owned(),
            format!("linker={}", RUSTC_CODEGEN_CLR_LINKER.display()),
            "--edition".to_owned(),
            STANDALONE_TEST_EDITION.to_owned(),
        ]
        .into()
    }
}
/// Flags that need to be passed to cargo in order to build a project with this linker.
#[must_use]
pub fn cargo_build_env() -> String {
    RUSTC_BUILD_STATUS.as_ref().expect("Could not build rustc!");
    let backend = absolute_backend_path();
    let backend = backend.display().to_string();
    let linker = RUSTC_CODEGEN_CLR_LINKER.display().to_string();
    let link_args = "--cargo-support";
    let radomize_layout = if crate::config::current().randomize_layout() {
        "-Z randomize-layout"
    } else {
        ""
    };

    format!(
        "-Z codegen-backend={backend} -C linker={linker} -C link-args={link_args}   {radomize_layout}"
    )
}
