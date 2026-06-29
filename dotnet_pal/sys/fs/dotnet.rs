//! Filesystem for the .NET ("dotnet") platform.
//!
//! Backs `std::fs` (File / read / write / metadata / dirs) with the .NET BCL
//! (`System.IO.FileStream` / `File` / `Directory` / `FileInfo`) through a set of
//! `extern "C"` hooks the cilly linker maps to BCL calls — the same
//! `MissingMethodPatcher` mechanism the alloc / stdio / env / thread arms use.
//! See `cilly/src/ir/builtins/dotnet.rs::insert_dotnet_fs`.
//!
//! Handle model: an open `File` is a `*mut u8` opaque `GCHandle` pinning a
//! managed `FileStream`; a `ReadDir` snapshot is a `*mut u8` `GCHandle` pinning
//! a managed `string[]`. No managed object is ever passed through a Rust
//! signature — only opaque handles and `(ptr, len)` UTF-8 path buffers (decoded
//! BCL-side, mirroring the env arm's `as_encoded_bytes()` convention).
//!
//! FIXED extern contract (the names must match EXACTLY on the linker side):
//!
//! * `rcl_dotnet_fs_open(path_ptr, path_len, mode, access, append) -> *mut u8`
//! * `rcl_dotnet_fs_read(handle, buf_ptr, len) -> isize`
//! * `rcl_dotnet_fs_write(handle, buf_ptr, len) -> isize`
//! * `rcl_dotnet_fs_seek(handle, offset, origin) -> i64`
//! * `rcl_dotnet_fs_flush(handle)`
//! * `rcl_dotnet_fs_close(handle)`
//! * `rcl_dotnet_fs_len(handle) -> i64`
//! * `rcl_dotnet_fs_stat(path_ptr, path_len, out_size, out_is_dir) -> i32`
//! * `rcl_dotnet_fs_exists(path_ptr, path_len) -> i32`
//! * `rcl_dotnet_fs_mkdir(path_ptr, path_len) -> i32`
//! * `rcl_dotnet_fs_rmdir(path_ptr, path_len) -> i32`
//! * `rcl_dotnet_fs_unlink(path_ptr, path_len) -> i32`
//! * `rcl_dotnet_fs_rename(old_ptr, old_len, new_ptr, new_len) -> i32`
//! * `rcl_dotnet_fs_readdir_open(path_ptr, path_len) -> *mut u8`
//! * `rcl_dotnet_fs_readdir_count(handle) -> usize`
//! * `rcl_dotnet_fs_readdir_get(handle, idx) -> *mut u8`  (NUL-term UTF-8, caller frees)
//! * `rcl_dotnet_fs_readdir_close(handle)`
//! * `rcl_dotnet_cotaskmem_free(ptr)`  (shared with args/env — frees readdir_get bufs)
//!
//! REAL (BCL-backed): `open`, `read`, `write`, `seek`, `flush`, `close`, file
//! length (`size`/`tell`), `stat`/`lstat`/`metadata`, `exists`, `mkdir`
//! (`create_dir`), `rmdir`, `unlink` (`remove_file`), `rename`, `readdir`
//! (`read_dir`). `copy` and `remove_dir_all` are delegated to the shared
//! `common` arm (they compose the primitives above).
//!
//! STUBBED to `Err(Unsupported)` (cfg-gated os=dotnet-only; none are exercised
//! by the Phase-4 probe, and being os=dotnet-only they cannot affect the
//! surrogate target or `::stable`):
//!   * `symlink` / `link` / `readlink` — the BCL abstraction here models no
//!     symlinks (`FileType::is_symlink` is always `false`). .NET *does* offer
//!     `File.CreateSymbolicLink` / `CreateHardLink`; wiring them is a follow-up.
//!   * `canonicalize` — could later map to `Path.GetFullPath`. Its only caller
//!     is `common::Dir::open`, and `Dir` is never instantiated by the probe.
//!   * `lstat` — aliases `stat` (no symlink distinction on this PAL).
//!   * `set_perm` / `set_times` / `set_times_nofollow` — `FileAttributes.ReadOnly`
//!     / `File.SetLastWriteTime` exist but minimal-correct skips them.
//!   * `File::fsync` / `File::datasync` — mapped to `flush` (a `FileStream.Flush`
//!     is sufficient for the probe; no separate fdatasync on the abstraction).
//!   * `File::lock` / `lock_shared` / `unlock` / `try_lock` / `try_lock_shared`
//!     — `FileStream` has no advisory-lock API (mirrors `motor.rs`).
//!   * `File::duplicate` — no `FileStream` clone wired up.
//!   * `File::truncate` — could map to `FileStream.SetLength` via an extra hook;
//!     not exercised, so left `Unsupported`.
//!   * `File::set_permissions` — no-op-erroring (no perms model).
//!   * `FileAttr::modified` / `accessed` / `created` — `Err(Unsupported)` for now
//!     (the probe only checks `len`/`is_file`; real impl would read
//!     `FileInfo.LastWriteTimeUtc` etc. via an extended stat hook).
//!   * `FilePermissions::readonly` returns the stored bool (default `false`);
//!     `set_readonly` stores it but it is not persisted to the BCL.
//!
//! SEMANTIC GAP (not a stub, documented): `DirBuilder::mkdir` ->
//! `Directory.CreateDirectory` is recursive AND idempotent, whereas std's
//! `create_dir` expects non-recursive + `AlreadyExists` on an existing dir. The
//! probe calls `create_dir` once on a fresh dir, so it passes; a faithful impl
//! would pre-check `Directory.Exists` and return `AlreadyExists`.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::ffi::{CStr, OsStr, OsString};
use crate::fmt;
use crate::fs::TryLockError;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut, SeekFrom};
use crate::path::{Path, PathBuf};
use crate::sys::os_str::Buf;
use crate::sys::time::SystemTime;
use crate::sys::unsupported;
use crate::sys::FromInner;

