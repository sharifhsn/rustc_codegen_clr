//! The `linker` binary (invoked as `-C linker=` by the rustc backend): loads each crate's
//! serialized `cilly` `Assembly` from its rlib (`load.rs`), merges them into one program via
//! `asm_link`, patches in libc/intrinsic implementations referenced but never defined by
//! rustc-generated code via a `MissingMethodPatcher` (`patch.rs`), optionally runs Native AOT
//! (`aot.rs`), then hands the result to the direct PE exporter to emit the final `.NET` executable.
#![deny(unused_must_use)]
#![allow(clippy::module_name_repetitions)]
use cilly::{
    DEAD_CODE_ELIMINATION,
    libc_fns::{self, LIBC_FNS, LIBC_MODIFIES_ERRNO},
    {
        ArtifactAbiConfig, ArtifactAbiConfigMismatch, Assembly, BasicBlock, CILRoot, ClassDef,
        ClassRef, Const, DotnetRuntime, Int, MethodImpl, Type, asm::MissingMethodPatcher,
        cilnode::MethodKind,
    },
};
mod load;
mod patch;
use fxhash::FxHashMap;
use patch::call_alias;
use std::{
    collections::HashMap,
    env,
    ffi::{OsStr, OsString},
    io::Write,
    num::NonZeroU32,
    path::{Path, PathBuf},
};

fn effective_abi_config(
    artifact_config: Option<ArtifactAbiConfig>,
    process_config: ArtifactAbiConfig,
) -> Result<ArtifactAbiConfig, ArtifactAbiConfigMismatch> {
    match artifact_config {
        Some(artifact_config) => {
            artifact_config.ensure_compatible(&process_config)?;
            Ok(artifact_config)
        }
        None => Ok(process_config),
    }
}

/// Process-local final-link policy, parsed once and never serialized into crate artifacts.
#[derive(Clone, Debug, Eq, PartialEq)]
struct LinkerConfig {
    process_abi: ArtifactAbiConfig,
    guaranteed_align: u8,
    pool_alloc: bool,
    panic_managed_backtrace: bool,
    managed_identity: Option<ManagedIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ManagedIdentity {
    assembly_name: String,
    module_full_name: Option<String>,
}

impl LinkerConfig {
    fn capture() -> Result<Self, String> {
        const SETTINGS: &[&str] = &[
            "ASCI_IDENT",
            "DOTNET_VERSION",
            "GUARANTEED_ALIGN",
            "MAX_STATIC_SIZE",
            "RCL_MANAGED_ASSEMBLY_NAME",
            "RCL_MANAGED_IDENTITY_SCHEMA",
            "RCL_MANAGED_MODULE_TYPE",
            "RCL_MANAGED_PACKAGE_ID",
            "RCL_MANAGED_ROOT_NAMESPACE",
            "NO_UNWIND",
            "PANIC_MANAGED_BT",
            "POOL_ALLOC",
        ];
        let snapshot: HashMap<OsString, OsString> = std::env::vars_os().collect();
        let environment: HashMap<String, String> = SETTINGS
            .iter()
            .filter_map(|name| snapshot.get(OsStr::new(name)).map(|value| (*name, value)))
            .map(|(name, value)| {
                value
                    .to_str()
                    .map(|value| (name.to_owned(), value.to_owned()))
                    .ok_or_else(|| format!("environment setting {name} is not valid Unicode"))
            })
            .collect::<Result<_, _>>()?;
        Self::from_environment(&environment)
    }

