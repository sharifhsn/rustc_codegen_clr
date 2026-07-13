//! Declarative, idempotent PAL injection into the toolchain's `rust-src`.
//!
//! This is the Rust-native re-architecture of the bash core's `inject_arm` /
//! `inject_arm_anchor` / `inject_method` / `inject_libc` zoo (the worst accidental
//! complexity in `feasibility/_cargo_dotnet_core.sh`, lines ~78-624). Instead of ~14
//! one-liner arm strings scattered through a shell script, ANCHOR-keyed `sed`/`awk`
//! mutations, and a GNU-vs-BSD `sed -i` portability shim, the injection is now:
//!
//!   * a DECLARATIVE [`manifest`] — a `Vec` of [`Target`]s (a rust-src-relative file +
//!     its [`Injection`]s), each carrying the bash's rationale as a Rust doc/`//`
//!     comment;
//!   * ONE idempotent ENGINE ([`apply_one`]) — read file; if the per-arm MARKER is
//!     already present, SKIP (the re-runnable guarantee, byte-for-byte the bash
//!     grep-for-marker guard); else splice after the ANCHOR (erroring loudly if the
//!     anchor is missing — surfacing nightly drift immediately, like the bash `!!
//!     anchor not found`); write atomically (tmp + rename);
//!   * a unit-TESTED core (fixture in -> fixture out per `Injection` variant, plus an
//!     idempotency test and an anchor-missing-errors test).
//!
//! ANCHORS over ORDINALS: the bash keyed ~22 cfg_select! arms by "which 1-based
//! cfg_select! block", which drifts every time upstream inserts a block (the
//! thread_local `destructors`-shift saga that left `guard::enable` undefined). Here
//! ONE injection ([`exit.rs` nth=2]) is the documented [`Anchor::Ordinal`] exception;
//! everything else is [`Anchor::After`].
//!
//! Rust string ops are platform-agnostic, so the BSD/GNU `sed -i` shim and the
//! `perl -0`/`tr`/`paste`/`mktemp` text-processing all vanish — a real win for the
//! macOS native path.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::context::Context as Ctx;

/// Where a `CfgArm` is inserted. Anchors are STRONGLY preferred; the single
/// `Ordinal` use is documented at its call site (exit.rs).
#[derive(Debug, Clone)]
pub enum Anchor {
    /// Insert into the FIRST `cfg_select! {` that appears at or after a line
    /// containing this fixed string. Robust to block-count drift across nightlies.
    After(String),
    /// Insert into the Nth (1-based) `cfg_select! {` in the file. The documented
    /// exception: exit.rs's two cfg_select!s have no distinguishing nearby string.
    Ordinal(usize),
}

/// A single source mutation. Each variant is idempotent (guarded on `marker`).
#[derive(Debug, Clone)]
pub enum Injection {
    /// Insert `target_os = "dotnet" => { body }` as the FIRST arm of a cfg_select!.
    /// `body` is the arm body (one or more lines, no surrounding braces). `marker`
    /// is a fixed substring whose presence means "already injected" (skip).
    CfgArm {
        anchor: Anchor,
        body: String,
        marker: String,
    },
    /// Insert a `#[cfg(target_os="dotnet")] #[stable] pub fn dotnet_raw_handle(...)`
    /// inherent method right after the `impl X {` line. (The mio handle accessor.)
    Method { impl_anchor: String, marker: String },
    /// Insert verbatim `lines` immediately BEFORE the first line containing `before`.
    LineInsert {
        before: String,
        lines: Vec<String>,
        marker: String,
    },
    /// Literal find/replace (no regex). `find` must occur exactly; replaced with
    /// `with`. `marker` guards idempotency (the post-replace text contains it).
    Replace {
        find: String,
        with: String,
        marker: String,
    },
}

/// A rust-src-relative file plus the injections it receives. `rel` is relative to the
/// root the [`Target`] is grouped under (see [`Root`]).
pub struct Target {
    pub rel: &'static str,
    pub injections: Vec<Injection>,
}

/// The five source trees the engine drives. `rel_paths` resolve against the root, so
/// the SAME engine handles std/sys, the std crate root, panic_unwind, unwind, libc.
#[derive(Debug, Clone, Copy)]
pub enum Root {
    /// `…/library/std/src/sys`
    Sys,
    /// `…/library/std/src` (the os/, net/, build.rs keystones above sys/)
    Std,
    /// `…/library/panic_unwind/src`
    PanicUnwind,
    /// `…/library/unwind/src`
    Unwind,
}

/// The outcome of applying ONE injection (for tests + logging).
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    Inserted,
    Skipped,
}

// ===========================================================================
// THE ENGINE
// ===========================================================================

/// Apply one [`Injection`] to `text`, returning the new text + whether it changed.
/// Pure (no I/O) so it is directly fixture-testable. Idempotent: if `marker` is
/// already present, returns `(text, Skipped)`. Errors loudly if the anchor is missing.
pub fn apply_one_str(text: &str, inj: &Injection) -> Result<(String, Applied)> {
    let marker = injection_marker(inj);
    if text.contains(marker) {
        return Ok((text.to_string(), Applied::Skipped));
    }
    let out = match inj {
        Injection::CfgArm { anchor, body, .. } => splice_cfg_arm(text, anchor, body)?,
        Injection::Method {
            impl_anchor,
            marker,
        } => splice_method(text, impl_anchor, marker)?,
        Injection::LineInsert { before, lines, .. } => splice_line_insert(text, before, lines)?,
        Injection::Replace { find, with, .. } => splice_replace(text, find, with)?,
    };
    Ok((out, Applied::Inserted))
}

fn injection_marker(inj: &Injection) -> &str {
    match inj {
        Injection::CfgArm { marker, .. }
        | Injection::Method { marker, .. }
        | Injection::LineInsert { marker, .. }
        | Injection::Replace { marker, .. } => marker,
    }
}

/// Insert `target_os = "dotnet" => { body }` as the first arm of the targeted
/// cfg_select!. The body is indented 8 spaces (matching the bash output shape).
fn splice_cfg_arm(text: &str, anchor: &Anchor, body: &str) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let target_idx = find_cfg_select(&lines, anchor)?;
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 4);
    for (i, line) in lines.iter().enumerate() {
        out.push((*line).to_string());
        if i == target_idx {
            out.push("    target_os = \"dotnet\" => {".to_string());
            for bl in body.lines() {
                out.push(format!("        {bl}"));
            }
            out.push("    }".to_string());
        }
    }
    Ok(join_preserving_trailing_newline(text, &out))
}

/// Locate the line index of the `cfg_select! {` an [`Anchor`] selects.
fn find_cfg_select(lines: &[&str], anchor: &Anchor) -> Result<usize> {
    let is_cfg = |l: &str| l.contains("cfg_select! {");
    match anchor {
        Anchor::After(a) => {
            let mut armed = false;
            for (i, l) in lines.iter().enumerate() {
                if l.contains(a.as_str()) {
                    armed = true;
                }
                if armed && is_cfg(l) {
                    return Ok(i);
                }
            }
            bail!("PAL inject: anchor {a:?} not found before any cfg_select! (rustc-src drift?)")
        }
        Anchor::Ordinal(n) => {
            let mut blk = 0usize;
            for (i, l) in lines.iter().enumerate() {
                if is_cfg(l) {
                    blk += 1;
                    if blk == *n {
                        return Ok(i);
                    }
                }
            }
            bail!(
                "PAL inject: cfg_select! ordinal {n} not found (only {blk} present — rustc-src drift?)"
            )
        }
    }
}

/// Insert the dotnet_raw_handle accessor right after the `impl X {` anchor line.
fn splice_method(text: &str, impl_anchor: &str, marker: &str) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let idx = lines
        .iter()
        .position(|l| l.contains(impl_anchor))
        .with_context(|| {
            format!("PAL inject: impl anchor {impl_anchor:?} not found (rustc-src drift?)")
        })?;
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 5);
    for (i, line) in lines.iter().enumerate() {
        out.push((*line).to_string());
        if i == idx {
            out.push(format!("    {marker}"));
            out.push("    #[cfg(target_os = \"dotnet\")]".to_string());
            out.push("    #[stable(feature = \"rust1\", since = \"1.0.0\")]".to_string());
            out.push("    #[allow(missing_docs)]".to_string());
            out.push(
                "    pub fn dotnet_raw_handle(&self) -> *mut u8 { self.0.dotnet_raw_handle() }"
                    .to_string(),
            );
        }
    }
    Ok(join_preserving_trailing_newline(text, &out))
}