// `Dir` lives in the shared `common` arm; `remove_dir_all` is delegated there too (it composes
// `read_dir` / `remove_file` / `remove_dir`, all real on this arm). `copy` is NOT taken from
// `common`: `common::copy` finishes with `writer.set_permissions(from's mode)`, and .NET has no
// Unix permission model so `File::set_permissions` is `Unsupported` — that made `fs::copy` fail at
// the very end after the bytes were already written. A dedicated `copy` below copies the bytes and
// skips the permission propagation (correct for managed files). `exists` is also NOT taken from
// `common` (its `metadata`-then-NotFound path would route through the io-error `Uncategorized` trap).
pub use crate::sys::fs::common::{remove_dir_all, Dir};

/// `std::fs::copy` for the .NET arm: copy the file's bytes, but DON'T propagate the source's Unix
/// mode (managed files have no such model — `File::set_permissions` is `Unsupported`). Mirrors
/// `sys::fs::common::copy` minus the final `set_permissions`.
pub fn copy(from: &Path, to: &Path) -> io::Result<u64> {
    use crate::fs;
    let mut reader = fs::File::open(from)?;
    if !reader.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the source path is not an existing regular file",
        ));
    }
    let mut writer = fs::File::create(to)?;
    io::copy(&mut reader, &mut writer)
}

// ===========================================================================
// PACKAGE A — symbols the `target-family="unix"` flip requires from this arm.
//
// The dotnet fs arm-0 is injected ahead of the
// `any(target_family="unix", target_os="wasi")` cascade arm, so it still wins
// post-flip. But the flip removes the FREE `with_native_path` fallback
// (`sys/fs/mod.rs:55` is `cfg(not(any(target_family="unix",...)))` -> DROPS for
// dotnet), and turns on `os/fd/owned.rs`'s `#[cfg(unix)]` call to
// `crate::sys::fs::debug_assert_fd_is_open`. Both must now be supplied HERE, and
// the injected arm widened to re-export them (see feasibility/dev.sh fs arm).
// ===========================================================================

/// No-CStr `with_native_path` — the unix arm models paths as `CStr`
/// (`run_path_with_cstr`); the dotnet PAL has no CStr path model (paths are UTF-8
/// `(ptr,len)` decoded BCL-side), so this is the verbatim copy of the upstream
/// free fallback body (`sys/fs/mod.rs:55-58`).
///
/// **LEAKY (L6):** interior-NUL paths are not rejected at the std boundary; they
/// surface as a `System.IO` exception rather than `ErrorKind::InvalidInput`.
#[inline]
pub fn with_native_path<T>(path: &Path, f: &dyn Fn(&Path) -> io::Result<T>) -> io::Result<T> {
    f(path)
}

/// No-op `debug_assert_fd_is_open` — the unix arm verifies an fd is open before
/// `close` via `libc::fcntl(F_GETFD)` (a UB-check diagnostic only). `os/fd/owned.rs`
/// calls this under `#[cfg(unix)]` (active post-flip) right before `libc::close`.
/// The dotnet fd-table already validates fds on close, so this is a sound no-op.
/// **CLEAN** (protects the EXISTING-green os::fd from breaking under the flip).
#[allow(unused_variables)]
pub(crate) fn debug_assert_fd_is_open(fd: crate::os::fd::RawFd) {}

// POSIX ownership / named-pipe / chroot primitives. They have NO BCL equivalent
// (CoreCLR exposes no portable uid/gid/chroot/mkfifo), so they compile as
// `Err(Unsupported)` stubs. They MUST EXIST because (a) the unix cascade arm
// re-exports them (`pub use unix::{chown,fchown,lchown,mkfifo,chroot}`) — the
// dotnet arm mirrors that re-export so `sys::fs::chown` resolves — and (b) the
// `os::unix::fs` free fns (`chown`/`fchown`/`lchown`/`chroot`/`mkfifo`) call
// `sys::fs::*` directly (Package B surface). **IMPOSSIBLE (I3):** synthetic
// Unsupported; never succeed at runtime. Signatures mirror `sys/fs/unix.rs`.
pub fn chown(_path: &Path, _uid: u32, _gid: u32) -> io::Result<()> {
    unsupported()
}

pub fn fchown(_fd: crate::os::fd::RawFd, _uid: u32, _gid: u32) -> io::Result<()> {
    unsupported()
}

pub fn lchown(_path: &Path, _uid: u32, _gid: u32) -> io::Result<()> {
    unsupported()
}

pub fn chroot(_dir: &Path) -> io::Result<()> {
    unsupported()
}

pub fn mkfifo(_path: &Path, _mode: u32) -> io::Result<()> {
    unsupported()
}

