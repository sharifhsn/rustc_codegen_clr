//! `sys::process` inner `imp` for the .NET ("dotnet") platform.
//!
//! This is a patched copy of upstream `sys/process/unsupported.rs`: the dotnet
//! arm cannot simply `#[path]`-include the verbatim upstream file any more,
//! because the `target-family=["unix"]` flip activates the whole
//! `std::os::unix::process` public surface (`CommandExt` / `ExitStatusExt` /
//! `ChildExt` / `FromRawFd for Stdio`), which calls inherent methods/variants
//! that the verbatim `unsupported.rs` does NOT provide:
//!
//!   * `Command::{uid,gid,groups,pre_exec,exec,set_arg_0,pgroup,chroot,setsid}`
//!     (CommandExt) — **IMPOSSIBLE (I6):** no `fork`/`execve` on CoreCLR, so these
//!     are store-and-ignore / `Err(Unsupported)` Package-A stubs (no child is ever
//!     spawned — `spawn` itself is the upstream `unsupported()`).
//!   * `From<i32> for ExitStatus` + `ExitStatus::{signal,core_dumped,
//!     stopped_signal,continued,into_raw}` (ExitStatusExt) — **IMPOSSIBLE (I7):**
//!     no POSIX wait-status on CLR, so synthetic (`None`/`false`/`0`).
//!   * `Process::{send_signal,send_process_group_signal}` (ChildExt) — `Process`
//!     is uninhabited (`Process(!)`) because spawn never succeeds, so these
//!     diverge on the never value exactly like the existing `id`/`kill`/`wait`.
//!   * `Stdio::Fd(FileDesc)` variant — `FromRawFd for process::Stdio` /
//!     `From<OwnedFd> for process::Stdio` build a `sys::process::Stdio::Fd(fd)`.
//!     Added as a real variant carrying the unified fd-table `FileDesc`.
//!
//! Everything else is byte-for-byte the upstream `unsupported.rs`. The dotnet arm
//! (`dotnet.rs`) `#[path]`-includes THIS file as `mod imp` instead of
//! `unsupported.rs`, and still shadows `getpid` with the real
//! `Environment.ProcessId` hook.
//!
//! DOTNET PAL ARM (Package A stub): the ext-trait methods here are
//! synthetic/Unsupported. Package C (a real `Process.Start` bridge with a
//! synthetic pid) would make spawn/wait real; the POSIX signal/wait-status
//! semantics (uid setuid, SIGKILL delivery, WIFSIGNALED) remain genuinely
//! unavailable on stock CoreCLR.
use super::env::{CommandEnv, CommandEnvs, CommandResolvedEnvs};
pub use crate::ffi::OsString as EnvKey;
use crate::ffi::{OsStr, OsString};
use crate::num::NonZero;
use crate::path::Path;
use crate::process::StdioPipes;
use crate::sys::fd::FileDesc;
use crate::sys::fs::File;
use crate::sys::unsupported;
use crate::{fmt, io};

// DOTNET PAL ARM — real process spawning via a `System.Diagnostics.Process` bridge (the hooks build
// a ProcessStartInfo, start it, and wait). Handles are GCHandle IntPtrs (fs/net convention).
unsafe extern "C" {
    fn rcl_dotnet_proc_psi_new(prog_ptr: *const u8, prog_len: usize) -> *mut u8;
    fn rcl_dotnet_proc_psi_args(psi: *mut u8, ptr: *const u8, len: usize);
    fn rcl_dotnet_proc_psi_cwd(psi: *mut u8, ptr: *const u8, len: usize);
    fn rcl_dotnet_proc_psi_capture(psi: *mut u8);
    fn rcl_dotnet_proc_start(psi: *mut u8) -> *mut u8;
    fn rcl_dotnet_proc_stdout(handle: *mut u8) -> *mut u8;
    fn rcl_dotnet_proc_stderr(handle: *mut u8) -> *mut u8;
    fn rcl_dotnet_proc_wait(handle: *mut u8) -> i32;
    fn rcl_dotnet_proc_has_exited(handle: *mut u8) -> i32;
    fn rcl_dotnet_proc_id(handle: *mut u8) -> u32;
    fn rcl_dotnet_proc_kill(handle: *mut u8);
    fn rcl_dotnet_proc_free(handle: *mut u8);
}