/// Insert `lines` immediately before the first line containing `before`.
fn splice_line_insert(text: &str, before: &str, ins: &[String]) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let idx = lines
        .iter()
        .position(|l| l.contains(before))
        .with_context(|| {
            format!("PAL inject: line-insert anchor {before:?} not found (rustc-src drift?)")
        })?;
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + ins.len());
    for (i, line) in lines.iter().enumerate() {
        if i == idx {
            for l in ins {
                out.push(l.clone());
            }
        }
        out.push((*line).to_string());
    }
    Ok(join_preserving_trailing_newline(text, &out))
}

/// Literal single-occurrence replace.
fn splice_replace(text: &str, find: &str, with: &str) -> Result<String> {
    if !text.contains(find) {
        bail!("PAL inject: replace target not found: {find:?} (rustc-src drift?)");
    }
    Ok(text.replacen(find, with, 1))
}

/// Rejoin lines, preserving whether the original ended with a newline.
fn join_preserving_trailing_newline(orig: &str, lines: &[String]) -> String {
    let mut s = lines.join("\n");
    if orig.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Apply one injection to a FILE on disk (read; apply; atomic write if changed).
/// Missing files are a hard error EXCEPT when `optional` (the bash `[ -f … ] || return 0`
/// guard for arms whose PAL source may be absent — here all manifest files exist).
pub fn apply_one(file: &Path, inj: &Injection) -> Result<Applied> {
    let text = fs::read_to_string(file)
        .with_context(|| format!("PAL inject: cannot read {}", file.display()))?;
    let (out, applied) =
        apply_one_str(&text, inj).with_context(|| format!("PAL inject into {}", file.display()))?;
    if applied == Applied::Inserted {
        atomic_write(file, &out)?;
    }
    Ok(applied)
}

/// Write atomically (tmp in the same dir + rename), like the bash `f.__t && mv`.
fn atomic_write(file: &Path, content: &str) -> Result<()> {
    let tmp = file.with_extension("__cd_tmp");
    fs::write(&tmp, content).with_context(|| format!("PAL inject: write {}", tmp.display()))?;
    fs::rename(&tmp, file)
        .with_context(|| format!("PAL inject: rename onto {}", file.display()))?;
    Ok(())
}

/// Mirror every file under `src` into `dst` (clean base each run, like the bash mirror
/// loop). Creates parent dirs. `dst` files are overwritten.
pub fn mirror_tree(src: &Path, dst: &Path) -> Result<usize> {
    let mut n = 0;
    mirror_tree_rec(src, src, dst, &mut n)?;
    Ok(n)
}

fn mirror_tree_rec(root: &Path, dir: &Path, dst_root: &Path, n: &mut usize) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            mirror_tree_rec(root, &path, dst_root, n)?;
        } else if path.is_file() {
            let rel = path.strip_prefix(root).unwrap();
            let dst = dst_root.join(rel);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("mkdir -p {}", parent.display()))?;
            }
            fs::copy(&path, &dst)
                .with_context(|| format!("cp {} -> {}", path.display(), dst.display()))?;
            *n += 1;
        }
    }
    Ok(())
}

// ===========================================================================
// THE MANIFEST — the contract, one entry per bash injection (rationale carried).
// ===========================================================================

fn arm(body: &str, marker: &str, anchor: Anchor) -> Injection {
    Injection::CfgArm {
        anchor,
        body: body.to_string(),
        marker: marker.to_string(),
    }
}

/// CfgArm whose marker IS its body (the common `mod dotnet; …` arms — the body string
/// is unique enough to double as the idempotency marker, exactly as the bash keyed on
/// `grep -qF "$2"` the arm body).
fn arm_blk1(body: &str) -> Injection {
    arm(body, body, Anchor::After("cfg_select! {".to_string()))
}

/// The std/sys manifest (`Root::Sys`). `rel` is relative to `…/std/src/sys`.
fn sys_targets() -> Vec<Target> {
    vec![
        // pal/mod.rs: declare + re-export the dotnet pal module.
        Target { rel: "pal/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use self::dotnet::*;")] },
        // alloc: the dotnet allocator (managed heap).
        Target { rel: "alloc/mod.rs", injections: vec![arm_blk1("mod dotnet;")] },
        Target { rel: "stdio/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::*;")] },
        Target { rel: "args/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::*;")] },
        Target { rel: "env/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::*;")] },
        Target { rel: "random/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::*;")] },
        Target { rel: "io/error/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::*;")] },
        // time/mod.rs uses the `mod X; use X as imp;` cascade shape (re-exports
        // imp::{Instant,SystemTime,UNIX_EPOCH}); dotnet backs them with Stopwatch/DateTime.
        Target { rel: "time/mod.rs", injections: vec![arm_blk1("mod dotnet; use dotnet as imp;")] },
        // thread/mod.rs: plain `pub use dotnet::*` cascade; System.Threading.Thread.
        Target { rel: "thread/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::*;")] },
        // fs/mod.rs: `mod X; use X as imp;` shape. The arm body is WIDENED for the
        // target-family=unix flip: it imports the dotnet with_native_path (shadowing
        // the dropped free fn) and re-exports the unix cascade's chown family etc.
        // These extra use/pub-use lines are harmless with families unset and
        // load-bearing under the flip.
        Target {
            rel: "fs/mod.rs",
            injections: vec![
                arm_blk1(
                    "mod dotnet; use dotnet as imp; #[cfg(target_family = \"unix\")] use dotnet::with_native_path; #[cfg(target_family = \"unix\")] pub use dotnet::{chown, fchown, lchown, mkfifo, chroot}; #[cfg(target_family = \"unix\")] pub(crate) use dotnet::debug_assert_fd_is_open;",
                ),
                // set_permissions_nofollow: route dotnet to the unimplemented! arm. The
                // real-impl gate excludes dotnet; the stub gate includes it. (raw O_NOFOLLOW
                // can't be expressed by the FileStream model — I4.)
                Injection::Replace {
                    find: "#[cfg(all(unix, not(target_os = \"vxworks\")))]".to_string(),
                    with: "#[cfg(all(unix, not(target_os = \"vxworks\"), not(target_os = \"dotnet\")))]".to_string(),
                    marker: "not(target_os = \"vxworks\"), not(target_os = \"dotnet\")".to_string(),
                },
                Injection::Replace {
                    find: "#[cfg(any(not(unix), target_os = \"vxworks\"))]".to_string(),
                    with: "#[cfg(any(not(unix), target_os = \"vxworks\", target_os = \"dotnet\"))]".to_string(),
                    marker: "target_os = \"vxworks\", target_os = \"dotnet\"".to_string(),
                },
            ],
        },
        // net/connection/mod.rs: the net IMPL cascade (TcpStream/TcpListener/UdpSocket
        // over System.Net.Sockets). net/mod.rs just re-exports connection::*.
        Target { rel: "net/connection/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::*;")] },
        // paths/mod.rs: `mod X; use X as imp;`; REAL getcwd/current_exe/chdir/temp_dir.
        Target { rel: "paths/mod.rs", injections: vec![arm_blk1("mod dotnet; use dotnet as imp;")] },
        // CAP-1 libc-shim foundation arms (load-bearing under the families flip).
        // sys::fd — FileDesc(OwnedFd); the net Socket onion needs it. Load-bearing now.
        Target { rel: "fd/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::*;")] },
        // sys::process — mirror unsupported + REAL getpid (Environment.ProcessId).
        Target { rel: "process/mod.rs", injections: vec![arm_blk1("mod dotnet; use dotnet as imp;")] },
        // sys::pipe — PRESENT-but-Unsupported.
        Target { rel: "pipe/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::{Pipe, pipe};")] },
        // sys::sync::* + thread_parking — REAL multi-thread sync (Class-D keystone).
        // mutex = SemaphoreSlim; thread_parking = a counting-SemaphoreSlim-backed
        // Parker; once/rwlock then ride std's GENERIC queue impls (pure Parker +
        // atomics); condvar = a SemaphoreSlim wakeup-counter. See
        // docs/THREADING_PAL_RESEARCH.md + dotnet_pal/sys/sync/*/dotnet.rs.
        Target { rel: "sync/mutex/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::Mutex;")] },
        Target { rel: "sync/rwlock/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::RwLock;")] },
        Target { rel: "sync/condvar/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::Condvar;")] },
        Target { rel: "sync/once/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::{Once, OnceState};")] },
        Target { rel: "sync/thread_parking/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::Parker;")] },
        // sys::net::hostname — REAL (Environment.MachineName).
        Target { rel: "net/hostname/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::hostname;")] },
        // sys::io is_terminal — the only cfg_select! in io/mod.rs (nested in `mod
        // is_terminal {`); generic is_terminal<T>(_)->false form.
        Target { rel: "io/mod.rs", injections: vec![arm_blk1("mod dotnet; pub use dotnet::*;")] },
        // thread_local/mod.rs: THREE arms, two ANCHOR-keyed (the destructors-shift
        // saga is precisely why ordinals are banned here).
        Target {
            rel: "thread_local/mod.rs",
            injections: vec![
                // Storage arm (block 1, the first cfg_select!): re-export STORAGE ITEMS
                // ONLY (a glob would leak key/guard and trip hidden_glob_reexports).
                // Slice 2: the dotnet TLS backend is now `os.rs`-shaped (per-thread,
                // key-backed) instead of `no_threads`-shaped (process-global), so it
                // exports `Storage`/`value_align` exactly like the `_`/`os` arm,
                // NOT `EagerStorage`/`LazyStorage`.
                arm(
                    "pub use dotnet::{Storage, thread_local_inner, value_align}; pub(crate) use dotnet::{LocalPointer, local_pointer}; mod dotnet;",
                    "pub use dotnet::{Storage, thread_local_inner, value_align};",
                    Anchor::After("cfg_select! {".to_string()),
                ),
                // Guard arm: anchored on `pub(crate) mod guard {` (super::dotnet::enable).
                arm(
                    "pub(crate) use super::dotnet::enable;",
                    "pub(crate) use super::dotnet::enable;",
                    Anchor::After("pub(crate) mod guard {".to_string()),
                ),
                // Key arm: anchored on `pub(crate) mod key {`. Slice 2 wires the
                // per-thread key module (one managed `ThreadLocal<IntPtr>` per key);
                // the `os.rs`-style storage above imports `super::key::{Key, LazyKey,
                // get, set}`, so this re-export is what makes storage per-thread.
                arm(
                    "pub(super) use super::dotnet::key::{Key, LazyKey, get, set};",
                    "pub(super) use super::dotnet::key::{Key, LazyKey, get, set};",
                    Anchor::After("pub(crate) mod key {".to_string()),
                ),
            ],
        },
        // exit.rs: the ONLY Ordinal in the whole manifest. block 1 is the file-level
        // `unique_thread_exit` cascade; block 2 is the in-fn one inside `pub fn exit`.
        // Its two cfg_select!s have no distinguishing nearby string, so Ordinal(2) is
        // the documented exception. Declares + calls `rcl_dotnet_exit(code)`, which the
        // cilly linker maps to `System.Environment.Exit((int)code)` — a CLEAN managed
        // process-exit WITH the code (matching native rustc). We canNOT call `libc::exit`
        // here: std's in-tree libc shim does not declare `exit` (E0425). Previously this
        // dropped the code and called `intrinsics::abort()` ("Called abort!", exit 134)
        // — a real differential divergence fixed in P2-S2 (cargo_tests/pal_exit_code).
        Target {
            rel: "exit.rs",
            injections: vec![arm(
                "unsafe { unsafe extern \"C\" { fn rcl_dotnet_exit(code: i32) -> !; } rcl_dotnet_exit(code) }",
                "unsafe { unsafe extern \"C\" { fn rcl_dotnet_exit(code: i32) -> !; } rcl_dotnet_exit(code) }",
                Anchor::Ordinal(2),
            )],
        },
        // personality/mod.rs: the eh_personality lang item (aborting stub; .NET's
        // managed EH runs the handlers, this is never called). block 1, the only one.
        Target {
            rel: "personality/mod.rs",
            injections: vec![arm_blk1(
                "#[lang = \"eh_personality\"] fn rust_eh_personality() { core::intrinsics::abort() }",
            )],
        },
    ]
}

