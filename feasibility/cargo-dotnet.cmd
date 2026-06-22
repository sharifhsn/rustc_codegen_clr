@echo off
REM feasibility\cargo-dotnet.cmd — Windows (x86_64) entry for the `cargo dotnet` DX.
REM
REM `cargo-dotnet` is a bash script; on Windows it runs under Git-Bash / MSYS2 / WSL.
REM This .cmd shim lets `cargo dotnet ...` (and a direct `feasibility\cargo-dotnet.cmd`)
REM work from a normal Windows shell by forwarding to bash with the NATIVE backend.
REM
REM PREREQS (see docs/CARGO_DOTNET.md "Windows (x86_64), best-effort / UNTESTED"):
REM   * Git for Windows (provides bash) on PATH, or WSL.
REM   * rustup nightly-2026-06-17-x86_64-pc-windows-msvc + rust-src + rustc-dev.
REM   * .NET 8 SDK on PATH (dotnet.exe).
REM   * The CoreCLR ILAsm tool (NuGet runtime.win-x64.Microsoft.NETCore.ILAsm) at
REM     %USERPROFILE%\.dotnet\ilasm-tool\ilasm.exe, or ILASM_PATH set to it.
REM   * The host backend built: librustc_codegen_clr.dll + linker.exe under target\release.
REM
REM This path is IMPLEMENTED DEFENSIVELY but NOT verified on Windows. See the docs.

setlocal enabledelayedexpansion
set CARGO_DOTNET_BACKEND=native

REM Find bash: prefer Git's, fall back to PATH, then WSL.
set "BASH_EXE="
if exist "%ProgramFiles%\Git\bin\bash.exe" set "BASH_EXE=%ProgramFiles%\Git\bin\bash.exe"
if not defined BASH_EXE for %%I in (bash.exe) do if not defined BASH_EXE set "BASH_EXE=%%~$PATH:I"

set "SCRIPT_DIR=%~dp0"
if defined BASH_EXE (
  REM Git-Bash / MSYS2 bash accepts a Windows path directly.
  "%BASH_EXE%" "%SCRIPT_DIR%cargo-dotnet" %*
) else (
  where wsl >nul 2>&1 || (
    echo cargo-dotnet.cmd: no bash found. Install Git for Windows or WSL. 1>&2
    exit /b 1
  )
  REM WSL bash needs a /mnt/c/... POSIX path, so translate via `wslpath` first.
  REM Delayed expansion (!WSL_SCRIPT!) is required because the var is both set
  REM and used inside this same parenthesized block.
  for /f "delims=" %%P in ('wsl wslpath "%SCRIPT_DIR%cargo-dotnet"') do set "WSL_SCRIPT=%%P"
  wsl bash "!WSL_SCRIPT!" %*
)
exit /b %ERRORLEVEL%