    fn from_environment(environment: &HashMap<String, String>) -> Result<Self, String> {
        for (retired, replacement) in [
            ("ASCI_IDENT", Some("ASCII_IDENTS")),
            ("JS_MODE", None),
            ("MAX_STATIC_SIZE", None),
        ] {
            if environment.contains_key(retired) {
                return Err(match replacement {
                    Some(replacement) => {
                        format!("retired setting {retired}; use {replacement} instead")
                    }
                    None => format!("retired no-op setting {retired}; remove it"),
                });
            }
        }

        let guaranteed_align = linker_number(environment, "GUARANTEED_ALIGN", 8_u8)?;
        if !guaranteed_align.is_power_of_two() {
            return Err(format!(
                "GUARANTEED_ALIGN must be a nonzero power of two, found {guaranteed_align}"
            ));
        }

        let managed_identity = managed_identity_from_environment(environment)?;
        Ok(Self {
            process_abi: ArtifactAbiConfig::from_environment(environment)
                .map_err(|error| error.to_string())?,
            guaranteed_align,
            pool_alloc: linker_bool(environment, "POOL_ALLOC", false)?,
            panic_managed_backtrace: linker_bool(environment, "PANIC_MANAGED_BT", false)?,
            managed_identity,
        })
    }
}

fn managed_identity_from_environment(
    environment: &HashMap<String, String>,
) -> Result<Option<ManagedIdentity>, String> {
    const KEYS: &[&str] = &[
        "RCL_MANAGED_IDENTITY_SCHEMA",
        "RCL_MANAGED_PACKAGE_ID",
        "RCL_MANAGED_ASSEMBLY_NAME",
        "RCL_MANAGED_ROOT_NAMESPACE",
        "RCL_MANAGED_MODULE_TYPE",
    ];
    if !KEYS.iter().any(|key| environment.contains_key(*key)) {
        return Ok(None);
    }
    let value = |key: &str| {
        environment
            .get(key)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("managed identity is incomplete: missing {key}"))
    };
    if value("RCL_MANAGED_IDENTITY_SCHEMA")? != "1" {
        return Err("managed identity schema must be 1".into());
    }
    // Carry package-id through this boundary even though this narrow exporter slice does not yet
    // use it as a filename. It prevents a partial caller from accidentally treating arbitrary
    // linker environment as a complete release identity.
    let _package_id = value("RCL_MANAGED_PACKAGE_ID")?;
    let assembly_name = value("RCL_MANAGED_ASSEMBLY_NAME")?.clone();
    let root_namespace = value("RCL_MANAGED_ROOT_NAMESPACE")?;
    let module_type = value("RCL_MANAGED_MODULE_TYPE")?;
    Ok(Some(ManagedIdentity {
        assembly_name,
        module_full_name: Some(format!("{root_namespace}.{module_type}")),
    }))
}

fn linker_bool(
    environment: &HashMap<String, String>,
    variable: &'static str,
    default: bool,
) -> Result<bool, String> {
    match environment.get(variable).map(String::as_str) {
        None => Ok(default),
        Some("0" | "false" | "False" | "FALSE") => Ok(false),
        Some("1" | "true" | "True" | "TRUE") => Ok(true),
        Some(value) => Err(format!(
            "boolean environment setting {variable} has invalid value {value:?}; expected 0/1 or false/true"
        )),
    }
}

fn linker_number<T>(
    environment: &HashMap<String, String>,
    variable: &'static str,
    default: T,
) -> Result<T, String>
where
    T: std::str::FromStr,
{
    environment.get(variable).map_or(Ok(default), |value| {
        value
            .parse()
            .map_err(|_| format!("environment setting {variable} has invalid value {value:?}"))
    })
}

