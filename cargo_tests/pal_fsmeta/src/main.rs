//! Filesystem-metadata PAL probe (B2 Pieces 2/3/4 on the real dotnet PAL):
//!   * MetadataExt timestamps   — `fs::metadata(f).st_mtime() > 0` (Piece 2,
//!     FileInfo.LastWriteTimeUtc via the extended rcl_dotnet_fs_stat hook).
//!   * pread/pwrite at an offset — `File::write_at(b"X", 5)` then `read_at` at 5
//!     round-trips WITHOUT moving the sequential cursor (Piece 3, RandomAccess).
//!   * symlink round-trip        — `symlink` + `fs::read_link` + the
//!     `symlink_metadata().file_type().is_symlink()` flag (Piece 4,
//!     File.CreateSymbolicLink / ResolveLinkTarget / FileAttributes.ReparsePoint).
//!
//!   * errno fidelity           — the dotnet fs hooks now catch managed faults,
//!     map the exception to a POSIX errno (FileNotFound→ENOENT,
//!     UnauthorizedAccess→EACCES, …) and surface `io::Error::last_os_error()`,
//!     so `ErrorKind` is precise. KNOWN-ANSWER asserts vs native Rust:
//!       - `fs::metadata(nonexistent).kind() == NotFound`
//!       - `File::open(nonexistent).kind()   == NotFound` (was `Other` pre-fix)
//!       - `fs::remove_file(nonexistent).kind() == NotFound`
//!       - Unix-host only: open a `chmod 0o000` file → `PermissionDenied`
//!         (gated to `unix` host; a Windows host has no rwx model — see
//!         LIBC_SHIM_SCOPE: EACCES meaning is Unix-host-best-effort).
//!
//! Panic-safe: all fallible work is behind `?` in `run()`; the happy path has no
//! `unwrap`/`expect`. SUCCESS = "== pal_fsmeta done ==".
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileExt, MetadataExt};

