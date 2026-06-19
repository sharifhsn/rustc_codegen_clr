//! Standard I/O for the .NET ("dotnet") platform.
//!
//! `Stdout` (fd 1) and `Stderr` (fd 2) forward their bytes to the real .NET
//! console streams through the `rcl_dotnet_write` hook, which the cilly linker
//! maps onto `System.Console` — this is what makes `println!` / `eprintln!`
//! actually appear when running Rust `std` on CoreCLR. `Stdin` is not wired to a
//! console reader and reports end-of-file (`Ok(0)`).
//!
//! Modeled on `sys/stdio/zkvm.rs`, the canonical minimal non-unix arm.

use crate::io::{self, BorrowedCursor};

// FIXED extern contract — the names must match EXACTLY on the linker side, where
// they are mapped to the .NET BCL (`System.Console`).
unsafe extern "C" {
    /// Write `len` UTF-8 bytes from `ptr` to the console stream selected by `fd`
    /// (1 = stdout, 2 = stderr). Returns the number of bytes written, or -1 on
    /// error.
    fn rcl_dotnet_write(fd: i32, ptr: *const u8, len: usize) -> isize;
}

mod fileno {
    pub const STDOUT: i32 = 1;
    pub const STDERR: i32 = 2;
}

/// Forward `buf` to the .NET console stream `fd`, translating the contract's
/// `-1`-on-error convention into an `io::Error`.
fn write_fd(fd: i32, buf: &[u8]) -> io::Result<usize> {
    // SAFETY: `buf` is a valid slice, so `as_ptr()`/`len()` describe a readable
    // region of exactly `buf.len()` bytes for the duration of the call.
    let n = unsafe { rcl_dotnet_write(fd, buf.as_ptr(), buf.len()) };
    if n < 0 {
        Err(io::Error::new(io::ErrorKind::Other, "rcl_dotnet_write failed"))
    } else {
        Ok(n as usize)
    }
}

pub struct Stdin;
pub struct Stdout;
pub struct Stderr;

impl Stdin {
    pub const fn new() -> Stdin {
        Stdin
    }
}

impl io::Read for Stdin {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        // No console-input hook is wired up; report immediate EOF.
        Ok(0)
    }

    fn read_buf(&mut self, _buf: BorrowedCursor<'_, u8>) -> io::Result<()> {
        // Leaves the cursor unadvanced, i.e. EOF.
        Ok(())
    }
}

impl Stdout {
    pub const fn new() -> Stdout {
        Stdout
    }
}

impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write_fd(fileno::STDOUT, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Stderr {
    pub const fn new() -> Stderr {
        Stderr
    }
}

impl io::Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write_fd(fileno::STDERR, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub const STDIN_BUF_SIZE: usize = crate::sys::io::DEFAULT_BUF_SIZE;

pub fn is_ebadf(_err: &io::Error) -> bool {
    true
}

pub fn panic_output() -> Option<impl io::Write> {
    Some(Stderr::new())
}