// FIXED extern contract — mapped to the .NET BCL by the cilly linker. Do not
// rename: the linker keys on these exact symbols.
unsafe extern "C" {
    fn rcl_dotnet_fs_open(
        path_ptr: *const u8,
        path_len: usize,
        mode: i32,
        access: i32,
        append: i32,
    ) -> *mut u8;
    fn rcl_dotnet_fs_read(handle: *mut u8, buf_ptr: *mut u8, len: usize) -> isize;
    fn rcl_dotnet_fs_write(handle: *mut u8, buf_ptr: *const u8, len: usize) -> isize;
    fn rcl_dotnet_fs_seek(handle: *mut u8, offset: i64, origin: i32) -> i64;
    fn rcl_dotnet_fs_flush(handle: *mut u8);
    fn rcl_dotnet_fs_close(handle: *mut u8);
    fn rcl_dotnet_fs_len(handle: *mut u8) -> i64;
    fn rcl_dotnet_fs_set_len(handle: *mut u8, len: i64) -> i32;
    fn rcl_dotnet_fs_stat(
        path_ptr: *const u8,
        path_len: usize,
        out_size: *mut u64,
        out_is_dir: *mut i32,
        // B2 Piece 2/4: real timestamps (Unix seconds) + symlink flag, written
        // only on the success path (rc==0); left untouched on rc==-1.
        out_mtime: *mut i64,
        out_atime: *mut i64,
        out_ctime: *mut i64,
        out_is_symlink: *mut i32,
    ) -> i32;
    fn rcl_dotnet_fs_exists(path_ptr: *const u8, path_len: usize) -> i32;
    fn rcl_dotnet_fs_mkdir(path_ptr: *const u8, path_len: usize) -> i32;
    fn rcl_dotnet_fs_rmdir(path_ptr: *const u8, path_len: usize) -> i32;
    fn rcl_dotnet_fs_unlink(path_ptr: *const u8, path_len: usize) -> i32;
    fn rcl_dotnet_fs_rename(
        old_ptr: *const u8,
        old_len: usize,
        new_ptr: *const u8,
        new_len: usize,
    ) -> i32;
    fn rcl_dotnet_fs_readdir_open(path_ptr: *const u8, path_len: usize) -> *mut u8;
    fn rcl_dotnet_fs_readdir_count(handle: *mut u8) -> usize;
    fn rcl_dotnet_fs_readdir_get(handle: *mut u8, idx: usize) -> *mut u8;
    fn rcl_dotnet_fs_readdir_close(handle: *mut u8);
    // B2 Piece 3: offset-relative I/O via System.IO.RandomAccess (does NOT move
    // the FileStream position). `offset` is i64.
    fn rcl_dotnet_fs_read_at(handle: *mut u8, buf_ptr: *mut u8, len: usize, offset: i64) -> isize;
    fn rcl_dotnet_fs_write_at(handle: *mut u8, buf_ptr: *const u8, len: usize, offset: i64) -> isize;
    // B2 Piece 4: symlink create / resolve. symlink returns 0 (throws on failure).
    // readlink returns a NUL-terminated UTF-8 C string (freed by
    // rcl_dotnet_cotaskmem_free) or NULL (not a link / not found).
    fn rcl_dotnet_fs_symlink(
        link_ptr: *const u8,
        link_len: usize,
        target_ptr: *const u8,
        target_len: usize,
    ) -> i32;
    fn rcl_dotnet_fs_readlink(path_ptr: *const u8, path_len: usize) -> *mut u8;
    /// Shared with args/env: `Marshal.FreeCoTaskMem`. Frees a buffer returned by
    /// `rcl_dotnet_fs_readdir_get`.
    fn rcl_dotnet_cotaskmem_free(ptr: *mut u8);
}

// .NET `System.IO.FileMode` (int-backed enum) values the BCL ctor expects.
const FILE_MODE_CREATE_NEW: i32 = 1;
const FILE_MODE_CREATE: i32 = 2;
const FILE_MODE_OPEN: i32 = 3;
const FILE_MODE_OPEN_OR_CREATE: i32 = 4;
const FILE_MODE_TRUNCATE: i32 = 5;
const FILE_MODE_APPEND: i32 = 6;
// .NET `System.IO.FileAccess`: Read=1, Write=2, ReadWrite=3.
const FILE_ACCESS_READ: i32 = 1;
const FILE_ACCESS_WRITE: i32 = 2;
const FILE_ACCESS_READ_WRITE: i32 = 3;
// .NET `System.IO.SeekOrigin`: Begin=0, Current=1, End=2.
const SEEK_ORIGIN_BEGIN: i32 = 0;
const SEEK_ORIGIN_CURRENT: i32 = 1;
const SEEK_ORIGIN_END: i32 = 2;