/// The std crate-root manifest (`Root::Std`). `rel` is relative to `…/std/src`.
fn std_targets() -> Vec<Target> {
    vec![
        // A spawned closure may contain genuine managed references. Native `Box<F>` storage is not
        // visible to CoreCLR's GC, so the dotnet target CLR-boxes and GCHandle-roots the concrete F
        // before std erases it to `dyn FnOnce`. The opaque token crosses the PAL ABI; `init_dotnet`
        // takes the typed value back out and frees the root on the new thread.
        Target {
            rel: "thread/lifecycle.rs",
            injections: vec![
                Injection::Replace {
                    find: r#"    let rust_start = unsafe {
        let ptr = Box::into_raw(Box::new(rust_start));
        let ptr = crate::mem::transmute::<
            *mut (dyn FnOnce() + Send + '_),
            *mut (dyn FnOnce() + Send + 'static),
        >(ptr);
        Box::from_raw(ptr)
    };

    let init = Box::new(ThreadInit { handle: thread.clone(), rust_start });"#.to_string(),
                    with: r#"    #[cfg(not(target_os = "dotnet"))]
    let rust_start = unsafe {
        let ptr = Box::into_raw(Box::new(rust_start));
        let ptr = crate::mem::transmute::<
            *mut (dyn FnOnce() + Send + '_),
            *mut (dyn FnOnce() + Send + 'static),
        >(ptr);
        Box::from_raw(ptr)
    };

    #[cfg(not(target_os = "dotnet"))]
    let init = Box::new(ThreadInit { handle: thread.clone(), rust_start });
    #[cfg(target_os = "dotnet")]
    let init = dotnet_managed_thread_init(thread.clone(), rust_start);"#.to_string(),
                    marker: "dotnet_managed_thread_init(thread.clone(), rust_start)".to_string(),
                },
                Injection::LineInsert {
                    before: "pub(crate) struct ThreadInit {".to_string(),
                    lines: r#"#[cfg(target_os = "dotnet")]
#[inline(never)]
fn rustc_clr_interop_managed_box_new<T>(_value: T) -> *mut u8 {
    core::intrinsics::abort()
}

#[cfg(target_os = "dotnet")]
#[inline(never)]
unsafe fn rustc_clr_interop_managed_box_take<T>(_handle: *mut u8) -> T {
    core::intrinsics::abort()
}

#[cfg(target_os = "dotnet")]
fn dotnet_managed_thread_init<F: FnOnce() + Send>(handle: Thread, rust_start: F) -> Box<ThreadInit> {
    unsafe fn run<F: FnOnce() + Send>(token: *mut u8) {
        let start = unsafe { rustc_clr_interop_managed_box_take::<F>(token) };
        start();
    }
    unsafe fn drop_token<F: FnOnce() + Send>(token: *mut u8) {
        drop(unsafe { rustc_clr_interop_managed_box_take::<F>(token) });
    }
    Box::new(ThreadInit {
        handle,
        rust_start_handle: rustc_clr_interop_managed_box_new(rust_start),
        rust_start_fn: run::<F>,
        rust_start_drop: drop_token::<F>,
    })
}
"#
                    .lines()
                    .map(str::to_string)
                    .collect(),
                    marker: "fn dotnet_managed_thread_init<F".to_string(),
                },
                // Migration for a PAL tree warmed by the first managed-box injection. Magic
                // stubs must never be MIR-inlined, or optimized builds can inline the abort body
                // before the codegen backend sees and substitutes the call.
                Injection::Replace {
                    find: "#[cfg(target_os = \"dotnet\")]\nfn rustc_clr_interop_managed_box_new<T>"
                        .to_string(),
                    with: "#[cfg(target_os = \"dotnet\")]\n#[inline(never)]\nfn rustc_clr_interop_managed_box_new<T>"
                        .to_string(),
                    marker: "#[inline(never)]\nfn rustc_clr_interop_managed_box_new<T>".to_string(),
                },
                Injection::Replace {
                    find: "#[cfg(target_os = \"dotnet\")]\nunsafe fn rustc_clr_interop_managed_box_take<T>"
                        .to_string(),
                    with: "#[cfg(target_os = \"dotnet\")]\n#[inline(never)]\nunsafe fn rustc_clr_interop_managed_box_take<T>"
                        .to_string(),
                    marker: "#[inline(never)]\nunsafe fn rustc_clr_interop_managed_box_take<T>"
                        .to_string(),
                },
                Injection::Replace {
                    find: r#"pub(crate) struct ThreadInit {
    pub handle: Thread,
    pub rust_start: Box<dyn FnOnce() + Send>,
}"#.to_string(),
                    with: r#"pub(crate) struct ThreadInit {
    pub handle: Thread,
    #[cfg(not(target_os = "dotnet"))]
    pub rust_start: Box<dyn FnOnce() + Send>,
    #[cfg(target_os = "dotnet")]
    rust_start_handle: *mut u8,
    #[cfg(target_os = "dotnet")]
    rust_start_fn: unsafe fn(*mut u8),
    #[cfg(target_os = "dotnet")]
    rust_start_drop: unsafe fn(*mut u8),
}"#.to_string(),
                    marker: "rust_start_handle: *mut u8".to_string(),
                },
                Injection::Replace {
                    find: "    pub fn init(self: Box<Self>) -> Box<dyn FnOnce() + Send> {".to_string(),
                    with: "    #[cfg(not(target_os = \"dotnet\"))]\n    pub fn init(self: Box<Self>) -> Box<dyn FnOnce() + Send> {".to_string(),
                    marker: "#[cfg(not(target_os = \"dotnet\"))]\n    pub fn init".to_string(),
                },
                Injection::Replace {
                    find: "        self.rust_start\n    }\n}".to_string(),
                    with: r#"        self.rust_start
    }

    #[cfg(target_os = "dotnet")]
    pub fn init_dotnet(mut self: Box<Self>) {
        if let Err(_thread) = set_current(self.handle.clone()) {
            rtabort!("current thread handle already set during thread spawn");
        }
        if let Some(name) = self.handle.cname() {
            imp::set_name(name);
        }
        let token = crate::mem::replace(&mut self.rust_start_handle, crate::ptr::null_mut());
        let run = self.rust_start_fn;
        unsafe { run(token) };
    }
}

#[cfg(target_os = "dotnet")]
impl Drop for ThreadInit {
    fn drop(&mut self) {
        if !self.rust_start_handle.is_null() {
            unsafe { (self.rust_start_drop)(self.rust_start_handle) };
            self.rust_start_handle = crate::ptr::null_mut();
        }
    }
}"#.to_string(),
                    marker: "pub fn init_dotnet".to_string(),
                },
            ],
        },
        // net/tcp.rs + net/udp.rs: the mio dotnet_raw_handle accessor (forwards the
        // inner sys handle on the PUBLIC std::net wrappers). #[stable] so it shadows
        // mio's own trait method without forcing a feature gate.
        Target {
            rel: "net/tcp.rs",
            injections: vec![
                Injection::Method {
                    impl_anchor: "impl TcpStream {".to_string(),
                    marker: "// DOTNET PAL ARM: mio handle accessor (TcpStream)".to_string(),
                },
                Injection::Method {
                    impl_anchor: "impl TcpListener {".to_string(),
                    marker: "// DOTNET PAL ARM: mio handle accessor (TcpListener)".to_string(),
                },
            ],
        },
        Target {
            rel: "net/udp.rs",
            injections: vec![Injection::Method {
                impl_anchor: "impl UdpSocket {".to_string(),
                marker: "// DOTNET PAL ARM: mio handle accessor (UdpSocket)".to_string(),
            }],
        },
        // os/mod.rs keystone: (1) widen the `pub mod fd` gate to include dotnet, and
        // (2) declare `pub mod dotnet`. Both are #[cfg] lists, not cfg_select!.
        Target {
            rel: "os/mod.rs",
            injections: vec![
                // Widen `pub mod fd`'s `#[cfg(any(` gate: insert `target_os="dotnet",`
                // as the first disjunct. We anchor the insert to the `#[cfg(any(` line
                // and rely on the std-fixed gate shape. (The bash walked UP from `pub
                // mod fd;`; here we LineInsert after the any( opener that precedes it —
                // see fd_gate_widen below for the os-specific handling.)
                // Declared `pub mod dotnet` before the first aix per-target block.
                Injection::LineInsert {
                    before: "#[cfg(target_os = \"aix\")]".to_string(),
                    lines: vec![
                        "#[cfg(target_os = \"dotnet\")]".to_string(),
                        "pub mod dotnet;".to_string(),
                    ],
                    marker: "pub mod dotnet;".to_string(),
                },
            ],
        },
        // os/unix/mod.rs: add the dotnet arm to the `mod platform { … }` list.
        Target {
            rel: "os/unix/mod.rs",
            injections: vec![Injection::LineInsert {
                before: "#[cfg(target_os = \"aix\")]".to_string(),
                lines: vec![
                    "    #[cfg(target_os = \"dotnet\")]".to_string(),
                    "    pub use crate::os::dotnet::*;".to_string(),
                ],
                marker: "crate::os::dotnet".to_string(),
            }],
        },
        // os/fd/{owned,raw}.rs: defer the File/Pipe fd-impls for dotnet (fs/pipe not
        // fd-backed yet). Widen each `#[cfg(not(target_os="trusty"))]` that gates a
        // File/Pipe impl to also exclude dotnet. These are handled by a dedicated
        // pass (fd_impl_defer) because they key on the FOLLOWING line's impl target.
        // os/unix/io/mod.rs: neutralise StdioExt null_fd() (fs not fd-backed).
        Target {
            rel: "os/unix/io/mod.rs",
            injections: vec![Injection::Replace {
                find: "let null_dev = crate::fs::OpenOptions::new().read(true).write(true).open(\"/dev/null\")?;\n    Ok(null_dev.into())".to_string(),
                with: "// dotnet: StdioExt null_fd unsupported (fs::File not fd-backed)\n    Err(io::Error::UNSUPPORTED_PLATFORM)".to_string(),
                marker: "dotnet: StdioExt null_fd unsupported".to_string(),
            }],
        },
        // build.rs (one dir above src): teach std it is a supported platform.
        Target {
            rel: "../build.rs",
            injections: vec![Injection::Replace {
                find: "target_os == \"linux\"".to_string(),
                with: "target_os == \"dotnet\"\n        || target_os == \"linux\"".to_string(),
                marker: "target_os == \"dotnet\"".to_string(),
            }],
        },
    ]
}