fn add_mandatory_statics(asm: &mut cilly::Assembly) {
    let main_module = asm.main_module();
    asm.add_static(
        cilly::Type::Int(cilly::Int::U8),
        "__rust_alloc_error_handler_should_panic",
        false,
        main_module,
        None,
        false,
    );
    asm.add_static(
        cilly::Type::Int(cilly::Int::U8),
        "__rust_no_alloc_shim_is_unstable",
        false,
        main_module,
        None,
        false,
    );
}
static LIBC: std::sync::LazyLock<String> = std::sync::LazyLock::new(get_libc_);
static LIBM: std::sync::LazyLock<String> = std::sync::LazyLock::new(get_libm_);
static BACKUP_STD: std::sync::LazyLock<Option<PathBuf>> = std::sync::LazyLock::new(|| {
    std::env::vars()
        .filter_map(|(key, value)| {
            if key == "BACKUP_STD" {
                Some(value.into())
            } else {
                None
            }
        })
        .next()
});
#[cfg(target_os = "linux")]
fn get_libc_() -> String {
    // Let CoreCLR apply its platform DllImport resolution. Embedding the build host's absolute
    // glibc path makes a NuGet assembly unusable on Alpine/musl or another Linux distribution.
    "libc".to_string()
}
#[cfg(target_os = "linux")]
fn get_libm_() -> String {
    "libm".to_string()
}
#[cfg(target_os = "windows")]
fn get_libc_() -> String {
    "msvcrt.dll".to_string()
}
#[cfg(target_os = "windows")]
fn get_libm_() -> String {
    "ucrtbase.dll".to_string()
}
#[cfg(target_os = "macos")]
fn get_libc_() -> String {
    // `libc` is a portable .NET native-library contract; CoreCLR resolves it to libSystem on macOS
    // and the appropriate C runtime on Linux instead of baking the build host into the assembly.
    "libc".to_string()
}
#[cfg(target_os = "macos")]
fn get_libm_() -> String {
    "libm".to_string()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn get_out_path(args: &[String]) -> &str {
    find_out_path(args).unwrap_or_else(|| panic!("No output file! {args:?}"))
}
#[cfg(target_os = "windows")]
fn get_out_path(args: &[String]) -> &str {
    // The custom target uses the GNU linker protocol even when the linker executable itself is a
    // Windows binary, so rustc passes `-o PATH`. Accept MSVC `/OUT:PATH` as well for direct use.
    find_out_path(args).unwrap_or_else(|| panic!("No output file! {args:?}"))
}

fn find_out_path(args: &[String]) -> Option<&str> {
    args.iter()
        .find_map(|arg| arg.strip_prefix("/OUT:"))
        .or_else(|| {
            args.windows(2)
                .find(|pair| pair[0] == "-o")
                .map(|pair| pair[1].as_str())
        })
}

#[cfg(test)]
mod output_path_tests {
    use super::find_out_path;

    #[test]
    fn accepts_gnu_and_msvc_output_spellings() {
        let gnu = vec!["input.bc".into(), "-o".into(), r"D:\build\app".into()];
        let msvc = vec!["input.bc".into(), r"/OUT:D:\build\app.exe".into()];
        assert_eq!(find_out_path(&gnu), Some(r"D:\build\app"));
        assert_eq!(find_out_path(&msvc), Some(r"D:\build\app.exe"));
    }
}
fn link_dir(path: &Path, ar_to_link: &mut Vec<String>) {
    let dir = std::fs::read_dir(path).unwrap();
    for entry in dir {
        let entry = entry.unwrap();
        let metadata = entry.metadata().unwrap();
        if metadata.is_file() && entry.file_name().to_str().unwrap().contains(".rlib") {
            ar_to_link.push(entry.path().to_str().unwrap().to_owned());
            eprintln!("Linking file {:?}.", entry.file_name());
        }
    }
}
// Links a prebuilt std if none present
fn link_backup_std(to_link: &[&String], ar_to_link: &mut Vec<String>, backup: &Path) {
    if !to_link.iter().chain(to_link).any(|linkable| {
        linkable.contains("std") | linkable.contains("core") | linkable.contains("alloc")
    }) {
        link_dir(backup, ar_to_link);
    }
}
fn main() {
    let linker_config = LinkerConfig::capture()
        .unwrap_or_else(|error| panic!("invalid linker configuration: {error}"));
    // Parse command line arguments

    let args: Vec<String> = env::args().collect();
    let args = &args[1..];
    // Input\/output files
    let to_link: Vec<_> = args
        .iter()
        .filter(|arg| arg.contains(".bc") || arg.contains(".cilly"))
        .collect();
    let mut ar_to_link: Vec<_> = args
        .iter()
        .filter(|arg| arg.contains(".rlib"))
        .cloned()
        .collect();

    if let Some(backup_std) = BACKUP_STD.as_ref() {
        link_backup_std(&to_link[..], &mut ar_to_link, backup_std);
    }
    //ar_to_link.extend(link_dir_files(args));
    let out_path = get_out_path(args);
    // Configs

    let cargo_support = args.iter().any(|arg| arg.contains("--cargo-support"));

    // Load assemblies from files

    let loaded = load::load_assemblies_with_config(to_link.as_slice(), ar_to_link.as_slice());
    let (mut final_assembly, artifact_abi_config) = loaded.into_parts();
    final_assembly
        .validate_fixed_array_layouts()
        .unwrap_or_else(|error| panic!("post-link {error}"));
    let effective_abi_config =
        effective_abi_config(artifact_abi_config, linker_config.process_abi.clone())
            .unwrap_or_else(|error| {
                panic!(
                    "linker process ABI does not match the serialized artifact contract: \
                 {error}"
                )
            });
    println!("==> Artifact ABI: {effective_abi_config}");
    println!("==> Linker configuration: {linker_config:?}");
    let no_unwind = effective_abi_config.no_unwind();
    let pool_alloc = linker_config.pool_alloc;
    let panic_managed_backtrace = linker_config.panic_managed_backtrace;
    let path: std::path::PathBuf = out_path.into();

    let is_lib = out_path.contains(".dll") || out_path.contains(".so") || out_path.contains(".o");

    let mut externs: FxHashMap<_, _> = LIBC_FNS
        .iter()
        .map(|fn_name| (*fn_name, LIBC.to_string()))
        .collect();

    let modifies_errno = LIBC_MODIFIES_ERRNO.iter().copied().collect();
    if let Some(f128_support) = libc_fns::f128_support_lib() {
        let f128_support = f128_support.to_str().to_owned().unwrap();
        externs.extend(
            libc_fns::F128_SYMBOLS
                .iter()
                .map(|fn_name| (*fn_name, f128_support.to_owned())),
        );
    }
    let mathf = LIBM.to_owned();
    externs.extend(
        libc_fns::LIBM_FNS
            .iter()
            .map(|fn_name| (*fn_name, mathf.to_owned())),
    );
    externs.extend(
        libc_fns::GB_FNS
            .iter()
            .map(|fn_name| (*fn_name, "gameboy".to_owned())),
    );
    let mut overrides = MissingMethodPatcher::default();
    overrides.insert(
        final_assembly.alloc_string("pthread_atfork"),
        Box::new(|_, asm: &mut Assembly| {
            let ret_val = asm.alloc_node(Const::I32(0));
            let blocks = vec![BasicBlock::new(
                vec![asm.alloc_root(CILRoot::Ret(ret_val))],
                1,
                None,
            )];
            MethodImpl::MethodBody {
                blocks,
                locals: vec![],
            }
        }),
    );
    if no_unwind {
        cilly::builtins::insert_exeception_stub(&mut final_assembly, &mut overrides);
    } else {
        cilly::builtins::insert_exception(&mut final_assembly, &mut overrides);
    };
    // Override allocator
    // Get the marshal class
    let marshal = ClassRef::marshal(&mut final_assembly);
    // Overrides calls to malloc
    let sig = final_assembly.sig([Type::Int(Int::ISize)], Type::Int(Int::ISize));
    let allochglobal =
        final_assembly.new_methodref(marshal, "AllocHGlobal", sig, MethodKind::Static, []);
    let mref = final_assembly[allochglobal].clone();
    call_alias(&mut overrides, &mut final_assembly, "malloc", mref);
    // Overrides calls to realloc
    let sig = final_assembly.sig(
        [Type::Int(Int::ISize), Type::Int(Int::ISize)],
        Type::Int(Int::ISize),
    );
    let realloc =
        final_assembly.new_methodref(marshal, "ReAllocHGlobal", sig, MethodKind::Static, []);
    let mref = final_assembly[realloc].clone();
    call_alias(&mut overrides, &mut final_assembly, "realloc", mref);
    // Overrides calls to free
    let sig = final_assembly.sig([Type::Int(Int::ISize)], Type::Void);
    let allochglobal =
        final_assembly.new_methodref(marshal, "FreeHGlobal", sig, MethodKind::Static, []);
    let mref = final_assembly[allochglobal].clone();
    call_alias(&mut overrides, &mut final_assembly, "free", mref);
    // Throw side of the panic ↔ managed-exception bridge: override `_Unwind_RaiseException` to throw a
    // `RustException` (the catch side is `insert_exception`/`insert_catch_unwind` above). Only for the
    // .NET path and only when unwinding is enabled — `NO_UNWIND` installs the ctor-less exception stub.
    if !panic_managed_backtrace && !no_unwind {
        cilly::builtins::unwind::raise_exception(&mut final_assembly, &mut overrides);
    }
    // Managed assemblies have no DWARF FDE for libunwind to inspect. Preserve the program
    // counter for std's symbol-address lookup, matching Rust's own fallback on targets where
    // `_Unwind_FindEnclosingFunction` is unavailable or unreliable. This capability is needed
    // independently of whether Rust panic unwinding itself is enabled.
    cilly::builtins::unwind::find_enclosing_function(&mut final_assembly, &mut overrides);
    cilly::builtins::unwind::get_cfa(&mut final_assembly, &mut overrides);
    cilly::builtins::unwind::get_ip(&mut final_assembly, &mut overrides);
    cilly::builtins::unwind::backtrace_end_of_stack(&mut final_assembly, &mut overrides);
    // `std_detect` may retain LLVM's native XCR0 reader in debug builds. Managed code cannot
    // execute `xgetbv`; report no extended state so feature detection remains conservative.
    cilly::builtins::x86::xgetbv_unavailable(&mut final_assembly, &mut overrides);
    // The matching AVX transition instruction only affects native register state; CIL has no
    // exposed upper-vector state, so preserve the intrinsic's Rust-visible no-op semantics.
    cilly::builtins::x86::vzeroupper_noop(&mut final_assembly, &mut overrides);

    overrides.insert(
        final_assembly.alloc_string("_Unwind_DeleteException"),
        Box::new(|_, asm| {
            let ret = asm.alloc_root(CILRoot::VoidRet);
            let blocks = vec![BasicBlock::new(vec![ret], 0, None)];
            MethodImpl::MethodBody {
                blocks,
                locals: vec![],
            }
        }),
    );
    overrides.insert(
        final_assembly.alloc_string("fork"),
        Box::new(|_, asm| {
            let ret_val = asm.alloc_node(Const::I32(-1));
            let blocks = vec![BasicBlock::new(
                vec![asm.alloc_root(CILRoot::Ret(ret_val))],
                0,
                None,
            )];
            MethodImpl::MethodBody {
                blocks,
                locals: vec![],
            }
        }),
    );
    overrides.insert(
        final_assembly.alloc_string("__cxa_thread_atexit_impl"),
        Box::new(|_, asm| {
            let blocks = vec![BasicBlock::new(
                vec![asm.alloc_root(CILRoot::VoidRet)],
                0,
                None,
            )];
            MethodImpl::MethodBody {
                blocks,
                locals: vec![],
            }
        }),
    );
    cilly::builtins::select::generate_int_selects(&mut final_assembly, &mut overrides);
    cilly::builtins::insert_swap_at_generic(&mut final_assembly, &mut overrides);
    cilly::builtins::insert_bounds_check(&mut final_assembly, &mut overrides);
    cilly::builtins::unaligned_read(&mut final_assembly, &mut overrides);

    cilly::builtins::casts::insert_casts(&mut final_assembly, &mut overrides);
    let unity_netstandard =
        effective_abi_config.dotnet_runtime() == DotnetRuntime::UnityNetStandard21;
    cilly::builtins::insert_heap(
        &mut final_assembly,
        &mut overrides,
        pool_alloc,
        unity_netstandard,
    );
    cilly::builtins::rust_assert(&mut final_assembly, &mut overrides);
    cilly::builtins::int128::generate_int128_ops(&mut final_assembly, &mut overrides);
    cilly::builtins::int128::i128_mul_ovf_check(&mut final_assembly, &mut overrides);
    cilly::builtins::int128::u128_mul_ovf_check(&mut final_assembly, &mut overrides);
    cilly::builtins::int_to_float::insert_int_to_float(&mut final_assembly, &mut overrides);
    cilly::builtins::int128::generate_x86_wide_carry(&mut final_assembly, &mut overrides);
    cilly::builtins::f16::generate_f16_ops(&mut final_assembly, &mut overrides);
    cilly::builtins::atomics::generate_all_atomics(
        &mut final_assembly,
        &mut overrides,
        !unity_netstandard,
    );
    cilly::builtins::transmute(&mut final_assembly, &mut overrides);
    cilly::builtins::create_slice(&mut final_assembly, &mut overrides);
    cilly::builtins::ovf_check_tuple(&mut final_assembly, &mut overrides);
    cilly::builtins::uninit_val(&mut final_assembly, &mut overrides);

    cilly::builtins::math::bitreverse(&mut final_assembly, &mut overrides);

    cilly::builtins::instert_threading(&mut final_assembly, &mut overrides);
    cilly::builtins::math::math(&mut final_assembly, &mut overrides);
    cilly::builtins::simd::simd(&mut final_assembly, &mut overrides);

    cilly::builtins::argc_argv_init(&mut final_assembly, &mut overrides);
    // .NET PAL BCL bindings (rcl_dotnet_alloc / _free / _write) used by the
    // std-side dotnet PAL. .NET-only: they emit calls into the BCL.
    cilly::builtins::dotnet::insert_dotnet_pal(
        &mut final_assembly,
        &mut overrides,
        pool_alloc,
        unity_netstandard,
    );
    // POSIX/libc-over-.NET shim (the proof slice): int-fd⇄GCHandle fd-table +
    // thread-local errno + the bare POSIX C-ABI symbol cluster (socket/read/
    // epoll_*/…), each re-packaging an existing rcl_dotnet_* body. .NET-only;
    // additive (os=dotnet symbols + overrides), so ::stable is untouched. The
    // fd-table MethodDefs are defined here; fixed-point missing-method resolution below
    // resolves the wrappers' forward refs to them. See
    // cilly/src/ir/builtins/posix.rs and docs/LIBC_SHIM_SCOPE.md.
    cilly::builtins::posix::insert_posix_shim(&mut final_assembly, &mut overrides);

    // Ensure the cctor and tcctor exist!
    let _ = final_assembly.tcctor();
    let _ = final_assembly.cctor();
    let float128 = final_assembly.alloc_string("f128");
    let low = final_assembly.alloc_string("low");
    let high = final_assembly.alloc_string("high");
    final_assembly
        .class_def(ClassDef::new(
            float128,
            true,
            0,
            None,
            vec![
                (Type::Int(Int::U64), low, Some(0)),
                (Type::Int(Int::U64), high, Some(8)),
            ],
            vec![],
            cilly::Access::Public,
            NonZeroU32::new(16),
            NonZeroU32::new(16),
            true,
        ))
        .unwrap();

    let resolution = final_assembly
        .try_resolve_missing_methods(&externs, &modifies_errno, &overrides)
        .unwrap_or_else(|error| panic!("missing-method resolution failed: {error}"));
    println!("==> Missing-method resolution: {resolution}");
    if resolution.unresolved_missing_methods != 0 {
        println!(
            "linker: deferring {} unresolved non-abstract MethodImpl::Missing runtime stub(s); \
             any that remain reachable after dead-code elimination will fail final verification",
            resolution.unresolved_missing_methods
        );
    }

    add_mandatory_statics(&mut final_assembly);

    // Mandatory CFG cleanup is independent of the optional optimizer. rustc's monomorphization
    // collector can omit a generic callee hidden behind a const-false branch; roots after the
    // already-unconditional transfer must be removed before call-graph DCE sees them.
    final_assembly.canonicalize_control_flow();

    if *DEAD_CODE_ELIMINATION {
        println!("==> Eliminating dead code");
        let dce_start = std::time::Instant::now();
        final_assembly.eliminate_dead_code();
        println!("==> Eliminating dead code in {:?}", dce_start.elapsed());
    }
    let mut fuel = final_assembly.fuel_from_env().fraction(0.25);
    let opt_start = std::time::Instant::now();
    final_assembly.opt(&mut fuel);
    println!("==> Optimizing in {:?}", opt_start.elapsed());
    final_assembly.eliminate_dead_code();
    if let Some(public_type_name) = linker_config
        .managed_identity
        .as_ref()
        .and_then(|identity| identity.module_full_name.as_deref())
    {
        let hidden = final_assembly.hide_main_module_implementation_details();
        if hidden != 0 {
            println!("==> Hid {hidden} internal MainModule methods from the public CLR API");
        }
        let projected = final_assembly.project_main_module_exports(public_type_name);
        println!(
            "==> Projected {projected} managed export(s) onto public facade {public_type_name}"
        );
        // Facade projection demotes the original MainModule exports to assembly visibility.
        // Recompute reachability now so implementation types that were roots only before the
        // projection do not leak into constrained consumers such as UnityLinker/IL2CPP.
        final_assembly.eliminate_dead_code_after_facade_projection(public_type_name);
    }
    let (mut final_assembly, compaction) = final_assembly.compact();
    if std::env::var("RCL_LINK_STATS").as_deref() == Ok("1") {
        println!("==> Compaction: {compaction}");
    }
    final_assembly.fix_alignment(linker_config.guaranteed_align);
    let final_assembly = final_assembly.verify_for_export().unwrap_or_else(|error| {
        panic!(
            "final post-link verification failed after patching, DCE, optimization, compaction, \
             and alignment: {error}"
        )
    });
    final_assembly
        .save_tmp(&mut std::fs::File::create(path.with_extension("cilly2")).unwrap())
        .unwrap();
    {
        // Hand-rolled ECMA-335 PE writer (`cilly::pe_exporter`) — bypasses `ilasm` entirely.
        //
        // Output-path convention mirrors `ILExporter::export` exactly (see that function): a
        // library's `.dll` bytes land at `path` itself; an executable's bytes land at
        // `path.with_extension("exe")` (the legacy launcher-loaded artifact).
        let asm_name = if is_lib {
            linker_config
                .managed_identity
                .as_ref()
                .map(|identity| identity.assembly_name.clone())
                .unwrap_or_else(|| {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.strip_prefix("lib").unwrap_or(s).to_string())
                        .unwrap_or_else(|| "rust_export".to_string())
                })
        } else {
            // Executable names are path-loaded, so use a name-agnostic placeholder.
            // (loaded by path, name irrelevant); `pe_exporter::ExportOptions` always needs a
            // concrete name, so use the same placeholder text.
            "_".to_string()
        };
        let exe_out = if is_lib {
            std::path::absolute(&path).unwrap()
        } else {
            std::path::absolute(path.with_extension("exe")).unwrap()
        };
        if let Err(err) = std::fs::remove_file(&exe_out) {
            match err.kind() {
                std::io::ErrorKind::NotFound => (),
                _ => panic!("Could not remove tmp file because {err:?}"),
            }
        }
        // `Module.Name` (§II.22.30) must be the OUTPUT file's own basename, not the assembly
        // identity (`asm_name`, `"_"` for an executable) — `ilasm`, given no explicit `.module`
        // directive (`il_exporter` never emits one), defaults the `Module` table's name to its
        // `-output:` argument's filename. Conflating the two here previously stamped
        // `Module.Name = "_"` for every executable, which `AssemblyLoadContext.InternalLoad`'s
        // native path rejects with a `FileLoadException` naming the mismatched assembly `"_"`
        // (`0x8007000C`), before the CLI-aware managed loader (or even
        // `System.Reflection.Metadata`'s reader, which accepts the file with zero errors) ever
        // sees it — see `ExportOptions::module_name`'s doc for the full root-cause writeup.
        let module_name = exe_out
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| asm_name.clone());
        // Portable PDB (Phase 2, `docs/PE_EMISSION_PLAN.md`): mirrors the ilasm path's
        // `{output_file_path}.pdb` convention (`bootstrap_source`'s `pdb_file` computation below).
        //
        // The RSDS-embedded path (this `pdb_file_name`) must be the filename `System.Reflection.
        // Metadata.PortableExecutable.PEReader.TryOpenAssociatedPortablePdb` will ACTUALLY try to
        // open — confirmed via that method's decompiled source: it resolves the candidate path as
        // `Combine(dllDirectory, GetFileName(codeViewData.Path))`, i.e. the RSDS payload's OWN
        // filename, not a fixed `<dll-stem>.pdb` convention this writer could just assume.
        //
        // For a LIBRARY, cargo-dotnet promotes Cargo's host-shaped `libfoo.so` output to the
        // managed public artifact `<assembly-name>.dll`. The PDB travels with that public DLL, so
        // the RSDS name must use the managed assembly identity rather than `exe_out`'s temporary
        // `lib*.so` stem. Otherwise a C# host can parse the copied PDB but CoreCLR looks for the
        // nonexistent `libfoo.pdb` beside `foo.dll` and emits no Rust file:line frames.
        //
        // For a `cargo_support` EXECUTABLE, `path`/`exe_out` are BOTH the linker's OWN `-o`
        // argument — cargo's internal, hash-suffixed `deps/<crate>-<hash>` name (e.g.
        // `cd_pdb-ee896e2c3711274b`), NOT the final artifact name cargo copies it to afterward
        // (`cd_pdb`, no hash — `dotnet_jumpstart.rs`'s launcher already handles this correctly for
        // the DLL bytes themselves via `current_exe().with_extension("dll")`, resolved dynamically
        // at RUNTIME, but the PDB is written ONCE at BUILD TIME to a fixed path, so it can't use
        // that same trick). A real bug caught during Phase 2 acceptance testing: using `path`'s own
        // hashed stem here embedded an RSDS path (`cd_pdb-<hash>.pdb`) that never exists on disk
        // post-cargo-copy (only `cd_pdb.pdb` does) — `TryOpenAssociatedPortablePdb` tried to open
        // exactly that nonexistent hashed name and silently returned `false`.
        //
        // Fix: prefer `CARGO_CRATE_NAME` (set by cargo on every `rustc`/linker invocation it
        // drives — confirmed present here; this is cargo's own source of truth for what the final,
        // unhashed artifact will be named) for the executable case; fall back to `path`'s own stem
        // (correct for the library case, and a reasonable degradation for any non-cargo caller
        // that never sets the env var, e.g. `bin/rustflags.rs`'s manual usage — such a caller has
        // no cargo-driven copy step to begin with, so `path`'s own name IS the final one there).
        let pdb_stem = if is_lib {
            Some(asm_name.clone())
        } else {
            std::env::var("CARGO_CRATE_NAME").ok()
        };
        let pdb_file_name = match pdb_stem {
            Some(stem) => format!("{stem}.pdb"),
            None => path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| format!("{s}.pdb"))
                .unwrap_or_else(|| format!("{module_name}.pdb")),
        };
        let pe_options = cilly::pe_exporter::export::ExportOptions {
            is_dll: is_lib,
            assembly_name: asm_name,
            // Managed identities are materialized as a real facade TypeDef during final linking.
            // Keeping the old metadata-only MainModule rename here would collide with that facade
            // and put implementation methods back on the public type.
            public_module_full_name: None,
            module_name,
            pdb_file_name: pdb_file_name.clone(),
            runtime: effective_abi_config.dotnet_runtime(),
        };
        let source_link_json = std::env::var("RCL_SOURCE_LINK_JSON")
            .ok()
            .filter(|value| !value.is_empty());
        let (bytes, pdb_bytes) = final_assembly
            .render_pe_with_source_link(&pe_options, source_link_json.as_deref())
            .unwrap_or_else(|error| panic!("direct-PE emission failed: {error}"));
        std::fs::write(&exe_out, bytes).unwrap();
        if !pdb_bytes.is_empty() {
            std::fs::write(exe_out.with_file_name(&pdb_file_name), pdb_bytes).unwrap();
        }
        // Mirrors the ilasm branch exactly (see its own `cargo_support && !is_lib` block just
        // below): a library IS the final artifact at `path`, no launcher needed. An executable's
        // real bytes live at `path.with_extension("exe")` (above), so cargo/`rustc`'s own
        // expected output path (`path`, no extension on macOS/Linux) needs a tiny native
        // (non-cg_clr) launcher that execs the `.exe` — otherwise cargo's `--emit link` output
        // file never appears and downstream tooling (e.g. `cargo dotnet run`'s artifact scan)
        // finds nothing at `path`.
        if cargo_support && !is_lib {
            let bootstrap = bootstrap_source(
                &path.with_extension("exe"),
                path.to_str().unwrap(),
                "dotnet",
                Some(&pdb_file_name),
                true,
                effective_abi_config.dotnet_runtime(),
            );
            let bootstrap_path = path.with_extension("rs");
            let mut bootstrap_file = std::fs::File::create(&bootstrap_path).unwrap();
            bootstrap_file.write_all(bootstrap.as_bytes()).unwrap();
            // See the identical comment on the ilasm branch's launcher build below for why only
            // the backend-selecting env vars are stripped (not the whole environment).
            let out = std::process::Command::new("rustc")
                .arg("-O")
                .arg(bootstrap_path)
                .arg("-o")
                .arg(out_path)
                .env_remove("RUSTFLAGS")
                .env_remove("CARGO_ENCODED_RUSTFLAGS")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "bootstrap launcher compilation failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    // Direct PE is the sole managed emitter.
    //todo!();
}
/// `pdb_file_override` is the exact PDB filename when the caller already knows it; otherwise the
/// output stem is used. `pdb_capable` controls whether the launcher carries PDB metadata.
fn bootstrap_source(
    fpath: &Path,
    output_file_path: &str,
    jumpstart_cmd: &str,
    pdb_file_override: Option<&str>,
    pdb_capable: bool,
    runtime: DotnetRuntime,
) -> String {
    if let Err(err) = std::fs::remove_file(output_file_path) {
        match err.kind() {
            std::io::ErrorKind::NotFound => (),
            _ => {
                panic!("Could not remove tmp file because {err:?}")
            }
        }
    };
    let pdb_file = match pdb_file_override {
        Some(name) => name.to_string(),
        None => format!(
            "{output_file_path}.pdb",
            output_file_path = fpath.file_stem().unwrap().to_string_lossy()
        ),
    };
    // Both modern ilasm and the direct-PE writer place a PDB in `fpath`'s directory when they
    // produced debug info. Check actual existence because ilasm's PDB writer can fail and fall
    // back silently for giant assemblies. `pdb_capable` is supplied by the exporter branch so this
    // shared packaging code never probes ILAsm on direct-PE paths.
    let has_pdb = pdb_capable && fpath.with_file_name(&pdb_file).exists();
    format!(
        include_str!("dotnet_jumpstart.rs"),
        jumpstart_cmd = jumpstart_cmd,
        // The launcher consumes the same immutable contract that was validated against every
        // input artifact before link-time mutation began.
        tfm = runtime.tfm(),
        framework_version = runtime.framework_version(),
        exec_file = fpath.file_name().unwrap().to_string_lossy(),
        has_pdb = has_pdb,
        pdb_file = if pdb_capable { pdb_file } else { String::new() },
    )
}