/// Map a `0 => Ok(()) / nonzero => Err` integer return code from an fs hook.
///
/// PAL-fidelity: the mutating fs hooks (`mkdir`/`rmdir`/`unlink`/`rename`) are
/// now wrapped in the cilly `errno_wrapped` machinery, so on a managed fault
/// they set the thread-local `errno` (FileNotFound→ENOENT,
/// UnauthorizedAccess→EACCES, …) and return `-1` instead of unwinding. We surface
/// that as `io::Error::last_os_error()`, which decodes the precise `ErrorKind`
/// (`NotFound` / `PermissionDenied` / …) via `decode_error_kind`. If `errno`
/// happens to be 0 (a non-fault nonzero rc — none of the current hooks produce
/// one), `last_os_error()` still yields a sensible `Uncategorized`-class error.
fn rc(code: i32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Borrow a `&Path` as the `(ptr, len)` UTF-8 byte buffer the hooks expect
/// (mirrors the env arm's `as_encoded_bytes()` convention).
#[inline]
fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileType {
    is_dir: bool,
    // B2 Piece 4: real symlink flag (from FileAttributes.ReparsePoint via the
    // stat hook). lstat does not currently follow links differently from stat,
    // but the flag IS meaningful because the underlying GetAttributes reports
    // the reparse-point bit on the symlink target path.
    is_symlink: bool,
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn is_file(&self) -> bool {
        !self.is_dir
    }

    pub fn is_symlink(&self) -> bool {
        // B2 Piece 4: REAL — set when FileAttributes.ReparsePoint was present.
        self.is_symlink
    }

    /// DOTNET PAL ARM (Package A/B) — `os::unix::fs::FileTypeExt` queries
    /// `self.as_inner().is(libc::S_IFBLK)` etc. The dotnet `FileType` carries a
    /// dir/file flag (+ symlink), so synthesize the `S_IFMT` masked type and
    /// compare. **LEAKY (L3):** block/char/fifo/socket are never modelled by the
    /// BCL, so `is(S_IFBLK/S_IFCHR/S_IFIFO/S_IFSOCK)` always answers `false`;
    /// `S_IFDIR`/`S_IFREG`/`S_IFLNK` can be `true`.
    pub fn is(&self, mode: i32) -> bool {
        // `mode` is a `libc::S_IF*` const, typed `c_int` (i32) in the dotnet libc
        // face — match that here.
        const S_IFMT: i32 = 0o170000;
        const S_IFDIR: i32 = 0o040000;
        const S_IFREG: i32 = 0o100000;
        const S_IFLNK: i32 = 0o120000;
        let synthetic = if self.is_symlink {
            S_IFLNK
        } else if self.is_dir {
            S_IFDIR
        } else {
            S_IFREG
        };
        mode & S_IFMT == synthetic
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FilePermissions {
    readonly: bool,
}

impl FilePermissions {
    pub fn readonly(&self) -> bool {
        self.readonly
    }

    pub fn set_readonly(&mut self, readonly: bool) {
        // Stored but not persisted to the BCL (no perms model on this arm).
        self.readonly = readonly;
    }

    /// DOTNET PAL ARM (Package A/B stub) — `os::unix::fs::PermissionsExt::mode`.
    /// The dotnet `FilePermissions` only carries a readonly bool; synthesize a
    /// conventional POSIX mode (0o444 read-only, else 0o644). **LEAKY (L2):** the
    /// real per-class rwx / suid / sgid / sticky bits are not modelled by the BCL.
    pub fn mode(&self) -> u32 {
        if self.readonly { 0o444 } else { 0o644 }
    }
}

/// DOTNET PAL ARM (Package A/B stub) — `os::unix::fs::PermissionsExt::{set_mode,
/// from_mode}` build a `FilePermissions` from a raw POSIX mode via
/// `FromInner::from_inner(mode)`. The dotnet model only honours the read-only
/// bit, so derive readonly from "no owner-write bit" (`0o200`). **LEAKY (L2).**
impl FromInner<u32> for FilePermissions {
    fn from_inner(mode: u32) -> FilePermissions {
        FilePermissions { readonly: mode & 0o200 == 0 }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FileTimes {}

impl FileTimes {
    pub fn set_accessed(&mut self, _t: SystemTime) {}
    pub fn set_modified(&mut self, _t: SystemTime) {}
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct FileAttr {
    size: u64,
    is_dir: bool,
    // B2 Piece 2/4: real Unix-seconds timestamps (0 when unknown, e.g. the
    // live-FileStream `file_attr()` path that can't cheaply re-stat) + symlink
    // flag (from FileAttributes.ReparsePoint).
    mtime: i64,
    atime: i64,
    ctime: i64,
    is_symlink: bool,
}

impl FileAttr {
    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn perm(&self) -> FilePermissions {
        FilePermissions { readonly: false }
    }

    pub fn file_type(&self) -> FileType {
        FileType { is_dir: self.is_dir, is_symlink: self.is_symlink }
    }

    // B2 Piece 2: real timestamps via the path-stat hook (FileInfo getters).
    // A zero value means "not available" (the live-FileStream path) and surfaces
    // as Unsupported so callers fall back gracefully rather than reporting 1970.
    pub fn modified(&self) -> io::Result<SystemTime> {
        time_from_unix_secs(self.mtime)
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        time_from_unix_secs(self.atime)
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        time_from_unix_secs(self.ctime)
    }

    /// Raw Unix-seconds accessors for the .NET `MetadataExt::st_{m,a,c}time`.
    #[inline]
    pub fn mtime(&self) -> i64 {
        self.mtime
    }
    #[inline]
    pub fn atime(&self) -> i64 {
        self.atime
    }
    #[inline]
    pub fn ctime(&self) -> i64 {
        self.ctime
    }
}

/// Map Unix-seconds (i64) to a `SystemTime`, or `Unsupported` when zero (the
/// timestamp wasn't recovered — e.g. a live FileStream that can't re-stat).
fn time_from_unix_secs(secs: i64) -> io::Result<SystemTime> {
    if secs <= 0 {
        return unsupported();
    }
    crate::sys::time::UNIX_EPOCH
        .checked_add_duration(&crate::time::Duration::from_secs(secs as u64))
        .ok_or_else(|| io::const_error!(io::ErrorKind::Uncategorized, "timestamp overflow"))
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions {
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
        }
    }

    pub fn read(&mut self, read: bool) {
        self.read = read;
    }
    pub fn write(&mut self, write: bool) {
        self.write = write;
    }
    pub fn append(&mut self, append: bool) {
        self.append = append;
    }
    pub fn truncate(&mut self, truncate: bool) {
        self.truncate = truncate;
    }
    pub fn create(&mut self, create: bool) {
        self.create = create;
    }
    pub fn create_new(&mut self, create_new: bool) {
        self.create_new = create_new;
    }

    /// DOTNET PAL ARM (Package A/B stub) — `os::unix::fs::OpenOptionsExt::mode`.
    /// .NET's `FileStream` ctor cannot set a POSIX creation mode, so this is
    /// stored-and-ignored. **LEAKY (L1):** the `mode` is silently dropped.
    pub fn mode(&mut self, _mode: u32) {}

    /// DOTNET PAL ARM (Package A/B stub) —
    /// `os::unix::fs::OpenOptionsExt::custom_flags`. .NET's `FileStream` has no raw
    /// `O_*` passthrough (`O_NOFOLLOW`/`O_DIRECT`/...), so flags are
    /// stored-and-ignored. **LEAKY (L1 / I4).**
    pub fn custom_flags(&mut self, _flags: i32) {}

    /// Compute the `(FileMode, FileAccess)` int pair the `FileStream` ctor wants
    /// from the high-level open flags, mapped to .NET enum semantics. Modelled
    /// on the unix `OpenOptions` precedence; returns `InvalidInput` for the
    /// spec-invalid combinations std itself rejects.
    fn to_mode_access(&self) -> io::Result<(i32, i32)> {
        // .NET FileMode.Append is special: it forces write-only access and
        // positions at end-of-file. It composes with create (open-or-create) but
        // is mutually exclusive with read/truncate on .NET, mirroring std's own
        // append/read+truncate rejections (which std applies before us).
        if self.append {
            // create_new still takes precedence (atomic-create semantics).
            let mode = if self.create_new { FILE_MODE_CREATE_NEW } else { FILE_MODE_APPEND };
            return Ok((mode, FILE_ACCESS_WRITE));
        }

        // Access mode (write implied by truncate/create requests on .NET).
        let access = match (self.read, self.write) {
            (true, true) => FILE_ACCESS_READ_WRITE,
            (true, false) => FILE_ACCESS_READ,
            (false, true) => FILE_ACCESS_WRITE,
            (false, false) => {
                return Err(io::const_error!(
                    io::ErrorKind::InvalidInput,
                    "no read/write/append access requested"
                ));
            }
        };

        // File mode precedence: create_new > (create, truncate combos) > open.
        let mode = if self.create_new {
            FILE_MODE_CREATE_NEW
        } else {
            match (self.create, self.truncate) {
                (true, true) => FILE_MODE_CREATE, // create + truncate == always-fresh
                (true, false) => FILE_MODE_OPEN_OR_CREATE,
                (false, true) => FILE_MODE_TRUNCATE, // truncate an existing file
                (false, false) => FILE_MODE_OPEN,
            }
        };

        Ok((mode, access))
    }
}

pub struct File {
    /// Opaque managed `GCHandle` `IntPtr` pinning a `System.IO.FileStream`.
    handle: *mut u8,
}

// SAFETY: the handle is an opaque managed `GCHandle` `IntPtr`; moving it between
// threads is sound (it identifies a managed `FileStream`, not thread-affine
// native state). Mirrors the `Thread` arm.
unsafe impl Send for File {}
unsafe impl Sync for File {}

impl File {
    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
        let (mode, access) = opts.to_mode_access()?;
        let bytes = path_bytes(path);
        // SAFETY: `(ptr, len)` describes a readable UTF-8 region for the call;
        // the hook only reads it. Returns an opaque non-null handle, or a
        // managed exception unwinds on failure.
        let handle = unsafe {
            rcl_dotnet_fs_open(
                bytes.as_ptr(),
                bytes.len(),
                mode,
                access,
                opts.append as i32,
            )
        };
        if handle.is_null() {
            // PAL-fidelity: the `rcl_dotnet_fs_open` hook catches a managed fault,
            // sets the thread-local `errno` (FileNotFound→ENOENT,
            // UnauthorizedAccess→EACCES, …) and returns null; surface the precise
            // `ErrorKind` (`NotFound` / `PermissionDenied` / …) via
            // `last_os_error()` instead of a coarse `Other`.
            return Err(io::Error::last_os_error());
        }
        Ok(File { handle })
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        // Only the length is recoverable from a live FileStream handle here.
        // Timestamps are left 0 (LEAKY, documented): there is no cheap fstat on a
        // live handle, and `modified()`/`accessed()`/`created()` surface
        // Unsupported for a 0. Path-based `stat`/`metadata` DO carry real times.
        // SAFETY: `self.handle` is a live FileStream handle.
        let len = unsafe { rcl_dotnet_fs_len(self.handle) };
        Ok(FileAttr {
            size: len.max(0) as u64,
            is_dir: false,
            mtime: 0,
            atime: 0,
            ctime: 0,
            is_symlink: false,
        })
    }

    pub fn fsync(&self) -> io::Result<()> {
        // No fdatasync on the abstraction; a flush is sufficient for the probe.
        self.flush()
    }

    pub fn datasync(&self) -> io::Result<()> {
        self.flush()
    }

    pub fn lock(&self) -> io::Result<()> {
        unsupported()
    }

    pub fn lock_shared(&self) -> io::Result<()> {
        unsupported()
    }

    pub fn try_lock(&self) -> Result<(), TryLockError> {
        Err(TryLockError::Error(io::Error::from(io::ErrorKind::Unsupported)))
    }

    pub fn try_lock_shared(&self) -> Result<(), TryLockError> {
        Err(TryLockError::Error(io::Error::from(io::ErrorKind::Unsupported)))
    }

    pub fn unlock(&self) -> io::Result<()> {
        unsupported()
    }

    pub fn truncate(&self, size: u64) -> io::Result<()> {
        // FileStream.SetLength(len): truncates or zero-grows the file to `size`.
        // SAFETY: `self.handle` is a live FileStream handle.
        rc(unsafe { rcl_dotnet_fs_set_len(self.handle, size as i64) })
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: `(buf.as_mut_ptr(), buf.len())` is an exclusively-borrowed
        // writable region; the hook writes at most `len` bytes into it.
        let n = unsafe { rcl_dotnet_fs_read(self.handle, buf.as_mut_ptr(), buf.len()) };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "read failed"));
        }
        Ok(n as usize)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        crate::io::default_read_vectored(|b| self.read(b), bufs)
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn read_buf(&self, cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        crate::io::default_read_buf(|buf| self.read(buf), cursor)
    }

    // =======================================================================
    // DOTNET PAL ARM (B2 Piece 3) — os::unix::fs::FileExt positioned I/O, REAL.
    //
    // The unix arm backs these with `pread`/`pwrite` (atomic offset-relative I/O
    // that does NOT move the stream position). .NET's `RandomAccess.{Read,Write}`
    // is the managed equivalent, wired via the new `rcl_dotnet_fs_read_at` /
    // `_write_at` hooks (cilly dotnet.rs) over the FileStream's SafeFileHandle.
    // read_buf_at/read_vectored_at/write_vectored_at delegate to the
    // `crate::io::default_*` adapters over read_at/write_at, exactly like the
    // sequential read_vectored/read_buf above.
    // =======================================================================

    pub fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: writable region; the hook reads at most `len` bytes at `offset`.
        let n = unsafe {
            rcl_dotnet_fs_read_at(self.handle, buf.as_mut_ptr(), buf.len(), offset as i64)
        };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "read_at failed"));
        }
        Ok(n as usize)
    }

    pub fn read_buf_at(&self, cursor: BorrowedCursor<'_, u8>, offset: u64) -> io::Result<()> {
        crate::io::default_read_buf(|buf| self.read_at(buf, offset), cursor)
    }

    pub fn read_vectored_at(&self, bufs: &mut [IoSliceMut<'_>], offset: u64) -> io::Result<usize> {
        crate::io::default_read_vectored(|b| self.read_at(b, offset), bufs)
    }

    pub fn write_at(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: readable region; the hook writes all `len` bytes at `offset`.
        let n = unsafe {
            rcl_dotnet_fs_write_at(self.handle, buf.as_ptr(), buf.len(), offset as i64)
        };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "write_at failed"));
        }
        Ok(n as usize)
    }

    pub fn write_vectored_at(&self, bufs: &[IoSlice<'_>], offset: u64) -> io::Result<usize> {
        crate::io::default_write_vectored(|b| self.write_at(b, offset), bufs)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: `(buf.as_ptr(), buf.len())` is a readable region; the hook
        // only reads it and writes all `len` bytes to the stream (or throws).
        let n = unsafe { rcl_dotnet_fs_write(self.handle, buf.as_ptr(), buf.len()) };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "write failed"));
        }
        Ok(n as usize)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        crate::io::default_write_vectored(|b| self.write(b), bufs)
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }

    pub fn flush(&self) -> io::Result<()> {
        // SAFETY: `self.handle` is a live FileStream handle.
        unsafe { rcl_dotnet_fs_flush(self.handle) };
        Ok(())
    }

    pub fn seek(&self, pos: SeekFrom) -> io::Result<u64> {
        let (offset, origin) = match pos {
            SeekFrom::Start(off) => (off as i64, SEEK_ORIGIN_BEGIN),
            SeekFrom::Current(off) => (off, SEEK_ORIGIN_CURRENT),
            SeekFrom::End(off) => (off, SEEK_ORIGIN_END),
        };
        // SAFETY: `self.handle` is a live FileStream handle.
        let pos = unsafe { rcl_dotnet_fs_seek(self.handle, offset, origin) };
        if pos < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "seek failed"));
        }
        Ok(pos as u64)
    }

    pub fn size(&self) -> Option<io::Result<u64>> {
        // SAFETY: `self.handle` is a live FileStream handle.
        let len = unsafe { rcl_dotnet_fs_len(self.handle) };
        Some(Ok(len.max(0) as u64))
    }

    pub fn tell(&self) -> io::Result<u64> {
        self.seek(SeekFrom::Current(0))
    }

    pub fn duplicate(&self) -> io::Result<File> {
        unsupported()
    }

    pub fn set_permissions(&self, _perm: FilePermissions) -> io::Result<()> {
        unsupported()
    }

    pub fn set_times(&self, _times: FileTimes) -> io::Result<()> {
        unsupported()
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: `self.handle` is a live FileStream handle; the hook
            // disposes the stream and frees the GCHandle exactly once (Drop).
            unsafe { rcl_dotnet_fs_close(self.handle) };
        }
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("File").field("handle", &self.handle).finish()
    }
}

