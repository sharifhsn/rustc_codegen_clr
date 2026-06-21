//! `sys::fd::FileDesc` for the .NET ("dotnet") platform — the int-fd file
//! descriptor abstraction, backed by the process-global **fd-table**
//! (`cilly/src/ir/builtins/posix.rs`: `rcl_fd_table` mapping int fd → `RclFdEntry`
//! {GCHandle, kind, flags}). This is the **unified** representation the libc-shim
//! tier (LIBC_SHIM_SCOPE §3) and the dotnet net `Socket` share: a `FileDesc` IS an
//! `OwnedFd`, an `OwnedFd` IS an int fd, and the int fd resolves to a managed
//! `Socket`/`FileStream` GCHandle through the fd-table. One source of truth.
//!
//! ## Cap-1 status (families UNSET)
//! This module is injected as the **first** `cfg_select!` arm of `sys/fd/mod.rs`
//! (whose `_ =>` arm is empty today). It is the intermediate type
//! `os/fd/net.rs` needs (`Socket(FileDesc)` → `FileDesc(OwnedFd)`), which is why
//! it is load-bearing even before the `families=["unix"]` capstone flip: the net
//! `Socket` onion (`sys/net/connection/dotnet.rs`) is built on `FileDesc`.
//!
//! ## Implementation
//! `FileDesc` is a thin newtype over `OwnedFd`. Its data-plane (`read`/`write`/
//! `close`) routes through the bare POSIX C-ABI symbols the shim already provides
//! (`read`/`write`/`close` in `posix_symbols.rs`), which themselves kind-dispatch
//! (FILE vs SOCKET vs STD) through the fd-table to the right `rcl_dotnet_*` body.
//! So `FileDesc` does **not** re-implement the BCL logic — it threads the int fd
//! to the shipped bodies, exactly the libc-shim seam.
//!
//! `libc` IS linked into dotnet std (`std/Cargo.toml` gates the dep on
//! `cfg(not(all(windows, msvc)))`, which includes dotnet), so `libc::{read,
//! write,close}` resolve to the shim's overrides at link time.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::sys::{AsInner, FromInner, IntoInner};

/// An owned int file descriptor over the fd-table. Newtype over `OwnedFd` so the
/// `os::fd` traits compose for free and the net `Socket` can be `Socket(FileDesc)`.
#[derive(Debug)]
pub struct FileDesc(OwnedFd);

impl FileDesc {
    /// Read into `buf`. Routes through the shim `read(fd, buf, len)` which
    /// kind-dispatches FILE/SOCKET/STD via the fd-table.
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: writable region; the shim writes at most `len` bytes and returns
        // a signed count (-1 + errno on fault).
        let n = unsafe { libc::read(self.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len()) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n as usize)
    }

    pub fn read_buf(&self, cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        crate::io::default_read_buf(|b| self.read(b), cursor)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        crate::io::default_read_vectored(|b| self.read(b), bufs)
    }

    #[inline]
    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn read_at(&self, _buf: &mut [u8], _offset: u64) -> io::Result<usize> {
        // Positional reads need pread; the floor uses sockets/streams only.
        Err(io::Error::UNSUPPORTED_PLATFORM)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: readable region the shim only reads.
        let n = unsafe { libc::write(self.as_raw_fd(), buf.as_ptr() as *const _, buf.len()) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n as usize)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        crate::io::default_write_vectored(|b| self.write(b), bufs)
    }

    #[inline]
    pub fn is_write_vectored(&self) -> bool {
        false
    }

    pub fn write_at(&self, _buf: &[u8], _offset: u64) -> io::Result<usize> {
        Err(io::Error::UNSUPPORTED_PLATFORM)
    }

    /// `dup` the fd. Shares the same managed GCHandle through the fd-table; the
    /// BorrowedFd path uses `F_DUPFD_CLOEXEC` (the shim's `fcntl`), which currently
    /// duplicates the table slot. Falls back to UNSUPPORTED if the host refuses.
    pub fn duplicate(&self) -> io::Result<FileDesc> {
        Ok(FileDesc(self.0.try_clone()?))
    }

    pub fn set_cloexec(&self) -> io::Result<()> {
        // FD_CLOEXEC is stored in the fd-table flags word but never honoured (no
        // exec on this platform). A no-op is correct.
        Ok(())
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        // SAFETY: ioctl(FIONBIO) is the shim's nonblocking path for sockets.
        let mut v = nonblocking as libc::c_int;
        let r = unsafe { libc::ioctl(self.as_raw_fd(), libc::FIONBIO, &mut v) };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl<'a> AsInner<OwnedFd> for FileDesc {
    #[inline]
    fn as_inner(&self) -> &OwnedFd {
        &self.0
    }
}

impl IntoInner<OwnedFd> for FileDesc {
    #[inline]
    fn into_inner(self) -> OwnedFd {
        self.0
    }
}

impl FromInner<OwnedFd> for FileDesc {
    #[inline]
    fn from_inner(owned_fd: OwnedFd) -> Self {
        Self(owned_fd)
    }
}

impl AsFd for FileDesc {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl AsRawFd for FileDesc {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl IntoRawFd for FileDesc {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.0.into_raw_fd()
    }
}

impl FromRawFd for FileDesc {
    #[inline]
    unsafe fn from_raw_fd(raw_fd: RawFd) -> Self {
        // SAFETY: caller guarantees `raw_fd` is an owned, live fd-table fd.
        Self(unsafe { OwnedFd::from_raw_fd(raw_fd) })
    }
}