#[cfg(test)]
mod linker_config_tests {
    use super::*;

    #[test]
    fn versioned_artifact_abi_must_match_linker_process() {
        let artifact = ArtifactAbiConfig::default();
        let process = ArtifactAbiConfig::default()
            .with_dotnet_runtime(DotnetRuntime::UnityNetStandard21)
            .with_no_unwind(true);

        let error = effective_abi_config(Some(artifact), process).unwrap_err();
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("dotnet_runtime: expected Net10, found UnityNetStandard21"));
        assert!(diagnostic.contains("no_unwind: expected false, found true"));
    }

    #[test]
    fn all_legacy_inputs_use_the_linker_process_snapshot() {
        let process = ArtifactAbiConfig::default()
            .with_dotnet_runtime(DotnetRuntime::Net10)
            .with_no_unwind(true);

        assert_eq!(
            effective_abi_config(None, process.clone()).unwrap(),
            process
        );
    }

    #[test]
    fn guaranteed_align_must_be_a_nonzero_power_of_two() {
        for invalid in ["0", "3"] {
            let environment = HashMap::from([("GUARANTEED_ALIGN".to_owned(), invalid.to_owned())]);
            let error = LinkerConfig::from_environment(&environment).unwrap_err();
            assert!(error.contains("nonzero power of two"), "{error}");
        }
    }
}
