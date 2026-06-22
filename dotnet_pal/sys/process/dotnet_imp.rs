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
        _default: Stdio,
        _needs_stdin: bool,
    ) -> io::Result<(Process, StdioPipes)> {
        unsupported()
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
pub struct ExitStatus();

impl ExitStatus {
    pub fn exit_ok(&self) -> Result<(), ExitStatusError> {
        Ok(())
    }

    pub fn code(&self) -> Option<i32> {
        Some(0)
    }

    // =======================================================================
    // DOTNET PAL ARM (Package A stub) — os::unix::process::ExitStatusExt surface.
    //
    // IMPOSSIBLE (I7): there is no POSIX wait-status on CoreCLR. `ExitStatus` is a
    // synthetic always-success (`code()==Some(0)`), so every signal/stop/continue
    // query is `None`/`false` and the raw form is `0`.
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
        0
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<dummy exit status>")
    }
}

// DOTNET PAL ARM (Package A stub): `ExitStatusExt::from_raw` is
// `process::ExitStatus::from_inner(From::from(raw))` — the inner `From<i32>`. The
// dotnet `ExitStatus` is the synthetic always-success unit, so any raw maps to it
// (the wait-status the int encodes is meaningless on CLR — I7).
impl From<i32> for ExitStatus {
    fn from(_raw: i32) -> ExitStatus {
        ExitStatus()
    }
}

pub struct ExitStatusError(!);

impl Clone for ExitStatusError {
    fn clone(&self) -> ExitStatusError {
        self.0
    }
}

impl Copy for ExitStatusError {}

impl PartialEq for ExitStatusError {
    fn eq(&self, _other: &ExitStatusError) -> bool {
        self.0
    }
}

impl Eq for ExitStatusError {}

impl fmt::Debug for ExitStatusError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
    }
}

impl Into<ExitStatus> for ExitStatusError {
    fn into(self) -> ExitStatus {
        self.0
    }
}

impl ExitStatusError {
    pub fn code(self) -> Option<NonZero<i32>> {
        self.0
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

pub struct Process(!);

impl Process {
    pub fn id(&self) -> u32 {
        self.0
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.0
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.0
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.0
    }

    // DOTNET PAL ARM (Package A stub) — os::unix::process::ChildExt surface.
    // `Process` is uninhabited (spawn never succeeds), so these diverge on the
    // never value exactly like `id`/`kill`/`wait` above. IMPOSSIBLE (I6): no
    // signal delivery to a child that does not exist.
    pub fn send_signal(&self, _signal: i32) -> io::Result<()> {
        self.0
    }

    pub fn send_process_group_signal(&self, _signal: i32) -> io::Result<()> {
        self.0
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
    _stdout: &mut Vec<u8>,
    _err: ChildPipe,
    _stderr: &mut Vec<u8>,
) -> io::Result<()> {
    match out.diverge() {}
}

pub fn getpid() -> u32 {
    panic!("no pids on this platform")
}