/// panic_unwind manifest (`Root::PanicUnwind`). Routes the FLAVOUR cfg_select! to gcc.
fn panic_unwind_targets() -> Vec<Target> {
    vec![Target {
        rel: "lib.rs",
        injections: vec![arm(
            "#[path = \"gcc.rs\"]\nmod imp;",
            "#[path = \"gcc.rs\"]",
            Anchor::After("cfg_select! {".to_string()),
        )],
    }]
}

/// unwind manifest (`Root::Unwind`). Routes the flavour cfg_select! to libunwind.
fn unwind_targets() -> Vec<Target> {
    vec![Target {
        rel: "lib.rs",
        injections: vec![arm(
            "mod libunwind;\npub use libunwind::*;",
            "mod libunwind;",
            Anchor::After("cfg_select! {".to_string()),
        )],
    }]
}

/// The full manifest, grouped by root.
pub fn manifest() -> Vec<(Root, Vec<Target>)> {
    vec![
        (Root::Sys, sys_targets()),
        (Root::Std, std_targets()),
        (Root::PanicUnwind, panic_unwind_targets()),
        (Root::Unwind, unwind_targets()),
    ]
}

// ===========================================================================
// The two os-specific passes (fd gate widen, fd-impl defer).
// ===========================================================================

fn root_dir(lib: &Path, root: Root) -> PathBuf {
    match root {
        Root::Sys => lib.join("std/src/sys"),
        Root::Std => lib.join("std/src"),
        Root::PanicUnwind => lib.join("panic_unwind/src"),
        Root::Unwind => lib.join("unwind/src"),
    }
}

/// os/mod.rs `pub mod fd` gate widen: add `target_os = "dotnet",` as the first
/// disjunct of the `#[cfg(any( … ))]` directly above `pub mod fd;`. Robust to the
/// disjunct set drifting (keys on the unique `pub mod fd;` line, like the bash awk).
fn widen_fd_gate(os_mod: &Path) -> Result<Applied> {
    let text = fs::read_to_string(os_mod).with_context(|| format!("read {}", os_mod.display()))?;
    // Idempotency: if the gate already lists dotnet near `pub mod fd`, skip. We mark by
    // a fixed comment-free heuristic: presence of the dotnet disjunct anywhere in the
    // file's `pub mod fd` gate region. Simpler + safe: skip if the inserted exact line
    // already precedes `pub mod fd;`.
    let lines: Vec<&str> = text.lines().collect();
    let fd_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with("pub mod fd;"))
        .context("os/mod.rs: `pub mod fd;` not found (rustc-src drift?)")?;
    // walk up to the opening `#[cfg(any(` of its gate.
    let any_idx = (0..fd_idx)
        .rev()
        .find(|&i| lines[i].trim_end().ends_with("#[cfg(any(") || lines[i].contains("#[cfg(any("))
        .context("os/mod.rs: `#[cfg(any(` gate above `pub mod fd;` not found")?;
    // already widened? (a dotnet disjunct between any_idx and fd_idx)
    if lines[any_idx..fd_idx]
        .iter()
        .any(|l| l.contains("target_os = \"dotnet\""))
    {
        return Ok(Applied::Skipped);
    }
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 1);
    for (i, l) in lines.iter().enumerate() {
        out.push((*l).to_string());
        if i == any_idx {
            out.push("    target_os = \"dotnet\",".to_string());
        }
    }
    atomic_write(os_mod, &join_preserving_trailing_newline(&text, &out))?;
    Ok(Applied::Inserted)
}