/// Quote `args` into a single string for `ProcessStartInfo.Arguments`, using .NET's
/// `PasteArguments` convention so .NET parses them back into the exact child argv (no shell). The
/// common no-space/no-quote case is just space-joined; quoting only kicks in for ' ', '\t', '"'.
fn paste_arguments(args: &[OsString]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        let bytes = arg.as_encoded_bytes();
        let needs_quote =
            bytes.is_empty() || bytes.iter().any(|&b| b == b' ' || b == b'\t' || b == b'"');
        if !needs_quote {
            out.extend_from_slice(bytes);
            continue;
        }
        out.push(b'"');
        let mut j = 0;
        while j < bytes.len() {
            let mut backslashes = 0;
            while j < bytes.len() && bytes[j] == b'\\' {
                backslashes += 1;
                j += 1;
            }
            if j == bytes.len() {
                // Escape all trailing backslashes so they don't escape the closing quote.
                for _ in 0..backslashes * 2 {
                    out.push(b'\\');
                }
            } else if bytes[j] == b'"' {
                for _ in 0..backslashes * 2 + 1 {
                    out.push(b'\\');
                }
                out.push(b'"');
                j += 1;
            } else {
                for _ in 0..backslashes {
                    out.push(b'\\');
                }
                out.push(bytes[j]);
                j += 1;
            }
        }
        out.push(b'"');
    }
    out
}