#[derive(Debug)]
pub struct DirBuilder {}

impl DirBuilder {
    pub fn new() -> DirBuilder {
        DirBuilder {}
    }

    pub fn mkdir(&self, p: &Path) -> io::Result<()> {
        let bytes = path_bytes(p);
        // SAFETY: `(ptr, len)` describes a readable UTF-8 region for the call.
        // NOTE: Directory.CreateDirectory is recursive + idempotent (see the
        // module-doc semantic-gap note).
        rc(unsafe { rcl_dotnet_fs_mkdir(bytes.as_ptr(), bytes.len()) })
    }

    /// DOTNET PAL ARM (Package A/B stub) — `os::unix::fs::DirBuilderExt::mode`.
    /// .NET's `Directory.CreateDirectory` takes no POSIX creation mode, so this is
    /// stored-and-ignored (the `DirBuilder` is a ZST). **LEAKY (L1).**
    pub fn set_mode(&mut self, _mode: u32) {}
}

/// A snapshot of a directory's entries (a managed `string[]` GCHandle), iterated
/// by index. Closed (GCHandle freed) on `Drop`.
pub struct ReadDir {
    handle: *mut u8,
    idx: usize,
    len: usize,
}

impl Drop for ReadDir {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: `self.handle` is a live string[] handle freed once (Drop).
            unsafe { rcl_dotnet_fs_readdir_close(self.handle) };
        }
    }
}