/// os/fd/{owned,raw}.rs File/Pipe fd-impl gating: for each
/// `#[cfg(not(target_os = "trusty"))]` whose NEXT line is an impl over fs::File /
/// io::Pipe{Reader,Writer}, widen the cfg to also exclude dotnet. Keys on the impl
/// target on the following line (mirrors the bash awk prevline machinery). Idempotent.
fn defer_fd_impls(file: &Path) -> Result<Applied> {
    let text = match fs::read_to_string(file) {
        Ok(t) => t,
        Err(_) => return Ok(Applied::Skipped), // file may not exist on every nightly
    };
    if text.contains("not(target_os = \"trusty\"), not(target_os = \"dotnet\")") {
        return Ok(Applied::Skipped);
    }
    let trusty = "#[cfg(not(target_os = \"trusty\"))]";
    let widened = "#[cfg(all(not(target_os = \"trusty\"), not(target_os = \"dotnet\")))]";
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut changed = false;
    for (i, l) in lines.iter().enumerate() {
        if l.trim() == trusty {
            let next = lines.get(i + 1).copied().unwrap_or("");
            let targets_file_or_pipe = next.contains("for fs::File")
                || next.contains("<fs::File>")
                || next.contains("for io::Pipe")
                || next.contains("<io::PipeReader>")
                || next.contains("<io::PipeWriter>");
            if targets_file_or_pipe {
                out.push(l.replace(trusty, widened));
                changed = true;
                continue;
            }
        }
        out.push((*l).to_string());
    }
    if changed {
        atomic_write(file, &join_preserving_trailing_newline(&text, &out))?;
        Ok(Applied::Inserted)
    } else {
        Ok(Applied::Skipped)
    }
}

// ===========================================================================
// libc patch (rust-src-shaped, reused across rust-src vendor + every registry copy).
// ===========================================================================

/// Patch one libc-0.2 source dir so `target_os="dotnet"` reuses libc's canonical Linux GNU
/// x86_64 module tree. The .NET PAL deliberately implements the Linux C ABI by symbol name;
/// keeping a second hand-maintained libc facade inevitably drifts as dependencies exercise more
/// of libc. This only widens libc's platform-selection cascades. It does not enable rustix's
/// linux-raw backend or emit host syscalls.
pub fn patch_libc(libc_dir: &Path) -> Result<bool> {
    let lib_rs = libc_dir.join("lib.rs");
    if !lib_rs.is_file() {
        return Ok(false);
    }
    // Migrate installations patched by the former standalone `dotnet.rs` facade back to libc's
    // normal Unix/posix roots. Each replacement is also a no-op on a pristine libc checkout.
    replace_in_file(
        &lib_rs,
        "} else if #[cfg(all(unix, not(target_os = \"dotnet\")))] {",
        "} else if #[cfg(unix)] {",
    )?;
    let new_mod = libc_dir.join("new/mod.rs");
    if new_mod.is_file() {
        replace_in_file(
            &new_mod,
            "if #[cfg(all(target_family = \"unix\", not(target_os = \"qurt\"), not(target_os = \"dotnet\")))] {",
            "if #[cfg(all(target_family = \"unix\", not(target_os = \"qurt\")))] {",
        )?;
        replace_all_in_file(
            &new_mod,
            "} else if #[cfg(target_os = \"linux\")] {",
            "} else if #[cfg(any(target_os = \"linux\", target_os = \"dotnet\"))] {",
        )?;
    }
    let new_common = libc_dir.join("new/common/mod.rs");
    if new_common.is_file() {
        replace_in_file(
            &new_common,
            "#[cfg(all(target_family = \"unix\", not(target_os = \"dotnet\")))]",
            "#[cfg(target_family = \"unix\")]",
        )?;
    }
    widen_dotnet_as_linux_in_tree(&libc_dir.join("new"))?;

    // The old facade was always appended at EOF. Remove it before selecting the upstream tree, or
    // its glob export would collide with the canonical Unix definitions.
    let lib_text = fs::read_to_string(&lib_rs)?;
    if let Some(marker) = lib_text.find("\n// DOTNET PAL: the single libc face for os=dotnet") {
        atomic_write(&lib_rs, &lib_text[..marker])?;
    }
    // Old installations also copied the facade beside `lib.rs`. It is unreferenced after the
    // appended module is removed, but clean it up when its distinctive header proves ownership.
    // Do not delete an arbitrary future upstream `dotnet.rs` merely because the name matches.
    let legacy_face = libc_dir.join("dotnet.rs");
    if legacy_face.is_file() && fs::read_to_string(&legacy_face)?.contains("SINGLE libc face for") {
        fs::remove_file(&legacy_face)
            .with_context(|| format!("remove legacy libc facade {}", legacy_face.display()))?;
    }

    // Route dotnet through unix -> linux_like -> linux -> gnu. The target spec declares
    // `env="gnu"`; spelling dotnet in the final branch too makes the intended ABI explicit and
    // keeps this source transformation self-describing.
    replace_in_file(
        &libc_dir.join("unix/mod.rs"),
        "        target_os = \"linux\",\n        target_os = \"l4re\",",
        "        target_os = \"linux\",\n        target_os = \"dotnet\",\n        target_os = \"l4re\",",
    )?;
    replace_in_file(
        &libc_dir.join("unix/linux_like/mod.rs"),
        "    } else if #[cfg(target_os = \"linux\")] {",
        "    } else if #[cfg(any(target_os = \"linux\", target_os = \"dotnet\"))] {",
    )?;
    replace_in_file(
        &libc_dir.join("unix/linux_like/linux/mod.rs"),
        "    } else if #[cfg(target_env = \"gnu\")] {",
        "    } else if #[cfg(any(target_env = \"gnu\", target_os = \"dotnet\"))] {",
    )?;
    widen_dotnet_as_linux_in_tree(&libc_dir.join("unix/linux_like/linux"))?;
    Ok(true)
}

/// Keep rustix on its libc backend while making its ABI cfgs agree with the canonical Linux GNU
/// libc ABI surface selected for dotnet. The build.rs `libc` decision remains unchanged (`dotnet !=
/// linux`), so this never enables rustix's linux-raw syscall backend.
#[cfg(test)]
pub fn patch_rustix(crate_dir: &Path) -> Result<bool> {
    let build_rs = crate_dir.join("build.rs");
    let src = crate_dir.join("src");
    if !build_rs.is_file() || !src.is_dir() {
        return Ok(false);
    }
    replace_in_file(
        &build_rs,
        "if os == \"linux\" || os == \"l4re\" || os == \"android\" || os == \"emscripten\" {",
        "if os == \"linux\" || os == \"dotnet\" || os == \"l4re\" || os == \"android\" || os == \"emscripten\" {",
    )?;
    replace_in_file(
        &build_rs,
        "if os == \"android\" || os == \"linux\" {",
        "if os == \"android\" || os == \"linux\" || os == \"dotnet\" {",
    )?;
    widen_dotnet_as_linux_in_tree(&src)?;
    Ok(true)
}

/// async-fs exposes raw-fd conversions under broad `cfg(unix)`. Dotnet's managed `std::fs::File`
/// intentionally is not fd-backed, so gate only those optional conversions.
#[cfg(test)]
pub fn patch_async_fs(crate_dir: &Path) -> Result<bool> {
    let lib_rs = crate_dir.join("src/lib.rs");
    if !lib_rs.is_file() {
        return Ok(false);
    }
    for impl_head in [
        "impl std::os::unix::io::AsRawFd for File",
        "impl From<std::os::unix::io::OwnedFd> for File",
        "impl std::os::unix::io::AsFd for File",
    ] {
        replace_in_file(
            &lib_rs,
            &format!("#[cfg(unix)]\n{impl_head}"),
            &format!("#[cfg(all(unix, not(target_os = \"dotnet\")))]\n{impl_head}"),
        )?;
    }
    Ok(true)
}

/// Route polling through its epoll backend on dotnet. The POSIX shim already owns
/// epoll_create1/ctl/wait over managed sockets; leaving polling on its generic Unix
/// fallback would emit host `pipe2`/`ppoll` P/Invokes instead.
#[cfg(test)]
pub fn patch_polling(crate_dir: &Path) -> Result<bool> {
    let src = crate_dir.join("src");
    if !src.is_dir() {
        return Ok(false);
    }
    widen_dotnet_as_linux_in_tree(&src)?;
    Ok(true)
}