/// Build a `ProcessStartInfo` from `cmd` and `Process.Start` it; returns the process GCHandle.
/// `capture` requests stdout/stderr redirection (for `output()`). `program` is `args[0]` (FileName);
/// the rest become `Arguments`. A null from a hook means the start failed (errno set BCL-side).
fn build_and_start(cmd: &Command, capture: bool) -> io::Result<*mut u8> {
    let prog = cmd.program.as_encoded_bytes();
    let psi = unsafe { rcl_dotnet_proc_psi_new(prog.as_ptr(), prog.len()) };
    if psi.is_null() {
        return Err(io::Error::last_os_error());
    }
    if cmd.args.len() > 1 {
        let pasted = paste_arguments(&cmd.args[1..]);
        unsafe { rcl_dotnet_proc_psi_args(psi, pasted.as_ptr(), pasted.len()) };
    }
    if let Some(cwd) = &cmd.cwd {
        let b = cwd.as_encoded_bytes();
        unsafe { rcl_dotnet_proc_psi_cwd(psi, b.as_ptr(), b.len()) };
    }
    if capture {
        unsafe { rcl_dotnet_proc_psi_capture(psi) };
    }
    let handle = unsafe { rcl_dotnet_proc_start(psi) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(handle)
}

////////////////////////////////////////////////////////////////////////////////
// Command
////////////////////////////////////////////////////////////////////////////////

pub struct Command {
    program: OsString,
    args: Vec<OsString>,
    env: CommandEnv,

    cwd: Option<OsString>,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,

    // DOTNET PAL ARM (Package A stub): the CommandExt fields are STORED so the
    // setters can mirror the unix arm's `&mut process::Command` builder contract,
    // but they are never consumed — `spawn` is `unsupported()`, no child exists.
    arg0: Option<OsString>,
}

pub enum Stdio {
    Inherit,
    Null,
    MakePipe,
    ParentStdout,
    ParentStderr,
    #[allow(dead_code)] // This variant exists only for the Debug impl
    InheritFile(File),
    // DOTNET PAL ARM (Package A): `FromRawFd for process::Stdio` /
    // `From<OwnedFd> for process::Stdio` (os/unix/process.rs) build this. The fd is
    // a unified fd-table `FileDesc`; with no real spawn it is only carried/dropped.
    Fd(FileDesc),
}

impl Command {
    pub fn new(program: &OsStr) -> Command {
        Command {
            program: program.to_owned(),
            args: vec![program.to_owned()],
            env: Default::default(),
            cwd: None,
            stdin: None,
            stdout: None,
            stderr: None,
            arg0: None,
        }
    }

    pub fn arg(&mut self, arg: &OsStr) {
        self.args.push(arg.to_owned());
    }

    pub fn env_mut(&mut self) -> &mut CommandEnv {
        &mut self.env
    }

    pub fn cwd(&mut self, dir: &OsStr) {
        self.cwd = Some(dir.to_owned());
    }

    pub fn stdin(&mut self, stdin: Stdio) {
        self.stdin = Some(stdin);
    }

    pub fn stdout(&mut self, stdout: Stdio) {
        self.stdout = Some(stdout);
    }

    pub fn stderr(&mut self, stderr: Stdio) {
        self.stderr = Some(stderr);
    }

    pub fn get_program(&self) -> &OsStr {
        &self.program
    }

    pub fn get_args(&self) -> CommandArgs<'_> {
        let mut iter = self.args.iter();
        iter.next();
        CommandArgs { iter }
    }

    pub fn get_envs(&self) -> CommandEnvs<'_> {
        self.env.iter()
    }

    pub fn get_env_clear(&self) -> bool {
        self.env.does_clear()
    }

    pub fn get_resolved_envs(&self) -> CommandResolvedEnvs {
        CommandResolvedEnvs::new(self.env.capture())
    }

    pub fn get_current_dir(&self) -> Option<&Path> {
        self.cwd.as_ref().map(|cs| Path::new(cs))
    }

    pub fn spawn(
        &mut self,
        default: Stdio,
        _needs_stdin: bool,
    ) -> io::Result<(Process, StdioPipes)> {
        // PACKAGE C (this arm): real spawn for INHERITED stdio (`Command::status`, and any
        // spawn that doesn't pipe). Captured/piped stdio (`Stdio::MakePipe` — `Command::output`
        // and `Stdio::piped()`) needs readable `AnonPipe`s over the child streams and is handled
        // by the dedicated `output()` path below; a direct piped `spawn` stays Unsupported for now.
        let is_pipe = |s: &Option<Stdio>| matches!(s, Some(Stdio::MakePipe));
        let default_is_pipe = matches!(default, Stdio::MakePipe);
        let wants_pipe = is_pipe(&self.stdin)
            || is_pipe(&self.stdout)
            || is_pipe(&self.stderr)
            || (default_is_pipe
                && (self.stdin.is_none() || self.stdout.is_none() || self.stderr.is_none()));
        if wants_pipe {
            // CAPTURE path (Command::output): start with stdout/stderr redirected and hand back
            // readable Pipes over the child streams. stdin is left inherited (None) — output()
            // drops it anyway; a live piped stdin (streaming `spawn`) is the deferred case.
            let handle = build_and_start(self, true)?;
            let stdout = unsafe { rcl_dotnet_proc_stdout(handle) };
            let stderr = unsafe { rcl_dotnet_proc_stderr(handle) };
            let pipes = StdioPipes {
                stdin: None,
                stdout: Some(crate::sys::pipe::Pipe::from_handle(stdout)),
                stderr: Some(crate::sys::pipe::Pipe::from_handle(stderr)),
            };
            return Ok((Process { handle }, pipes));
        }
        let handle = build_and_start(self, false)?;
        Ok((Process { handle }, StdioPipes { stdin: None, stdout: None, stderr: None }))
    }

    // =======================================================================
    // DOTNET PAL ARM (Package A stub) — os::unix::process::CommandExt surface.
    //
    // IMPOSSIBLE (I6): stock CoreCLR has no `fork`/`execve`, so a child is never
    // spawned (`spawn` above is `unsupported()`). Each setter mirrors the unix
    // arm's mutable-builder shape (store the value, return nothing — the public
    // ext-trait `impl` re-borrows `&mut process::Command`) but the stored value is
    // never acted on. `exec` returns the Unsupported error directly.
    // =======================================================================

    pub fn uid(&mut self, _id: u32) {
        // setuid in a non-existent child — stored-and-ignored.
    }

    pub fn gid(&mut self, _id: u32) {
        // setgid in a non-existent child — stored-and-ignored.
    }

    pub fn groups(&mut self, _groups: &[u32]) {
        // setgroups in a non-existent child — stored-and-ignored.
    }

    pub fn pre_exec(&mut self, _f: Box<dyn FnMut() -> io::Result<()> + Send + Sync>) {
        // The pre-`exec` hook can never run (no `exec` happens) — dropped.
    }

    pub fn exec(&mut self, _default: Stdio) -> io::Error {
        // `exec` replaces the current image; impossible on CoreCLR. On the unix
        // arm a successful `exec` never returns and only the failure is an
        // `io::Error`; here it is always the Unsupported failure.
        io::Error::from(io::ErrorKind::Unsupported)
    }

    pub fn set_arg_0(&mut self, arg: &OsStr) {
        self.arg0 = Some(arg.to_owned());
    }

    pub fn pgroup(&mut self, _pgroup: i32) {
        // setpgid in a non-existent child — stored-and-ignored.
    }

    pub fn chroot(&mut self, _dir: &Path) {
        // chroot in a non-existent child — stored-and-ignored (no chroot on CLR).
    }

    pub fn setsid(&mut self, _setsid: bool) {
        // setsid in a non-existent child — stored-and-ignored.
    }
}