impl fmt::Debug for ReadDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadDir").field("len", &self.len).field("idx", &self.idx).finish()
    }
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        if self.idx >= self.len {
            return None;
        }
        let i = self.idx;
        self.idx += 1;
        // SAFETY: `self.handle` is a live string[] handle and `i < len`. The
        // returned buffer is a freshly-allocated NUL-terminated UTF-8 C string
        // we own and free below after copying it out.
        let ptr = unsafe { rcl_dotnet_fs_readdir_get(self.handle, i) };
        if ptr.is_null() {
            return Some(Err(io::const_error!(io::ErrorKind::Other, "readdir entry failed")));
        }
        // SAFETY: `ptr` is a valid NUL-terminated C string until freed below.
        let bytes = unsafe { CStr::from_ptr(ptr.cast()) }.to_bytes().to_vec();
        // SAFETY: `ptr` came from the hook and has not been freed; the bytes
        // were copied out, so releasing it now is sound.
        unsafe { rcl_dotnet_cotaskmem_free(ptr) };
        // GetFileSystemEntries returns full paths already, so the entry path is
        // the buffer verbatim. Build the OsString via the platform-agnostic
        // `Buf` + `FromInner` (no unix-only `OsStringExt`), matching the env arm.
        let path = PathBuf::from(OsString::from_inner(Buf { inner: bytes }));
        Some(Ok(DirEntry { path }))
    }
}