/// Replace the FIRST occurrence of `find` with `with` in `file` IFF `find` is present
/// and `with` is not already (idempotent). A missing `find` is NOT an error here: under
/// the families flip these patterns may already be narrowed by a prior run, and the
/// post-narrow text simply no longer matches. (Mirrors the bash sed-is-a-no-op-on-rerun.)
fn replace_in_file(file: &Path, find: &str, with: &str) -> Result<()> {
    let text = fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
    if text.contains(with) || !text.contains(find) {
        return Ok(());
    }
    atomic_write(file, &text.replacen(find, with, 1))
}

/// Replace every occurrence of a platform-selection arm, preserving idempotence.
fn replace_all_in_file(file: &Path, find: &str, with: &str) -> Result<()> {
    let text = fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
    if text.contains(with) || !text.contains(find) {
        return Ok(());
    }
    atomic_write(file, &text.replace(find, with))
}

/// libc's in-progress `new/` tree repeats Linux cfg predicates across its UAPI, glibc, pthread,
/// and common-header modules. Normalize our own prior widening before applying it, which makes
/// the mechanical ABI alias idempotent without carrying a fork of those definitions.
fn widen_dotnet_as_linux_in_tree(dir: &Path) -> Result<()> {
    let linux = "target_os = \"linux\"";
    let linux_or_dotnet = "any(target_os = \"linux\", target_os = \"dotnet\")";
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            widen_dotnet_as_linux_in_tree(&path)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            let text = fs::read_to_string(&path)?;
            let widened = text
                .replace(linux_or_dotnet, linux)
                .replace(linux, linux_or_dotnet);
            if widened != text {
                atomic_write(&path, &widened)?;
            }
        }
    }
    Ok(())
}

/// Find every `libc-0.2*/src` dir under `root` that has a `lib.rs` (rust-src vendor or
/// registry), shallow-recursively.
pub fn find_libc_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    find_libc_rec(root, 0, &mut out);
    out
}

fn find_libc_rec(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 8 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.starts_with("libc-0.2") {
                let src = path.join("src");
                if src.join("lib.rs").is_file() {
                    out.push(src);
                }
            }
            // keep descending (registry nests libc under index/cache dirs).
            find_libc_rec(&path, depth + 1, out);
        }
    }
}

// ===========================================================================
// THE DRIVER — mirror PAL trees, drive the manifest, patch rust-src vendor libc.
// ===========================================================================