pub fn output(_cmd: &mut Command) -> io::Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    unsupported()
}

impl From<ChildPipe> for Stdio {
    fn from(pipe: ChildPipe) -> Stdio {
        pipe.diverge()
    }
}

impl From<io::Stdout> for Stdio {
    fn from(_: io::Stdout) -> Stdio {
        Stdio::ParentStdout
    }
}

impl From<io::Stderr> for Stdio {
    fn from(_: io::Stderr) -> Stdio {
        Stdio::ParentStderr
    }
}

impl From<File> for Stdio {
    fn from(file: File) -> Stdio {
        Stdio::InheritFile(file)
    }
}

impl fmt::Debug for Stdio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stdio::Inherit => f.write_str("Inherit"),
            Stdio::Null => f.write_str("Null"),
            Stdio::MakePipe => f.write_str("MakePipe"),
            Stdio::ParentStdout => f.write_str("ParentStdout"),
            Stdio::ParentStderr => f.write_str("ParentStderr"),
            Stdio::InheritFile(file) => f.debug_tuple("InheritFile").field(file).finish(),
            Stdio::Fd(_) => f.write_str("Fd"),
        }
    }
}

impl fmt::Debug for Command {
    // show all attributes
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            let mut debug_command = f.debug_struct("Command");
            debug_command.field("program", &self.program).field("args", &self.args);
            if !self.env.is_unchanged() {
                debug_command.field("env", &self.env);
            }

            if self.cwd.is_some() {
                debug_command.field("cwd", &self.cwd);
            }

            if self.stdin.is_some() {
                debug_command.field("stdin", &self.stdin);
            }
            if self.stdout.is_some() {
                debug_command.field("stdout", &self.stdout);
            }
            if self.stderr.is_some() {
                debug_command.field("stderr", &self.stderr);
            }