pub struct DirEntry {
    path: PathBuf,
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn file_name(&self) -> OsString {
        // Strip the leaf std-side rather than trusting the BCL.
        self.path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| self.path.as_os_str().to_os_string())
    }

    pub fn metadata(&self) -> io::Result<FileAttr> {
        stat(&self.path)
    }

    pub fn file_type(&self) -> io::Result<FileType> {
        Ok(stat(&self.path)?.file_type())
    }

    /// DOTNET PAL ARM (Package A/B stub) — `os::unix::fs::DirEntryExt::ino`. There
    /// is no inode identity on CoreCLR (**IMPOSSIBLE / I1**), so synthetic `0`.
    pub fn ino(&self) -> u64 {
        0
    }

    /// DOTNET PAL ARM (Package A/B) — `os::unix::fs::DirEntryExt2::file_name_ref`
    /// borrows the leaf as `&OsStr`. Real (the dotnet DirEntry stores the full
    /// `PathBuf`); just strip the leaf without an allocation.
    pub fn file_name_os_str(&self) -> &OsStr {
        self.path.file_name().unwrap_or_else(|| self.path.as_os_str())
    }
}

pub fn readdir(p: &Path) -> io::Result<ReadDir> {
    let bytes = path_bytes(p);
    // SAFETY: `(ptr, len)` describes a readable UTF-8 region for the call;
    // returns an opaque non-null handle, or a managed exception unwinds.
    let handle = unsafe { rcl_dotnet_fs_readdir_open(bytes.as_ptr(), bytes.len()) };
    if handle.is_null() {
        return Err(io::const_error!(io::ErrorKind::NotFound, "read_dir failed"));
    }
    // SAFETY: `handle` is a live string[] handle.
    let len = unsafe { rcl_dotnet_fs_readdir_count(handle) };
    Ok(ReadDir { handle, idx: 0, len })
}

pub fn unlink(p: &Path) -> io::Result<()> {
    let bytes = path_bytes(p);
    // SAFETY: `(ptr, len)` describes a readable UTF-8 region for the call.
    rc(unsafe { rcl_dotnet_fs_unlink(bytes.as_ptr(), bytes.len()) })
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    let ob = path_bytes(old);
    let nb = path_bytes(new);
    // SAFETY: both `(ptr, len)` pairs describe readable UTF-8 regions.
    rc(unsafe { rcl_dotnet_fs_rename(ob.as_ptr(), ob.len(), nb.as_ptr(), nb.len()) })
}

pub fn set_perm(_p: &Path, _perm: FilePermissions) -> io::Result<()> {
    // No perms model on this arm (see module-doc STUBBED list).
    unsupported()
}