/// Run PAL injection against an explicitly provisioned rust-src library root.
pub fn inject_all_at(ctx: &Ctx, lib: &Path) -> Result<()> {
    let sys_dst = lib.join("std/src/sys");
    let std_dst = lib.join("std/src");

    if ctx.flags.verbose {
        eprintln!(
            "==> injecting dotnet PAL into rust-src ({})",
            sys_dst.display()
        );
    }

    // 1) mirror the PAL trees (clean base each run).
    let pal_sys = ctx.paths.pal_root.join("sys");
    if !pal_sys.is_dir() {
        bail!("no PAL sys tree at {}", pal_sys.display());
    }
    let n = mirror_tree(&pal_sys, &sys_dst)?;
    if ctx.flags.verbose {
        eprintln!("==> mirrored {n} dotnet_pal/sys files");
    }

    // os/dotnet platform tree -> std/src/os/dotnet.
    let pal_os_dotnet = ctx.paths.pal_root.join("os/dotnet");
    if pal_os_dotnet.is_dir() {
        let dst = std_dst.join("os/dotnet");
        let m = mirror_tree(&pal_os_dotnet, &dst)?;
        if ctx.flags.verbose {
            eprintln!("==> mirrored {m} dotnet_pal/os/dotnet files");
        }
    }

    // panic_unwind/unwind doc-only markers.
    let pu_marker = ctx.paths.pal_root.join("panic_unwind/dotnet.rs");
    if pu_marker.is_file() {
        let dst = root_dir(&lib, Root::PanicUnwind).join("dotnet.rs");
        let _ = fs::copy(&pu_marker, &dst);
    }

    // 2) drive the manifest.
    for (root, targets) in manifest() {
        let base = root_dir(lib, root);
        for t in targets {
            let file = base.join(t.rel);
            if !file.is_file() {
                // The PAL source for some arms may be absent on a given tree; the bash
                // guarded each with `[ -f … ]`. Skip a target whose file is missing.
                continue;
            }
            for inj in &t.injections {
                let res = apply_one(&file, inj)
                    .with_context(|| format!("manifest injection into {}", file.display()))?;
                if res == Applied::Inserted {
                    eprintln!("==> injected: {}", t.rel);
                }
            }
        }
    }

    // 3) the os-specific passes (gate widen + fd-impl defer).
    let os_mod = std_dst.join("os/mod.rs");
    if os_mod.is_file() {
        widen_fd_gate(&os_mod).context("os/mod.rs fd gate widen")?;
    }
    for f in ["os/fd/owned.rs", "os/fd/raw.rs"] {
        let p = std_dst.join(f);
        defer_fd_impls(&p).with_context(|| format!("{f} fd-impl defer"))?;
    }

    // 4) patch the rust-src VENDOR libc copies (the registry pass is in buildstd).
    for d in find_libc_dirs(lib) {
        if patch_libc(&d)? && ctx.flags.verbose {
            eprintln!("==> patched libc: {}", d.display());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- CfgArm via After-anchor (the common case) ----
    const CFG_AFTER_IN: &str = "\
pub mod thing {
    cfg_select! {
        target_os = \"linux\" => { mod linux; pub use linux::*; }
        _ => { mod unsupported; pub use unsupported::*; }
    }
}
";

    #[test]
    fn cfg_arm_after_anchor_inserts_first() {
        let inj = arm_blk1("mod dotnet; pub use dotnet::*;");
        let (out, applied) = apply_one_str(CFG_AFTER_IN, &inj).unwrap();
        assert_eq!(applied, Applied::Inserted);
        // dotnet arm must be the FIRST arm (right after the cfg_select! line).
        let body_pos = out.find("mod dotnet; pub use dotnet::*;").unwrap();
        let linux_pos = out.find("mod linux;").unwrap();
        assert!(
            body_pos < linux_pos,
            "dotnet arm must precede the linux arm"
        );
        assert!(out.contains("target_os = \"dotnet\" => {"));
    }

    #[test]
    fn cfg_arm_is_idempotent() {
        let inj = arm_blk1("mod dotnet; pub use dotnet::*;");
        let (once, _) = apply_one_str(CFG_AFTER_IN, &inj).unwrap();
        let (twice, applied) = apply_one_str(&once, &inj).unwrap();
        assert_eq!(applied, Applied::Skipped);
        assert_eq!(once, twice, "apply twice == apply once");
    }

    // ---- CfgArm via specific After-anchor (thread_local guard) ----
    const GUARD_IN: &str = "\
pub(crate) mod guard {
    cfg_select! {
        all(target_thread_local) => { pub(crate) use thing::enable; }
        _ => {}
    }
}
";

    #[test]
    fn cfg_arm_anchored_targets_the_right_block() {
        let inj = arm(
            "pub(crate) use super::dotnet::enable;",
            "pub(crate) use super::dotnet::enable;",
            Anchor::After("pub(crate) mod guard {".to_string()),
        );
        let (out, applied) = apply_one_str(GUARD_IN, &inj).unwrap();
        assert_eq!(applied, Applied::Inserted);
        assert!(out.contains("pub(crate) use super::dotnet::enable;"));
        assert!(out.contains("target_os = \"dotnet\" => {"));
    }

    // ---- CfgArm via Ordinal (exit.rs nth=2) ----
    const ORDINAL_IN: &str = "\
fn unique() {
    cfg_select! {
        target_os = \"x\" => { a() }
        _ => { b() }
    }
}
pub fn exit(code: i32) -> ! {
    cfg_select! {
        any(target_family = \"unix\") => { libc::exit(code) }
        _ => { abort() }
    }
}
";

    #[test]
    fn cfg_arm_ordinal_two_targets_second_block() {
        let inj = arm(
            "unsafe { unsafe extern \"C\" { fn rcl_dotnet_exit(code: i32) -> !; } rcl_dotnet_exit(code) }",
            "unsafe { unsafe extern \"C\" { fn rcl_dotnet_exit(code: i32) -> !; } rcl_dotnet_exit(code) }",
            Anchor::Ordinal(2),
        );
        let (out, applied) = apply_one_str(ORDINAL_IN, &inj).unwrap();
        assert_eq!(applied, Applied::Inserted);
        // must be inside the SECOND cfg_select! (after `pub fn exit`), not the first.
        let arm_pos = out.find("rcl_dotnet_exit(code)").unwrap();
        let exit_pos = out.find("pub fn exit").unwrap();
        assert!(
            arm_pos > exit_pos,
            "ordinal-2 arm must land in the exit() block"
        );
    }

    // ---- Method ----
    const METHOD_IN: &str = "\
impl TcpStream {
    pub fn connect() {}
}
";

    #[test]
    fn method_injects_accessor() {
        let inj = Injection::Method {
            impl_anchor: "impl TcpStream {".to_string(),
            marker: "// DOTNET PAL ARM: mio handle accessor (TcpStream)".to_string(),
        };
        let (out, applied) = apply_one_str(METHOD_IN, &inj).unwrap();
        assert_eq!(applied, Applied::Inserted);
        assert!(out.contains("pub fn dotnet_raw_handle(&self) -> *mut u8"));
        assert!(out.contains("#[cfg(target_os = \"dotnet\")]"));
        // idempotent
        let (twice, applied2) = apply_one_str(&out, &inj).unwrap();
        assert_eq!(applied2, Applied::Skipped);
        assert_eq!(out, twice);
    }

    // ---- LineInsert ----
    const LINE_IN: &str = "\
#[cfg(target_os = \"linux\")]
pub mod linux;
#[cfg(target_os = \"aix\")]
pub mod aix;
";

    #[test]
    fn line_insert_before_anchor() {
        let inj = Injection::LineInsert {
            before: "#[cfg(target_os = \"aix\")]".to_string(),
            lines: vec![
                "#[cfg(target_os = \"dotnet\")]".to_string(),
                "pub mod dotnet;".to_string(),
            ],
            marker: "pub mod dotnet;".to_string(),
        };
        let (out, applied) = apply_one_str(LINE_IN, &inj).unwrap();
        assert_eq!(applied, Applied::Inserted);
        let dotnet_pos = out.find("pub mod dotnet;").unwrap();
        let aix_pos = out.find("pub mod aix;").unwrap();
        assert!(
            dotnet_pos < aix_pos,
            "dotnet decl must precede the aix block"
        );
    }

    // ---- Replace ----
    #[test]
    fn replace_single_occurrence() {
        let inj = Injection::Replace {
            find: "#[cfg(all(unix, not(target_os = \"vxworks\")))]".to_string(),
            with: "#[cfg(all(unix, not(target_os = \"vxworks\"), not(target_os = \"dotnet\")))]"
                .to_string(),
            marker: "not(target_os = \"vxworks\"), not(target_os = \"dotnet\")".to_string(),
        };
        let input = "#[cfg(all(unix, not(target_os = \"vxworks\")))]\nfn set_perms() {}\n";
        let (out, applied) = apply_one_str(input, &inj).unwrap();
        assert_eq!(applied, Applied::Inserted);
        assert!(out.contains("not(target_os = \"dotnet\")"));
        // idempotent (the marker is now present)
        let (twice, applied2) = apply_one_str(&out, &inj).unwrap();
        assert_eq!(applied2, Applied::Skipped);
        assert_eq!(out, twice);
    }

    #[test]
    fn thread_lifecycle_managed_box_rewrite_is_complete_and_idempotent() {
        let mut lifecycle = r#"fn spawn_unchecked() {
    let rust_start = unsafe {
        let ptr = Box::into_raw(Box::new(rust_start));
        let ptr = crate::mem::transmute::<
            *mut (dyn FnOnce() + Send + '_),
            *mut (dyn FnOnce() + Send + 'static),
        >(ptr);
        Box::from_raw(ptr)
    };

    let init = Box::new(ThreadInit { handle: thread.clone(), rust_start });
}

pub(crate) struct ThreadInit {
    pub handle: Thread,
    pub rust_start: Box<dyn FnOnce() + Send>,
}

impl ThreadInit {
    pub fn init(self: Box<Self>) -> Box<dyn FnOnce() + Send> {
        if let Err(_thread) = set_current(self.handle.clone()) {
            rtabort!("current thread handle already set during thread spawn");
        }
        if let Some(name) = self.handle.cname() {
            imp::set_name(name);
        }
        self.rust_start
    }
}
"#
        .to_string();
        let target = std_targets()
            .into_iter()
            .find(|target| target.rel == "thread/lifecycle.rs")
            .expect("thread lifecycle target");

        for injection in &target.injections {
            let (rewritten, applied) = apply_one_str(&lifecycle, injection).unwrap();
            if applied == Applied::Skipped {
                assert!(lifecycle.contains(injection_marker(injection)));
            }
            lifecycle = rewritten;
        }
        assert!(lifecycle.contains("rustc_clr_interop_managed_box_new(rust_start)"));
        assert!(lifecycle.contains("rustc_clr_interop_managed_box_take::<F>(token)"));
        assert!(lifecycle.contains("#[inline(never)]\nfn rustc_clr_interop_managed_box_new<T>"));
        assert!(
            lifecycle.contains("#[inline(never)]\nunsafe fn rustc_clr_interop_managed_box_take<T>")
        );
        assert!(lifecycle.contains("pub fn init_dotnet(mut self: Box<Self>)"));
        assert!(lifecycle.contains("impl Drop for ThreadInit"));

        let once = lifecycle.clone();
        for injection in &target.injections {
            let (rewritten, applied) = apply_one_str(&lifecycle, injection).unwrap();
            assert_eq!(applied, Applied::Skipped);
            lifecycle = rewritten;
        }
        assert_eq!(lifecycle, once);
    }

    // ---- anchor-missing errors loudly (surfaces nightly drift) ----
    #[test]
    fn missing_cfg_anchor_errors() {
        let inj = arm(
            "x",
            "x",
            Anchor::After("nonexistent anchor string".to_string()),
        );
        let err = apply_one_str("fn f() {}\n", &inj).unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }

    #[test]
    fn missing_method_anchor_errors() {
        let inj = Injection::Method {
            impl_anchor: "impl Nope {".to_string(),
            marker: "marker-x".to_string(),
        };
        let err = apply_one_str("fn f() {}\n", &inj).unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }

    #[test]
    fn missing_replace_target_errors() {
        let inj = Injection::Replace {
            find: "absent".to_string(),
            with: "present".to_string(),
            marker: "present".to_string(),
        };
        let err = apply_one_str("fn f() {}\n", &inj).unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }

    // ---- canonical Linux libc routing + legacy-facade migration ----
    #[test]
    fn patch_libc_reuses_canonical_linux_tree_idempotently() {
        let dir = std::env::temp_dir().join(format!("cd_libc_test_{}", std::process::id()));
        let src = dir.join("libc-0.2.99/src");
        let new = src.join("new");
        let common = new.join("common");
        let linux_like = src.join("unix/linux_like");
        let linux = linux_like.join("linux");
        fs::create_dir_all(&common).unwrap();
        fs::create_dir_all(&linux).unwrap();
        fs::write(
            src.join("lib.rs"),
            "cfg_if! {\n} else if #[cfg(all(unix, not(target_os = \"dotnet\")))] {\n    mod unix;\n}\n\n// DOTNET PAL: the single libc face for os=dotnet\n#[cfg(target_os = \"dotnet\")]\nmod dotnet;\n#[cfg(target_os = \"dotnet\")]\npub use crate::dotnet::*;\n",
        )
        .unwrap();
        fs::write(
            new.join("mod.rs"),
            "if #[cfg(all(target_family = \"unix\", not(target_os = \"qurt\"), not(target_os = \"dotnet\")))] {\n}\n} else if #[cfg(target_os = \"linux\")] {\n    mod linux_uapi;\n}\n} else if #[cfg(target_os = \"linux\")] {\n    pub use linux::*;\n}\n",
        )
        .unwrap();
        fs::write(
            common.join("mod.rs"),
            "#[cfg(all(target_family = \"unix\", not(target_os = \"dotnet\")))]\npub(crate) mod posix;\n",
        )
        .unwrap();
        fs::write(
            src.join("unix/mod.rs"),
            "cfg_if! {\n    if #[cfg(any(\n        target_os = \"linux\",\n        target_os = \"l4re\",\n    ))] { mod linux_like; }\n}\n",
        )
        .unwrap();
        fs::write(
            linux_like.join("mod.rs"),
            "cfg_if! {\n    } else if #[cfg(target_os = \"linux\")] {\n        mod linux;\n    }\n}\n",
        )
        .unwrap();
        fs::write(
            linux.join("mod.rs"),
            "cfg_if! {\n    } else if #[cfg(target_env = \"gnu\")] {\n        mod gnu;\n    }\n}\n",
        )
        .unwrap();
        fs::write(
            linux.join("abi.rs"),
            "#[cfg(target_os = \"linux\")]\npub const ABI: u32 = 1;\n",
        )
        .unwrap();
        fs::write(
            src.join("dotnet.rs"),
            "//! libc bindings for dotnet — the SINGLE libc face for legacy installs\n",
        )
        .unwrap();

        let ok = patch_libc(&src).unwrap();
        assert!(ok);
        let lib = fs::read_to_string(src.join("lib.rs")).unwrap();
        assert!(lib.contains("else if #[cfg(unix)]"));
        assert!(!lib.contains("mod dotnet;"));
        assert!(!src.join("dotnet.rs").exists());
        let nm = fs::read_to_string(new.join("mod.rs")).unwrap();
        assert!(!nm.contains("not(target_os = \"dotnet\")"));
        assert_eq!(
            nm.matches("any(target_os = \"linux\", target_os = \"dotnet\")")
                .count(),
            2
        );
        let cm = fs::read_to_string(common.join("mod.rs")).unwrap();
        assert!(cm.contains("#[cfg(target_family = \"unix\")]"));
        assert!(
            fs::read_to_string(src.join("unix/mod.rs"))
                .unwrap()
                .contains("target_os = \"dotnet\"")
        );
        assert!(
            fs::read_to_string(linux_like.join("mod.rs"))
                .unwrap()
                .contains("any(target_os = \"linux\", target_os = \"dotnet\")")
        );
        assert!(
            fs::read_to_string(linux.join("mod.rs"))
                .unwrap()
                .contains("any(target_env = \"gnu\", target_os = \"dotnet\")")
        );
        assert!(
            fs::read_to_string(linux.join("abi.rs"))
                .unwrap()
                .contains("any(target_os = \"linux\", target_os = \"dotnet\")")
        );

        // Idempotent: rerunning must not duplicate cascade arms.
        patch_libc(&src).unwrap();
        let lib2 = fs::read_to_string(src.join("lib.rs")).unwrap();
        assert_eq!(lib, lib2, "patch_libc must be idempotent");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn registry_portability_patches_are_narrow_and_idempotent() {
        let dir =
            std::env::temp_dir().join(format!("cd_registry_patch_test_{}", std::process::id()));
        let rustix = dir.join("rustix-1.0.0");
        fs::create_dir_all(rustix.join("src/backend")).unwrap();
        fs::write(
            rustix.join("build.rs"),
            "if os == \"linux\" || os == \"l4re\" || os == \"android\" || os == \"emscripten\" {\n}\nif os == \"android\" || os == \"linux\" {\n}\n",
        )
        .unwrap();
        fs::write(
            rustix.join("src/backend/mod.rs"),
            "#[cfg(target_os = \"linux\")]\nmod abi;\n",
        )
        .unwrap();

        let async_fs = dir.join("async-fs-2.0.0");
        fs::create_dir_all(async_fs.join("src")).unwrap();
        let async_source = "#[cfg(unix)]\nimpl std::os::unix::io::AsRawFd for File {}\n#[cfg(unix)]\nimpl From<std::os::unix::io::OwnedFd> for File {}\n#[cfg(unix)]\nimpl std::os::unix::io::AsFd for File {}\n";
        fs::write(async_fs.join("src/lib.rs"), async_source).unwrap();

        let polling = dir.join("polling-3.0.0");
        fs::create_dir_all(polling.join("src")).unwrap();
        fs::write(
            polling.join("src/lib.rs"),
            "#[cfg(any(target_os = \"linux\", target_os = \"android\"))]\nmod epoll;\n",
        )
        .unwrap();

        assert!(patch_rustix(&rustix).unwrap());
        assert!(patch_async_fs(&async_fs).unwrap());
        assert!(patch_polling(&polling).unwrap());
        let rustix_build = fs::read_to_string(rustix.join("build.rs")).unwrap();
        let rustix_source = fs::read_to_string(rustix.join("src/backend/mod.rs")).unwrap();
        let async_patched = fs::read_to_string(async_fs.join("src/lib.rs")).unwrap();
        let polling_patched = fs::read_to_string(polling.join("src/lib.rs")).unwrap();
        assert!(rustix_build.contains("os == \"dotnet\""));
        assert!(rustix_source.contains("target_os = \"dotnet\""));
        assert_eq!(
            async_patched.matches("not(target_os = \"dotnet\")").count(),
            3
        );
        assert!(polling_patched.contains("target_os = \"dotnet\""));

        patch_rustix(&rustix).unwrap();
        patch_async_fs(&async_fs).unwrap();
        patch_polling(&polling).unwrap();
        assert_eq!(
            rustix_build,
            fs::read_to_string(rustix.join("build.rs")).unwrap()
        );
        assert_eq!(
            rustix_source,
            fs::read_to_string(rustix.join("src/backend/mod.rs")).unwrap()
        );
        assert_eq!(
            async_patched,
            fs::read_to_string(async_fs.join("src/lib.rs")).unwrap()
        );
        assert_eq!(
            polling_patched,
            fs::read_to_string(polling.join("src/lib.rs")).unwrap()
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dotnet_target_declares_the_linux_gnu_abi() {
        let target = include_str!("../../../x86_64-unknown-dotnet.json");
        assert!(target.contains("\"llvm-target\": \"x86_64-unknown-linux-gnu\""));
        assert!(target.contains("\"env\": \"gnu\""));
    }

    #[test]
    fn dotnet_fs_file_type_uses_canonical_libc_mode_type() {
        let fs_pal = include_str!("../../../dotnet_pal/sys/fs/dotnet.rs");
        assert!(fs_pal.contains("pub fn is(&self, mode: libc::mode_t) -> bool"));
        assert!(fs_pal.contains("mode & libc::S_IFMT == synthetic"));
        assert!(!fs_pal.contains("const S_IFMT: i32"));
    }

    #[test]
    fn shell_overlay_fallback_supports_macos_bash_3() {
        let core = include_str!("../../../feasibility/_cargo_dotnet_core.sh");
        assert!(
            !core
                .lines()
                .any(|line| line.trim_start().starts_with("declare -A")),
            "the fallback script must run under the Bash 3.2 shipped by macOS"
        );
        assert!(
            core.contains("awk -v want=\"$n\""),
            "multi-version overlay selection still needs to count names"
        );
    }

    // ---- fd gate widen (the os/mod.rs keystone) ----
    #[test]
    fn fd_gate_widen_via_tempfile() {
        let dir = std::env::temp_dir().join(format!("cd_osmod_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("mod.rs");
        fs::write(
            &f,
            "#[cfg(any(\n    unix,\n    target_os = \"hermit\",\n))]\npub mod fd;\n",
        )
        .unwrap();
        let a = widen_fd_gate(&f).unwrap();
        assert_eq!(a, Applied::Inserted);
        let t = fs::read_to_string(&f).unwrap();
        assert!(t.contains("target_os = \"dotnet\","));
        let dotnet_pos = t.find("target_os = \"dotnet\",").unwrap();
        let fd_pos = t.find("pub mod fd;").unwrap();
        assert!(dotnet_pos < fd_pos);
        // idempotent
        let a2 = widen_fd_gate(&f).unwrap();
        assert_eq!(a2, Applied::Skipped);
        fs::remove_dir_all(&dir).ok();
    }

    // ---- fd-impl defer (the following-line key) ----
    #[test]
    fn defer_fd_impls_widens_file_pipe_only() {
        let dir = std::env::temp_dir().join(format!("cd_osfd_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("owned.rs");
        fs::write(
            &f,
            "#[cfg(not(target_os = \"trusty\"))]\nimpl AsFd for fs::File {}\n#[cfg(not(target_os = \"trusty\"))]\nimpl AsFd for crate::net::TcpStream {}\n",
        )
        .unwrap();
        let a = defer_fd_impls(&f).unwrap();
        assert_eq!(a, Applied::Inserted);
        let t = fs::read_to_string(&f).unwrap();
        // the fs::File impl gate is widened...
        assert!(t.contains("all(not(target_os = \"trusty\"), not(target_os = \"dotnet\")))]\nimpl AsFd for fs::File"));
        // ...but the TcpStream impl gate is left ENABLED for dotnet.
        assert!(
            t.contains("#[cfg(not(target_os = \"trusty\"))]\nimpl AsFd for crate::net::TcpStream")
        );
        // idempotent
        let a2 = defer_fd_impls(&f).unwrap();
        assert_eq!(a2, Applied::Skipped);
        fs::remove_dir_all(&dir).ok();
    }
}