            debug_command.finish()
        } else {
            if let Some(ref cwd) = self.cwd {
                write!(f, "cd {cwd:?} && ")?;
            }
            if self.env.does_clear() {
                write!(f, "env -i ")?;
                // Altered env vars will be printed next, that should exactly work as expected.
            } else {
                // Removed env vars need the command to be wrapped in `env`.
                let mut any_removed = false;
                for (key, value_opt) in self.get_envs() {
                    if value_opt.is_none() {
                        if !any_removed {
                            write!(f, "env ")?;
                            any_removed = true;
                        }
                        write!(f, "-u {} ", key.to_string_lossy())?;
                    }
                }
            }
            // Altered env vars can just be added in front of the program.
            for (key, value_opt) in self.get_envs() {
                if let Some(value) = value_opt {
                    write!(f, "{}={value:?} ", key.to_string_lossy())?;
                }
            }
            if self.program != self.args[0] {
                write!(f, "[{:?}] ", self.program)?;
            }
            write!(f, "{:?}", self.args[0])?;

            for arg in &self.args[1..] {
                write!(f, " {:?}", arg)?;
            }
            Ok(())
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct ExitStatus(i32);

impl ExitStatus {
    pub fn exit_ok(&self) -> Result<(), ExitStatusError> {
        // .NET reports only a plain exit code (no POSIX wait-status). 0 = success.
        match NonZero::new(self.0) {
            None => Ok(()),
            Some(c) => Err(ExitStatusError(c)),
        }
    }

    pub fn code(&self) -> Option<i32> {
        Some(self.0)
    }

    // =======================================================================
    // os::unix::process::ExitStatusExt surface. IMPOSSIBLE (I7): there is no POSIX
    // wait-status on CoreCLR — a child only yields a plain exit code — so every
    // signal/stop/continue query is `None`/`false` and `into_raw` is the code.
    // =======================================================================

    pub fn signal(&self) -> Option<i32> {
        None
    }

    pub fn core_dumped(&self) -> bool {
        false
    }

    pub fn stopped_signal(&self) -> Option<i32> {
        None
    }

    pub fn continued(&self) -> bool {
        false
    }

    pub fn into_raw(&self) -> i32 {
        self.0
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit status: {}", self.0)
    }
}

// `ExitStatusExt::from_raw`. .NET has no wait-status encoding, so the raw IS the exit code.
impl From<i32> for ExitStatus {
    fn from(raw: i32) -> ExitStatus {
        ExitStatus(raw)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ExitStatusError(NonZero<i32>);

impl Into<ExitStatus> for ExitStatusError {
    fn into(self) -> ExitStatus {
        ExitStatus(self.0.get())
    }
}

impl ExitStatusError {
    pub fn code(self) -> Option<NonZero<i32>> {
        Some(self.0)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitCode(u8);

impl ExitCode {
    pub const SUCCESS: ExitCode = ExitCode(0);
    pub const FAILURE: ExitCode = ExitCode(1);

    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }
}

impl From<u8> for ExitCode {
    fn from(code: u8) -> Self {
        Self(code)
    }
}

/// A spawned child: a `GCHandle` `IntPtr` pinning the managed `System.Diagnostics.Process`.
pub struct Process {
    handle: *mut u8,
}

// The handle is an opaque GCHandle IntPtr; the managed Process it pins is owned solely by this
// value (freed on Drop), so it is sound to move across threads like the net/fs handles.
unsafe impl Send for Process {}
unsafe impl Sync for Process {}

impl Process {
    pub fn id(&self) -> u32 {
        // SAFETY: `self.handle` pins a live managed Process until Drop.
        unsafe { rcl_dotnet_proc_id(self.handle) }
    }

    pub fn kill(&mut self) -> io::Result<()> {
        // SAFETY: live handle.
        unsafe { rcl_dotnet_proc_kill(self.handle) };
        Ok(())
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        // SAFETY: live handle; WaitForExit + ExitCode.
        Ok(ExitStatus(unsafe { rcl_dotnet_proc_wait(self.handle) }))
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        // SAFETY: live handle. HasExited is non-blocking; if exited, WaitForExit returns at once.
        if unsafe { rcl_dotnet_proc_has_exited(self.handle) } != 0 {
            Ok(Some(ExitStatus(unsafe { rcl_dotnet_proc_wait(self.handle) })))
        } else {
            Ok(None)
        }
    }

    // os::unix::process::ChildExt surface. IMPOSSIBLE (I6): stock CoreCLR has no POSIX signal
    // delivery to a child (only `Kill`), so arbitrary signal sends are Unsupported.
    pub fn send_signal(&self, _signal: i32) -> io::Result<()> {
        unsupported()
    }

    pub fn send_process_group_signal(&self, _signal: i32) -> io::Result<()> {
        unsupported()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // Release our reference to the managed Process (Rust drops a Child as "detach"; the OS
        // process keeps running). SAFETY: handle freed exactly once, here.
        unsafe { rcl_dotnet_proc_free(self.handle) };
    }
}

pub struct CommandArgs<'a> {
    iter: crate::slice::Iter<'a, OsString>,
}

impl<'a> Iterator for CommandArgs<'a> {
    type Item = &'a OsStr;
    fn next(&mut self) -> Option<&'a OsStr> {
        self.iter.next().map(|os| &**os)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a> ExactSizeIterator for CommandArgs<'a> {
    fn len(&self) -> usize {
        self.iter.len()
    }
    fn is_empty(&self) -> bool {
        self.iter.is_empty()
    }
}

impl<'a> fmt::Debug for CommandArgs<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter.clone()).finish()
    }
}

pub type ChildPipe = crate::sys::pipe::Pipe;

pub fn read_output(
    out: ChildPipe,
    stdout: &mut Vec<u8>,
    err: ChildPipe,
    stderr: &mut Vec<u8>,
) -> io::Result<()> {
    // Drain stderr on a worker thread so stdout+stderr empty CONCURRENTLY — a child that fills one
    // pipe's buffer while we block reading the other would otherwise deadlock (the child blocks on
    // write, never closes the stream we're reading). Threads are real on this PAL.
    let reader = crate::thread::Builder::new().spawn(move || {
        let mut buf = Vec::new();
        err.read_to_end(&mut buf).map(|_| buf)
    })?;
    out.read_to_end(stdout)?;
    match reader.join() {
        Ok(res) => *stderr = res?,
        Err(_) => {
            return Err(io::const_error!(io::ErrorKind::Other, "stderr reader thread panicked"));
        }
    }
    Ok(())
}

pub fn getpid() -> u32 {
    panic!("no pids on this platform")
}
