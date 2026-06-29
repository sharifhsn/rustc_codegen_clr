//! `sys::pipe::Pipe` for the dotnet arm — a handle to a child process's redirected stdout/stderr/
//! stdin *byte* `Stream` (created by `System.Diagnostics.Process`, exposed via the `proc_{stdout,
//! stderr,stdin}` hooks). Backs `Command::output` capture: blocking `read`/`read_to_end` over the
//! child stream. A fresh anonymous OS pipe pair (`pipe()`) is NOT created on this arm — process
//! pipes come from the managed `Process` — so `pipe()` stays Unsupported (System.IO.Pipes are
//! Streams not Sockets, so they cannot ride the per-fd Socket.Poll readiness loop the net PAL uses).
use crate::fmt;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};

unsafe extern "C" {
    fn rcl_dotnet_stream_read(handle: *mut u8, ptr: *mut u8, len: usize) -> i32;
    fn rcl_dotnet_stream_write(handle: *mut u8, ptr: *const u8, len: usize) -> i32;
    fn rcl_dotnet_stream_close(handle: *mut u8);
}

/// A handle (GCHandle `IntPtr`) to a child process's redirected byte `Stream`.
pub struct Pipe {
    handle: *mut u8,
}

// The handle is an opaque GCHandle IntPtr to a Stream owned solely by this value (closed on Drop),
// so it is sound to move across threads (read_output drains stderr on a worker thread).
unsafe impl Send for Pipe {}
unsafe impl Sync for Pipe {}

impl Pipe {
    /// Wrap a raw child-stream GCHandle (from `rcl_dotnet_proc_{stdout,stderr,stdin}`).
    pub(crate) fn from_handle(handle: *mut u8) -> Pipe {
        Pipe { handle }
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        crate::sys::unsupported()
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        // SAFETY: `(ptr,len)` is a writable region the hook fills via Stream.Read (0 = EOF).
        let n = unsafe { rcl_dotnet_stream_read(self.handle, buf.as_mut_ptr(), buf.len()) };
        if n < 0 { Err(io::Error::last_os_error()) } else { Ok(n as usize) }
    }

    pub fn read_buf(&self, mut cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        let mut tmp = [0u8; 8192];
        let cap = cursor.capacity().min(tmp.len());
        let n = self.read(&mut tmp[..cap])?;
        cursor.append(&tmp[..n]);
        Ok(())
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        for b in bufs {
            if !b.is_empty() {
                return self.read(b);
            }
        }
        Ok(0)
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn read_to_end(&self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let start = buf.len();
        let mut tmp = [0u8; 8192];
        loop {
            let n = self.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
        }
        Ok(buf.len() - start)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        // SAFETY: readable region; the hook writes it via Stream.Write.
        let n = unsafe { rcl_dotnet_stream_write(self.handle, buf.as_ptr(), buf.len()) };
        if n < 0 { Err(io::Error::last_os_error()) } else { Ok(n as usize) }
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        for b in bufs {
            if !b.is_empty() {
                return self.write(b);
            }
        }
        Ok(0)
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }

    pub fn diverge(&self) -> ! {
        panic!("`Pipe::diverge` called on a real dotnet child-stream pipe")
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        // SAFETY: Dispose + GCHandle free, exactly once. For a stdin pipe this signals EOF to the
        // child; for stdout/stderr it releases the already-drained stream.
        unsafe { rcl_dotnet_stream_close(self.handle) };
    }
}

#[inline]
pub fn pipe() -> io::Result<(Pipe, Pipe)> {
    Err(io::Error::UNSUPPORTED_PLATFORM)
}

impl fmt::Debug for Pipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pipe").finish_non_exhaustive()
    }
}

#[cfg(any(unix, target_os = "hermit", target_os = "wasi"))]
mod unix_traits {
    use super::Pipe;
    use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
    use crate::sys::{FromInner, IntoInner};

    // A managed `Stream` has no backing OS fd. These exist for the `os::unix` child-pipe surface
    // but are not reached by `Command::output` capture (which only `read`s). `-1` is the
    // conventional "no fd"; the constructors are genuinely impossible.
    impl AsRawFd for Pipe {
        fn as_raw_fd(&self) -> RawFd {
            -1
        }
    }
    impl AsFd for Pipe {
        fn as_fd(&self) -> BorrowedFd<'_> {
            panic!("a dotnet child-stream pipe has no OS fd")
        }
    }
    impl IntoRawFd for Pipe {
        fn into_raw_fd(self) -> RawFd {
            -1
        }
    }
    impl FromRawFd for Pipe {
        unsafe fn from_raw_fd(_: RawFd) -> Self {
            panic!("creating a pipe from a raw fd is unsupported on this platform")
        }
    }
    impl FromInner<OwnedFd> for Pipe {
        fn from_inner(_: OwnedFd) -> Self {
            panic!("creating a pipe from an OwnedFd is unsupported on this platform")
        }
    }
    impl IntoInner<OwnedFd> for Pipe {
        fn into_inner(self) -> OwnedFd {
            panic!("a dotnet child-stream pipe has no OwnedFd")
        }
    }
}
