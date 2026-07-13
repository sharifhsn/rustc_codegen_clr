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
    if crate::config::current().c_mode() {
        let out = std::process::Command::new("timeout")
            .current_dir(test_dir)
            .arg("-v")
            .arg("1")
            .arg(exec_path)
            .output()
            .expect("failed to run test program!");
        let stderr = String::from_utf8(out.stderr).expect("stderr is not UTF8 String!");
        assert!(
            stderr.is_empty(),
            "Test program failed with message {stderr:}"
        );
        return String::from_utf8_lossy(&out.stdout).to_string();
    }

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
        let runtime_config = cilly::il_exporter::get_runtime_config(
            crate::config::current().artifact_abi().dotnet_runtime(),
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

run_test! {intrinsics,bswap,stable}
run_test! {intrinsics,caller_location,stable}
run_test! {intrinsics,catch,stable}
run_test! {intrinsics,cmp_bytes,stable}
run_test! {intrinsics,copy_nonoverlaping,stable}
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