pub fn set_times(_p: &Path, _times: FileTimes) -> io::Result<()> {
    unsupported()
}

pub fn set_times_nofollow(_p: &Path, _times: FileTimes) -> io::Result<()> {
    unsupported()
}

pub fn rmdir(p: &Path) -> io::Result<()> {
    let bytes = path_bytes(p);
    // SAFETY: `(ptr, len)` describes a readable UTF-8 region for the call.
    rc(unsafe { rcl_dotnet_fs_rmdir(bytes.as_ptr(), bytes.len()) })
}

pub fn exists(path: &Path) -> io::Result<bool> {
    let bytes = path_bytes(path);
    // SAFETY: `(ptr, len)` describes a readable UTF-8 region for the call.
    // The hook is bool-valued (1/0), never errno-based, so this cannot surface
    // the io-error `Uncategorized` trap on a missing path.
    let r = unsafe { rcl_dotnet_fs_exists(bytes.as_ptr(), bytes.len()) };
    Ok(r != 0)
}

pub fn readlink(p: &Path) -> io::Result<PathBuf> {
    // B2 Piece 4: REAL — File.ResolveLinkTarget(path, returnFinalTarget=false).
    // The hook returns a freshly-allocated NUL-terminated UTF-8 C string (the
    // resolved target's full path), or NULL when `p` is not a symlink / missing.
    let bytes = path_bytes(p);
    // SAFETY: `(ptr, len)` is a readable UTF-8 region; the hook reads it and
    // returns an owned C string (or null).
    let ptr = unsafe { rcl_dotnet_fs_readlink(bytes.as_ptr(), bytes.len()) };
    if ptr.is_null() {
        return Err(io::const_error!(io::ErrorKind::NotFound, "not a symbolic link"));
    }
    // SAFETY: `ptr` is a valid NUL-terminated C string until freed below.
    let out = unsafe { CStr::from_ptr(ptr.cast()) }.to_bytes().to_vec();
    // SAFETY: bytes copied out; releasing the hook buffer is sound.
    unsafe { rcl_dotnet_cotaskmem_free(ptr) };
    // Build the PathBuf platform-agnostically (Buf + FromInner), matching readdir.
    Ok(PathBuf::from(OsString::from_inner(Buf { inner: out })))
}

pub fn symlink(original: &Path, link: &Path) -> io::Result<()> {
    // B2 Piece 4: REAL — File.CreateSymbolicLink(link, original). `original` is
    // the target the link points at; `link` is the new symlink location.
    let target = path_bytes(original);
    let link_b = path_bytes(link);
    // SAFETY: both `(ptr, len)` pairs are readable UTF-8 regions the hook reads.
    let rc = unsafe {
        rcl_dotnet_fs_symlink(link_b.as_ptr(), link_b.len(), target.as_ptr(), target.len())
    };
    if rc != 0 {
        return Err(io::const_error!(io::ErrorKind::Other, "symlink failed"));
    }
    Ok(())
}

pub fn link(_src: &Path, _dst: &Path) -> io::Result<()> {
    // STAYS STUBBED (honest): .NET has NO managed `File.CreateHardLink` in the
    // BCL — only Win32 P/Invoke or libc `link(2)`, neither portable here. Hard
    // links have no clean managed path, so this remains Unsupported (I3).
    unsupported()
}

pub fn stat(p: &Path) -> io::Result<FileAttr> {
    let bytes = path_bytes(p);
    let mut size: u64 = 0;
    let mut is_dir: i32 = 0;
    // B2 Piece 2/4: timestamps (Unix seconds) + symlink flag out-locals.
    let mut mtime: i64 = 0;
    let mut atime: i64 = 0;
    let mut ctime: i64 = 0;
    let mut is_symlink: i32 = 0;
    // SAFETY: `(ptr, len)` describes a readable UTF-8 region; every `&mut` is a
    // valid out-pointer the hook writes through (only on the success path).
    let rc = unsafe {
        rcl_dotnet_fs_stat(
            bytes.as_ptr(),
            bytes.len(),
            &mut size as *mut u64,
            &mut is_dir as *mut i32,
            &mut mtime as *mut i64,
            &mut atime as *mut i64,
            &mut ctime as *mut i64,
            &mut is_symlink as *mut i32,
        )
    };
    if rc == -1 {
        // CRITICAL: must be NotFound so `exists`/`remove_dir_all`/`copy`'s
        // NotFound-keyed logic (and std::fs::metadata callers) behave.
        return Err(io::const_error!(io::ErrorKind::NotFound, "no such file or directory"));
    }
    if rc != 0 {
        return Err(io::const_error!(io::ErrorKind::Uncategorized, "stat failed"));
    }
    Ok(FileAttr {
        size,
        is_dir: is_dir != 0,
        mtime,
        atime,
        ctime,
        is_symlink: is_symlink != 0,
    })
}

pub fn lstat(p: &Path) -> io::Result<FileAttr> {
    // B2 Piece 4: aliases `stat`, but the FileAttr now carries a meaningful
    // `is_symlink` flag (from FileAttributes.ReparsePoint), so
    // `symlink_metadata().file_type().is_symlink()` is correct. LEAKY: this does
    // not avoid following the link for size/timestamps the way a true lstat would
    // — the BCL static getters resolve the target — but the type is reported.
    stat(p)
}

pub fn canonicalize(_p: &Path) -> io::Result<PathBuf> {
    // Could map to Path.GetFullPath; not exercised (STUBBED).
    unsupported()
}
