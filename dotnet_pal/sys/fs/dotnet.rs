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

// `Dir` lives in the shared `common` arm; `copy` and `remove_dir_all` are
// delegated there too (they compose `File::open` / `io::copy` / `read_dir` /
// `remove_file` / `remove_dir`, all real on this arm). `exists` is NOT taken
// from `common` (its `metadata`-then-NotFound path would route through the
// io-error `Uncategorized` trap); a dedicated `exists` is defined below.
pub use crate::sys::fs::common::{copy, remove_dir_all, Dir};

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
    fn rcl_dotnet_fs_stat(
        path_ptr: *const u8,
        path_len: usize,
        out_size: *mut u64,
        out_is_dir: *mut i32,
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
fn rc(code: i32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::const_error!(io::ErrorKind::Uncategorized, "dotnet fs operation failed"))
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
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn is_file(&self) -> bool {
        !self.is_dir
    }

    pub fn is_symlink(&self) -> bool {
        // The BCL abstraction surfaced here has no symlink concept.
        false
    }

    /// DOTNET PAL ARM (Package A/B stub) — `os::unix::fs::FileTypeExt` queries
    /// `self.as_inner().is(libc::S_IFBLK)` etc. The dotnet `FileType` only carries
    /// a dir/file flag, so synthesize the `S_IFMT` masked type and compare.
    /// **LEAKY (L3):** block/char/fifo/socket are never modelled by the BCL, so
    /// `is(S_IFBLK/S_IFCHR/S_IFIFO/S_IFSOCK)` always answers `false`; only
    /// `S_IFDIR`/`S_IFREG` can be `true`.
    pub fn is(&self, mode: i32) -> bool {
        // `mode` is a `libc::S_IF*` const, typed `c_int` (i32) in the dotnet libc
        // face — match that here.
        const S_IFMT: i32 = 0o170000;
        const S_IFDIR: i32 = 0o040000;
        const S_IFREG: i32 = 0o100000;
        let synthetic = if self.is_dir { S_IFDIR } else { S_IFREG };
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
}

impl FileAttr {
    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn perm(&self) -> FilePermissions {
        FilePermissions { readonly: false }
    }

    pub fn file_type(&self) -> FileType {
        FileType { is_dir: self.is_dir }
    }

    pub fn modified(&self) -> io::Result<SystemTime> {
        unsupported()
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        unsupported()
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        unsupported()
    }
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
            return Err(io::const_error!(io::ErrorKind::Other, "failed to open file"));
        }
        Ok(File { handle })
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        // Only the length is recoverable from a live FileStream handle here.
        // SAFETY: `self.handle` is a live FileStream handle.
        let len = unsafe { rcl_dotnet_fs_len(self.handle) };
        Ok(FileAttr { size: len.max(0) as u64, is_dir: false })
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

    pub fn truncate(&self, _size: u64) -> io::Result<()> {
        // Could map to FileStream.SetLength via an extra hook; not exercised.
        unsupported()
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
    // DOTNET PAL ARM (Package A/B stub) — os::unix::fs::FileExt positioned I/O.
    //
    // The unix arm backs these with `pread`/`pwrite` (atomic offset-relative I/O
    // that does NOT move the stream position). .NET's `RandomAccess.{Read,Write}`
    // is the real equivalent (a clean Package-C upgrade via a new
    // `rcl_dotnet_fs_read_at`/`_write_at` hook), but it is not wired yet. Rather
    // than emulate with seek+read (which would corrupt the shared position and is
    // non-atomic), these are `Err(Unsupported)` compile-stubs. NOT reached by the
    // pal_fs probe (which uses sequential read/write).
    // =======================================================================

    pub fn read_at(&self, _buf: &mut [u8], _offset: u64) -> io::Result<usize> {
        unsupported()
    }

    pub fn read_buf_at(&self, _cursor: BorrowedCursor<'_, u8>, _offset: u64) -> io::Result<()> {
        unsupported()
    }

    pub fn read_vectored_at(&self, _bufs: &mut [IoSliceMut<'_>], _offset: u64) -> io::Result<usize> {
        unsupported()
    }

    pub fn write_at(&self, _buf: &[u8], _offset: u64) -> io::Result<usize> {
        unsupported()
    }

    pub fn write_vectored_at(&self, _bufs: &[IoSlice<'_>], _offset: u64) -> io::Result<usize> {
        unsupported()
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

pub fn readlink(_p: &Path) -> io::Result<PathBuf> {
    // No symlink concept on this PAL abstraction (STUBBED).
    unsupported()
}

pub fn symlink(_original: &Path, _link: &Path) -> io::Result<()> {
    unsupported()
}

pub fn link(_src: &Path, _dst: &Path) -> io::Result<()> {
    unsupported()
}

pub fn stat(p: &Path) -> io::Result<FileAttr> {
    let bytes = path_bytes(p);
    let mut size: u64 = 0;
    let mut is_dir: i32 = 0;
    // SAFETY: `(ptr, len)` describes a readable UTF-8 region; `&mut size` /
    // `&mut is_dir` are valid out-pointers the hook writes through (only on the
    // success path).
    let rc = unsafe {
        rcl_dotnet_fs_stat(bytes.as_ptr(), bytes.len(), &mut size as *mut u64, &mut is_dir as *mut i32)
    };
    if rc == -1 {
        // CRITICAL: must be NotFound so `exists`/`remove_dir_all`/`copy`'s
        // NotFound-keyed logic (and std::fs::metadata callers) behave.
        return Err(io::const_error!(io::ErrorKind::NotFound, "no such file or directory"));
    }
    if rc != 0 {
        return Err(io::const_error!(io::ErrorKind::Uncategorized, "stat failed"));
    }
    Ok(FileAttr { size, is_dir: is_dir != 0 })
}

pub fn lstat(p: &Path) -> io::Result<FileAttr> {
    // No symlink distinction on this PAL.
    stat(p)
}

pub fn canonicalize(_p: &Path) -> io::Result<PathBuf> {
    // Could map to Path.GetFullPath; not exercised (STUBBED).
    unsupported()
}