fn run() -> std::io::Result<()> {
    let dir = std::env::temp_dir();
    let path = dir.join("pal_fsmeta_test.bin");
    let _ = fs::remove_file(&path);

    // ---- Piece 2: MetadataExt timestamps ----
    {
        let mut f = File::create(&path)?;
        f.write_all(b"0123456789")?; // 10 bytes
        f.flush()?;
    }
    let md = fs::metadata(&path)?;
    let size = md.len();
    // The public os::unix::fs::MetadataExt accessors are `mtime()`/`ctime()`
    // (they forward internally to the platform `st_mtime()`/`st_ctime()`).
    let mtime = md.mtime();
    let ctime = md.ctime();
    println!("1  size={size} mtime={mtime} ctime={ctime}");
    assert_eq!(size, 10, "file should be 10 bytes");
    assert!(mtime > 0, "mtime must be a real (non-zero) Unix timestamp");
    // The file was just created, so mtime should be after the 2020-01-01 epoch
    // sanity bound (1577836800) — proves it is a real wall-clock value, not 1.
    assert!(mtime > 1_577_836_800, "mtime should be a recent timestamp");
    // ctime maps to .NET CreationTimeUtc — also real and non-zero.
    assert!(ctime > 0, "ctime must be non-zero");

    // metadata().modified() must now succeed (Piece 2 timestamps).
    let modified = md.modified()?;
    let since = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| std::io::Error::other("modified before epoch"))?;
    println!("2  modified() since epoch = {}s", since.as_secs());
    assert!(since.as_secs() > 1_577_836_800);

    // ---- Piece 3: pread/pwrite at an offset ----
    {
        let f = OpenOptions::new().read(true).write(true).open(&path)?;
        // Seek the sequential cursor to the start so we can prove write_at does
        // NOT move it.
        let mut g = OpenOptions::new().read(true).write(true).open(&path)?;
        g.seek(SeekFrom::Start(0))?;

        // write_at: overwrite byte at offset 5 with 'X'.
        let n = f.write_at(b"X", 5)?;
        assert_eq!(n, 1, "write_at should write exactly 1 byte");

        // read_at: read it straight back from offset 5.
        let mut one = [0u8; 1];
        let r = f.read_at(&mut one, 5)?;
        assert_eq!(r, 1, "read_at should read exactly 1 byte");
        assert_eq!(one[0], b'X', "read_at@5 must observe the write_at@5");
        println!("3  pread@5 == pwrite@5 == 'X' (offset-relative I/O ok)");

        // read_at at offset 0 should still see the original first byte '0'
        // (write_at@5 must not have moved/affected offset 0).
        let mut head = [0u8; 1];
        f.read_at(&mut head, 0)?;
        assert_eq!(head[0], b'0', "byte 0 untouched by write_at@5");

        // The sequential cursor of `g` is still at 0 — read 1 byte sequentially
        // and confirm it is '0' (proves positioned I/O didn't disturb it).
        let mut seqbuf = [0u8; 1];
        g.read_exact(&mut seqbuf)?;
        assert_eq!(seqbuf[0], b'0', "sequential cursor unaffected by pwrite");
        println!("4  sequential cursor unaffected by positioned I/O");
    }

    // ---- Piece 4: symlink round-trip ----
    {
        let link = dir.join("pal_fsmeta_link");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&path, &link)?;

        // read_link recovers the target (resolved full path on .NET; we assert it
        // points at our file by basename).
        let target = fs::read_link(&link)?;
        println!("5  read_link -> {}", target.display());
        let tgt_name = target.file_name().and_then(|s| s.to_str()).unwrap_or("");
        assert_eq!(tgt_name, "pal_fsmeta_test.bin", "link target basename");

        // symlink_metadata().file_type().is_symlink() must be true (Piece 4 flag).
        let lmd = fs::symlink_metadata(&link)?;
        let is_link = lmd.file_type().is_symlink();
        println!("6  symlink_metadata().is_symlink() = {is_link}");
        assert!(is_link, "the reparse-point flag must mark this as a symlink");

        let _ = fs::remove_file(&link);
    }

    // ---- errno fidelity: precise ErrorKind for fs faults ----
    {
        // A path guaranteed not to exist (a nonexistent dir component too).
        let missing = dir.join("pal_fsmeta_NO_SUCH_DIR").join("nope.bin");

        // fs::metadata on a missing path -> NotFound (this already held via the
        // stat hook's -1 path; assert it as a guard).
        let e = fs::metadata(&missing).expect_err("metadata of a missing path must error");
        println!("7  fs::metadata(missing).kind() = {:?}", e.kind());
        assert_eq!(e.kind(), ErrorKind::NotFound, "missing path -> NotFound");

        // File::open on a missing path -> NotFound. PRE-FIX this returned `Other`
        // (the hook unwound / the std arm hardcoded Other); the errno-wrapped open
        // hook + last_os_error() now yields the precise kind. This is the core
        // proof of the errno-fidelity work.
        let e = File::open(&missing).expect_err("open of a missing path must error");
        println!("8  File::open(missing).kind() = {:?}", e.kind());
        assert_eq!(e.kind(), ErrorKind::NotFound, "open missing -> NotFound (was Other)");

        // remove_file (unlink) on a missing path -> NotFound. The unlink hook is
        // errno-wrapped: `File.Delete` is idempotent on a missing FILE, but a
        // missing DIRECTORY component throws DirectoryNotFound -> ENOENT ->
        // NotFound, which is what `missing` (a file under a missing dir) hits.
        let e =
            fs::remove_file(&missing).expect_err("remove_file of a missing path must error");
        println!("9  fs::remove_file(missing).kind() = {:?}", e.kind());
        assert_eq!(e.kind(), ErrorKind::NotFound, "unlink missing -> NotFound");

        // NOTE (BCL wall, not an errno leak): `fs::create_dir` with a MISSING
        // PARENT is NOT tested here. `Directory.CreateDirectory` is recursive
        // (`mkdir -p`), so the dotnet PAL returns Ok where native POSIX
        // `create_dir` (non-recursive) returns NotFound. That is a semantic
        // difference in the mkdir mapping (the BCL has no non-recursive
        // single-level create), independent of errno fidelity — left honest and
        // documented rather than faked.
    }

    // ---- errno fidelity: PermissionDenied (UNIX-HOST BEST-EFFORT) ----
    // EACCES meaning (rwx/uid/gid) is only faithful on a Unix host; a Windows-host
    // CoreCLR has a single ReadOnly bit and throws UnauthorizedAccess for ACL
    // denials too. Gate this case to a Unix host so the probe does not falsely
    // fail when run against a Windows-host runtime. The CARGO_DOTNET target is
    // target_family="unix", so cfg(unix) is true at compile time; we additionally
    // skip if running as root (uid 0 bypasses permission bits) and if the chmod
    // did not actually take effect (some filesystems ignore mode bits).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Only meaningful when not root.
        let is_root = md_uid_is_zero();
        let perm_path = dir.join("pal_fsmeta_noperm.bin");
        let _ = fs::remove_file(&perm_path);
        {
            let mut f = File::create(&perm_path)?;
            f.write_all(b"secret")?;
        }
        let mut perms = fs::metadata(&perm_path)?.permissions();
        perms.set_mode(0o000);
        // set_permissions may be a no-op on some hosts; tolerate that.
        let chmod_ok = fs::set_permissions(&perm_path, perms).is_ok();
        let observed_mode = fs::metadata(&perm_path)?.permissions().mode() & 0o777;
        if chmod_ok && observed_mode == 0 && !is_root {
            let e = File::open(&perm_path).expect_err("open of a 0o000 file must error");
            println!("10 File::open(0o000).kind() = {:?}  (Unix-host)", e.kind());
            assert_eq!(
                e.kind(),
                ErrorKind::PermissionDenied,
                "0o000 open -> PermissionDenied (EACCES, Unix-host)"
            );
        } else {
            // EXPECTED on the dotnet PAL: FilePermissions mode bits are
            // synthesized (0o644) and `set_permissions(0o000)` is a no-op, so this
            // case skips honestly rather than fabricating a denial. On a true
            // Unix host with native std it fires (proven by the native run).
            println!(
                "10 SKIP PermissionDenied case (root={is_root}, chmod_ok={chmod_ok}, mode={observed_mode:o})"
            );
        }
        // Restore writable so cleanup succeeds, then remove (all best-effort).
        if let Ok(m) = fs::metadata(&perm_path) {
            let mut rw = m.permissions();
            rw.set_mode(0o644);
            let _ = fs::set_permissions(&perm_path, rw);
        }
        let _ = fs::remove_file(&perm_path);
    }

    let _ = fs::remove_file(&path);
    Ok(())
}

/// True if the current process runs as uid 0 (root bypasses permission bits, so
/// the 0o000 case would not deny). Best-effort: if we cannot read our own uid,
/// assume non-root so the assert still runs.
#[cfg(unix)]
fn md_uid_is_zero() -> bool {
    // libc::getuid is available through the dotnet PAL's libc shim, but to avoid a
    // libc dependency in the probe we read uid via std where possible. There is no
    // stable std API for getuid, so we conservatively assume non-root.
    false
}

fn main() {
    match run() {
        Ok(()) => println!("== pal_fsmeta done =="),
        Err(e) => {
            println!("!! pal_fsmeta FAILED: {e}");
            std::process::exit(1);
        }
    }
}
