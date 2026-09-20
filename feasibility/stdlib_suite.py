#!/usr/bin/env python3
"""Measure pinned-rust correctness suites through ``cargo dotnet test``.

This is deliberately a measurement harness, not a compatibility claim. It copies the selected
suite out of the active rust-src sysroot, validates or performs one product build/list per enabled
test target, then executes host-sized adaptive exact-name batches through the captured immutable
apphost. A missing toolchain, failed build, or broken terminal protocol is a no-score terminal
state.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shlex
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import tomllib
import uuid
from concurrent.futures import ThreadPoolExecutor
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUT = ROOT / "target" / "stdlib-suites"


@dataclass(frozen=True)
class TargetPolicy:
    """Declarative runner and applicability; absence means ordinary libtest."""

    identity: str
    runner_kind: str = "libtest"
    reason: str = ""
    applicable_target_os: tuple[str, ...] = ()
    applicable_target_arch: tuple[str, ...] = ()
    reason_when_inapplicable: str = ""


@dataclass(frozen=True)
class TargetCfg:
    target_os: str
    target_arch: str
    target_families: tuple[str, ...]
    target_pointer_width: int
    target_spec_path: str
    target_spec_sha256: str


@dataclass(frozen=True)
class SourcePatch:
    """Small, deterministic source compatibility shim applied to a staged suite root.

    The path is relative to ``SuiteSpec.manifest.parent``.  Patches are intentionally
    insertion-only and idempotent: an already-staged source bundle can be reused without
    accumulating duplicate feature gates.  The exact patch text is part of the source identity
    and therefore rotates the immutable source-cache key when it changes.
    """

    path: Path
    marker: str
    insertion: str

    def identity(self) -> dict[str, str]:
        return {
            "path": self.path.as_posix(),
            "marker": self.marker,
            "insertion": self.insertion,
        }


@dataclass(frozen=True)
class SourceRewrite:
    """Small, deterministic source compatibility replacement for a staged suite root.

    Most compatibility changes are insertion-only and use :class:`SourcePatch`.  A handful of
    upstream test crates need a dependency rename (for example, to keep a package named
    ``compiler_builtins`` distinct from build-std's private copy); those changes must replace one
    manifest/source fragment rather than leave the old fragment active.  Rewrites are still
    deliberately exact, one-occurrence transformations and are included in the immutable source
    identity just like insertions.
    """

    path: Path
    marker: str
    replacement: str

    def identity(self) -> dict[str, str]:
        return {
            "path": self.path.as_posix(),
            "marker": self.marker,
            "replacement": self.replacement,
        }


@dataclass(frozen=True)
class SuiteSpec:
    """Pinned rust-library suite selection and its non-Cargo source inputs."""

    name: str
    manifest: Path
    package: str
    source_package: str | None = None
    workspace_manifest: Path | None = None
    lockfile: Path = Path("Cargo.lock")
    manifest_overlay: str | None = None
    include_lib_target: bool = True
    extra_source_roots: tuple[Path, ...] = ()
    source_patches: tuple[SourcePatch, ...] = ()
    source_rewrites: tuple[SourceRewrite, ...] = ()
    cargo_features: tuple[str, ...] = ()
    cargo_no_default_features: bool = False
    target_identities: tuple[str, ...] = ()
    target_policies: tuple[TargetPolicy, ...] = ()

    def source_identity(self) -> dict[str, object]:
        return {
            "name": self.name,
            "manifest": self.manifest.as_posix(),
            "package": self.package,
            "source_package": self.source_package,
            "workspace_manifest": (
                self.workspace_manifest.as_posix()
                if self.workspace_manifest is not None
                else None
            ),
            "lockfile": self.lockfile.as_posix(),
            "manifest_overlay_sha256": (
                hashlib.sha256(self.manifest_overlay.encode()).hexdigest()
                if self.manifest_overlay is not None
                else None
            ),
            "extra_source_roots": [path.as_posix() for path in self.extra_source_roots],
            "source_patches": [patch.identity() for patch in self.source_patches],
            "source_rewrites": [rewrite.identity() for rewrite in self.source_rewrites],
            "cargo_features": list(self.cargo_features),
            "cargo_no_default_features": self.cargo_no_default_features,
        }

    def execution_policy_identity(self) -> dict[str, object]:
        return {
            "include_lib_target": self.include_lib_target,
            "target_identities": list(self.target_identities),
            "target_policies": [asdict(policy) for policy in self.target_policies],
        }

    def identity(self) -> dict[str, object]:
        return {
            "source": self.source_identity(),
            "execution_policy": self.execution_policy_identity(),
        }


SUITES = {
    "coretests": SuiteSpec(
        name="coretests",
        manifest=Path("coretests/Cargo.toml"),
        package="coretests",
    ),
    "alloctests": SuiteSpec(
        name="alloctests",
        manifest=Path("alloctests/Cargo.toml"),
        package="alloctests",
        # alloctests includes library/alloc sources with #[path], outside Cargo's graph.
        extra_source_roots=(Path("alloc"),),
    ),
    "stdtests": SuiteSpec(
        name="stdtests",
        manifest=Path("std/Cargo.toml"),
        package="rust-std-integration-tests",
        source_package="std",
        workspace_manifest=Path("Cargo.toml"),
        manifest_overlay="""[package]
name = "rust-std-integration-tests"
version = "0.0.0"
edition = "2024"
autolib = false
build = false
autobenches = false

[dev-dependencies]
rand = { version = "0.9.0", default-features = false, features = ["alloc"] }
rand_xorshift = "0.4.0"

[[test]]
name = "pipe-subprocess"
path = "tests/pipe_subprocess.rs"
harness = false

[[test]]
name = "sync"
path = "tests/sync/lib.rs"

[[test]]
name = "thread_local"
path = "tests/thread_local/lib.rs"

[workspace]
""",
        include_lib_target=False,
        source_patches=(
            SourcePatch(
                path=Path("tests/sync/lib.rs"),
                marker="#![feature(sync_nonpoison)]\n",
                insertion="#![feature(sync_poison_mod)]\n",
            ),
            SourcePatch(
                path=Path("tests/sync/rwlock.rs"),
                marker="        miri => 100,\n",
                insertion='        target_os = "dotnet" => 100,\n',
            ),
            SourcePatch(
                path=Path("tests/switch-stdout.rs"),
                marker='#![cfg(any(target_family = "unix", target_family = "windows"))]\n',
                insertion='#![cfg(not(target_os = "dotnet"))]\n',
            ),
        ),
        target_policies=(
            TargetPolicy(
                identity="ambiguous-hash_map",
                runner_kind="compile-only",
                reason=(
                    "upstream source is a zero-#[test] compile regression with fn main; "
                    "successful CLR build/list is the complete target oracle"
                ),
            ),
            TargetPolicy(
                identity="win_delete_self",
                applicable_target_os=("windows",),
                reason_when_inapplicable=(
                    "upstream target is gated to target_os=windows; the receipt-bound "
                    ".NET compiler target has target_os=dotnet"
                ),
            ),
            TargetPolicy(
                identity="windows",
                applicable_target_os=("windows",),
                reason_when_inapplicable=(
                    "upstream target is gated to target_os=windows; the receipt-bound "
                    ".NET compiler target has target_os=dotnet"
                ),
            ),
            TargetPolicy(
                identity="windows_unix_socket",
                applicable_target_os=("windows",),
                reason_when_inapplicable=(
                    "upstream target requires the Windows Unix-socket implementation; "
                    "the receipt-bound .NET compiler target has target_os=dotnet"
                ),
            ),
            TargetPolicy(
                identity="switch-stdout",
                applicable_target_os=(
                    "android",
                    "darwin",
                    "dragonfly",
                    "freebsd",
                    "ios",
                    "linux",
                    "macos",
                    "netbsd",
                    "openbsd",
                    "windows",
                ),
                reason_when_inapplicable=(
                    "upstream test requires OS-level stdout descriptor/handle duplication; "
                    "the receipt-bound .NET target has no such portable primitive"
                ),
            ),
        ),
        # compiler-builtins deliberately includes sibling libm Rust sources with #[path].
        extra_source_roots=(
            Path("backtrace"),
            Path("compiler-builtins/libm"),
            Path("portable-simd/crates/core_simd"),
            Path("portable-simd/crates/std_float"),
            Path("stdarch/crates/core_arch"),
        ),
    ),
    # portable-simd is a workspace without a package at its root.  Select one
    # member manifest at a time while copying the complete pinned workspace so
    # Cargo resolves the same path dependencies and lockfile as upstream.
    "portable-simd": SuiteSpec(
        name="portable-simd",
        manifest=Path("portable-simd/crates/core_simd/Cargo.toml"),
        package="core_simd",
        workspace_manifest=Path("portable-simd/Cargo.toml"),
        lockfile=Path("portable-simd/Cargo.lock"),
        target_policies=(
            TargetPolicy(
                identity="lib",
                runner_kind="compile-only",
                reason=(
                    "core_simd exposes its correctness suite as integration tests; "
                    "the package library target has no libtest cases"
                ),
            ),
            TargetPolicy(
                identity="ops_macros",
                runner_kind="compile-only",
                reason=(
                    "upstream ops_macros is a macro-definition integration target with "
                    "no #[test] functions; successful CLR build/list is the target oracle"
                ),
            ),
        ),
    ),
    "portable-simd-std-float": SuiteSpec(
        name="portable-simd-std-float",
        manifest=Path("portable-simd/crates/std_float/Cargo.toml"),
        package="std_float",
        workspace_manifest=Path("portable-simd/Cargo.toml"),
        lockfile=Path("portable-simd/Cargo.lock"),
        target_policies=(
            TargetPolicy(
                identity="lib",
                runner_kind="compile-only",
                reason=(
                    "std_float is a support library; its correctness cases live in the separate "
                    "float integration target"
                ),
            ),
        ),
    ),
    # These are deliberately separate package campaigns: builtins-test checks
    # compiler-builtin arithmetic/memory intrinsics, while libm and libm-test
    # exercise the pure-Rust math implementation and its generated oracle tests.
    # The complete compiler-builtins workspace is copied for exact path edges;
    # target-specific or non-libtest members remain visible as explicit
    # exclusions in each receipt rather than being silently counted.
    "compiler-builtins": SuiteSpec(
        name="compiler-builtins",
        manifest=Path("compiler-builtins/builtins-test/Cargo.toml"),
        package="builtins-test",
        workspace_manifest=Path("compiler-builtins/Cargo.toml"),
        lockfile=Path("compiler-builtins/Cargo.lock"),
        # The workspace intentionally excludes the real compiler-builtins crate and tests it
        # through builtins-shim.  Copy that excluded sibling because the shim's manifest/build
        # script and lib path resolve into it via `../compiler-builtins/...`.
        extra_source_roots=(
            Path("compiler-builtins/compiler-builtins"),
            Path("compiler-builtins/etc"),
        ),
        # build-std supplies its own private `compiler_builtins` crate to core/alloc/std.  The
        # upstream out-of-tree harness also depends on a same-named shim, which Cargo would pass
        # to rustc alongside the private copy and trigger E0464.  Rename only the test dependency
        # and introduce a local source alias so both implementations remain available without
        # changing the pinned package or lockfile identity.
        source_rewrites=(
            SourceRewrite(
                path=Path("Cargo.toml"),
                marker=(
                    "compiler_builtins = { workspace = true, default-features = false, "
                    "features = [\"unstable-public-internals\"] }\n"
                ),
                replacement=(
                    "compiler_builtins_test = { package = \"compiler_builtins\", "
                    "path = \"../builtins-shim\", default-features = false, "
                    "features = [\"unstable-public-internals\"] } # cargo-dotnet alias\n"
                ),
            ),
            SourceRewrite(
                path=Path("Cargo.toml"),
                marker='default = ["compiler_builtins/arch"]\n',
                replacement=(
                    "default = [] # cargo-dotnet: generic Rust implementation; CLR has no inline asm\n"
                ),
            ),
            SourceRewrite(
                path=Path("Cargo.toml"),
                marker='c = ["compiler_builtins/c"]\n',
                replacement='c = ["compiler_builtins_test/c"]\n',
            ),
            SourceRewrite(
                path=Path("tests/mem.rs"),
                marker="extern crate compiler_builtins;\n",
                replacement="extern crate compiler_builtins_test as compiler_builtins;\n",
            ),
        ),
        source_patches=(
            SourcePatch(
                path=Path("src/lib.rs"),
                marker='#![cfg_attr(f16_enabled, feature(f16))]\n',
                insertion="\nextern crate compiler_builtins_test as compiler_builtins;\n",
            ),
            *(
                SourcePatch(
                    path=Path(f"tests/{name}.rs"),
                    marker="use builtins_test::*;\n",
                    insertion="extern crate compiler_builtins_test as compiler_builtins;\n",
                )
                for name in (
                    "addsub",
                    "cmp",
                    "conv",
                    "div_rem",
                    "float_pow",
                    "misc",
                    "mul",
                    "shift",
                )
            ),
            SourcePatch(
                path=Path("tests/lse.rs"),
                marker="use std::sync::Mutex;\n",
                insertion="\nextern crate compiler_builtins_test as compiler_builtins;\n",
            ),
        ),
        target_policies=(
            TargetPolicy(
                identity="lib",
                runner_kind="compile-only",
                reason=(
                    "builtins-test exposes compiler-builtin checks as integration targets; "
                    "the package library target contains no #[test] functions"
                ),
            ),
            TargetPolicy(
                identity="lse",
                runner_kind="compile-only",
                reason=(
                    "the lock-free-sequence example is cfg-gated away on the synthetic CLR "
                    "target; successful compilation/listing is the available oracle"
                ),
            ),
        ),
    ),
    "libm": SuiteSpec(
        name="libm",
        manifest=Path("compiler-builtins/libm/Cargo.toml"),
        package="libm",
        workspace_manifest=Path("compiler-builtins/Cargo.toml"),
        lockfile=Path("compiler-builtins/Cargo.lock"),
        extra_source_roots=(Path("compiler-builtins/compiler-builtins"),),
    ),
    "libm-tests": SuiteSpec(
        name="libm-tests",
        manifest=Path("compiler-builtins/libm-test/Cargo.toml"),
        package="libm-test",
        workspace_manifest=Path("compiler-builtins/Cargo.toml"),
        lockfile=Path("compiler-builtins/Cargo.lock"),
        extra_source_roots=(
            Path("compiler-builtins/compiler-builtins"),
            Path("compiler-builtins/etc"),
        ),
        # libm-test's MPFR oracle is a native build dependency.  Explicitly opt into the
        # upstream force-cross switch so the test generator can still compile for the CLR target
        # instead of aborting before any Rust test reaches the backend.
        # The default MPFR oracle is a native C build and cannot target the synthetic `dotnet`
        # triple.  The no-default-features run still exercises the generated pure-Rust/libm
        # correctness corpus; MPFR/musl-dependent targets remain explicit feature exclusions.
        cargo_no_default_features=True,
        # The pinned workspace also contains rustc's private compiler_builtins build. Give the
        # test crate an explicit alias to builtins-shim so rustc does not see two rlibs under the
        # conventional `compiler_builtins` name (E0464).
        source_patches=(
            SourcePatch(
                path=Path("src/lib.rs"),
                marker="#![allow(unstable_name_collisions)] // FIXME(float_bits_const): remove when stable\n",
                insertion="\nextern crate compiler_builtins_test as compiler_builtins;\n",
            ),
        ),
        target_policies=(
            TargetPolicy(
                identity="compare_built_musl",
                runner_kind="compile-only",
                reason=(
                    "the target is cfg-gated behind libm-test's build-musl feature; "
                    "the no-default-features CLR profile intentionally checks compilation only"
                ),
            ),
            TargetPolicy(
                identity="multiprecision",
                runner_kind="compile-only",
                reason=(
                    "the target is cfg-gated behind the native MPFR build-mpfr feature; "
                    "the no-default-features CLR profile intentionally checks compilation only"
                ),
            ),
            TargetPolicy(
                identity="u256",
                runner_kind="compile-only",
                reason=(
                    "the target is cfg-gated behind the native MPFR build-mpfr feature; "
                    "the no-default-features CLR profile intentionally checks compilation only"
                ),
            ),
        ),
        # `rand`'s default `thread_rng` feature pulls in getrandom, which intentionally rejects
        # the synthetic CLR target.  The test generator only needs a seed for its deterministic
        # ChaCha stream, so use the no-OS-RNG feature set and a fixed fallback seed.
        source_rewrites=(
            SourceRewrite(
                path=Path("Cargo.toml"),
                marker=(
                    "compiler_builtins = { workspace = true, default-features = false, "
                    "features = [\"unstable-public-internals\"] }\n"
                ),
                replacement=(
                    "compiler_builtins_test = { package = \"compiler_builtins\", "
                    "path = \"../builtins-shim\", default-features = false, "
                    "features = [\"unstable-public-internals\"] } # cargo-dotnet alias\n"
                ),
            ),
            SourceRewrite(
                path=Path("Cargo.toml"),
                marker='default = ["build-mpfr", "unstable-float", "compiler_builtins/arch"]\n',
                replacement='default = ["build-mpfr", "unstable-float", "compiler_builtins_test/arch"]\n',
            ),
            SourceRewrite(
                path=Path("Cargo.toml"),
                marker="rand.workspace = true\n",
                replacement=(
                    "rand = { version = \"0.10.0\", default-features = false, "
                    "features = [\"std\", \"std_rng\"] }\n"
                ),
            ),
            SourceRewrite(
                path=Path("src/generate/random.rs"),
                marker="        let mut rng = rand::rng();\n",
                replacement=(
                    "        let mut rng = ChaCha8Rng::from_seed([0x5a; 32]);\n"
                ),
            ),
        ),
    ),
}
TEST_LIST_LINE = re.compile(r"^(?P<name>.+): (?P<kind>test|benchmark)$")
TEST_LIST_FOOTER = re.compile(r"^(?P<tests>\d+) tests?, (?P<benchmarks>\d+) benchmarks?$")
APPHOST_LINE = re.compile(r"^== running #\[test\] harness on \.NET: (?P<path>.+) ==$")
BUILD_CACHE_SCHEMA = 2
PRODUCER_BUILD_SCHEMA = 1
SUITE_ARTIFACT_SCHEMA = 2
MAX_BATCH_ARGUMENT_BYTES = 24_000
POSIX_BATCH_ARGUMENT_BYTES = 128_000
ARGUMENT_ENVIRONMENT_RESERVE_BYTES = 32_768


@dataclass(frozen=True)
class TestResult:
    name: str
    status: str
    duration_seconds: float
    returncode: int | None
    stdout: str
    stderr: str
    command: list[str]
    terminal_summary: dict[str, int | str] | None = None
    batch_attempt_id: int | None = None


@dataclass(frozen=True)
class TestTarget:
    identity: str
    selector: tuple[str, ...]
    artifact_name: str
    package: str | None = None
    package_id: str | None = None


@dataclass(frozen=True)
class ExcludedTestTarget:
    identity: str
    reason: str
    status: str = "unsupported"


SUMMARY_LINE = re.compile(
    r"^test result: (?P<outcome>ok|FAILED)\. (?P<passed>\d+) passed; "
    r"(?P<failed>\d+) failed; (?P<ignored>\d+) ignored; (?P<measured>\d+) measured; "
    r"(?P<filtered>\d+) filtered out;"
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


GENERATED_ROOTS = (Path(".cargo-dotnet-nuget-assets"), Path("target"))


def tree_digest(
    root: Path,
    *,
    exclude_names: frozenset[str] = frozenset(),
    exclude_paths: frozenset[Path] = frozenset(),
    exclude_roots: tuple[Path, ...] = (),
) -> str:
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if (
            not path.is_file()
            or path.name in exclude_names
            or relative in exclude_paths
            or any(
                relative == generated or generated in relative.parents
                for generated in exclude_roots
            )
        ):
            continue
        relative_bytes = relative.as_posix().encode()
        contents = path.read_bytes()
        digest.update(b"file\0")
        digest.update(len(relative_bytes).to_bytes(8, "big"))
        digest.update(relative_bytes)
        digest.update(len(contents).to_bytes(8, "big"))
        digest.update(contents)
    return digest.hexdigest()


def generated_root_identities(root: Path) -> list[dict[str, str]]:
    identities = []
    for relative in GENERATED_ROOTS:
        path = root / relative
        if path.exists():
            identities.append({"path": relative.as_posix(), "sha256": tree_digest(path) if path.is_dir() else sha256_file(path)})
    return identities


def normalize_generated_roots(root: Path) -> None:
    for relative in GENERATED_ROOTS:
        path = root / relative
        if path.is_symlink():
            raise RuntimeError(f"generated source root must not be a symlink: {path}")
        if path.is_dir():
            shutil.rmtree(path)
        elif path.exists():
            path.unlink()


def reject_symlinked_cache_path(base: Path, path: Path) -> None:
    """Fail closed before reading from or cleaning a deterministic source cache."""
    base = base.absolute()
    path = path.absolute()
    try:
        relative = path.relative_to(base)
    except ValueError as error:
        raise RuntimeError(f"suite cache path escapes its output root: {path}") from error
    current = base
    if current.is_symlink():
        raise RuntimeError(f"suite cache output root must not be a symlink: {current}")
    for part in relative.parts:
        current /= part
        if current.is_symlink():
            raise RuntimeError(f"suite cache path must not contain symlinks: {current}")
        if current.exists() and not current.is_dir():
            raise RuntimeError(f"suite cache directory path is not a directory: {current}")


class TimeoutCleanupError(RuntimeError):
    def __init__(self, message: str, stdout: str, stderr: str):
        super().__init__(message)
        self.stdout = stdout
        self.stderr = stderr


def _kill_process_tree(proc: subprocess.Popen[str]) -> None:
    """Terminate the entire build/test tree, not only Cargo's immediate child."""
    if os.name == "nt":
        # Windows has no portable stdlib Job Object wrapper. taskkill /T covers the normal
        # Cargo -> rustc -> linker/dotnet tree; fall back to the direct child if unavailable.
        taskkill = subprocess.run(
            ["taskkill", "/PID", str(proc.pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if taskkill.returncode != 0:
            proc.kill()
            raise RuntimeError("Windows taskkill /T failed; parent was killed but descendant cleanup is unproven")
        proc.kill()
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass

    # SIGTERM commonly tears down the whole freshly-created process group before the parent wait
    # returns. A follow-up signal can race that teardown; macOS may report a vanished group as
    # EPERM rather than ESRCH. Treat neither errno as cleanup proof: verify the process group is
    # actually empty below.
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass
    if proc.poll() is None:
        try:
            proc.kill()
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + 1.0
    while True:
        members = _process_group_members(proc.pid)
        if not members:
            break
        if time.monotonic() >= deadline:
            raise RuntimeError(
                f"timed-out POSIX process group {proc.pid} still has members: {members}"
            )
        time.sleep(0.02)


def _process_group_members(group: int) -> list[int]:
    """Return live members of one POSIX process group, or fail if cleanup cannot be proven."""
    proc = subprocess.run(
        ["ps", "-eo", "pid=,pgid="],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"could not verify timed-out process-group cleanup: {proc.stderr}")
    members = []
    for line in proc.stdout.splitlines():
        fields = line.split()
        if len(fields) == 2 and fields[1] == str(group):
            members.append(int(fields[0]))
    return members


def run(
    command: list[str], *, timeout: float | None = None, cwd: Path = ROOT, env: dict[str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    options: dict[str, object] = {
        "cwd": cwd,
        "stdout": subprocess.PIPE,
        "stderr": subprocess.PIPE,
        "text": True,
        "encoding": "utf-8",
        "errors": "replace",
        "env": env,
    }
    if os.name == "nt":
        options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    else:
        options["start_new_session"] = True
    proc = subprocess.Popen(command, **options)  # type: ignore[arg-type]
    try:
        stdout, stderr = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired as error:
        try:
            _kill_process_tree(proc)
        except RuntimeError as cleanup_error:
            raise TimeoutCleanupError(
                f"timed-out process-tree cleanup failed: {cleanup_error}",
                decode_timeout_value(error.stdout),
                decode_timeout_value(error.stderr),
            ) from cleanup_error
        try:
            stdout, stderr = proc.communicate(timeout=10)
        except subprocess.TimeoutExpired as drain_error:
            raise RuntimeError("timed-out process group did not terminate after SIGKILL") from drain_error
        raise subprocess.TimeoutExpired(command, timeout, output=stdout or error.stdout, stderr=stderr or error.stderr)
    return subprocess.CompletedProcess(command, proc.returncode, stdout, stderr)


def pinned_toolchain() -> str:
    with (ROOT / "rust-toolchain.toml").open("rb") as source:
        document = tomllib.load(source)
    channel = document.get("toolchain", {}).get("channel")
    if not isinstance(channel, str) or not channel:
        raise RuntimeError("rust-toolchain.toml does not declare a nonempty toolchain.channel")
    return channel


def active_sysroot(explicit: Path | None, toolchain: str) -> Path:
    proc = run(["rustup", "run", toolchain, "rustc", "--print", "sysroot"], timeout=30)
    if proc.returncode:
        raise RuntimeError(f"rustup run {toolchain} rustc --print sysroot failed:\n{proc.stderr}")
    expected = Path(proc.stdout.strip()).resolve()
    if explicit is not None and explicit.resolve() != expected:
        raise RuntimeError(
            f"--sysroot {explicit.resolve()} does not match the repository's pinned "
            f"{toolchain} sysroot {expected}"
        )
    return expected


def command_version(command: list[str]) -> dict[str, str | int | None]:
    """Capture a best-effort launcher identity; compilation of checkout cargo-run is not a gate."""
    try:
        proc = run([*command, "--version"], timeout=30)
    except subprocess.TimeoutExpired:
        return {"status": "timed-out", "returncode": None, "output": None}
    return {
        "status": "ok" if proc.returncode == 0 else "failed",
        "returncode": proc.returncode,
        "output": (proc.stdout + proc.stderr).strip(),
    }


def path_is_within(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
    except ValueError:
        return False
    return True


def command_uses_checkout(command: list[str]) -> bool:
    executable = Path(command[0]).resolve(strict=True)
    if path_is_within(executable, ROOT):
        return True
    checkout_manifest = (ROOT / "tools" / "cargo-dotnet" / "Cargo.toml").resolve()
    for index, argument in enumerate(command[:-1]):
        if argument == "--manifest-path" and Path(command[index + 1]).resolve() == checkout_manifest:
            return True
    if executable.name in {"cargo", "cargo.exe"} and len(command) > 1 and command[1] == "dotnet":
        subcommand = shutil.which("cargo-dotnet")
        return subcommand is not None and path_is_within(Path(subcommand).resolve(), ROOT)
    return False


def checkout_release_artifact_identity() -> dict[str, dict[str, object]]:
    if os.name == "nt":
        backend_name, linker_name = "rustc_codegen_clr.dll", "linker.exe"
    elif platform.system() == "Darwin":
        backend_name, linker_name = "librustc_codegen_clr.dylib", "linker"
    else:
        backend_name, linker_name = "librustc_codegen_clr.so", "linker"
    artifacts = {
        "backend": ROOT / "target" / "release" / backend_name,
        "linker": ROOT / "target" / "release" / linker_name,
    }
    identity: dict[str, dict[str, object]] = {}
    for label, path in artifacts.items():
        if path.is_symlink():
            raise RuntimeError(f"checkout {label} must not be a symlink: {path}")
        resolved = path.resolve(strict=True)
        if not resolved.is_file():
            raise RuntimeError(f"checkout {label} is not a regular file: {resolved}")
        identity[label] = {
            "path": str(resolved),
            "sha256": sha256_file(resolved),
            "bytes": resolved.stat().st_size,
        }
    return identity


def build_checkout_producer(
    timeout: float, command: list[str] | None = None
) -> dict[str, object]:
    command = command or ["cargo", "build", "--release", "--workspace", "--locked"]
    started = time.monotonic()
    print(f"producer build: starting (timeout {timeout:g}s)", flush=True)
    try:
        proc = run(command, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        return {
            "command": command,
            "state": "timeout",
            "duration_seconds": time.monotonic() - started,
            "stdout": decode_timeout_value(error.stdout),
            "stderr": decode_timeout_value(error.stderr),
            "returncode": None,
        }
    record: dict[str, object] = {
        "command": command,
        "state": "completed" if proc.returncode == 0 else "failed",
        "duration_seconds": time.monotonic() - started,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "returncode": proc.returncode,
    }
    if proc.returncode == 0:
        record["artifacts"] = checkout_release_artifact_identity()
    print(
        f"producer build: {record['state']} in {record['duration_seconds']:.3f}s",
        flush=True,
    )
    return record


def command_identity(command: list[str]) -> dict[str, object]:
    """Bind reusable Cargo output to the launcher bytes, not only its display version."""
    executable = Path(command[0]).resolve(strict=True)
    identity: dict[str, object] = {
        "argv": command,
        "executable": {
            "path": str(executable),
            "sha256": sha256_file(executable),
        },
    }
    if executable.name in {"cargo", "cargo.exe"} and len(command) > 1 and command[1] == "dotnet":
        subcommand = shutil.which("cargo-dotnet")
        if subcommand is None:
            raise RuntimeError("cargo dotnet is selected but cargo-dotnet is not on PATH")
        resolved = Path(subcommand).resolve(strict=True)
        identity["cargo_subcommand"] = {
            "path": str(resolved),
            "sha256": sha256_file(resolved),
        }
    if command_uses_checkout(command):
        identity["checkout_release_artifacts"] = checkout_release_artifact_identity()
    return identity


def semantic_environment_sha256() -> str:
    """Hash build-affecting environment without writing values or credentials to receipts."""
    exact = {
        "AR",
        "CC",
        "CFLAGS",
        "CXX",
        "CXXFLAGS",
        "DOTNET_ROOT",
        "OPTIMIZE_CIL",
    }
    prefixes = ("CARGO_", "RUST", "CARGO_DOTNET_")
    material = {
        key: value
        for key, value in os.environ.items()
        if key in exact or key.startswith(prefixes)
    }
    return hashlib.sha256(
        json.dumps(material, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


class BuildCacheLease:
    """Non-blocking cross-process lease for one reusable Cargo target namespace."""

    def __init__(self, path: Path):
        self.path = path
        if path.is_symlink():
            raise RuntimeError(f"refusing symlinked stdlib build-cache lease {path}")
        flags = os.O_RDWR | os.O_CREAT
        flags |= getattr(os, "O_CLOEXEC", 0)
        flags |= getattr(os, "O_NOFOLLOW", 0)
        flags |= getattr(os, "O_BINARY", 0)
        descriptor = os.open(path, flags, 0o600)
        self.file = os.fdopen(descriptor, "r+b")
        if not stat.S_ISREG(os.fstat(self.file.fileno()).st_mode) or path.is_symlink():
            self.file.close()
            raise RuntimeError(f"invalid stdlib build-cache lease {path}")
        if self.file.seek(0, os.SEEK_END) == 0:
            self.file.write(b"0")
            self.file.flush()
        self.file.seek(0)
        self.locked = False

    def try_acquire(self) -> bool:
        try:
            if os.name == "nt":
                import msvcrt

                msvcrt.locking(self.file.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(self.file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return False
        except OSError as error:
            # Windows reports a non-blocking byte-range collision as EACCES/EAGAIN. Other I/O
            # failures are not contention and must not silently downgrade provenance reuse.
            if os.name == "nt" and error.errno in {13, 11}:
                return False
            raise
        self.locked = True
        return True

    def close(self) -> None:
        if self.file.closed:
            return
        if self.locked:
            if os.name == "nt":
                import msvcrt

                self.file.seek(0)
                msvcrt.locking(self.file.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl

                fcntl.flock(self.file.fileno(), fcntl.LOCK_UN)
        self.file.close()


def producer_build_input(
    source_sha256: str, toolchain: str, rustc_version: str
) -> dict[str, object]:
    rustup = shutil.which("rustup")
    if rustup is None:
        raise RuntimeError("rustup is required to build the pinned checkout producer")
    rustup_path = Path(rustup).resolve(strict=True)
    command = [
        str(rustup_path),
        "run",
        toolchain,
        "cargo",
        "build",
        "--release",
        "--workspace",
        "--locked",
    ]
    cargo = run(
        [str(rustup_path), "run", toolchain, "cargo", "-Vv"], timeout=30
    )
    if cargo.returncode:
        raise RuntimeError(f"could not identify pinned cargo:\n{cargo.stderr}")
    return {
        "schema": PRODUCER_BUILD_SCHEMA,
        "root": str(ROOT.resolve()),
        "command": command,
        "producer_source_sha256": source_sha256,
        "toolchain": toolchain,
        "rustc_version": rustc_version,
        "cargo_version": cargo.stdout.strip(),
        "rustup": {
            "path": str(rustup_path),
            "sha256": sha256_file(rustup_path),
        },
        "semantic_environment_sha256": semantic_environment_sha256(),
        "host": {"system": platform.system(), "machine": platform.machine()},
    }


def _validated_producer_receipt(
    path: Path, expected_input: dict[str, object]
) -> dict[str, object] | None:
    if not path.exists():
        return None
    if path.is_symlink() or not path.is_file():
        return None
    try:
        receipt = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    if (
        not isinstance(receipt, dict)
        or receipt.get("schema") != PRODUCER_BUILD_SCHEMA
        or receipt.get("input") != expected_input
    ):
        return None
    try:
        artifacts = checkout_release_artifact_identity()
    except (OSError, RuntimeError):
        return None
    if receipt.get("artifacts") != artifacts:
        return None
    return receipt


def ensure_checkout_producer(
    timeout: float, source_sha256: str, toolchain: str, rustc_version: str
) -> dict[str, object]:
    """Reuse only a post-success receipt that still binds source, tools, and binary bytes."""
    started = time.monotonic()
    cache = DEFAULT_OUT / "producer-build" / "release"
    cache.mkdir(parents=True, exist_ok=True)
    receipt_path = cache / "receipt.json"
    lease = BuildCacheLease(cache / "lease.lock")
    acquire_deadline = time.monotonic() + timeout
    while not lease.try_acquire():
        if time.monotonic() >= acquire_deadline:
            lease.close()
            return {
                "command": [],
                "state": "contended",
                "duration_seconds": time.monotonic() - started,
                "stdout": "",
                "stderr": "checkout producer lease remained busy",
                "returncode": None,
            }
        time.sleep(0.05)
    try:
        expected_input = producer_build_input(
            source_sha256, toolchain, rustc_version
        )
        if receipt := _validated_producer_receipt(receipt_path, expected_input):
            return {
                "command": expected_input["command"],
                "state": "reused",
                "duration_seconds": time.monotonic() - started,
                "stdout": "",
                "stderr": "",
                "returncode": 0,
                "receipt": {
                    "path": str(receipt_path),
                    "sha256": sha256_file(receipt_path),
                },
                "artifacts": receipt["artifacts"],
            }

        record = build_checkout_producer(
            timeout, command=list(expected_input["command"])
        )
        if record["state"] != "completed":
            return record
        if producer_source_sha256() != source_sha256:
            raise RuntimeError("compiler producer sources changed during producer build")
        if producer_build_input(source_sha256, toolchain, rustc_version) != expected_input:
            raise RuntimeError("producer build inputs changed during producer build")
        artifacts = checkout_release_artifact_identity()
        receipt = {
            "schema": PRODUCER_BUILD_SCHEMA,
            "input": expected_input,
            "artifacts": artifacts,
        }
        atomic_write(
            receipt_path, json.dumps(receipt, indent=2, sort_keys=True) + "\n"
        )
        record.update(
            {
                "artifacts": artifacts,
                "receipt": {
                    "path": str(receipt_path),
                    "sha256": sha256_file(receipt_path),
                },
            }
        )
        return record
    finally:
        lease.close()


def build_cache_material(
    *,
    suite: str,
    profile: str,
    toolchain: str,
    rustc_version: str,
    provenance: dict[str, object],
    producer_source: str,
    cargo_dotnet_identity: dict[str, object],
    cargo_dotnet_version: dict[str, object],
) -> dict[str, object]:
    return {
        "schema": BUILD_CACHE_SCHEMA,
        "suite": suite,
        "profile": profile,
        "toolchain": toolchain,
        "rustc_version": rustc_version,
        "source_material_sha256": provenance["material_sha256"],
        "isolated_lock_sha256": provenance["isolated_lock_sha256"],
        "workspace_isolation_manifest_sha256": provenance[
            "workspace_isolation_manifest_sha256"
        ],
        "producer_source_sha256": producer_source,
        "cargo_dotnet": cargo_dotnet_identity,
        "cargo_dotnet_version": cargo_dotnet_version,
        "semantic_environment_sha256": semantic_environment_sha256(),
        "host": {"system": platform.system(), "machine": platform.machine()},
    }


def prepare_build_target(
    out: Path,
    suite: str,
    profile: str,
    run_id: str,
    material: dict[str, object],
    *,
    fresh: bool,
) -> tuple[Path, dict[str, object], BuildCacheLease | None]:
    fresh_target = out / "build" / suite / profile / run_id
    encoded = json.dumps(material, sort_keys=True, separators=(",", ":"))
    key = hashlib.sha256(encoded.encode()).hexdigest()
    if fresh:
        return fresh_target, {"mode": "fresh", "state": "fresh", "key": key}, None

    parent = out / "build-cache" / suite / profile
    parent.mkdir(parents=True, exist_ok=True)
    if parent.is_symlink():
        raise RuntimeError(f"refusing symlinked stdlib build-cache root {parent}")
    namespace = parent / key
    marker_name = "provenance.json"
    if not namespace.exists():
        with tempfile.TemporaryDirectory(dir=parent, prefix=".new-") as temporary:
            staged = Path(temporary) / key
            staged.mkdir()
            atomic_write(
                staged / marker_name,
                json.dumps(material, indent=2, sort_keys=True) + "\n",
            )
            try:
                os.replace(staged, namespace)
            except OSError:
                if not namespace.is_dir():
                    raise
    if namespace.is_symlink() or not namespace.is_dir():
        raise RuntimeError(f"invalid stdlib build-cache namespace {namespace}")
    marker = namespace / marker_name
    if marker.is_symlink() or not marker.is_file():
        raise RuntimeError(
            f"stdlib build-cache namespace has no regular provenance marker: {namespace}"
        )
    try:
        recorded = json.loads(marker.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(
            f"could not validate stdlib build-cache marker {marker}: {error}"
        ) from error
    if recorded != material:
        raise RuntimeError(
            f"stdlib build-cache provenance does not match its digest namespace: {namespace}"
        )

    target = namespace / "target"
    existed = target.exists()
    if target.is_symlink() or (existed and not target.is_dir()):
        raise RuntimeError(f"invalid reusable Cargo target {target}")
    lease = BuildCacheLease(namespace / "lease.lock")
    if not lease.try_acquire():
        lease.close()
        return (
            fresh_target,
            {
                "mode": "reuse",
                "state": "contended-fresh",
                "key": key,
                "namespace": str(namespace),
            },
            None,
        )
    return (
        target,
        {
            "mode": "reuse",
            "state": "reused" if existed else "new",
            "key": key,
            "namespace": str(namespace),
        },
        lease,
    )


def build_target_must_be_fresh(requested_fresh: bool, uses_checkout: bool) -> bool:
    """External launchers cannot prove the backend/linker bytes behind their Cargo state."""
    return requested_fresh or not uses_checkout


def checkout_cargo_dotnet_command() -> list[str]:
    name = "cargo-dotnet.exe" if os.name == "nt" else "cargo-dotnet"
    return [str(ROOT / "target" / "release" / name)]


def split_command(value: str) -> list[str]:
    if os.name != "nt":
        return shlex.split(value)
    # CommandLineToArgvW-compatible enough for the harness contract: preserve backslashes and
    # remove surrounding quotes from paths such as "C:\\Program Files\\cargo-dotnet.exe".
    result, token, quoted = [], [], False
    for char in value:
        if char == '"':
            quoted = not quoted
        elif char.isspace() and not quoted:
            if token:
                result.append("".join(token)); token = []
        else:
            token.append(char)
    if quoted:
        raise RuntimeError("unterminated quote in --cargo-dotnet command")
    if token:
        result.append("".join(token))
    return result


def repository_identity() -> dict[str, str | bool | None]:
    revision = run(["git", "rev-parse", "HEAD"], timeout=30)
    status = run(["git", "status", "--porcelain"], timeout=30)
    diff = run(["git", "diff", "--binary", "HEAD"], timeout=30)
    untracked = run(["git", "ls-files", "--others", "--exclude-standard", "-z"], timeout=30)
    if diff.returncode or untracked.returncode:
        raise RuntimeError("could not establish complete repository worktree identity")
    worktree = hashlib.sha256(diff.stdout.encode())
    for relative in sorted(filter(None, untracked.stdout.split("\0"))):
        path = ROOT / relative
        if path.is_file():
            worktree.update(relative.encode())
            worktree.update(path.read_bytes())
    return {
        "root": str(ROOT),
        "revision": revision.stdout.strip() if revision.returncode == 0 else None,
        "dirty": bool(status.stdout.strip()) if status.returncode == 0 else None,
        "cargo_lock_sha256": sha256_file(ROOT / "Cargo.lock"),
        "worktree_state_sha256": worktree.hexdigest(),
    }


PRODUCER_SOURCE_PATHS = (
    ".cargo",
    "Cargo.lock",
    "Cargo.toml",
    "cilly",
    "crates",
    "dotnet_aot",
    "dotnet_macros",
    "dotnet_overlays",
    "dotnet_pal",
    "mycorrhiza",
    "rust-toolchain.toml",
    "src",
    "tools/cargo-dotnet",
    "x86_64-unknown-dotnet.json",
)


def producer_source_sha256() -> str:
    """Hash checked-out compiler inputs while excluding prose and unrelated fixture churn."""
    listed = run(
        [
            "git",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            *PRODUCER_SOURCE_PATHS,
        ],
        timeout=30,
    )
    if listed.returncode:
        raise RuntimeError(f"could not enumerate compiler producer sources:\n{listed.stderr}")
    digest = hashlib.sha256()
    paths = sorted(filter(None, listed.stdout.split("\0")))
    if not paths:
        raise RuntimeError("compiler producer source set is empty")
    for relative in paths:
        path = ROOT / relative
        digest.update(relative.encode())
        if path.is_symlink():
            digest.update(b"symlink\0")
            digest.update(os.readlink(path).encode())
        elif path.is_file():
            digest.update(b"file\0")
            digest.update(b"executable\0" if os.access(path, os.X_OK) else b"regular\0")
            digest.update(path.read_bytes())
        else:
            digest.update(b"missing\0")
    return digest.hexdigest()


def lock_packages(lockfile: Path) -> set[tuple[str, str, str | None, str | None]]:
    with lockfile.open("rb") as source:
        document = tomllib.load(source)
    packages = document.get("package", [])
    if not isinstance(packages, list):
        raise RuntimeError(f"{lockfile} has no package list")
    result: set[tuple[str, str, str | None, str | None]] = set()
    for package in packages:
        if not isinstance(package, dict):
            raise RuntimeError(f"{lockfile} has a malformed package entry")
        name, version = package.get("name"), package.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            raise RuntimeError(f"{lockfile} has a package without name/version")
        source_value = package.get("source")
        checksum = package.get("checksum")
        if source_value is not None and not isinstance(source_value, str):
            raise RuntimeError(f"{lockfile} has a malformed source identity")
        if checksum is not None and not isinstance(checksum, str):
            raise RuntimeError(f"{lockfile} has a malformed checksum identity")
        result.add((name, version, source_value, checksum))
    return result


def derived_lock_extras(derived: Path, original: Path) -> set[tuple[str, str, str | None, str | None]]:
    return lock_packages(derived) - lock_packages(original)


def resolve_lock_dependency(
    records: dict[tuple[str, str, str | None, str | None], dict], dependency: str, lock: Path
) -> tuple[str, str, str | None, str | None]:
    """Resolve Cargo.lock's name-only, version, and optional `(source)` dependency syntax."""
    parts = dependency.split()
    if not parts:
        raise RuntimeError(f"{lock} has an empty dependency edge")
    name = parts[0]
    version: str | None = None
    source: str | None = None
    rest = parts[1:]
    if rest and not rest[0].startswith("("):
        version = rest.pop(0)
    if rest:
        rendered = " ".join(rest)
        if not (rendered.startswith("(") and rendered.endswith(")")):
            raise RuntimeError(f"{lock} has malformed dependency edge {dependency}")
        source = rendered[1:-1]
    candidates = [identity for identity in records if identity[0] == name and (version is None or identity[1] == version) and (source is None or identity[2] == source)]
    # Cargo omits the source qualifier for a workspace/path package even when a registry
    # package with the same name and version is also present.  The compiler-builtins lock uses
    # exactly this spelling for its local `libm`, while the registry edge is rendered with an
    # explicit `(registry+...)` suffix.  Resolve that unqualified edge to the unique local
    # package; retain the hard ambiguity failure when multiple source-less candidates remain.
    if source is None:
        source_less = [candidate for candidate in candidates if candidate[2] is None]
        if len(source_less) == 1:
            return source_less[0]
    if len(candidates) != 1:
        detail = "ambiguous" if candidates else "unresolved"
        raise RuntimeError(f"{lock} has {detail} dependency edge {dependency}")
    return candidates[0]


def verify_derived_lock(
    derived: Path, original: Path, root_manifest: Path | None = None
) -> None:
    with (root_manifest or derived.parent / "Cargo.toml").open("rb") as source:
        root_name = tomllib.load(source)["package"]["name"]
    def closure(lock: Path) -> tuple[set[tuple[str, str, str | None, str | None]], set[tuple[tuple[str, str, str | None, str | None], tuple[str, str, str | None, str | None]]]]:
        with lock.open("rb") as source:
            packages = tomllib.load(source).get("package", [])
        records = {(p["name"], p["version"], p.get("source"), p.get("checksum")): p for p in packages}
        root = next((identity for identity in records if identity[0] == root_name and identity[2] is None), None)
        if root is None:
            raise RuntimeError(f"{lock} lacks selected suite root {root_name}")
        seen, edges, pending = set(), set(), [root]
        while pending:
            identity = pending.pop()
            if identity in seen: continue
            seen.add(identity)
            for dep in records[identity].get("dependencies", []):
                candidate = resolve_lock_dependency(records, dep, lock)
                edges.add((identity, candidate)); pending.append(candidate)
        return seen, edges
    derived_closure, derived_edges = closure(derived)
    original_closure, original_edges = closure(original)
    if derived_closure != original_closure or derived_edges != original_edges:
        raise RuntimeError("isolated Cargo.lock differs from pinned rust-library selected-suite dependency closure or edges")


def constrain_derived_lock(
    manifest: Path,
    original: Path,
    toolchain: str,
    derived: Path | None = None,
    allowed_local_packages: frozenset[str] = frozenset(),
) -> None:
    """Bring Cargo's fresh standalone resolution back to versions proven by rust's lock."""
    derived = derived or manifest.parent / "Cargo.lock"
    original_packages = lock_packages(original)
    for _ in range(4):
        extras = {
            package
            for package in derived_lock_extras(derived, original)
            if not (package[2] is None and package[0] in allowed_local_packages)
        }
        if not extras:
            return
        for name, _, source_value, _ in sorted(extras):
            candidates = sorted({version for candidate_name, version, candidate_source, _ in original_packages if candidate_name == name and candidate_source == source_value})
            if len(candidates) != 1:
                raise RuntimeError(f"cannot uniquely constrain isolated lock package {name} to pinned rust-library versions")
            update = run(
                ["rustup", "run", toolchain, "cargo", "update", "-p", name, "--precise", candidates[0], "--manifest-path", str(manifest)],
                timeout=180,
                cwd=manifest.parent,
            )
            if update.returncode:
                raise RuntimeError(f"could not constrain isolated Cargo.lock package {name}:\n{update.stderr}")
    verify_derived_lock(derived, original, manifest)


def verify_derived_lock_subset(
    derived: Path, original: Path, local_root_package: str
) -> None:
    """Bind an overlay harness's external resolution to the pinned Rust lock."""
    derived_packages = lock_packages(derived)
    original_packages = lock_packages(original)
    roots = {
        package
        for package in derived_packages
        if package[0] == local_root_package and package[2] is None
    }
    if len(roots) != 1:
        raise RuntimeError(
            f"isolated Cargo.lock lacks overlay root package {local_root_package}"
        )
    external = derived_packages - roots
    if not external.issubset(original_packages):
        raise RuntimeError(
            "isolated Cargo.lock contains dependencies outside the pinned Rust lock"
        )


def cargo_metadata_document(manifest: Path, toolchain: str) -> dict:
    proc = run(
        ["rustup", "run", toolchain, "cargo", "metadata", "--locked", "--no-deps", "--format-version", "1", "--manifest-path", str(manifest)],
        timeout=180,
        cwd=manifest.parent,
    )
    if proc.returncode:
        raise RuntimeError(f"isolated cargo metadata --locked failed:\n{proc.stderr}")
    try:
        document = json.loads(proc.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("cargo metadata returned malformed JSON") from error
    if not isinstance(document, dict) or not isinstance(document.get("packages"), list):
        raise RuntimeError("cargo metadata returned a malformed package inventory")
    return document


def cargo_metadata_locked(manifest: Path, toolchain: str) -> None:
    cargo_metadata_document(manifest, toolchain)


DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def _suite_spec(suite: str | SuiteSpec) -> SuiteSpec:
    return SUITES[suite] if isinstance(suite, str) else suite


def _manifest_document(manifest: Path) -> dict:
    if not manifest.is_file():
        raise RuntimeError(f"missing pinned Cargo manifest {manifest}")
    with manifest.open("rb") as source:
        document = tomllib.load(source)
    if not isinstance(document, dict):
        raise RuntimeError(f"Cargo manifest is not a table: {manifest}")
    return document


def _all_dependency_tables(document: dict) -> Iterable[dict]:
    for name in DEPENDENCY_TABLES:
        table = document.get(name)
        if isinstance(table, dict):
            yield table
    targets = document.get("target")
    if isinstance(targets, dict):
        for target in targets.values():
            if not isinstance(target, dict):
                continue
            for name in DEPENDENCY_TABLES:
                table = target.get(name)
                if isinstance(table, dict):
                    yield table


def _workspace_dependencies(library: Path, spec: SuiteSpec) -> dict:
    if spec.workspace_manifest is None:
        return {}
    document = _manifest_document(library / spec.workspace_manifest)
    workspace = document.get("workspace")
    if not isinstance(workspace, dict):
        raise RuntimeError(
            f"declared workspace manifest lacks [workspace]: {spec.workspace_manifest}"
        )
    dependencies = workspace.get("dependencies", {})
    return dependencies if isinstance(dependencies, dict) else {}


def _workspace_root(library: Path, spec: SuiteSpec) -> Path:
    if spec.workspace_manifest is None:
        return library
    return (library / spec.workspace_manifest).parent.resolve()


def _workspace_member_manifests(library: Path, spec: SuiteSpec) -> list[Path]:
    if spec.workspace_manifest is None:
        return []
    document = _manifest_document(library / spec.workspace_manifest)
    workspace = document.get("workspace")
    if not isinstance(workspace, dict):
        raise RuntimeError(
            f"declared workspace manifest lacks [workspace]: {spec.workspace_manifest}"
        )
    members = workspace.get("members", [])
    if not isinstance(members, list):
        raise RuntimeError("workspace.members must be an array")
    workspace_root = _workspace_root(library, spec)
    manifests: set[Path] = set()
    for pattern in members:
        if not isinstance(pattern, str):
            raise RuntimeError("workspace.members entries must be strings")
        matches = [
            member / "Cargo.toml"
            for member in workspace_root.glob(pattern)
            if (member / "Cargo.toml").is_file()
        ]
        if not matches:
            raise RuntimeError(f"workspace member pattern matched no packages: {pattern}")
        manifests.update(matches)
    return sorted(manifest.relative_to(library) for manifest in manifests)


def _workspace_patch_manifests(library: Path, spec: SuiteSpec) -> list[Path]:
    """Return local patch/replace packages required for the copied workspace to parse."""
    if spec.workspace_manifest is None:
        return []
    document = _manifest_document(library / spec.workspace_manifest)
    workspace_root = _workspace_root(library, spec)
    dependency_tables: list[dict] = []
    patches = document.get("patch")
    if isinstance(patches, dict):
        dependency_tables.extend(
            table for table in patches.values() if isinstance(table, dict)
        )
    replacements = document.get("replace")
    if isinstance(replacements, dict):
        dependency_tables.append(replacements)
    manifests: set[Path] = set()
    for table in dependency_tables:
        for dependency in table.values():
            if not isinstance(dependency, dict):
                continue
            path = dependency.get("path")
            if not isinstance(path, str) or not path:
                continue
            manifest = (workspace_root / path / "Cargo.toml").resolve()
            try:
                relative = manifest.relative_to(library.resolve())
            except ValueError as error:
                raise RuntimeError(
                    f"workspace patch escapes pinned rust library: {path}"
                ) from error
            manifests.add(relative)
    return sorted(manifests)


def _local_dependency_paths(
    document: dict, workspace_dependencies: dict
) -> Iterable[tuple[str, bool]]:
    for table in _all_dependency_tables(document):
        for name, dependency in table.items():
            if not isinstance(dependency, dict):
                continue
            selected = dependency
            if dependency.get("workspace") is True:
                inherited = workspace_dependencies.get(name)
                if not isinstance(inherited, dict):
                    continue
                selected = inherited
            path = selected.get("path")
            if isinstance(path, str) and path:
                yield path, dependency.get("workspace") is True


def suite_closure(library: Path, suite: str | SuiteSpec) -> dict[str, Path]:
    """Resolve the selected manifest's recursive, local Cargo source closure."""
    spec = _suite_spec(suite)
    library = library.resolve()
    workspace_dependencies = _workspace_dependencies(library, spec)
    workspace_root = _workspace_root(library, spec)
    pending = [
        spec.manifest,
        *_workspace_member_manifests(library, spec),
        *_workspace_patch_manifests(library, spec),
    ]
    closure: dict[str, Path] = {}
    seen_manifests: set[Path] = set()
    while pending:
        relative_manifest = pending.pop()
        manifest = (library / relative_manifest).resolve()
        try:
            manifest.relative_to(library)
        except ValueError as error:
            raise RuntimeError(
                f"local dependency escapes pinned rust library: {relative_manifest}"
            ) from error
        if manifest in seen_manifests:
            continue
        seen_manifests.add(manifest)
        document = _manifest_document(manifest)
        source_root = manifest.parent
        relative_root = source_root.relative_to(library).as_posix()
        closure[relative_root] = source_root
        for dependency_path, workspace_relative in _local_dependency_paths(
            document, workspace_dependencies
        ):
            dependency_root = (
                (workspace_root if workspace_relative else source_root)
                / dependency_path
            ).resolve()
            try:
                dependency_relative = dependency_root.relative_to(library)
            except ValueError as error:
                raise RuntimeError(
                    f"local dependency escapes pinned rust library: {dependency_path}"
                ) from error
            pending.append(dependency_relative / "Cargo.toml")
    for extra in spec.extra_source_roots:
        source_root = (library / extra).resolve()
        try:
            relative_root = source_root.relative_to(library).as_posix()
        except ValueError as error:
            raise RuntimeError(f"extra source root escapes pinned rust library: {extra}") from error
        if not source_root.is_dir():
            raise RuntimeError(
                f"missing pinned source-closure member for {spec.name}: {extra}"
            )
        closure[relative_root] = source_root
    return dict(sorted(closure.items()))


def isolated_manifest_bytes(manifest: Path, spec: SuiteSpec) -> bytes:
    if spec.manifest_overlay is not None:
        return spec.manifest_overlay.rstrip().encode() + b"\n"
    original = manifest.read_bytes().rstrip()
    # Manifest rewrites are source-relative to the selected package root.  Apply them before
    # materializing the isolated workspace manifest so creation and cache validation share the
    # same canonical bytes (notably the compiler-builtins test dependency alias).
    manifest_rewrites = tuple(
        rewrite for rewrite in spec.source_rewrites if rewrite.path == Path("Cargo.toml")
    )
    if manifest_rewrites:
        original = _transformed_file_bytes(
            manifest, rewrites=manifest_rewrites
        ).rstrip()
    document = _manifest_document(manifest)
    if spec.workspace_manifest is not None or isinstance(document.get("workspace"), dict):
        return original + b"\n"
    return original + b"\n\n[workspace]\n"


def _validate_source_patch_path(path: Path) -> None:
    if path.is_absolute() or ".." in path.parts:
        raise RuntimeError(f"source compatibility patch escapes the suite root: {path}")


def _transformed_file_bytes(
    path: Path,
    patches: tuple[SourcePatch, ...] = (),
    rewrites: tuple[SourceRewrite, ...] = (),
) -> bytes:
    contents = path.read_bytes()
    for patch in patches:
        _validate_source_patch_path(patch.path)
        marker = patch.marker.encode()
        insertion = patch.insertion.encode()
        if not marker or not insertion:
            raise RuntimeError(f"source compatibility patch must have nonempty text: {patch.path}")
        if insertion in contents:
            continue
        occurrences = contents.count(marker)
        if occurrences != 1:
            raise RuntimeError(
                f"source compatibility patch marker for {patch.path} occurred "
                f"{occurrences} times (expected exactly once)"
            )
        contents = contents.replace(marker, marker + insertion, 1)
    for rewrite in rewrites:
        _validate_source_patch_path(rewrite.path)
        marker = rewrite.marker.encode()
        replacement = rewrite.replacement.encode()
        if not marker or not replacement:
            raise RuntimeError(
                f"source compatibility rewrite must have nonempty text: {rewrite.path}"
            )
        if replacement in contents and marker not in contents:
            continue
        occurrences = contents.count(marker)
        if occurrences != 1:
            raise RuntimeError(
                f"source compatibility rewrite marker for {rewrite.path} occurred "
                f"{occurrences} times (expected exactly once)"
            )
        contents = contents.replace(marker, replacement, 1)
    return contents


def _patched_tree_digest(
    root: Path,
    patches: tuple[SourcePatch, ...],
    *,
    rewrites: tuple[SourceRewrite, ...] = (),
    exclude_names: frozenset[str] = frozenset(),
    exclude_paths: frozenset[Path] = frozenset(),
    exclude_roots: tuple[Path, ...] = (),
) -> str:
    """Digest a source root as it will exist after deterministic compatibility patches."""
    patches_by_path: dict[Path, list[SourcePatch]] = {}
    for patch in patches:
        patches_by_path.setdefault(patch.path, []).append(patch)
    rewrites_by_path: dict[Path, list[SourceRewrite]] = {}
    for rewrite in rewrites:
        rewrites_by_path.setdefault(rewrite.path, []).append(rewrite)
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if (
            not path.is_file()
            or path.name in exclude_names
            or relative in exclude_paths
            or any(
                relative == generated or generated in relative.parents
                for generated in exclude_roots
            )
        ):
            continue
        relative_bytes = relative.as_posix().encode()
        contents = _transformed_file_bytes(
            path,
            patches=tuple(patches_by_path.get(relative, ())),
            rewrites=tuple(rewrites_by_path.get(relative, ())),
        )
        digest.update(b"file\0")
        digest.update(len(relative_bytes).to_bytes(8, "big"))
        digest.update(relative_bytes)
        digest.update(len(contents).to_bytes(8, "big"))
        digest.update(contents)
    for patch in patches:
        if not (root / patch.path).is_file():
            raise RuntimeError(f"source compatibility patch target is missing: {root / patch.path}")
    for rewrite in rewrites:
        if not (root / rewrite.path).is_file():
            raise RuntimeError(
                f"source compatibility rewrite target is missing: {root / rewrite.path}"
            )
    return digest.hexdigest()


def _apply_source_patches(
    root: Path,
    patches: tuple[SourcePatch, ...],
    rewrites: tuple[SourceRewrite, ...] = (),
) -> None:
    """Apply compatibility patches and rewrites to one freshly copied suite root."""
    seen: set[Path] = set()
    for patch in patches:
        _validate_source_patch_path(patch.path)
        seen.add(patch.path)
    for rewrite in rewrites:
        _validate_source_patch_path(rewrite.path)
        seen.add(rewrite.path)
    for path in seen:
        target = root / path
        if not target.is_file():
            raise RuntimeError(f"source compatibility target is missing: {target}")
        original = target.read_bytes()
        transformed = _transformed_file_bytes(
            target,
            patches=tuple(patch for patch in patches if patch.path == path),
            rewrites=tuple(rewrite for rewrite in rewrites if rewrite.path == path),
        )
        if transformed != original:
            target.write_bytes(transformed)


def missing_workspace_members(
    library: Path, spec: SuiteSpec, closure: dict[str, Path]
) -> list[str]:
    """Reject partial workspace roots; Cargo would otherwise resolve uncopied members."""
    if spec.workspace_manifest is None:
        return []
    document = _manifest_document(library / spec.workspace_manifest)
    workspace = document.get("workspace", {})
    members = workspace.get("members", []) if isinstance(workspace, dict) else []
    if not isinstance(members, list):
        raise RuntimeError("workspace.members must be an array")
    copied = {path.resolve() for path in closure.values()}
    workspace_root = (library / spec.workspace_manifest).parent
    missing: set[str] = set()
    for pattern in members:
        if not isinstance(pattern, str):
            raise RuntimeError("workspace.members entries must be strings")
        for member in workspace_root.glob(pattern):
            if (member / "Cargo.toml").is_file() and member.resolve() not in copied:
                missing.add(member.relative_to(workspace_root).as_posix())
    return sorted(missing)


def copy_suite(sysroot: Path, suite: str, out_root: Path, toolchain: str) -> tuple[Path, dict[str, object]]:
    library = sysroot / "lib" / "rustlib" / "src" / "rust" / "library"
    spec = _suite_spec(suite)
    closure = suite_closure(library, spec)
    if missing := missing_workspace_members(library, spec, closure):
        raise RuntimeError(
            "workspace-aware suite copy requires every declared workspace member; "
            f"uncopied members: {', '.join(missing)}"
        )
    source = library / spec.manifest.parent
    manifest = library / spec.manifest
    lockfile = (library / spec.lockfile).resolve()
    try:
        lockfile.relative_to(library.resolve())
    except ValueError as error:
        raise RuntimeError(
            f"suite lockfile escapes pinned rust library: {spec.lockfile}"
        ) from error
    if not manifest.is_file():
        raise RuntimeError(f"missing {suite} at {source}; install rust-src for the pinned toolchain")
    if not lockfile.is_file():
        raise RuntimeError(f"missing rust library lockfile at {lockfile}")
    upstream_closure_hashes = {name: tree_digest(path) for name, path in closure.items()}
    workspace_source_manifest = (
        library / spec.workspace_manifest
        if spec.workspace_manifest is not None
        else manifest
    )
    workspace_source_manifest_hash = sha256_file(workspace_source_manifest)
    suite_root_key = spec.manifest.parent.as_posix()
    source_hash = (
        _patched_tree_digest(
            source,
            spec.source_patches,
            rewrites=spec.source_rewrites,
        )
        if spec.source_patches or spec.source_rewrites
        else upstream_closure_hashes[suite_root_key]
    )
    closure_hashes = dict(upstream_closure_hashes)
    closure_hashes[suite_root_key] = source_hash
    lock_hash = sha256_file(lockfile)
    material_hash = hashlib.sha256(
        (
            json.dumps(closure_hashes, sort_keys=True)
            + ":"
            + json.dumps(spec.source_identity(), sort_keys=True)
            + ":"
            + workspace_source_manifest_hash
            + ":"
            + lock_hash
        ).encode()
    ).hexdigest()
    # `isolated-v3` records that manifest/source rewrites are part of the materialized manifest.
    # Keep rewrite-free suites on the established cache key to avoid invalidating unrelated
    # campaigns while ensuring old compiler-builtins bundles cannot pass validation.
    bundle_version = "isolated-v3" if spec.source_rewrites else "isolated-v2"
    bundle = out_root / "source" / suite / f"{material_hash[:20]}-{bundle_version}"
    destination = bundle / "library" / spec.manifest.parent
    source_workspace_relative = (
        spec.workspace_manifest.parent
        if spec.workspace_manifest is not None
        else spec.manifest.parent
    )
    effective_workspace_relative = (
        spec.manifest.parent
        if spec.manifest_overlay is not None
        else source_workspace_relative
    )
    copied_workspace = bundle / "library" / effective_workspace_relative
    reject_symlinked_cache_path(out_root, bundle)
    if not destination.exists():
        bundle.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=bundle.parent, prefix=f".{suite}-") as tmp:
            staged_bundle = Path(tmp) / "bundle"
            staged_library = staged_bundle / "library"
            for relative, closure_source in closure.items():
                shutil.copytree(closure_source, staged_library / relative)
            if spec.workspace_manifest is not None:
                workspace_source = library / spec.workspace_manifest
                workspace_destination = staged_library / spec.workspace_manifest
                workspace_destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(workspace_source, workspace_destination)
            staged = staged_library / spec.manifest.parent
            staged_workspace = staged_library / effective_workspace_relative
            original_lock = staged_workspace / "Cargo.lock.rust-library"
            shutil.copy2(lockfile, original_lock)
            staged_manifest = staged / "Cargo.toml"
            staged_manifest.write_bytes(isolated_manifest_bytes(manifest, spec))
            # Apply source compatibility after the selected manifest is materialized; otherwise
            # the manifest write above would overwrite a rewrite while leaving source aliases in
            # place and producing a misleading E0463 at compile time.
            _apply_source_patches(
                staged,
                spec.source_patches,
                spec.source_rewrites,
            )
            staged_lock = staged_workspace / "Cargo.lock"
            if spec.workspace_manifest is not None and spec.manifest_overlay is None:
                # A copied workspace retains its exact pinned resolution. Generating a fresh
                # workspace lock would unnecessarily permit unrelated members to drift.
                shutil.copy2(lockfile, staged_lock)
            else:
                generate = run(
                    [
                        "rustup",
                        "run",
                        toolchain,
                        "cargo",
                        "generate-lockfile",
                        "--manifest-path",
                        str(staged_manifest),
                    ],
                    timeout=180,
                    cwd=staged,
                )
                if generate.returncode:
                    raise RuntimeError(
                        f"could not generate isolated Cargo.lock:\n{generate.stderr}"
                    )
                constrain_derived_lock(
                    staged_manifest,
                    original_lock,
                    toolchain,
                    staged_lock,
                    allowed_local_packages=(
                        frozenset({spec.package})
                        if spec.manifest_overlay is not None
                        else frozenset()
                    ),
                )
            if spec.manifest_overlay is not None:
                verify_derived_lock_subset(staged_lock, original_lock, spec.package)
            else:
                verify_derived_lock(staged_lock, original_lock, staged_manifest)
            cargo_metadata_locked(staged_manifest, toolchain)
            os.replace(staged_bundle, bundle)
    reject_symlinked_cache_path(out_root, bundle)
    reject_symlinked_cache_path(out_root, destination)
    reject_symlinked_cache_path(out_root, copied_workspace)
    # Cached suite runs inherit cargo-dotnet output from a previous invocation. Remove only the
    # two declared root-level generated trees before proving immutable rust-src inputs.
    normalize_generated_roots(destination)
    if copied_workspace != destination:
        normalize_generated_roots(copied_workspace)
    copied_manifest = destination / "Cargo.toml"
    expected_manifest = isolated_manifest_bytes(manifest, spec)
    if copied_manifest.read_bytes() != expected_manifest:
        raise RuntimeError(f"cached copied {suite} Cargo.toml does not match the required workspace-isolation overlay")
    copied_source_workspace_manifest = (
        bundle / "library" / spec.workspace_manifest
        if spec.workspace_manifest is not None
        else copied_manifest
    )
    expected_workspace_manifest_hash = (
        workspace_source_manifest_hash
        if spec.workspace_manifest is not None
        else hashlib.sha256(expected_manifest).hexdigest()
    )
    if sha256_file(copied_source_workspace_manifest) != expected_workspace_manifest_hash:
        raise RuntimeError(
            f"cached copied {suite} workspace Cargo.toml does not match pinned rust-src"
        )
    original_lock = copied_workspace / "Cargo.lock.rust-library"
    copied_lock = copied_workspace / "Cargo.lock"
    upstream_source_without_harness_files = tree_digest(
        source, exclude_paths=frozenset({Path("Cargo.toml")})
    )
    source_without_harness_files = (
        _patched_tree_digest(
            source,
            spec.source_patches,
            rewrites=spec.source_rewrites,
            exclude_paths=frozenset({Path("Cargo.toml")}),
        )
        if spec.source_patches or spec.source_rewrites
        else tree_digest(source, exclude_paths=frozenset({Path("Cargo.toml")}))
    )
    copied_without_harness_files = tree_digest(
        destination,
        exclude_paths=frozenset(
            {Path("Cargo.lock"), Path("Cargo.lock.rust-library"), Path("Cargo.toml")}
        ),
        exclude_roots=GENERATED_ROOTS,
    )
    if copied_without_harness_files != source_without_harness_files or sha256_file(original_lock) != lock_hash:
        raise RuntimeError(f"cached copied {suite} source or original Cargo.lock does not match pinned rust-src inputs")
    if spec.manifest_overlay is not None:
        verify_derived_lock_subset(copied_lock, original_lock, spec.package)
    else:
        verify_derived_lock(copied_lock, original_lock, copied_manifest)
    cargo_metadata_locked(copied_manifest, toolchain)
    dependency_hashes = {
        name: digest
        for name, digest in closure_hashes.items()
        if name != suite_root_key
    }
    for name, digest in dependency_hashes.items():
        if tree_digest(bundle / "library" / name) != digest:
            raise RuntimeError(f"cached copied {suite} sibling source closure changed: {name}")
    return destination, {
        "suite_identity": spec.identity(),
        "suite_source_identity": spec.source_identity(),
        "suite_execution_policy_identity": spec.execution_policy_identity(),
        "suite_source_sha256": source_hash,
        "source_without_manifest_sha256": source_without_harness_files,
        "upstream_source_without_manifest_sha256": upstream_source_without_harness_files,
        "library_lock_sha256": lock_hash,
        "isolated_lock_sha256": sha256_file(copied_lock),
        "lock_validation": (
            "pinned-dependency-subset-with-local-overlay-root"
            if spec.manifest_overlay is not None
            else "exact-selected-package-graph"
        ),
        "material_sha256": material_hash,
        "source_path": str(source),
        "source_library_path": str(library),
        "copied_source_path": str(destination),
        "copied_library_path": str(bundle / "library"),
        "copied_workspace_path": str(copied_workspace),
        "copied_source_workspace_manifest_path": str(
            copied_source_workspace_manifest
        ),
        "source_manifest_sha256": sha256_file(manifest),
        "workspace_isolation_manifest_sha256": sha256_file(copied_manifest),
        "workspace_root_manifest_sha256": sha256_file(
            copied_source_workspace_manifest
        ),
        "sibling_source_sha256": dependency_hashes,
    }


def verify_copied_inputs(copied_source: Path, provenance: dict[str, str], toolchain: str) -> None:
    copied_source_digest = tree_digest(
        copied_source,
        exclude_paths=frozenset(
            {Path("Cargo.lock"), Path("Cargo.lock.rust-library"), Path("Cargo.toml")}
        ),
        exclude_roots=GENERATED_ROOTS,
    )
    expected_source_digest = provenance.get("source_without_manifest_sha256")
    if not isinstance(expected_source_digest, str):
        expected_source_digest = tree_digest(
            Path(provenance["source_path"]),
            exclude_paths=frozenset({Path("Cargo.toml")}),
        )
    if copied_source_digest != expected_source_digest:
        raise RuntimeError("copied suite source changed during product build")
    upstream_source_digest = provenance.get("upstream_source_without_manifest_sha256")
    if isinstance(upstream_source_digest, str) and tree_digest(
        Path(provenance["source_path"]),
        exclude_paths=frozenset({Path("Cargo.toml")}),
    ) != upstream_source_digest:
        raise RuntimeError("pinned suite source changed during product build")
    siblings = provenance.get("sibling_source_sha256", {})
    if not isinstance(siblings, dict):
        raise RuntimeError("source closure provenance is malformed")
    for name, digest in siblings.items():
        copied = Path(provenance.get("copied_library_path", copied_source.parent)) / name
        source = Path(provenance.get("source_library_path", Path(provenance["source_path"]).parent)) / name
        if not copied.is_dir() or tree_digest(copied) != digest or tree_digest(source) != digest:
            raise RuntimeError(f"copied sibling source closure changed during product build: {name}")
    copied_workspace = Path(provenance.get("copied_workspace_path", copied_source))
    original_lock = copied_workspace / "Cargo.lock.rust-library"
    if sha256_file(original_lock) != provenance["library_lock_sha256"]:
        raise RuntimeError("copied suite original Cargo.lock changed during product build")
    copied_lock = copied_workspace / "Cargo.lock"
    if sha256_file(copied_lock) != provenance["isolated_lock_sha256"]:
        raise RuntimeError("copied suite isolated Cargo.lock changed during product build")
    if provenance.get("lock_validation") == "pinned-dependency-subset-with-local-overlay-root":
        suite_identity = provenance.get("suite_source_identity")
        package = suite_identity.get("package") if isinstance(suite_identity, dict) else None
        if not isinstance(package, str):
            raise RuntimeError("overlay lock provenance lacks its local package identity")
        verify_derived_lock_subset(copied_lock, original_lock, package)
    else:
        verify_derived_lock(copied_lock, original_lock, copied_source / "Cargo.toml")
    cargo_metadata_locked(copied_source / "Cargo.toml", toolchain)
    if sha256_file(copied_source / "Cargo.toml") != provenance["workspace_isolation_manifest_sha256"]:
        raise RuntimeError("copied suite workspace-isolation manifest changed during product build")
    workspace_manifest = Path(
        provenance.get(
            "copied_source_workspace_manifest_path",
            copied_workspace / "Cargo.toml",
        )
    )
    if sha256_file(workspace_manifest) != provenance["workspace_root_manifest_sha256"]:
        raise RuntimeError("copied suite workspace-root manifest changed during product build")


def parse_test_listing(output: str) -> tuple[list[str], list[str]]:
    """Accept one complete libtest listing only; never infer completeness from records alone."""
    tests: list[str] = []
    benchmarks: list[str] = []
    footers: list[re.Match[str]] = []
    for line in output.splitlines():
        match = TEST_LIST_LINE.match(line.strip())
        if match:
            (tests if match.group("kind") == "test" else benchmarks).append(match.group("name"))
        if footer := TEST_LIST_FOOTER.match(line.strip()):
            footers.append(footer)
    if len(footers) != 1:
        raise RuntimeError(f"expected exactly one complete libtest list footer, found {len(footers)}")
    if len(tests) != len(set(tests)) or len(benchmarks) != len(set(benchmarks)):
        raise RuntimeError("libtest listing contains duplicate test or benchmark names")
    footer = footers[0]
    if int(footer.group("tests")) != len(tests) or int(footer.group("benchmarks")) != len(benchmarks):
        raise RuntimeError("libtest list footer counts do not equal parsed unique records")
    return sorted(tests), sorted(benchmarks)


def discover_test_targets(
    manifest: Path,
    *,
    package: str | None = None,
    toolchain: str | None = None,
) -> tuple[list[TestTarget], list[ExcludedTestTarget]]:
    """Return Cargo's canonical libtest targets and explicit non-libtest exclusions."""
    with manifest.open("rb") as source:
        document = tomllib.load(source)
    package_table = document.get("package", {})
    package_name = package_table.get("name") if isinstance(package_table, dict) else None
    if package is not None and package_name != package:
        raise RuntimeError(
            f"{manifest} package identity is {package_name!r}, expected {package!r}"
        )
    if toolchain is not None:
        metadata = cargo_metadata_document(manifest, toolchain)
        resolved_manifest = manifest.resolve()
        candidates = [
            entry
            for entry in metadata["packages"]
            if isinstance(entry, dict)
            and entry.get("name") == package_name
            and isinstance(entry.get("manifest_path"), str)
            and Path(entry["manifest_path"]).resolve() == resolved_manifest
        ]
        if len(candidates) != 1:
            raise RuntimeError(
                f"cargo metadata did not identify exactly one selected package for {manifest}"
            )
        metadata_package = candidates[0]
        package_id = metadata_package.get("id")
        metadata_targets = metadata_package.get("targets")
        if not isinstance(package_id, str) or not isinstance(metadata_targets, list):
            raise RuntimeError("cargo metadata selected package is malformed")
    else:
        package_id = None
        metadata_targets = None

    targets: list[TestTarget] = []
    excluded: list[ExcludedTestTarget] = []
    lib = document.get("lib")
    explicit_tests = {
        entry.get("name"): entry
        for entry in document.get("test", [])
        if isinstance(entry, dict) and isinstance(entry.get("name"), str)
    }

    if metadata_targets is not None:
        for metadata_target in metadata_targets:
            if not isinstance(metadata_target, dict) or metadata_target.get("test") is False:
                continue
            name = metadata_target.get("name")
            kinds = metadata_target.get("kind")
            if not isinstance(name, str) or not isinstance(kinds, list) or not all(
                isinstance(kind, str) for kind in kinds
            ):
                raise RuntimeError("cargo metadata contains a malformed target")
            kind_set = set(kinds)
            if "bench" in kind_set:
                continue
            if "test" in kind_set:
                identity = name
                selector = ("--test", name)
                explicit = explicit_tests.get(name, {})
            elif kind_set.intersection(
                {"lib", "rlib", "dylib", "staticlib", "cdylib", "proc-macro"}
            ):
                identity = "lib"
                selector = ("--lib",)
                explicit = lib if isinstance(lib, dict) else {}
            else:
                continue
            if isinstance(explicit, dict) and explicit.get("test") is False:
                continue
            if isinstance(explicit, dict) and explicit.get("harness") is False:
                excluded.append(
                    ExcludedTestTarget(identity, "harness=false: not a libtest target")
                )
            else:
                targets.append(
                    TestTarget(
                        identity,
                        selector,
                        name.replace("-", "_") if identity == "lib" else name,
                        package,
                        package_id,
                    )
                )
    elif isinstance(lib, dict) and lib.get("test") is not False:
        lib_name = lib.get("name") if isinstance(lib.get("name"), str) else package_name
        if not isinstance(lib_name, str) or not lib_name:
            raise RuntimeError(f"{manifest} has an enabled lib without a Cargo target name")
        if lib.get("harness") is False:
            excluded.append(
                ExcludedTestTarget("lib", "harness=false: not a libtest target")
            )
        else:
            targets.append(
                TestTarget(
                    "lib", ("--lib",), lib_name.replace("-", "_"), package
                )
            )
    for entry in document.get("test", []) if metadata_targets is None else []:
        if not isinstance(entry, dict):
            continue
        name = entry.get("name")
        if isinstance(name, str) and name and entry.get("test") is not False:
            if entry.get("harness") is False:
                excluded.append(
                    ExcludedTestTarget(name, "harness=false: not a libtest target")
                )
            else:
                targets.append(TestTarget(name, ("--test", name), name, package))
    if not targets:
        raise RuntimeError(f"{manifest} declares no enabled libtest targets")
    identities = [target.identity for target in targets] + [
        target.identity for target in excluded
    ]
    if len(identities) != len(set(identities)):
        raise RuntimeError(f"{manifest} declares duplicate test target names")
    return targets, excluded


def test_target_plan_identity(target: TestTarget) -> dict[str, object]:
    """Compare pinned and copied target plans without path-bearing Cargo package IDs."""
    return {
        "identity": target.identity,
        "selector": target.selector,
        "artifact_name": target.artifact_name,
    }


def test_target_from_record(stored: dict[str, object]) -> TestTarget:
    selector = stored.get("selector")
    if not isinstance(selector, (list, tuple)) or not all(
        isinstance(value, str) for value in selector
    ):
        raise RuntimeError("stored test target selector is malformed")
    identity, artifact_name = stored.get("identity"), stored.get("artifact_name")
    package, package_id = stored.get("package"), stored.get("package_id")
    if not isinstance(identity, str) or not isinstance(artifact_name, str):
        raise RuntimeError("stored test target identity is malformed")
    if package is not None and not isinstance(package, str):
        raise RuntimeError("stored test target package is malformed")
    if package_id is not None and not isinstance(package_id, str):
        raise RuntimeError("stored test target package ID is malformed")
    return TestTarget(
        identity=identity,
        selector=tuple(selector),
        artifact_name=artifact_name,
        package=package,
        package_id=package_id,
    )


def suite_target_policy(spec: SuiteSpec, identity: str) -> TargetPolicy:
    matches = [policy for policy in spec.target_policies if policy.identity == identity]
    if len(matches) > 1:
        raise RuntimeError(f"suite declares duplicate target policy for {identity}")
    policy = matches[0] if matches else TargetPolicy(identity)
    if policy.runner_kind not in {"libtest", "compile-only"}:
        raise RuntimeError(
            f"suite target {identity} has unknown runner kind {policy.runner_kind!r}"
        )
    if policy.runner_kind != "libtest" and not policy.reason:
        raise RuntimeError(
            f"suite target {identity} runner kind requires a nonempty reason"
        )
    if (
        policy.applicable_target_os or policy.applicable_target_arch
    ) and not policy.reason_when_inapplicable:
        raise RuntimeError(
            f"suite target {identity} platform constraint requires a nonempty reason"
        )
    return policy


def platform_inapplicability(
    spec: SuiteSpec, identity: str, target_cfg: TargetCfg
) -> str | None:
    policy = suite_target_policy(spec, identity)
    if (
        policy.applicable_target_os
        and target_cfg.target_os not in policy.applicable_target_os
    ):
        return policy.reason_when_inapplicable
    if (
        policy.applicable_target_arch
        and target_cfg.target_arch not in policy.applicable_target_arch
    ):
        return policy.reason_when_inapplicable
    return None


def target_cfg_from_receipt(receipt: dict) -> TargetCfg:
    document = receipt.get("document")
    identity = document.get("target_spec") if isinstance(document, dict) else None
    if not isinstance(identity, dict):
        raise RuntimeError("build receipt lacks a target-spec identity")
    path_value, expected_hash = identity.get("path"), identity.get("sha256")
    if not isinstance(path_value, str) or not isinstance(expected_hash, str):
        raise RuntimeError("build receipt target-spec identity is malformed")
    path = Path(path_value)
    if not path.is_file() or sha256_file(path) != expected_hash:
        raise RuntimeError("build receipt target spec changed before cfg classification")
    try:
        target = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(f"could not parse receipt-bound target spec: {error}") from error
    families = target.get("target-family")
    if isinstance(families, str):
        families = [families]
    target_os = target.get("os")
    target_arch = target.get("arch")
    pointer_width = target.get("target-pointer-width")
    if (
        not isinstance(target_os, str)
        or not isinstance(target_arch, str)
        or not isinstance(families, list)
        or not all(isinstance(family, str) for family in families)
        or not isinstance(pointer_width, int)
    ):
        raise RuntimeError("receipt-bound target spec lacks a complete cfg contract")
    return TargetCfg(
        target_os=target_os,
        target_arch=target_arch,
        target_families=tuple(families),
        target_pointer_width=pointer_width,
        target_spec_path=str(path.resolve()),
        target_spec_sha256=expected_hash,
    )


def empty_listing_outcome(
    spec: SuiteSpec, identity: str, target_cfg: TargetCfg
) -> tuple[str, str]:
    policy = suite_target_policy(spec, identity)
    if policy.runner_kind == "compile-only":
        return "passed", policy.reason
    if reason := platform_inapplicability(spec, identity, target_cfg):
        return "not-applicable", reason
    raise RuntimeError(
        "libtest listing succeeded but no test names were parsed for "
        f"{identity}; no declarative compile-only or platform policy applies"
    )


def target_outcome_counts(outcomes: Iterable[dict[str, object]]) -> dict[str, int]:
    categories = (
        "passed",
        "failed",
        "timeout",
        "unsupported",
        "not-applicable",
        "listed",
    )
    values = {category: 0 for category in categories}
    for outcome in outcomes:
        status = outcome.get("status")
        if status not in values:
            raise RuntimeError(f"unknown target outcome status {status!r}")
        values[status] += 1
    return values


def initial_target_outcomes(
    spec: SuiteSpec,
    planned: Iterable[TestTarget],
    excluded: Iterable[ExcludedTestTarget],
) -> list[dict[str, object]]:
    outcomes = [
        {
            "identity": target.identity,
            "status": target.status,
            "reason": target.reason,
            "runner_kind": "excluded",
        }
        for target in excluded
    ]
    outcomes.extend(
        {
            "identity": target.identity,
            "status": "pending",
            "reason": "target has not reached a terminal outcome",
            "runner_kind": suite_target_policy(spec, target.identity).runner_kind,
        }
        for target in planned
    )
    identities = [outcome["identity"] for outcome in outcomes]
    if len(identities) != len(set(identities)):
        raise RuntimeError("target outcome plan contains duplicate identities")
    return outcomes


def terminalize_target_outcomes(
    outcomes: Iterable[dict[str, object]],
    *,
    status: str,
    reason: str,
    replace_statuses: frozenset[str] = frozenset({"pending"}),
) -> None:
    if status not in {"failed", "timeout", "unsupported", "not-applicable"}:
        raise ValueError(f"invalid target terminal status {status!r}")
    for outcome in outcomes:
        if outcome.get("status") in replace_statuses:
            outcome.update({"status": status, "reason": reason})


def terminalize_unresolved_tests(
    selected: Iterable[str],
    results_by_name: dict[str, TestResult],
    *,
    status: str,
    reason: str,
) -> list[str]:
    if status not in {"failed", "timeout", "unsupported", "not-applicable"}:
        raise ValueError(f"invalid unresolved terminal status {status!r}")
    unresolved = [name for name in selected if name not in results_by_name]
    for name in unresolved:
        results_by_name[name] = TestResult(
            name=name,
            status=status,
            duration_seconds=0.0,
            returncode=None,
            stdout="",
            stderr=reason,
            command=[],
        )
    return unresolved


def apply_suite_target_filter(
    spec: SuiteSpec,
    targets: list[TestTarget],
    excluded: list[ExcludedTestTarget],
) -> tuple[list[TestTarget], list[ExcludedTestTarget]]:
    if not spec.include_lib_target:
        targets = [target for target in targets if target.identity != "lib"]
        excluded = [target for target in excluded if target.identity != "lib"]
    if not spec.target_identities:
        selected_targets, selected_excluded = targets, excluded
    else:
        requested = list(spec.target_identities)
        duplicates = sorted(
            identity for identity in set(requested) if requested.count(identity) > 1
        )
        if duplicates:
            raise RuntimeError(
                f"suite target policy contains duplicates: {', '.join(duplicates)}"
            )
        available = {target.identity for target in targets} | {
            target.identity for target in excluded
        }
        unknown = sorted(set(requested) - available)
        if unknown:
            raise RuntimeError(
                f"suite target policy names unknown targets: {', '.join(unknown)}"
            )
        selected = set(requested)
        selected_targets = [target for target in targets if target.identity in selected]
        selected_excluded = [
            target for target in excluded if target.identity in selected
        ]
    available = {target.identity for target in selected_targets} | {
        target.identity for target in selected_excluded
    }
    unknown_policies = sorted(
        {policy.identity for policy in spec.target_policies} - available
    )
    if unknown_policies:
        raise RuntimeError(
            f"suite target policies name unavailable targets: {', '.join(unknown_policies)}"
        )
    for identity in available:
        suite_target_policy(spec, identity)
    return selected_targets, selected_excluded
def targets_for_exact_names(
    targets: list[TestTarget], exact_names: list[str]
) -> list[TestTarget]:
    """Prune builds by the qualified target prefix without guessing inner test names."""
    if not exact_names:
        return list(targets)
    duplicates = sorted(name for name in set(exact_names) if exact_names.count(name) > 1)
    if duplicates:
        raise RuntimeError(
            f"duplicate --test-name selectors: {', '.join(duplicates)}"
        )
    target_by_identity = {target.identity: target for target in targets}
    ambiguous = sorted(identity for identity in target_by_identity if "::" in identity)
    if ambiguous:
        raise RuntimeError(
            "enabled test target identities contain the reserved `::` separator: "
            + ", ".join(ambiguous)
        )
    requested_targets: set[str] = set()
    for exact_name in exact_names:
        target_identity, separator, inner_name = exact_name.partition("::")
        if not separator or not target_identity or not inner_name:
            raise RuntimeError(
                "--test-name must be TARGET_IDENTITY::EXACT_LIBTEST_NAME: "
                f"{exact_name!r}"
            )
        if target_identity not in target_by_identity:
            available = ", ".join(target_by_identity) or "none"
            raise RuntimeError(
                f"unknown test target identity in --test-name {exact_name!r}: "
                f"{target_identity!r}; enabled targets: {available}"
            )
        requested_targets.add(target_identity)
    return [target for target in targets if target.identity in requested_targets]


def excluded_targets_for_selection(
    excluded: Iterable[ExcludedTestTarget],
    planned_targets: Iterable[TestTarget],
    exact_names: Iterable[str],
) -> list[ExcludedTestTarget]:
    if not list(exact_names):
        return list(excluded)
    planned_identities = {target.identity for target in planned_targets}
    return [
        target for target in excluded if target.identity in planned_identities
    ]


def targets_for_selection(
    targets: list[TestTarget],
    excluded: list[ExcludedTestTarget],
    *,
    exact_names: list[str],
    requested_target_identities: list[str],
) -> tuple[list[TestTarget], list[ExcludedTestTarget]]:
    """Select whole Cargo test targets or exact libtest cases, with a closed denominator."""
    if exact_names and requested_target_identities:
        raise RuntimeError("--target and --test-name are mutually exclusive")
    duplicates = sorted(
        identity
        for identity in set(requested_target_identities)
        if requested_target_identities.count(identity) > 1
    )
    if duplicates:
        raise RuntimeError(
            "duplicate --target selectors: " + ", ".join(duplicates)
        )
    if requested_target_identities:
        available = {target.identity for target in targets}.union(
            target.identity for target in excluded
        )
        unknown = sorted(set(requested_target_identities) - available)
        if unknown:
            raise RuntimeError(
                "unknown --target selectors: " + ", ".join(unknown)
            )
        requested = set(requested_target_identities)
        return (
            [target for target in targets if target.identity in requested],
            [target for target in excluded if target.identity in requested],
        )
    planned = targets_for_exact_names(targets, exact_names)
    return planned, excluded_targets_for_selection(excluded, planned, exact_names)


def qualified_name(target: TestTarget, name: str) -> str:
    return f"{target.identity}::{name}"


def parse_terminal_summary(output: str) -> dict[str, int | str] | None:
    summaries = [match.groupdict() for line in output.splitlines() if (match := SUMMARY_LINE.match(line.strip()))]
    if len(summaries) != 1:
        return None
    parsed: dict[str, int | str] = {"outcome": summaries[0]["outcome"]}
    parsed.update({key: int(summaries[0][key]) for key in ("passed", "failed", "ignored", "measured", "filtered")})
    return parsed


def proves_exactly_one_test_ran(summary: dict[str, int | str] | None) -> bool:
    if summary is None:
        return False
    return int(summary["ignored"]) == 0 and int(summary["measured"]) == 0 and (
        (summary["outcome"] == "ok" and int(summary["passed"]) == 1 and int(summary["failed"]) == 0)
        or (summary["outcome"] == "FAILED" and int(summary["passed"]) == 0 and int(summary["failed"]) == 1)
    )


def exact_run_protocol(summary: dict[str, int | str] | None, returncode: int) -> bool:
    if not proves_exactly_one_test_ran(summary):
        return False
    return (summary["outcome"] == "ok" and returncode == 0) or (summary["outcome"] == "FAILED" and returncode != 0)


def apphost_identities(output: str) -> list[dict[str, str]]:
    identities: list[dict[str, str]] = []
    for line in output.splitlines():
        match = APPHOST_LINE.match(line.strip())
        if not match:
            continue
        path = Path(match.group("path"))
        entry = {"path": str(path), "exists": str(path.is_file()).lower()}
        if path.is_file():
            entry["sha256"] = sha256_file(path)
        identities.append(entry)
    return identities


def exact_apphost(identities: list[dict[str, str]]) -> Path:
    existing = [Path(entry["path"]) for entry in identities if entry["exists"] == "true"]
    if len(existing) != 1:
        raise RuntimeError(f"expected exactly one existing managed test apphost, found {len(existing)}")
    return existing[0]


def verify_apphost_identity(apphost: Path, identities: list[dict[str, str]]) -> None:
    expected = next((entry.get("sha256") for entry in identities if entry["path"] == str(apphost)), None)
    if expected is None or not apphost.is_file() or sha256_file(apphost) != expected:
        raise RuntimeError("captured apphost changed after build/list; refusing stale or mutable execution")


def build_receipt(apphost: Path, target: TestTarget, target_dir: Path, provenance: dict[str, str], profile: str, toolchain: str) -> dict:
    path = Path(f"{apphost}.rustdotnet.receipt.json")
    if not path.is_file():
        raise RuntimeError(f"cargo dotnet did not emit the expected build receipt {path}")
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(f"could not parse build receipt {path}: {error}") from error
    artifact = document.get("artifact")
    if document.get("schema") != 2 or not isinstance(artifact, dict) or artifact.get("sha256") != sha256_file(apphost) or artifact.get("path") != str(apphost):
        raise RuntimeError("cargo-dotnet receipt does not bind the captured apphost hash")
    selection = document.get("test_target")
    for field in ("private_sysroot_receipt", "backend", "linker", "target_spec"):
        identity = document.get(field)
        if not isinstance(identity, dict) or not isinstance(identity.get("path"), str) or not identity["path"] or not isinstance(identity.get("sha256"), str) or len(identity["sha256"]) != 64:
            raise RuntimeError(f"cargo-dotnet receipt has malformed producer identity {field}")
    expected_selector = "--lib" if target.selector == ("--lib",) else f"--test={target.identity}"
    expected_kind = ["lib"] if target.selector == ("--lib",) else ["test"]
    expected_name = target.artifact_name
    # cargo-dotnet resolves `--package` to one exact member and deliberately rewrites the
    # forwarded Cargo arguments to that member's canonical `--manifest-path`.  The compiler
    # artifact's Cargo package ID is therefore the durable package-selection evidence; requiring
    # the consumed spelling to remain in `cargo_arguments` would reject a correctly bound receipt.
    if not isinstance(selection, dict) or selection.get("selector") != expected_selector or selection.get("package_id") != target.package_id or selection.get("artifact_name") != expected_name or selection.get("artifact_kind") != expected_kind or selection.get("target_dir") != str(target_dir) or selection.get("locked") is not True or selection.get("cargo_lock_sha256") != provenance["isolated_lock_sha256"] or document.get("profile") != profile or document.get("toolchain") != toolchain or document.get("dotnet") != "10":
        raise RuntimeError("cargo-dotnet receipt does not bind the selected locked test target")
    return {"path": str(path), "sha256": sha256_file(path), "document": document}


def producer_identity_tuple(receipt: dict) -> dict[str, object]:
    document = receipt["document"]
    if not isinstance(document, dict):
        raise RuntimeError("cargo-dotnet receipt document is malformed")
    keys = (
        "source", "profile", "target", "dotnet", "toolchain", "private_sysroot_receipt",
        "backend", "linker", "target_spec", "pal_tree_sha256", "overlays_tree_sha256",
    )
    identity = {key: document.get(key) for key in keys}
    if any(value is None for value in identity.values()):
        raise RuntimeError("cargo-dotnet receipt is missing a producer identity field")
    return identity


def suite_artifact_manifest_path(build_cache: dict[str, object]) -> Path | None:
    namespace = build_cache.get("namespace")
    if (
        build_cache.get("mode") != "reuse"
        or build_cache.get("state") not in {"new", "reused"}
        or not isinstance(namespace, str)
    ):
        return None
    return Path(namespace) / f"suite-artifacts-v{SUITE_ARTIFACT_SCHEMA}.json"


SUITE_ARTIFACT_RECORD_KEYS = (
    "target",
    "harness_artifacts",
    "generated_source_roots",
    "apphost",
    "build_receipt",
    "producer_identity",
    "listed_tests",
    "benchmarks_excluded",
    "upstream_ignored_tests",
    "terminal_outcome",
)


def write_suite_artifact_manifest(
    path: Path,
    cache_material: dict[str, object],
    target_dir: Path,
    target_runs: list[dict[str, object]],
) -> None:
    listing_semantics = sha256_file(Path(__file__).resolve())
    records_by_identity: dict[str, dict[str, object]] = {}
    if path.exists():
        if path.is_symlink() or not path.is_file():
            raise RuntimeError(f"invalid suite artifact manifest {path}")
        try:
            previous = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            previous = None
        if (
            isinstance(previous, dict)
            and previous.get("schema") == SUITE_ARTIFACT_SCHEMA
            and previous.get("listing_semantics_sha256") == listing_semantics
            and previous.get("cache_material") == cache_material
            and previous.get("target_dir") == str(target_dir)
            and isinstance(previous.get("targets"), list)
        ):
            for record in previous["targets"]:
                target = record.get("target") if isinstance(record, dict) else None
                identity = target.get("identity") if isinstance(target, dict) else None
                if (
                    not isinstance(identity, str)
                    or identity in records_by_identity
                    or any(key not in record for key in SUITE_ARTIFACT_RECORD_KEYS)
                ):
                    records_by_identity.clear()
                    break
                records_by_identity[identity] = record
    for target_record in target_runs:
        missing = [
            key for key in SUITE_ARTIFACT_RECORD_KEYS if key not in target_record
        ]
        if missing:
            raise RuntimeError(
                f"cannot publish incomplete suite artifact record: {', '.join(missing)}"
            )
        record = {key: target_record[key] for key in SUITE_ARTIFACT_RECORD_KEYS}
        identity = record["target"].get("identity")
        if not isinstance(identity, str):
            raise RuntimeError("cannot publish target record without an identity")
        records_by_identity[identity] = record
    document = {
        "schema": SUITE_ARTIFACT_SCHEMA,
        "listing_semantics_sha256": listing_semantics,
        "cache_material": cache_material,
        "target_dir": str(target_dir),
        "targets": [records_by_identity[key] for key in sorted(records_by_identity)],
    }
    atomic_write(path, json.dumps(document, indent=2, sort_keys=True) + "\n")


def restore_suite_artifact_manifest(
    path: Path,
    cache_material: dict[str, object],
    spec: SuiteSpec,
    planned_targets: list[TestTarget],
    target_dir: Path,
    provenance: dict[str, str],
    profile: str,
    toolchain: str,
    source: Path,
    list_timeout: float = 120.0,
    apphost_env: dict[str, str] | None = None,
) -> list[dict[str, object]] | None:
    if not path.exists():
        return None
    if path.is_symlink() or not path.is_file():
        raise RuntimeError(f"invalid suite artifact manifest {path}")
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(f"could not parse suite artifact manifest {path}: {error}") from error
    if (
        not isinstance(document, dict)
        or document.get("schema") != SUITE_ARTIFACT_SCHEMA
        or document.get("listing_semantics_sha256")
        != sha256_file(Path(__file__).resolve())
        or document.get("cache_material") != cache_material
        or document.get("target_dir") != str(target_dir)
        or not isinstance(document.get("targets"), list)
    ):
        raise RuntimeError("suite artifact manifest provenance does not match this run")

    stored_by_identity: dict[str, dict[str, object]] = {}
    for stored in document["targets"]:
        if not isinstance(stored, dict) or not isinstance(stored.get("target"), dict):
            raise RuntimeError("suite artifact manifest contains a malformed target")
        identity = stored["target"].get("identity")
        if not isinstance(identity, str) or identity in stored_by_identity:
            raise RuntimeError("suite artifact manifest contains duplicate target identities")
        stored_by_identity[identity] = stored

    target_root = target_dir.resolve(strict=True)
    restored: list[dict[str, object]] = []
    apphosts: set[Path] = set()
    for target in planned_targets:
        stored = stored_by_identity.get(target.identity)
        expected_target = asdict(target)
        expected_target["selector"] = list(target.selector)
        if stored is None or stored.get("target") != expected_target:
            raise RuntimeError(
                f"suite artifact manifest does not cover target {target.identity}"
            )
        if any(key not in stored for key in SUITE_ARTIFACT_RECORD_KEYS):
            raise RuntimeError("suite artifact manifest target record is incomplete")
        apphost_value = stored.get("apphost")
        if not isinstance(apphost_value, str):
            raise RuntimeError("suite artifact manifest apphost path is malformed")
        apphost = Path(apphost_value)
        if apphost.is_symlink():
            raise RuntimeError("suite artifact apphost must not be a symlink")
        resolved_apphost = apphost.resolve(strict=True)
        if (
            resolved_apphost in apphosts
            or not resolved_apphost.is_file()
            or not path_is_within(resolved_apphost, target_root)
        ):
            raise RuntimeError("suite artifact apphost is duplicated or outside its target")
        apphosts.add(resolved_apphost)
        receipt_path = Path(f"{apphost}.rustdotnet.receipt.json")
        if receipt_path.is_symlink() or not path_is_within(
            receipt_path.resolve(strict=True), target_root
        ):
            raise RuntimeError("suite artifact receipt is outside its target")

        current_receipt = build_receipt(
            apphost, target, target_dir, provenance, profile, toolchain
        )
        if current_receipt != stored.get("build_receipt"):
            raise RuntimeError("suite artifact build receipt changed")
        current_producer = producer_identity_tuple(current_receipt)
        if current_producer != stored.get("producer_identity"):
            raise RuntimeError("suite artifact producer identity changed")
        harness_artifacts = stored.get("harness_artifacts")
        if not isinstance(harness_artifacts, list):
            raise RuntimeError("suite artifact apphost identity is malformed")
        verify_apphost_identity(apphost, harness_artifacts)

        prefix = f"{target.identity}::"
        listed = stored.get("listed_tests")
        benchmarks = stored.get("benchmarks_excluded")
        ignored = stored.get("upstream_ignored_tests")
        if not all(isinstance(value, list) for value in (listed, benchmarks, ignored)):
            raise RuntimeError("suite artifact test lists are malformed")
        if (
            len(listed) != len(set(listed))
            or any(not isinstance(name, str) or not name.startswith(prefix) for name in listed)
            or any(
                not isinstance(name, str) or not name.startswith(prefix)
                for name in benchmarks
            )
            or any(name not in set(listed) for name in ignored)
        ):
            raise RuntimeError("suite artifact test lists violate target boundaries")
        terminal_outcome = stored.get("terminal_outcome")
        if not isinstance(terminal_outcome, dict):
            raise RuntimeError("suite artifact terminal target outcome is malformed")
        if listed:
            if terminal_outcome.get("status") != "listed":
                raise RuntimeError("suite artifact listed target lacks a listed outcome")
        else:
            target_cfg = target_cfg_from_receipt(current_receipt)
            expected_status, expected_reason = empty_listing_outcome(
                spec, target.identity, target_cfg
            )
            if terminal_outcome != {
                "status": expected_status,
                "reason": expected_reason,
            }:
                raise RuntimeError(
                    "suite artifact empty target outcome differs from declarative policy"
                )

        list_started = time.monotonic()
        list_command = [str(apphost), "--list"]
        ignored_list_command = [str(apphost), "--ignored", "--list"]
        with ThreadPoolExecutor(max_workers=2) as executor:
            listed_future = executor.submit(
                run,
                list_command,
                timeout=list_timeout,
                cwd=source,
                env=apphost_env,
            )
            ignored_future = executor.submit(
                run,
                ignored_list_command,
                timeout=list_timeout,
                cwd=source,
                env=apphost_env,
            )
            listed_proc = listed_future.result()
            ignored_proc = ignored_future.result()
        if listed_proc.returncode:
            raise RuntimeError(
                f"cached apphost could not reproduce the test list for {target.identity}"
            )
        current_names, current_benchmarks = parse_test_listing(listed_proc.stdout)
        if ignored_proc.returncode:
            raise RuntimeError(
                f"cached apphost could not reproduce the ignored list for {target.identity}"
            )
        current_ignored, ignored_benchmarks = parse_test_listing(
            ignored_proc.stdout
        )
        if ignored_benchmarks or not set(current_ignored).issubset(current_names):
            raise RuntimeError(
                f"cached apphost produced an invalid ignored list for {target.identity}"
            )
        qualified_names = [qualified_name(target, name) for name in current_names]
        qualified_benchmarks = [
            qualified_name(target, name) for name in current_benchmarks
        ]
        qualified_ignored = sorted(
            qualified_name(target, name) for name in current_ignored
        )
        if (
            listed != qualified_names
            or benchmarks != qualified_benchmarks
            or ignored != qualified_ignored
        ):
            raise RuntimeError(
                f"suite artifact denominator differs from the cached apphost for {target.identity}"
            )
        list_duration = time.monotonic() - list_started

        restored.append(
            {
                **stored,
                "apphost": str(apphost),
                "list_command": list_command,
                "list_state": "reused-validated-relisted",
                "build_list_duration_seconds": list_duration,
                "list_stdout": listed_proc.stdout,
                "list_stderr": listed_proc.stderr,
                "list_returncode": listed_proc.returncode,
                "list_working_directory": str(source),
                "ignored_list_command": ignored_list_command,
                "ignored_list_state": "reused-validated-relisted",
                "ignored_list_stdout": ignored_proc.stdout,
                "ignored_list_stderr": ignored_proc.stderr,
                "ignored_list_returncode": ignored_proc.returncode,
                "ignored_list_working_directory": str(source),
            }
        )
    return restored


def parse_policy_skips(values: list[str]) -> dict[str, str]:
    skips: dict[str, str] = {}
    for value in values:
        name, separator, reason = value.partition("=")
        if not separator or not name.strip() or not reason.strip():
            raise RuntimeError("--skip-test must be EXACT_TEST_NAME=NONEMPTY_REASON")
        key = name.strip()
        if key in skips:
            raise RuntimeError(f"duplicate backend-policy skip selector: {key}")
        skips[key] = reason.strip()
    return skips


def listing_failure_status(output: str) -> str:
    """Classify failures before normal libtest terminal completion without inventing a score."""
    lower = output.lower()
    if apphost_identities(output):
        return "infrastructure-failed"
    if "type verifier" in lower or "verification failed" in lower or "bad il" in lower:
        return "verifier-failed"
    # No machine-readable cargo-dotnet build-stage receipt exists yet. Do not classify a
    # textual linker mention as definitive; conservative compile failure is evidence-honest.
    return "compile-failed"


def cargo_dotnet_command(
    cargo_dotnet: list[str],
    manifest: Path,
    target: TestTarget,
    debug: bool,
    target_dir: Path,
    args: list[str],
    features: tuple[str, ...] = (),
    no_default_features: bool = False,
) -> list[str]:
    command = [
        *cargo_dotnet,
        "test",
        "--manifest-path",
        str(manifest),
    ]
    if target.package is not None:
        command.extend(["--package", target.package])
    for feature in features:
        command.extend(["--features", feature])
    if no_default_features:
        command.append("--no-default-features")
    command.extend(
        [
            *target.selector,
            "--target-dir",
            str(target_dir),
            "--locked",
            "--backend",
            "native",
        ]
    )
    if debug:
        command.append("--debug")
    command.extend(["--", *args])
    return command


def run_target_build_list(
    command: list[str],
    *,
    target: TestTarget,
    index: int,
    total: int,
    timeout: float,
    cwd: Path,
    record: dict,
) -> subprocess.CompletedProcess[str]:
    """Run one product build/list while retaining progress and terminal evidence."""
    started = time.monotonic()
    record.update(
        {
            "list_state": "pending",
            "build_list_started_at": datetime.now(timezone.utc).isoformat(),
        }
    )
    prefix = f"build/list target {index}/{total} {target.identity}"
    print(f"{prefix}: starting (timeout {timeout:g}s)", flush=True)
    try:
        proc = run(command, timeout=timeout, cwd=cwd)
    except subprocess.TimeoutExpired as error:
        duration = time.monotonic() - started
        record.update(
            {
                "list_state": "timeout",
                "build_list_duration_seconds": duration,
                "list_stdout": decode_timeout_value(error.stdout),
                "list_stderr": decode_timeout_value(error.stderr),
            }
        )
        print(f"{prefix}: timeout after {duration:.3f}s", flush=True)
        raise
    except (OSError, RuntimeError) as error:
        duration = time.monotonic() - started
        record.update(
            {
                "list_state": "infrastructure-failed",
                "build_list_duration_seconds": duration,
                "list_stdout": getattr(error, "stdout", ""),
                "list_stderr": getattr(error, "stderr", ""),
                "list_error": str(error),
            }
        )
        print(f"{prefix}: infrastructure-failed after {duration:.3f}s", flush=True)
        raise
    duration = time.monotonic() - started
    state = "completed" if proc.returncode == 0 else "failed"
    record.update(
        {
            "list_state": state,
            "build_list_duration_seconds": duration,
            "list_returncode": proc.returncode,
            "list_stdout": proc.stdout,
            "list_stderr": proc.stderr,
        }
    )
    print(f"{prefix}: {state} in {duration:.3f}s", flush=True)
    return proc


def apphost_runtime_environment() -> tuple[dict[str, str], dict[str, str | None]]:
    """Mirror cargo-dotnet's .NET 10 PATH/DOTNET_ROOT self-heal for direct apphost runs."""
    env = os.environ.copy()
    listed = run(["dotnet", "--list-runtimes"], timeout=30, env=env)
    if listed.returncode == 0 and "Microsoft.NETCore.App 10." in listed.stdout:
        return env, {"mode": "path", "dotnet_root": env.get("DOTNET_ROOT")}
    home = Path.home() / ".dotnet"
    executable = home / ("dotnet.exe" if os.name == "nt" else "dotnet")
    runtime = home / "shared" / "Microsoft.NETCore.App"
    if executable.is_file() and runtime.is_dir() and any(path.name.startswith("10.") for path in runtime.iterdir()):
        env["PATH"] = str(home) + os.pathsep + env.get("PATH", "")
        env["DOTNET_ROOT"] = str(home)
        return env, {"mode": "self-healed", "dotnet_root": str(home)}
    raise RuntimeError(".NET 10 runtime unavailable for direct captured apphost execution")


def run_one(
    command: list[str], timeout: float, env: dict[str, str], cwd: Path = ROOT
) -> TestResult:
    started = time.monotonic()
    try:
        proc = run(command, timeout=timeout, cwd=cwd, env=env)
    except subprocess.TimeoutExpired as error:
        return TestResult(
            name="",
            status="timeout",
            duration_seconds=time.monotonic() - started,
            returncode=None,
            stdout=decode_timeout_value(error.stdout),
            stderr=decode_timeout_value(error.stderr),
            command=command,
        )
    combined = proc.stdout + "\n" + proc.stderr
    summary = parse_terminal_summary(combined)
    if proc.returncode == 0 and not exact_run_protocol(summary, proc.returncode):
        raise ProtocolError("test apphost exited 0 without one exact libtest terminal summary", proc.stdout, proc.stderr, proc.returncode, summary)
    if not exact_run_protocol(summary, proc.returncode):
        status = "crashed"
    elif summary["outcome"] == "FAILED":
        status = "failed"
    else:
        status = "passed"
    return TestResult(
        name="",
        status=status,
        duration_seconds=time.monotonic() - started,
        returncode=proc.returncode,
        stdout=proc.stdout,
        stderr=proc.stderr,
        command=command,
        terminal_summary=summary,
    )


def run_recorded_exact(
    attempt: dict[str, object],
    timeout: float,
    env: dict[str, str],
    cwd: Path,
) -> TestResult:
    """Execute one exact test while retaining protocol and cleanup evidence in its attempt."""
    command = attempt["command"]
    if not isinstance(command, list) or not all(
        isinstance(argument, str) for argument in command
    ):
        raise RuntimeError("exact attempt command is malformed")
    try:
        result = run_one(command, timeout, env, cwd=cwd)
    except ProtocolError as error:
        attempt.update(
            {
                "state": "protocol-error",
                "error": str(error),
                "stdout": error.stdout,
                "stderr": error.stderr,
                "returncode": error.returncode,
                "terminal_summary": error.summary,
            }
        )
        raise
    except RuntimeError as error:
        attempt.update({"state": "protocol-error", "error": str(error)})
        if isinstance(error, TimeoutCleanupError):
            attempt.update(
                {
                    "state": "timeout-cleanup-failed",
                    "stdout": error.stdout,
                    "stderr": error.stderr,
                }
            )
        raise
    attempt.update({"state": "completed", "result": asdict(result)})
    return result


def recovery_exact_timeout(
    configured_timeout: float, remaining_recovery: float
) -> tuple[float, bool]:
    """Return the available timeout and whether recovery, not the test policy, limits it."""
    if configured_timeout <= 0 or remaining_recovery <= 0:
        raise ValueError("exact and recovery timeouts must be positive")
    return (
        min(configured_timeout, remaining_recovery),
        remaining_recovery < configured_timeout,
    )


def host_batch_argument_bytes() -> int:
    """Use more of POSIX ARG_MAX while retaining the conservative Windows ceiling."""
    if os.name == "nt":
        return MAX_BATCH_ARGUMENT_BYTES
    try:
        argument_max = int(os.sysconf("SC_ARG_MAX"))
    except (AttributeError, OSError, TypeError, ValueError):
        return MAX_BATCH_ARGUMENT_BYTES
    environment_bytes = sum(
        len(os.fsencode(key)) + len(os.fsencode(value)) + 2
        for key, value in os.environ.items()
    )
    usable = argument_max - environment_bytes - ARGUMENT_ENVIRONMENT_RESERVE_BYTES
    if usable <= 0:
        raise RuntimeError(
            "the process environment leaves no safe command-line budget for batched tests"
        )
    return min(POSIX_BATCH_ARGUMENT_BYTES, usable)


def exact_name_batches(
    names: Iterable[str],
    max_tests: int,
    max_bytes: int = MAX_BATCH_ARGUMENT_BYTES,
) -> list[list[str]]:
    """Bound both test count and command-line size for portable exact-name batches."""
    if max_tests <= 0 or max_bytes <= 0:
        raise ValueError("max_tests and max_bytes must be positive")
    batches: list[list[str]] = []
    current: list[str] = []
    current_bytes = 0
    for name in names:
        name_bytes = len(os.fsencode(name)) + 1
        if name_bytes > max_bytes:
            raise RuntimeError(f"libtest name is too long to batch portably: {name!r}")
        if current and (
            len(current) >= max_tests
            or current_bytes + name_bytes > max_bytes
        ):
            batches.append(current)
            current = []
            current_bytes = 0
        current.append(name)
        current_bytes += name_bytes
    if current:
        batches.append(current)
    return batches


def bisect_exact_name_batch(names: list[str]) -> list[list[str]]:
    """Split unresolved exact names stably; singletons deliberately fall back to run_one."""
    if len(names) <= 1:
        return []
    midpoint = len(names) // 2
    return [names[:midpoint], names[midpoint:]]


def parse_batch_json(
    output: str,
    expected_tests: set[str],
    expected_ignored: set[str],
    command: list[str],
    *,
    expected_filtered: int = 0,
) -> tuple[dict[str, TestResult], bool, dict[str, object] | None]:
    """Parse libtest JSON strictly; partial terminal records remain useful after a killed batch."""
    suite_started: dict[str, object] | None = None
    suite_terminal: dict[str, object] | None = None
    terminal_events: dict[str, dict[str, object]] = {}
    for line_number, line in enumerate(output.splitlines(), 1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as error:
            raise RuntimeError(f"batch libtest output line {line_number} is not JSON: {error}") from error
        if not isinstance(event, dict) or event.get("type") not in {"suite", "test"}:
            raise RuntimeError(f"batch libtest output line {line_number} has an unknown record type")
        kind = event["type"]
        state = event.get("event")
        if kind == "suite":
            if state == "started":
                if suite_started is not None:
                    raise RuntimeError("batch libtest output contains two suite-start records")
                suite_started = event
            elif state in {"ok", "failed"}:
                if suite_terminal is not None:
                    raise RuntimeError("batch libtest output contains two suite-terminal records")
                suite_terminal = event
            else:
                raise RuntimeError(f"batch libtest output has unknown suite event {state!r}")
            continue

        name = event.get("name")
        if not isinstance(name, str) or name not in expected_tests:
            raise RuntimeError(f"batch libtest output contains unexpected test {name!r}")
        if state == "started":
            continue
        if state not in {"ok", "failed", "ignored"}:
            raise RuntimeError(f"batch libtest output has unknown test event {state!r}")
        if name in terminal_events:
            raise RuntimeError(f"batch libtest output contains duplicate terminal event for {name}")
        terminal_events[name] = event

    if suite_started is None or suite_started.get("test_count") != len(expected_tests):
        raise RuntimeError("batch libtest suite-start count does not match the selected target")
    emitted_ignored = {
        name for name, event in terminal_events.items() if event["event"] == "ignored"
    }
    if not emitted_ignored.issubset(expected_ignored):
        raise RuntimeError("batch libtest marked a non-upstream-ignored test as ignored")

    complete = suite_terminal is not None
    if not complete and set(terminal_events) == expected_tests:
        raise RuntimeError(
            "batch libtest emitted every test terminal event without a suite-terminal record"
        )
    if complete:
        if set(terminal_events) != expected_tests or emitted_ignored != expected_ignored:
            raise RuntimeError("completed batch libtest output lacks an exact terminal test partition")
        observed = {
            "passed": sum(event["event"] == "ok" for event in terminal_events.values()),
            "failed": sum(event["event"] == "failed" for event in terminal_events.values()),
            "ignored": len(emitted_ignored),
        }
        for field, value in observed.items():
            if suite_terminal.get(field) != value:
                raise RuntimeError(f"batch libtest suite {field} count does not match test records")
        if (
            suite_terminal.get("measured") != 0
            or suite_terminal.get("filtered_out") != expected_filtered
        ):
            raise RuntimeError("full-target batch measured tests or filtered an unexpected count")
        expected_state = "failed" if observed["failed"] else "ok"
        if suite_terminal.get("event") != expected_state:
            raise RuntimeError("batch libtest suite outcome does not match per-test outcomes")

    results: dict[str, TestResult] = {}
    for name, event in terminal_events.items():
        state = event["event"]
        if state == "ignored":
            continue
        duration = event.get("exec_time", 0.0)
        if not isinstance(duration, (int, float)) or duration < 0:
            raise RuntimeError(f"batch libtest event has invalid duration for {name}")
        stdout = event.get("stdout", "")
        if not isinstance(stdout, str):
            raise RuntimeError(f"batch libtest event has invalid stdout for {name}")
        results[name] = TestResult(
            name=name,
            status="passed" if state == "ok" else "failed",
            duration_seconds=float(duration),
            returncode=0 if state == "ok" else 101,
            stdout=stdout,
            stderr="",
            command=command,
        )
    return results, complete, suite_terminal


class ProtocolError(RuntimeError):
    def __init__(self, message: str, stdout: str, stderr: str, returncode: int, summary: dict[str, int | str] | None):
        super().__init__(message)
        self.stdout, self.stderr, self.returncode, self.summary = stdout, stderr, returncode, summary


def decode_timeout_value(value: str | bytes | None) -> str:
    if value is None:
        return ""
    return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value


def counts(results: Iterable[TestResult]) -> dict[str, int]:
    values = {
        key: 0
        for key in (
            "passed",
            "failed",
            "crashed",
            "timeout",
            "unsupported",
            "not-applicable",
            "upstream-ignored",
            "backend-policy-skip",
        )
    }
    for result in results:
        if result.status not in values:
            raise RuntimeError(f"unknown test result status {result.status!r}")
        values[result.status] += 1
    return values


def finalize_target_outcomes(
    target_outcomes: list[dict[str, object]], results: Iterable[TestResult]
) -> None:
    results_by_target: dict[str, list[TestResult]] = {}
    for result in results:
        target, separator, _ = result.name.partition("::")
        if separator:
            results_by_target.setdefault(target, []).append(result)
    for outcome in target_outcomes:
        identity = outcome.get("identity")
        if not isinstance(identity, str):
            raise RuntimeError("target outcome lacks an identity")
        target_results = results_by_target.get(identity, [])
        if not target_results:
            if outcome.get("status") == "pending":
                outcome.update(
                    {
                        "status": "listed",
                        "reason": "target listed successfully; no cases were selected",
                    }
                )
            continue
        statuses = {result.status for result in target_results}
        if "timeout" in statuses:
            status = "timeout"
        elif statuses.intersection({"failed", "crashed"}):
            status = "failed"
        elif "unsupported" in statuses:
            status = "unsupported"
        elif "not-applicable" in statuses:
            status = "not-applicable"
        else:
            status = "passed"
        summary = counts(target_results)
        outcome.update(
            {
                "status": status,
                "reason": "terminal test-case partition: "
                + ", ".join(
                    f"{name}={value}" for name, value in summary.items() if value
                ),
            }
        )


def markdown(report: dict) -> str:
    lines = [f"# {report['suite']} measurement ({report['profile']})", ""]
    lines.append(f"Terminal status: **{report['terminal_status']}**")
    lines.append("")
    if report["terminal_status"] == "listed":
        lines.append("Discovery completed successfully. This report intentionally contains no execution score.")
        lines.append("")
        lines.append(f"Listed: **{len(report.get('listed_tests', []))}**; selected: **{len(report.get('selected_tests', []))}**.")
        return "\n".join(lines) + "\n"
    if report["terminal_status"] != "complete":
        lines.append("No pass percentage is available because the suite did not reach measured terminal completion.")
        lines.append("")
        lines.append(f"Error: `{report.get('error', 'unknown infrastructure error')}`")
        return "\n".join(lines) + "\n"
    result_counts = report["counts"]
    executed = sum(result_counts[key] for key in ("passed", "failed", "crashed", "timeout"))
    selected = sum(result_counts.values())
    selected_score = 100.0 * result_counts["passed"] / selected if selected else None
    executed_score = 100.0 * result_counts["passed"] / executed if executed else None
    lines.extend([
        "| Status | Count |",
        "| --- | ---: |",
        *[f"| {key} | {value} |" for key, value in result_counts.items()],
        f"| selected denominator | {selected} |",
        f"| executed | {executed} |",
        f"| pass percentage of selected | {selected_score:.2f}% |"
        if selected_score is not None
        else "| pass percentage of selected | n/a |",
        f"| pass percentage of executed | {executed_score:.2f}% |"
        if executed_score is not None
        else "| pass percentage of executed | n/a |",
        "",
        "The selected denominator includes upstream ignores and every backend-policy skip. The",
        "executed percentage is diagnostic only and must not be used as the compatibility headline.",
    ])
    return "\n".join(lines) + "\n"


def atomic_write(path: Path, contents: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
    temporary.write_text(contents, encoding="utf-8")
    os.replace(temporary, path)


def write_report(path: Path, report: dict, canonical_path: Path | None = None) -> None:
    atomic_write(path, json.dumps(report, indent=2, sort_keys=True) + "\n")
    atomic_write(path.with_suffix(".md"), markdown(report))
    if canonical_path is not None:
        atomic_write(canonical_path, json.dumps({"run_report": str(path), "run_id": report["run_id"], "run_report_sha256": sha256_file(path)}, indent=2) + "\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite", choices=sorted(SUITES))
    parser.add_argument("--sysroot", type=Path, help="Pinned rustc sysroot; defaults to active rustc")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--debug", action="store_true", help="Use cargo-dotnet's debug profile")
    parser.add_argument("--filter", help="Run/list only exact libtest names containing this text")
    parser.add_argument(
        "--test-name",
        action="append",
        default=[],
        metavar="TARGET::EXACT_LIBTEST_NAME",
        help="Qualified exact libtest name; repeatable. The target prefix prunes unrelated harness builds",
    )
    parser.add_argument(
        "--target",
        action="append",
        default=[],
        metavar="CARGO_TEST_TARGET",
        help=(
            "Run/list every case in one Cargo test target; repeatable. Mutually exclusive "
            "with --test-name and also selects compile-only or excluded target outcomes"
        ),
    )
    parser.add_argument(
        "--skip-test",
        action="append",
        default=[],
        metavar="NAME=REASON",
        help="Explicit backend-policy skip; repeatable and recorded in the selected denominator",
    )
    parser.add_argument("--list", action="store_true", help="List and report selected tests without execution")
    parser.add_argument("--timeout", type=float, default=120.0, help="Per-test terminal timeout in seconds")
    parser.add_argument(
        "--batch-timeout",
        type=float,
        default=30.0,
        help="Per-shard JSON batch hang guard in seconds; unresolved tests fall back to exact runs",
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=4096,
        help="Initial exact-name batch cap; failures split adaptively and command size remains bounded",
    )
    parser.add_argument(
        "--adaptive-attempt-limit",
        type=int,
        default=64,
        help="Maximum batch and exact-fallback launches per target during adaptive execution",
    )
    parser.add_argument(
        "--adaptive-recovery-timeout",
        type=float,
        default=120.0,
        help="Total batch and exact-fallback recovery budget per target in seconds",
    )
    parser.add_argument(
        "--exact-execution",
        action="store_true",
        help="Disable full-target JSON batching and launch one apphost process per selected test",
    )
    parser.add_argument(
        "--build-timeout",
        type=float,
        default=1800.0,
        help="Build/list timeout per planned test target in seconds",
    )
    parser.add_argument(
        "--fresh-build",
        action="store_true",
        help="Use a new run-local Cargo target instead of the provenance-keyed reusable cache",
    )
    parser.add_argument(
        "--cargo-dotnet",
        help="Shell-like command used for product builds; defaults to this checkout's cargo-dotnet source",
    )
    args = parser.parse_args(argv)
    if (
        args.timeout <= 0
        or args.batch_timeout <= 0
        or args.build_timeout <= 0
        or args.batch_size <= 0
        or args.adaptive_attempt_limit <= 0
        or args.adaptive_recovery_timeout <= 0
    ):
        parser.error(
            "timeouts, --batch-size, and --adaptive-attempt-limit must be positive"
        )
    started = time.monotonic()
    out = args.out.resolve()
    default_cargo_dotnet = args.cargo_dotnet is None
    cargo_dotnet = (
        split_command(args.cargo_dotnet)
        if args.cargo_dotnet
        else checkout_cargo_dotnet_command()
    )
    if not cargo_dotnet:
        parser.error("--cargo-dotnet must not be empty")
    uses_checkout = default_cargo_dotnet
    if not default_cargo_dotnet:
        executable = shutil.which(cargo_dotnet[0])
        if executable is None:
            parser.error(f"--cargo-dotnet executable is not on PATH: {cargo_dotnet[0]}")
        cargo_dotnet[0] = executable
        uses_checkout = command_uses_checkout(cargo_dotnet)
    profile = "debug" if args.debug else "release"
    batch_argument_limit = host_batch_argument_bytes()
    run_id = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ") + "-" + uuid.uuid4().hex[:12]
    report_path = out / "reports" / args.suite / profile / f"{run_id}.json"
    canonical_path = out / "reports" / args.suite / profile / "latest-full.json"
    report: dict = {
        "schema_version": 1,
        "suite": args.suite,
        "profile": profile,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "run_id": run_id,
        "terminal_status": "preparing",
        "selection": {
            "filter": args.filter,
            "exact_names": args.test_name,
            "backend_policy_skip_arguments": args.skip_test,
            "execution_mode": (
                "exact"
                if args.exact_execution
                else "adaptive-sharded-batch-with-exact-fallback"
            ),
            "batch_timeout_seconds": args.batch_timeout,
            "batch_size": args.batch_size,
            "batch_argument_byte_limit": batch_argument_limit,
            "adaptive_attempt_limit": args.adaptive_attempt_limit,
            "adaptive_recovery_timeout_seconds": args.adaptive_recovery_timeout,
        },
    }
    cache_lease: BuildCacheLease | None = None
    try:
        toolchain = pinned_toolchain()
        sysroot = active_sysroot(args.sysroot, toolchain)
        rustc = run(["rustup", "run", toolchain, "rustc", "-Vv"], timeout=30)
        if rustc.returncode:
            raise RuntimeError(f"rustup run {toolchain} rustc -Vv failed:\n{rustc.stderr}")
        producer_source = producer_source_sha256()
        if uses_checkout:
            producer_build = ensure_checkout_producer(
                args.build_timeout,
                producer_source,
                toolchain,
                rustc.stdout.strip(),
            )
            report["producer_build"] = producer_build
            if producer_build["state"] not in {"completed", "reused"}:
                raise RuntimeError("checkout release producer build did not complete")
        executable = shutil.which(cargo_dotnet[0])
        if executable is None:
            parser.error(f"--cargo-dotnet executable is not on PATH: {cargo_dotnet[0]}")
        cargo_dotnet[0] = executable
        repository = repository_identity()
        cargo_dotnet_version = command_version(cargo_dotnet)
        cargo_dotnet_identity = command_identity(cargo_dotnet)
        policy_skips = parse_policy_skips(args.skip_test)
        suite_spec = SUITES[args.suite]
        pinned_manifest = (
            sysroot
            / "lib"
            / "rustlib"
            / "src"
            / "rust"
            / "library"
            / suite_spec.manifest
        )
        pinned_targets, pinned_excluded_targets = discover_test_targets(
            pinned_manifest,
            package=suite_spec.source_package or suite_spec.package,
            toolchain=toolchain,
        )
        pinned_targets, pinned_excluded_targets = apply_suite_target_filter(
            suite_spec, pinned_targets, pinned_excluded_targets
        )
        planned_pinned_targets, planned_pinned_excluded_targets = targets_for_selection(
            pinned_targets,
            pinned_excluded_targets,
            exact_names=args.test_name,
            requested_target_identities=args.target,
        )
        source, provenance = copy_suite(sysroot, args.suite, out, toolchain)
        manifest = source / "Cargo.toml"
        targets, excluded_targets = discover_test_targets(
            manifest, package=suite_spec.package, toolchain=toolchain
        )
        targets, excluded_targets = apply_suite_target_filter(
            suite_spec, targets, excluded_targets
        )
        if (
            [test_target_plan_identity(target) for target in targets]
            != [test_target_plan_identity(target) for target in pinned_targets]
            or excluded_targets != pinned_excluded_targets
        ):
            raise RuntimeError(
                "copied suite test-target plan differs from the pinned rust-src manifest"
            )
        planned_targets, accounted_excluded_targets = targets_for_selection(
            targets,
            excluded_targets,
            exact_names=args.test_name,
            requested_target_identities=args.target,
        )
        if [test_target_plan_identity(target) for target in planned_targets] != [
            test_target_plan_identity(target) for target in planned_pinned_targets
        ] or accounted_excluded_targets != planned_pinned_excluded_targets:
            raise RuntimeError(
                "copied suite exact-target build plan differs from pinned rust-src preflight"
            )
        target_outcomes = initial_target_outcomes(
            suite_spec, planned_targets, accounted_excluded_targets
        )
        target_outcome_by_identity = {
            outcome["identity"]: outcome for outcome in target_outcomes
        }
        cache_material = build_cache_material(
            suite=args.suite,
            profile=profile,
            toolchain=toolchain,
            rustc_version=rustc.stdout.strip(),
            provenance=provenance,
            producer_source=producer_source,
            cargo_dotnet_identity=cargo_dotnet_identity,
            cargo_dotnet_version=cargo_dotnet_version,
        )
        target_dir, build_cache, cache_lease = prepare_build_target(
            out,
            args.suite,
            profile,
            run_id,
            cache_material,
            fresh=build_target_must_be_fresh(args.fresh_build, uses_checkout),
        )
        report.update({
            "suite_identity": suite_spec.identity(),
            "excluded_test_targets": [
                asdict(target) for target in accounted_excluded_targets
            ],
            "target_outcomes": target_outcomes,
            "sysroot": str(sysroot),
            "toolchain": toolchain,
            "rustc_version": rustc.stdout.strip(),
            "host": {"platform": platform.platform(), "python": platform.python_version()},
            "repository": repository,
            "provenance": provenance,
            "test_targets": [asdict(target) for target in targets],
            "planned_test_targets": [asdict(target) for target in planned_targets],
            "target_dir": str(target_dir),
            "build_cache": build_cache,
            "harness_identity": {
                "script_sha256": sha256_file(Path(__file__).resolve()),
                "cargo_dotnet_command": cargo_dotnet,
                "cargo_dotnet_version": cargo_dotnet_version,
                "cargo_dotnet_identity": cargo_dotnet_identity,
                "producer_source_sha256": producer_source,
                "checkout_cargo_dotnet_manifest_sha256": sha256_file(ROOT / "tools" / "cargo-dotnet" / "Cargo.toml"),
                "checkout_test_command_sha256": sha256_file(ROOT / "tools" / "cargo-dotnet" / "src" / "test.rs"),
                "forced_backend": "native",
                "ambient_cargo_dotnet_backend": os.environ.get("CARGO_DOTNET_BACKEND"),
            },
            "selection": {
                "filter": args.filter,
                "exact_names": args.test_name,
                "backend_policy_skips": policy_skips,
                "execution_mode": (
                    "exact"
                    if args.exact_execution
                    else "adaptive-sharded-batch-with-exact-fallback"
                ),
                "batch_timeout_seconds": args.batch_timeout,
                "batch_size": args.batch_size,
                "batch_argument_byte_limit": batch_argument_limit,
                "adaptive_attempt_limit": args.adaptive_attempt_limit,
                "adaptive_recovery_timeout_seconds": args.adaptive_recovery_timeout,
                "target_pruning": {
                    "mode": (
                        "whole-cargo-test-target"
                        if args.target
                        else "qualified-exact-name-prefix"
                        if args.test_name
                        else "none"
                    ),
                    "requested_target_identities": args.target,
                    "enabled_target_identities": [target.identity for target in targets],
                    "planned_target_identities": [
                        target.identity for target in planned_targets
                    ],
                },
            },
        })
        apphost_env, runtime_environment = apphost_runtime_environment()
        report["apphost_runtime_environment"] = runtime_environment
        target_runs: list[dict] = []
        all_tests: list[str] = []
        upstream_ignored: set[str] = set()
        captured_apphosts: dict[str, str] = {}
        producer_identity: dict[str, object] | None = None
        artifact_manifest = (
            suite_artifact_manifest_path(build_cache) if uses_checkout else None
        )
        restored_targets: list[dict[str, object]] | None = None
        if artifact_manifest is not None:
            try:
                restored_targets = restore_suite_artifact_manifest(
                    artifact_manifest,
                    cache_material,
                    suite_spec,
                    planned_targets,
                    target_dir,
                    provenance,
                    profile,
                    toolchain,
                    source,
                    list_timeout=args.build_timeout,
                    apphost_env=apphost_env,
                )
            except subprocess.TimeoutExpired as error:
                report["suite_artifact_cache"] = {
                    "state": "invalid-miss",
                    "path": str(artifact_manifest),
                    "reason": str(error),
                    "validation_stdout": decode_timeout_value(error.stdout),
                    "validation_stderr": decode_timeout_value(error.stderr),
                }
            except (OSError, RuntimeError) as error:
                report["suite_artifact_cache"] = {
                    "state": "invalid-miss",
                    "path": str(artifact_manifest),
                    "reason": str(error),
                }
            else:
                report["suite_artifact_cache"] = {
                    "state": "hit" if restored_targets is not None else "miss",
                    "path": str(artifact_manifest),
                }
        else:
            report["suite_artifact_cache"] = {
                "state": (
                    "disabled" if uses_checkout else "disabled-external-producer"
                )
            }

        targets_to_build = planned_targets
        if restored_targets is not None:
            verify_copied_inputs(source, provenance, toolchain)
            target_runs.extend(restored_targets)
            report["target_runs"] = target_runs
            targets_to_build = []
            for target_record in target_runs:
                stored_target = target_record["target"]
                target = test_target_from_record(stored_target)
                target_record.setdefault("original_build_list_command", cargo_dotnet_command(
                    cargo_dotnet,
                    manifest,
                    target,
                    args.debug,
                    target_dir,
                    ["--list"],
                    suite_spec.cargo_features,
                    suite_spec.cargo_no_default_features,
                ))
                target_record.setdefault("ignored_list_command", [
                    target_record["apphost"],
                    "--ignored",
                    "--list",
                ])
                restored_identity = stored_target["identity"]
                restored_count = len(target_record["listed_tests"])
                stored_outcome = target_record["terminal_outcome"]
                if stored_outcome.get("status") == "listed":
                    stored_outcome = {
                        "status": "listed",
                        "reason": f"revalidated {restored_count} cached libtest cases",
                    }
                    target_record["terminal_outcome"] = stored_outcome
                target_outcome_by_identity[restored_identity].update(stored_outcome)
                apphost = target_record["apphost"]
                if apphost in captured_apphosts:
                    raise RuntimeError("suite artifact manifest reuses one apphost")
                captured_apphosts[apphost] = target.identity
                all_tests.extend(target_record["listed_tests"])
                upstream_ignored.update(target_record["upstream_ignored_tests"])
                receipt_identity = target_record["producer_identity"]
                if producer_identity is None:
                    producer_identity = receipt_identity
                elif producer_identity != receipt_identity:
                    raise RuntimeError(
                        "suite artifact producer identity differs between targets"
                    )
                restored_cfg = target_cfg_from_receipt(
                    target_record["build_receipt"]
                )
                target_record["target_cfg"] = asdict(restored_cfg)
                recorded_target_cfg = report.get("compiler_target_cfg")
                if recorded_target_cfg is None:
                    report["compiler_target_cfg"] = asdict(restored_cfg)
                elif recorded_target_cfg != asdict(restored_cfg):
                    raise RuntimeError(
                        "compiler target cfg differs between restored targets"
                    )

        for target_index, target in enumerate(targets_to_build, 1):
            list_cmd = cargo_dotnet_command(
                cargo_dotnet,
                manifest,
                target,
                args.debug,
                target_dir,
                ["--list"],
                suite_spec.cargo_features,
                suite_spec.cargo_no_default_features,
            )
            target_record = {
                "target": asdict(target),
                "list_command": list_cmd,
                "list_working_directory": str(source),
                "list_state": "pending",
            }
            # Persist evidence before parsing, receipt, or ignored-list validation can fail.
            target_runs.append(target_record)
            report["target_runs"] = target_runs
            listed_proc = run_target_build_list(
                list_cmd,
                target=target,
                index=target_index,
                total=len(targets_to_build),
                timeout=args.build_timeout,
                cwd=source,
                record=target_record,
            )
            combined = listed_proc.stdout + "\n" + listed_proc.stderr
            identities = apphost_identities(combined)
            target_record["harness_artifacts"] = identities
            if listed_proc.returncode:
                target_outcome_by_identity[target.identity].update(
                    {
                        "status": "failed",
                        "reason": "cargo dotnet test --list did not complete",
                    }
                )
                terminalize_target_outcomes(
                    target_outcomes,
                    status="failed",
                    reason=(
                        "not attempted because an earlier selected target failed "
                        "during build/list"
                    ),
                )
                report.update({"target_runs": target_runs, "terminal_status": listing_failure_status(combined), "error": f"cargo dotnet test --list did not complete for {target.identity}"})
                write_report(report_path, report)
                return 1
            verify_copied_inputs(source, provenance, toolchain)
            target_record["generated_source_roots"] = generated_root_identities(source)
            names, benchmarks = parse_test_listing(listed_proc.stdout)
            apphost = exact_apphost(identities)
            if (owner := captured_apphosts.get(str(apphost))) is not None:
                raise RuntimeError(f"captured apphost is reused by distinct test targets: {owner}, {target.identity}")
            captured_apphosts[str(apphost)] = target.identity
            target_record["apphost"] = str(apphost)
            target_record["build_receipt"] = build_receipt(apphost, target, target_dir, provenance, profile, toolchain)
            receipt_identity = producer_identity_tuple(target_record["build_receipt"])
            if producer_identity is None:
                producer_identity = receipt_identity
            elif producer_identity != receipt_identity:
                raise RuntimeError("cargo-dotnet producer identity differs between enabled test targets")
            target_record["producer_identity"] = receipt_identity
            target_cfg = target_cfg_from_receipt(target_record["build_receipt"])
            target_record["target_cfg"] = asdict(target_cfg)
            recorded_target_cfg = report.get("compiler_target_cfg")
            if recorded_target_cfg is None:
                report["compiler_target_cfg"] = asdict(target_cfg)
            elif recorded_target_cfg != asdict(target_cfg):
                raise RuntimeError("compiler target cfg differs between selected targets")
            policy = suite_target_policy(suite_spec, target.identity)
            if not names:
                outcome_status, outcome_reason = empty_listing_outcome(
                    suite_spec, target.identity, target_cfg
                )
                target_record.update(
                    {
                        "listed_tests": [],
                        "benchmarks_excluded": [
                            qualified_name(target, name) for name in benchmarks
                        ],
                        "upstream_ignored_tests": [],
                        "terminal_outcome": {
                            "status": outcome_status,
                            "reason": outcome_reason,
                        },
                    }
                )
                target_outcome_by_identity[target.identity].update(
                    {"status": outcome_status, "reason": outcome_reason}
                )
                continue
            if policy.runner_kind == "compile-only":
                raise RuntimeError(
                    f"compile-only target {target.identity} unexpectedly listed tests"
                )
            ignored_command = [str(apphost), "--ignored", "--list"]
            target_record.update({
                "ignored_list_command": ignored_command,
                "ignored_list_working_directory": str(source),
                "ignored_list_state": "pending",
            })
            try:
                ignored_proc = run(
                    ignored_command,
                    timeout=args.build_timeout,
                    cwd=source,
                    env=apphost_env,
                )
            except subprocess.TimeoutExpired as error:
                target_record.update({"ignored_list_state": "timeout", "ignored_list_stdout": decode_timeout_value(error.stdout), "ignored_list_stderr": decode_timeout_value(error.stderr)})
                raise
            target_record.update({"ignored_list_state": "completed", "ignored_list_returncode": ignored_proc.returncode, "ignored_list_stdout": ignored_proc.stdout, "ignored_list_stderr": ignored_proc.stderr})
            if ignored_proc.returncode:
                raise RuntimeError(f"could not list upstream-ignored tests for {target.identity}")
            qualified = [qualified_name(target, name) for name in names]
            all_tests.extend(qualified)
            ignored_names, ignored_benchmarks = parse_test_listing(ignored_proc.stdout)
            if not set(ignored_names).issubset(names):
                raise RuntimeError(f"ignored listing contains tests absent from full listing for {target.identity}")
            if ignored_benchmarks:
                raise RuntimeError(f"ignored libtest listing unexpectedly included benchmarks for {target.identity}")
            upstream_ignored.update(qualified_name(target, name) for name in ignored_names)
            target_record["listed_tests"] = qualified
            target_record["benchmarks_excluded"] = [qualified_name(target, name) for name in benchmarks]
            target_record["upstream_ignored_tests"] = sorted(upstream_ignored.intersection(qualified))
            target_record["terminal_outcome"] = {
                "status": "listed",
                "reason": f"listed {len(qualified)} libtest cases",
            }
            target_outcome_by_identity[target.identity].update(
                {
                    "status": "listed",
                    "reason": f"listed {len(qualified)} libtest cases",
                }
            )
        cacheable_target_runs = all(
            (
                bool(target_record.get("listed_tests"))
                and target_record.get("terminal_outcome", {}).get("status") == "listed"
            )
            or (
                not target_record.get("listed_tests")
                and target_record.get("terminal_outcome", {}).get("status")
                in {"passed", "not-applicable"}
            )
            for target_record in target_runs
        )
        if targets_to_build and artifact_manifest is not None and cacheable_target_runs:
            previous_cache_state = report.get("suite_artifact_cache")
            write_suite_artifact_manifest(
                artifact_manifest, cache_material, target_dir, target_runs
            )
            report["suite_artifact_cache"] = {
                "state": "published",
                "path": str(artifact_manifest),
                "sha256": sha256_file(artifact_manifest),
            }
            if (
                isinstance(previous_cache_state, dict)
                and previous_cache_state.get("state") == "invalid-miss"
            ):
                report["suite_artifact_cache"]["replaced_invalid_cache"] = (
                    previous_cache_state
                )
        if repository_identity() != report["repository"]:
            raise RuntimeError("repository identity changed during product build/list; refusing to score mixed inputs")
        if producer_source_sha256() != producer_source:
            raise RuntimeError("compiler producer sources changed during product build/list")
        if command_identity(cargo_dotnet) != cargo_dotnet_identity:
            raise RuntimeError("cargo-dotnet producer artifacts changed during product build/list")
        unknown_names = sorted(set(args.test_name) - set(all_tests))
        if unknown_names:
            raise RuntimeError(f"requested exact tests were not listed: {', '.join(unknown_names)}")
        selected = [name for name in all_tests if (not args.filter or args.filter in name) and (not args.test_name or name in args.test_name)]
        if args.test_name and set(selected) != set(args.test_name):
            raise RuntimeError("requested exact test set did not exactly match the selected set")
        if not selected:
            terminal_target_statuses = {
                outcome["status"] for outcome in target_outcomes
            }
            if terminal_target_statuses.issubset(
                {"passed", "not-applicable", "unsupported"}
            ):
                report.update(
                    {
                        "terminal_status": "complete",
                        "listed_tests": sorted(all_tests),
                        "selected_tests": [],
                        "results": [],
                        "counts": counts([]),
                        "target_counts": target_outcome_counts(target_outcomes),
                        "duration_seconds": time.monotonic() - started,
                        "terminal_completion": {
                            "selected": 0,
                            "recorded_results": 0,
                            "complete": True,
                        },
                    }
                )
                write_report(report_path, report)
                print(f"Report: {report_path}")
                return 0
            raise RuntimeError(
                "no tests matched the requested --target/--filter/--test-name selection"
            )
        unselected_skips = sorted(set(policy_skips) - set(selected))
        if unselected_skips:
            raise RuntimeError(f"backend-policy skips are outside the selected test set: {', '.join(unselected_skips)}")
        overlapping_skips = sorted(set(policy_skips).intersection(upstream_ignored))
        if overlapping_skips:
            raise RuntimeError(f"backend-policy skips overlap upstream ignored tests: {', '.join(overlapping_skips)}")
        report.update({"target_runs": target_runs, "listed_tests": sorted(all_tests), "selected_tests": selected, "upstream_ignored_tests": sorted(upstream_ignored)})
        report["benchmarks_excluded"] = sorted(
            benchmark for target_record in target_runs for benchmark in target_record["benchmarks_excluded"]
        )
        if args.list:
            if producer_source_sha256() != producer_source or command_identity(cargo_dotnet) != cargo_dotnet_identity:
                raise RuntimeError("compiler producer changed before list-only receipt completion")
            report["terminal_status"] = "listed"
            report["target_counts"] = target_outcome_counts(target_outcomes)
            report["duration_seconds"] = time.monotonic() - started
            write_report(report_path, report)
            print("\n".join(selected))
            print(f"Report: {report_path}")
            return 0
        apphosts = {
            entry["target"]["identity"]: Path(entry["apphost"])
            for entry in target_runs
            if "apphost" in entry
        }
        results_by_name: dict[str, TestResult] = {}
        for name in selected:
            if name in upstream_ignored:
                results_by_name[name] = TestResult(
                    name, "upstream-ignored", 0.0, None, "", "", []
                )
            elif name in policy_skips:
                results_by_name[name] = TestResult(
                    name,
                    "backend-policy-skip",
                    0.0,
                    None,
                    "",
                    policy_skips[name],
                    [],
                )
        report["results"] = []

        if not args.exact_execution:
            for target_record in target_runs:
                target_name = target_record["target"]["identity"]
                executable_names = sorted(
                    name
                    for name in selected
                    if name.startswith(f"{target_name}::")
                    and name not in results_by_name
                )
                if not executable_names:
                    continue
                apphost = apphosts[target_name]
                verify_apphost_identity(apphost, target_record["harness_artifacts"])
                target_case_count = len(target_record["listed_tests"]) + len(
                    target_record["benchmarks_excluded"]
                )
                inner_names = [name.split("::", 1)[1] for name in executable_names]
                batches = exact_name_batches(
                    inner_names,
                    args.batch_size,
                    max_bytes=batch_argument_limit,
                )
                attempts: list[dict[str, object]] = []
                target_record["batch_attempts"] = attempts
                adaptive_started = time.monotonic()
                adaptive_deadline = adaptive_started + args.adaptive_recovery_timeout
                adaptive_attempts = 0
                target_record["adaptive_budget"] = {
                    "attempt_limit": args.adaptive_attempt_limit,
                    "timeout_seconds": args.adaptive_recovery_timeout,
                    "state": "running",
                }

                def claim_adaptive_attempt() -> float | None:
                    nonlocal adaptive_attempts
                    if adaptive_attempts >= args.adaptive_attempt_limit:
                        target_record["adaptive_budget"].update(
                            {
                                "state": "attempt-limit-exhausted",
                                "attempts_used": adaptive_attempts,
                                "duration_seconds": time.monotonic()
                                - adaptive_started,
                            }
                        )
                        return None
                    remaining = adaptive_deadline - time.monotonic()
                    if remaining <= 0:
                        target_record["adaptive_budget"].update(
                            {
                                "state": "timeout-exhausted",
                                "attempts_used": adaptive_attempts,
                                "duration_seconds": time.monotonic()
                                - adaptive_started,
                            }
                        )
                        return None
                    adaptive_attempts += 1
                    return remaining

                queue: list[tuple[list[str], int | None, int]] = [
                    (batch, None, 0) for batch in reversed(batches)
                ]
                while queue:
                    remaining_recovery = claim_adaptive_attempt()
                    if remaining_recovery is None:
                        break
                    batch_inner, parent_attempt_id, split_depth = queue.pop()
                    attempt_id = len(attempts) + 1
                    attempt_timeout = min(args.batch_timeout, remaining_recovery)
                    expected_inner = set(batch_inner)
                    batch_command = [
                        str(apphost),
                        "-Z",
                        "unstable-options",
                        "--format",
                        "json",
                        "--report-time",
                        "--test",
                        "--exact",
                        *batch_inner,
                    ]
                    batch_attempt: dict[str, object] = {
                        "attempt_id": attempt_id,
                        "parent_attempt_id": parent_attempt_id,
                        "split_depth": split_depth,
                        "initial_batch_count": len(batches),
                        "requested_tests": batch_inner,
                        "command": batch_command,
                        "working_directory": str(source),
                        "timeout_seconds": attempt_timeout,
                        "state": "pending",
                    }
                    attempts.append(batch_attempt)
                    batch_started = time.monotonic()
                    timed_out = False
                    try:
                        batch_proc = run(
                            batch_command,
                            timeout=attempt_timeout,
                            cwd=source,
                            env=apphost_env,
                        )
                        batch_stdout = batch_proc.stdout
                        batch_stderr = batch_proc.stderr
                        batch_returncode: int | None = batch_proc.returncode
                    except subprocess.TimeoutExpired as error:
                        timed_out = True
                        batch_stdout = decode_timeout_value(error.stdout)
                        batch_stderr = decode_timeout_value(error.stderr)
                        batch_returncode = None
                    batch_attempt.update(
                        {
                            "stdout": batch_stdout,
                            "stderr": batch_stderr,
                            "returncode": batch_returncode,
                            "duration_seconds": time.monotonic() - batch_started,
                        }
                    )
                    parse_error: RuntimeError | None = None
                    try:
                        batch_results, batch_complete, suite_terminal = parse_batch_json(
                            batch_stdout,
                            expected_inner,
                            set(),
                            batch_command,
                            expected_filtered=target_case_count - len(expected_inner),
                        )
                    except RuntimeError as error:
                        parse_error = error
                        batch_results = {}
                        batch_complete = False
                        suite_terminal = None

                    trusted_partial = parse_error is None
                    split_reason: str | None = None
                    if batch_complete:
                        returncode_matches = (
                            suite_terminal["event"] == "ok" and batch_returncode == 0
                        ) or (
                            suite_terminal["event"] == "failed"
                            and batch_returncode not in {None, 0}
                        )
                        if timed_out or not returncode_matches:
                            trusted_partial = False
                            batch_results = {}
                            split_reason = "protocol"
                            batch_attempt["error"] = (
                                "batch process return code does not match its terminal suite event"
                            )
                        else:
                            batch_state = "completed"
                    elif parse_error is not None:
                        split_reason = "protocol"
                        batch_attempt["error"] = str(parse_error)
                    elif timed_out:
                        batch_state = "timeout-split"
                        split_reason = "timeout"
                    elif batch_returncode == 0:
                        trusted_partial = False
                        batch_results = {}
                        split_reason = "protocol"
                        batch_attempt["error"] = (
                            "batch exited successfully without a terminal suite event"
                        )
                    else:
                        batch_state = "crashed-split"
                        split_reason = "crash"

                    resolved: list[str] = []
                    if trusted_partial:
                        for inner_name, result in batch_results.items():
                            qualified = f"{target_name}::{inner_name}"
                            if qualified in results_by_name:
                                raise RuntimeError(
                                    f"adaptive batch resolved test twice: {qualified}"
                                )
                            results_by_name[qualified] = TestResult(
                                qualified,
                                result.status,
                                result.duration_seconds,
                                result.returncode,
                                result.stdout,
                                result.stderr,
                                [],
                                result.terminal_summary,
                                attempt_id,
                            )
                            resolved.append(qualified)

                    unresolved_inner = [
                        inner_name
                        for inner_name in batch_inner
                        if f"{target_name}::{inner_name}" not in results_by_name
                    ]
                    recovery = "completed"
                    fallback_inner: str | None = None
                    if unresolved_inner:
                        children = bisect_exact_name_batch(unresolved_inner)
                        if children:
                            recovery = "split"
                            queue.extend(
                                (child, attempt_id, split_depth + 1)
                                for child in reversed(children)
                            )
                        else:
                            recovery = "exact-fallback"
                            fallback_inner = unresolved_inner[0]
                        if split_reason is None:
                            split_reason = "protocol"
                        batch_state = f"{split_reason}-{recovery}"
                    batch_attempt.update(
                        {
                            "state": batch_state,
                            "complete_protocol": batch_complete,
                            "suite_terminal": suite_terminal,
                            "resolved_tests": sorted(resolved),
                            "unresolved_tests": unresolved_inner,
                            "recovery": recovery,
                            "split_reason": split_reason,
                        }
                    )
                    report["results"] = [
                        asdict(results_by_name[selected_name])
                        for selected_name in selected
                        if selected_name in results_by_name
                    ]
                    print(
                        f"batch {target_name} attempt {attempt_id}: {batch_state}; "
                        f"resolved {len(resolved)}/{len(expected_inner)}, "
                        f"unresolved {len(unresolved_inner)}",
                        flush=True,
                    )

                    if fallback_inner is not None:
                        remaining_recovery = claim_adaptive_attempt()
                        if remaining_recovery is None:
                            break
                        exact_timeout, recovery_limited = recovery_exact_timeout(
                            args.timeout, remaining_recovery
                        )
                        qualified = f"{target_name}::{fallback_inner}"
                        exact_attempt: dict[str, object] = {
                            "name": qualified,
                            "adaptive_parent_attempt_id": attempt_id,
                            "command": [
                                str(apphost),
                                "--exact",
                                fallback_inner,
                                "--nocapture",
                            ],
                            "working_directory": str(source),
                            "timeout_seconds": exact_timeout,
                            "timeout_limited_by_recovery": recovery_limited,
                            "state": "pending",
                        }
                        report.setdefault("exact_attempts", []).append(exact_attempt)
                        result = run_recorded_exact(
                            exact_attempt,
                            exact_timeout,
                            apphost_env,
                            source,
                        )
                        if result.status == "timeout" and recovery_limited:
                            exact_attempt["state"] = "adaptive-recovery-timeout"
                            target_record["adaptive_budget"].update(
                                {
                                    "state": "timeout-exhausted",
                                    "attempts_used": adaptive_attempts,
                                    "duration_seconds": time.monotonic()
                                    - adaptive_started,
                                }
                            )
                            terminalize_unresolved_tests(
                                executable_names,
                                results_by_name,
                                status="timeout",
                                reason=(
                                    "adaptive recovery deadline exhausted during "
                                    f"exact fallback for target {target_name}"
                                ),
                            )
                            break
                        results_by_name[qualified] = TestResult(
                            qualified,
                            result.status,
                            result.duration_seconds,
                            result.returncode,
                            result.stdout,
                            result.stderr,
                            result.command,
                            result.terminal_summary,
                            attempt_id,
                        )
                        report["results"] = [
                            asdict(results_by_name[selected_name])
                            for selected_name in selected
                            if selected_name in results_by_name
                        ]
                        print(
                            f"exact fallback {result.status:16} {qualified}",
                            flush=True,
                        )

                budget_state = target_record["adaptive_budget"]["state"]
                if budget_state == "attempt-limit-exhausted":
                    terminalize_unresolved_tests(
                        executable_names,
                        results_by_name,
                        status="failed",
                        reason=(
                            "adaptive attempt limit exhausted before a terminal "
                            f"result for target {target_name}"
                        ),
                    )
                elif budget_state == "timeout-exhausted":
                    terminalize_unresolved_tests(
                        executable_names,
                        results_by_name,
                        status="timeout",
                        reason=(
                            "adaptive recovery deadline exhausted before a terminal "
                            f"result for target {target_name}"
                        ),
                    )
                else:
                    target_record["adaptive_budget"].update(
                        {
                            "state": "complete",
                            "attempts_used": adaptive_attempts,
                            "duration_seconds": time.monotonic() - adaptive_started,
                        }
                    )

            unresolved = [name for name in selected if name not in results_by_name]
            if unresolved:
                terminalize_unresolved_tests(
                    unresolved,
                    results_by_name,
                    status="failed",
                    reason="adaptive execution ended without a terminal protocol result",
                )

        if args.exact_execution:
            for index, name in enumerate(selected, 1):
                if name in results_by_name:
                    continue
                target_name, test_name = name.split("::", 1)
                target_record = next(
                    entry
                    for entry in target_runs
                    if entry["target"]["identity"] == target_name
                )
                apphost = apphosts[target_name]
                verify_apphost_identity(
                    apphost, target_record["harness_artifacts"]
                )
                attempt: dict[str, object] = {
                    "name": name,
                    "command": [str(apphost), "--exact", test_name, "--nocapture"],
                    "working_directory": str(source),
                    "timeout_seconds": args.timeout,
                    "state": "pending",
                }
                report.setdefault("exact_attempts", []).append(attempt)
                report["results"] = [
                    asdict(results_by_name[selected_name])
                    for selected_name in selected
                    if selected_name in results_by_name
                ]
                result = run_recorded_exact(
                    attempt, args.timeout, apphost_env, source
                )
                results_by_name[name] = TestResult(
                    name,
                    result.status,
                    result.duration_seconds,
                    result.returncode,
                    result.stdout,
                    result.stderr,
                    result.command,
                    result.terminal_summary,
                )
                report["results"] = [
                    asdict(results_by_name[selected_name])
                    for selected_name in selected
                    if selected_name in results_by_name
                ]
                print(
                    f"{index}/{len(selected)} {result.status:16} {name}",
                    flush=True,
                )
        if set(results_by_name) != set(selected):
            raise RuntimeError("execution did not produce exactly one result for every selected test")
        results = [results_by_name[name] for name in selected]
        finalize_target_outcomes(target_outcomes, results)
        for target_record in target_runs:
            identity = target_record["target"]["identity"]
            outcome = target_outcome_by_identity.get(identity)
            if outcome is not None:
                target_record["terminal_outcome"] = {
                    "status": outcome["status"],
                    "reason": outcome["reason"],
                }
        if producer_source_sha256() != producer_source:
            raise RuntimeError("compiler producer sources changed during test execution")
        if command_identity(cargo_dotnet) != cargo_dotnet_identity:
            raise RuntimeError("cargo-dotnet producer artifacts changed during test execution")
        report.update({
            "terminal_status": "complete",
            "results": [asdict(result) for result in results],
            "counts": counts(results),
            "target_counts": target_outcome_counts(target_outcomes),
            "duration_seconds": time.monotonic() - started,
            "terminal_completion": {"selected": len(selected), "recorded_results": len(results), "complete": len(selected) == len(results)},
        })
        full_run = not args.filter and not args.test_name and not args.target
        write_report(report_path, report, canonical_path if full_run else None)
        print(f"Report: {report_path}")
        return 0 if all(result.status in {"passed", "unsupported", "not-applicable", "upstream-ignored", "backend-policy-skip"} for result in results) else 1
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        if "target_outcomes" in locals():
            terminalize_target_outcomes(
                target_outcomes,
                status=(
                    "timeout"
                    if isinstance(error, subprocess.TimeoutExpired)
                    else "failed"
                ),
                reason=f"suite aborted before target completion: {error}",
                replace_statuses=frozenset({"pending", "listed"}),
            )
            report["target_outcomes"] = target_outcomes
            report["target_counts"] = target_outcome_counts(target_outcomes)
        report.update({
            "terminal_status": "infrastructure-failed",
            "error": str(error),
            "duration_seconds": time.monotonic() - started,
        })
        write_report(report_path, report)
        print(f"Report: {report_path}", file=sys.stderr)
        return 1
    finally:
        if cache_lease is not None:
            cache_lease.close()


if __name__ == "__main__":
    raise SystemExit(main())
