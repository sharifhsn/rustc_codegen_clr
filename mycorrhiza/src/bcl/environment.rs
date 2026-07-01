//! Idiomatic Rust wrapper over the static .NET class [`System.Environment`] (assembly
//! `System.Private.CoreLib`) — process/OS information and environment-variable access, spelled the
//! way a Rust programmer expects (`std::env`-flavoured names, `&str` in, `String`/`Option<String>`
//! out).
//!
//! `System.Environment` is a *static* class in .NET — every member is a static property or method, so
//! there is nothing to construct. This wrapper mirrors that: it is a zero-sized [`Environment`] unit
//! type whose members are all associated functions.
//!
//! ```ignore
//! use mycorrhiza::bcl::environment::Environment;
//!
//! println!("host = {}", Environment::machine_name());
//! println!("cpus = {}", Environment::processor_count());
//! if let Some(path) = Environment::var("PATH") {
//!     // …
//! }
//! Environment::set_var("MYCORRHIZA", "1");
//! ```
//!
//! This is a thin, honest mapping: each function delegates to the corresponding low-level binding in
//! [`crate::System::Environment`]. Managed `System.String` results are decoded into Rust [`String`]s
//! (via [`DotNetString`]), and `&str` arguments are marshalled the other way. `GetEnvironmentVariable`
//! can return a null reference for a missing key; [`Environment::var`] maps that (and the empty
//! string) to `None`, matching `std::env::var(..).ok()`.

use crate::System::Environment as RawEnv;
use crate::system::{DotNetString, MString};

// The `System.String` binding carries the `IsNullOrEmpty` helper, which we use to turn a possibly-null
// `GetEnvironmentVariable` result into an `Option`.
use crate::System::String as BclString;

/// The static .NET class `System.Environment`. All members are associated functions — there is no
/// value to construct (it mirrors the C# `static class`). See the [module docs](self).
pub struct Environment;

/// Decode a managed `System.String` handle into an owned Rust [`String`] (UTF-16 → UTF-8).
#[inline]
fn to_rust(s: MString) -> String {
    DotNetString::from_handle(s).to_rust_string()
}

impl Environment {
    /// The NetBIOS name of this local computer (`Environment.MachineName`).
    pub fn machine_name() -> String {
        to_rust(RawEnv::get_machine_name())
    }

    /// The user name of the person currently logged on (`Environment.UserName`).
    pub fn user_name() -> String {
        to_rust(RawEnv::get_user_name())
    }

    /// The network domain name associated with the current user (`Environment.UserDomainName`).
    pub fn user_domain_name() -> String {
        to_rust(RawEnv::get_user_domain_name())
    }

    /// The fully qualified path of the current working directory (`Environment.CurrentDirectory`).
    pub fn current_directory() -> String {
        to_rust(RawEnv::get_current_directory())
    }

    /// Set the current working directory (`Environment.CurrentDirectory = path`).
    pub fn set_current_directory(path: &str) {
        RawEnv::set_current_directory(MString::from(path))
    }

    /// The full command line for this process (`Environment.CommandLine`).
    pub fn command_line() -> String {
        to_rust(RawEnv::get_command_line())
    }

    /// The newline string defined for this environment (`Environment.NewLine`) — `"\n"` on Unix,
    /// `"\r\n"` on Windows.
    pub fn new_line() -> String {
        to_rust(RawEnv::get_new_line())
    }

    /// The value of the environment variable `name`, or `None` if it is unset (or empty).
    ///
    /// `Environment.GetEnvironmentVariable` returns a null reference for a missing key; this maps
    /// that (and the empty string) to `None`, so it reads like `std::env::var(name).ok()`.
    pub fn var(name: &str) -> Option<String> {
        let raw = RawEnv::get_environment_variable(MString::from(name));
        if BclString::is_null_or_empty(raw) {
            None
        } else {
            Some(to_rust(raw))
        }
    }

    /// Create, modify, or delete the environment variable `name` for the current process
    /// (`Environment.SetEnvironmentVariable`). Passing an empty `value` deletes the variable in .NET.
    pub fn set_var(name: &str, value: &str) {
        RawEnv::set_environment_variable(MString::from(name), MString::from(value))
    }

    /// Replace each `%NAME%` token in `input` with the value of the corresponding environment variable
    /// (`Environment.ExpandEnvironmentVariables`).
    pub fn expand_variables(input: &str) -> String {
        to_rust(RawEnv::expand_environment_variables(MString::from(input)))
    }

    /// The unique identifier of the current process (`Environment.ProcessId`).
    pub fn process_id() -> i32 {
        RawEnv::get_process_id()
    }

    /// The number of processors available to the current process (`Environment.ProcessorCount`).
    pub fn processor_count() -> i32 {
        RawEnv::get_processor_count()
    }

    /// A unique identifier for the current managed thread (`Environment.CurrentManagedThreadId`).
    pub fn current_managed_thread_id() -> i32 {
        RawEnv::get_current_managed_thread_id()
    }

    /// Milliseconds elapsed since the system started (`Environment.TickCount`, wraps every ~24.9 days;
    /// prefer [`Environment::tick_count64`]).
    pub fn tick_count() -> i32 {
        RawEnv::get_tick_count()
    }

    /// Milliseconds elapsed since the system started, as a 64-bit value that does not wrap
    /// (`Environment.TickCount64`).
    pub fn tick_count64() -> i64 {
        RawEnv::get_tick_count64()
    }

    /// The number of bytes in the operating-system's memory page (`Environment.SystemPageSize`).
    pub fn system_page_size() -> i32 {
        RawEnv::get_system_page_size()
    }

    /// `true` if the current process is 64-bit (`Environment.Is64BitProcess`).
    pub fn is_64bit_process() -> bool {
        RawEnv::get_is64_bit_process()
    }

    /// `true` if the operating system is 64-bit (`Environment.Is64BitOperatingSystem`).
    pub fn is_64bit_operating_system() -> bool {
        RawEnv::get_is64_bit_operating_system()
    }

    /// `true` if the process is running with elevated privileges (`Environment.IsPrivilegedProcess`).
    pub fn is_privileged_process() -> bool {
        RawEnv::get_is_privileged_process()
    }

    /// `true` if the current process is running in user-interactive mode (`Environment.UserInteractive`).
    pub fn user_interactive() -> bool {
        RawEnv::get_user_interactive()
    }

    /// `true` once the common language runtime has begun shutting down (`Environment.HasShutdownStarted`).
    pub fn has_shutdown_started() -> bool {
        RawEnv::get_has_shutdown_started()
    }

    /// The exit code the process will return if it ends normally (`Environment.ExitCode`).
    pub fn exit_code() -> i32 {
        RawEnv::get_exit_code()
    }

    /// Set the exit code the process will return if it ends normally (`Environment.ExitCode = code`).
    pub fn set_exit_code(code: i32) {
        RawEnv::set_exit_code(code)
    }

    /// Terminate the process immediately with `exit_code` (`Environment.Exit`). Never returns.
    pub fn exit(exit_code: i32) -> ! {
        RawEnv::exit(exit_code);
        // `Environment.Exit` does not return; satisfy the `!` return type.
        loop {}
    }

    /// Immediately abort the process, writing `message` (`Environment.FailFast`). Never returns.
    pub fn fail_fast(message: &str) -> ! {
        RawEnv::fail_fast(MString::from(message));
        loop {}
    }
}
