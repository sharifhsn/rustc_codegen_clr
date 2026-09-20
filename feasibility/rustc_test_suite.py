#!/usr/bin/env python3
"""Run a source-bounded allowlist from Rust's compiletest corpus.

This is intentionally smaller than compiletest.  It selects exact, pinned source paths, stages
each one as an ordinary Cargo binary, runs a native Rust oracle, then runs the same source through
``cargo dotnet`` and compares the upstream run-pass/run-fail contract.  The allowlist is the
denominator; this script never scans the Rust repository and never turns a missing or unsupported
case into a pass.  Backend-neutral run-make/incremental/rustdoc/MIR entries are listed separately
and remain explicit ``unsupported`` until their specialised harnesses are ported.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import time
import uuid
from collections import Counter
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_RUST_ROOT = ROOT / "rust"
DEFAULT_OUT = ROOT / "target" / "rustc-tests"
TOOLCHAIN = "nightly-2026-09-18"
TARGET = "x86_64-unknown-dotnet"
MAX_OUTPUT = 32_768


@dataclass(frozen=True)
class RustcCase:
    """One immutable source path in the managed-test denominator."""

    case_id: str
    path: Path
    category: str
    expectation: str
    mode: str = "managed"


# These are deliberately boring, observable cases: no LLVM/FileCheck, debugger, target-specific
# ABI, auxiliary crates, or Miri contract is smuggled into the managed denominator.
MANAGED_CASES = (
    RustcCase(
        "ui-struct-functional-update",
        Path("tests/ui/structs/struct-lit-functional-no-fields.rs"),
        "ui-run-pass",
        "run-pass",
    ),
    RustcCase(
        "ui-closure-capture-call",
        Path("tests/ui/closures/simple-capture-and-call.rs"),
        "ui-run-pass",
        "run-pass",
    ),
    RustcCase(
        "ui-dyn-trait-format",
        Path("tests/ui/traits/dyn-trait.rs"),
        "ui-run-pass",
        "run-pass",
    ),
    RustcCase(
        "ui-fs-nul-byte-paths",
        Path("tests/ui/std/fs-nul-byte-paths.rs"),
        "ui-run-pass",
        "run-pass",
    ),
    RustcCase(
        "ui-thread-sleep-ms",
        Path("tests/ui/std/thread-sleep-ms.rs"),
        "ui-run-pass",
        "run-pass",
    ),
    RustcCase(
        "ui-explicit-panic",
        Path("tests/ui/match/expr-match-panic.rs"),
        "ui-run-fail",
        "run-fail",
    ),
    RustcCase(
        "ui-explicit-panic-message",
        Path("tests/ui/panics/explicit-panic-msg.rs"),
        "ui-run-fail",
        "run-fail",
    ),
)

# These entries make the next expansion explicit without pretending that a compiletest helper
# crate, rustdoc's doctest driver, or MIR snapshot is an ordinary managed executable.
BACKEND_NEUTRAL_CASES = (
    RustcCase(
        "run-make-exit-code-success",
        Path("tests/run-make/exit-code/success.rs"),
        "run-make",
        "compile-only",
        "backend-neutral",
    ),
    RustcCase(
        "incremental-issue-60629",
        Path("tests/incremental/issue-60629.rs"),
        "incremental",
        "compile-only",
        "backend-neutral",
    ),
    RustcCase(
        "rustdoc-doctest-manual-crate-name",
        Path("tests/rustdoc-html/doctest/doctest-manual-crate-name.rs"),
        "doctest",
        "compile-only",
        "backend-neutral",
    ),
    RustcCase(
        "mir-fn-pointer-shim",
        Path("tests/mir-opt/fn_ptr_shim.rs"),
        "mir-shape",
        "compile-only",
        "backend-neutral",
    ),
)


@dataclass(frozen=True)
class Directives:
    keys: frozenset[str]
    values: dict[str, str]


@dataclass(frozen=True)
class Completed:
    returncode: int | None
    duration_seconds: float
    stdout: str
    stderr: str
    timed_out: bool = False


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def git_output(rust_root: Path, *args: str) -> str | None:
    try:
        result = subprocess.run(
            ["git", "-C", str(rust_root), *args],
            check=True,
            capture_output=True,
            text=True,
            timeout=15,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout.strip() or None


def parse_directives(source: str) -> Directives:
    keys: set[str] = set()
    values: dict[str, str] = {}
    # Compiletest directives are front matter in current Rust tests.  Reading a bounded prefix
    # avoids treating a string literal containing "//@" as policy.
    for line in source.splitlines()[:160]:
        match = re.match(r"^\s*//@\s*([A-Za-z0-9_-]+)(?::\s*|\s+)?(.*)$", line)
        if not match:
            continue
        key, value = match.groups()
        keys.add(key)
        if value:
            values.setdefault(key, value.strip())
    return Directives(frozenset(keys), values)


def normalise_output(value: str, *roots: Path) -> str:
    result = value.replace("\\", "/")
    for root in roots:
        result = result.replace(str(root).replace("\\", "/"), "<staged>")
    return result


def error_pattern(directives: Directives) -> tuple[str | None, bool]:
    """Return (pattern, is_regex) for the compiletest failure oracle."""
    if "error-pattern" in directives.values:
        return directives.values["error-pattern"], False
    if "regex-error-pattern" in directives.values:
        return directives.values["regex-error-pattern"], True
    return None, False


def terminate_process(process: subprocess.Popen[str]) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except (ProcessLookupError, OSError):
        return
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except (ProcessLookupError, OSError):
            pass


def run_command(command: list[str], *, cwd: Path, env: dict[str, str], timeout: float) -> Completed:
    started = time.monotonic()
    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
    except OSError as error:
        return Completed(None, time.monotonic() - started, "", str(error))
    try:
        stdout, stderr = process.communicate(timeout=timeout)
        return Completed(
            process.returncode,
            time.monotonic() - started,
            stdout[-MAX_OUTPUT:],
            stderr[-MAX_OUTPUT:],
        )
    except subprocess.TimeoutExpired as error:
        terminate_process(process)
        stdout, stderr = process.communicate()
        return Completed(
            process.returncode,
            time.monotonic() - started,
            (error.stdout or "")[-MAX_OUTPUT:] + stdout[-MAX_OUTPUT:],
            (error.stderr or "")[-MAX_OUTPUT:] + stderr[-MAX_OUTPUT:],
            True,
        )


def source_case(rust_root: Path, case: RustcCase) -> Path:
    path = (rust_root / case.path).resolve()
    path.relative_to(rust_root.resolve())
    return path


def case_preflight(rust_root: Path, case: RustcCase) -> tuple[Path | None, Directives, str | None]:
    try:
        path = source_case(rust_root, case)
    except (OSError, ValueError):
        return None, Directives(frozenset(), {}), "allowlisted source escapes rust root"
    if not path.is_file():
        return None, Directives(frozenset(), {}), "allowlisted source is missing"
    directives = parse_directives(path.read_text(encoding="utf-8"))
    if case.mode == "managed":
        if case.expectation not in directives.keys:
            return path, directives, f"source no longer carries //@ {case.expectation}"
        if "aux-build" in directives.keys or "aux-crate" in directives.keys:
            return path, directives, "auxiliary-crate compiletest cases require a specialised runner"
        if "ignore-cross-compile" in directives.keys:
            return path, directives, "upstream explicitly ignores cross compilation"
        if "compile-flags" in directives.values:
            try:
                requested = shlex.split(directives.values["compile-flags"])
            except ValueError as error:
                return path, directives, f"invalid compile-flags directive: {error}"
            unsupported = [flag for flag in requested if flag not in {"--test"}]
            if unsupported:
                return path, directives, (
                    "compile-flags are outside the ordinary executable runner: "
                    + " ".join(unsupported)
                )
    return path, directives, None


def stage_case(source: Path, case: RustcCase, root: Path, edition: str) -> Path:
    crate = root / case.case_id
    (crate / "src").mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, crate / "src" / "main.rs")
    # Keep the small module closure available without copying the entire Rust checkout.
    text = source.read_text(encoding="utf-8")
    for module in re.findall(r"\bmod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", text):
        for candidate in (source.parent / f"{module}.rs", source.parent / module / "mod.rs"):
            if candidate.is_file():
                destination = crate / "src" / candidate.relative_to(source.parent)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(candidate, destination)
                break
    (crate / "Cargo.toml").write_text(
        "[package]\n"
        f'name = "rcl_{case.case_id.replace("-", "_")}"\n'
        'version = "0.0.0"\n'
        f'edition = "{edition}"\n'
        "\n[workspace]\n",
        encoding="utf-8",
    )
    return crate


def artifact_identity(path: Path) -> dict[str, object] | None:
    if not path.is_file():
        return None
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": sha256_file(path)}


def tree_sha256(path: Path) -> str:
    """Hash a small source tree with stable relative names and file contents."""
    digest = hashlib.sha256()
    if path.is_file():
        digest.update(path.name.encode("utf-8"))
        digest.update(sha256_file(path).encode("ascii"))
        return digest.hexdigest()
    for child in sorted((item for item in path.rglob("*") if item.is_file())):
        digest.update(child.relative_to(path).as_posix().encode("utf-8"))
        digest.update(b"\0")
        digest.update(sha256_file(child).encode("ascii"))
        digest.update(b"\0")
    return digest.hexdigest()


def managed_target_key(
    *, cargo_dotnet: Path, backend: Path, linker: Path, target_spec: Path
) -> str:
    """Choose a reusable target directory for exactly these managed build inputs.

    cargo-dotnet deliberately refuses to clean/reuse an explicitly routed target when the
    private-sysroot marker belongs to an older source closure.  Including the backend artifacts
    and the PAL source tree in the directory identity makes source changes select a fresh target,
    preserving fast warm runs while avoiding a stale-artifact or unsafe-clean fallback.
    """
    digest = hashlib.sha256()
    for label, path in (
        ("cargo-dotnet", cargo_dotnet),
        ("backend", backend),
        ("linker", linker),
        ("target-spec", target_spec),
        ("dotnet-pal", ROOT / "dotnet_pal"),
    ):
        digest.update(label.encode("utf-8"))
        digest.update(b"\0")
        digest.update(tree_sha256(path).encode("ascii"))
        digest.update(b"\0")
    return digest.hexdigest()[:20]


def expected_success(case: RustcCase) -> bool:
    return case.expectation == "run-pass"


def run_managed_case(
    *,
    case: RustcCase,
    source: Path,
    directives: Directives,
    staging: Path,
    managed_target: Path,
    native_target: Path,
    cargo_dotnet: Path,
    timeout: float,
    base_env: dict[str, str],
) -> dict[str, object]:
    edition = directives.values.get("edition", "2021").strip()
    # Compiletest's `2015..2021` means a family of edition revisions.  A standalone Cargo
    # staging crate can exercise one representative; choose the oldest edition so legacy
    # syntax remains source-compatible and the result is still bound to the exact source hash.
    if ".." in edition:
        edition = edition.split("..", 1)[0].strip()
    if edition not in {"2015", "2018", "2021", "2024"}:
        edition = "2021"
    crate = stage_case(source, case, staging, edition)
    tmpdir = crate / "tmp"
    tmpdir.mkdir(exist_ok=True)
    env = dict(base_env)
    env["RUST_TEST_TMPDIR"] = str(tmpdir)
    native_command = [
        "rustup",
        "run",
        TOOLCHAIN,
        "cargo",
        "run",
        "--manifest-path",
        str(crate / "Cargo.toml"),
        "--release",
        "--quiet",
        "--target-dir",
        str(native_target),
    ]
    managed_command = [
        str(cargo_dotnet),
        "run",
        str(crate),
        "--release",
        "--backend",
        "native",
        "--dotnet",
        "10",
        "--target-dir",
        str(managed_target),
    ]
    native = run_command(native_command, cwd=crate, env=env, timeout=timeout)
    if native.timed_out:
        return {
            "status": "timeout",
            "reason": "native Rust oracle exceeded the bounded case timeout",
            "native": asdict(native),
            "command": managed_command,
        }
    managed = run_command(managed_command, cwd=crate, env=env, timeout=timeout)
    if managed.timed_out:
        return {
            "status": "timeout",
            "reason": "managed case exceeded the bounded case timeout",
            "native": asdict(native),
            "managed": asdict(managed),
            "command": managed_command,
        }
    should_pass = expected_success(case)
    native_matches = (native.returncode == 0) == should_pass
    managed_matches = (managed.returncode == 0) == should_pass
    pattern, is_regex = error_pattern(directives)
    pattern_matches = True
    if pattern and not should_pass:
        def contains_pattern(value: str) -> bool:
            if is_regex:
                try:
                    return re.search(pattern, value) is not None
                except re.error:
                    return False
            return pattern in value

        native_matches = native_matches and contains_pattern(native.stdout + native.stderr)
        pattern_matches = contains_pattern(managed.stdout + managed.stderr)
        managed_matches = managed_matches and pattern_matches
    stdout_matches = True
    if should_pass and native_matches and managed_matches:
        stdout_matches = normalise_output(native.stdout, crate) == normalise_output(
            managed.stdout, crate
        )
    status = "passed" if native_matches and managed_matches and stdout_matches else "failed"
    reason = None
    if not native_matches:
        reason = "native Rust oracle did not satisfy the upstream run-pass/run-fail contract"
    elif not managed_matches:
        reason = "managed result did not satisfy the upstream run-pass/run-fail contract"
    elif not stdout_matches:
        reason = "run-pass stdout differs from the native Rust oracle"
    return {
        "status": status,
        "reason": reason,
        "native": asdict(native),
        "managed": asdict(managed),
        "command": managed_command,
        "error_pattern": pattern,
        "error_pattern_found": pattern_matches,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust-root", type=Path, default=DEFAULT_RUST_ROOT)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--list", action="store_true", help="print the exact allowlist and exit")
    parser.add_argument("--include-backend-neutral", action="store_true")
    parser.add_argument("--case", action="append", dest="case_ids")
    parser.add_argument("--timeout", type=float, default=180.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    rust_root = args.rust_root.resolve()
    cases = list(MANAGED_CASES)
    if args.include_backend_neutral:
        cases.extend(BACKEND_NEUTRAL_CASES)
    if args.case_ids:
        requested = set(args.case_ids)
        unknown = requested - {case.case_id for case in cases}
        if unknown:
            raise SystemExit(f"unknown allowlisted case(s): {', '.join(sorted(unknown))}")
        cases = [case for case in cases if case.case_id in requested]
    listing = []
    preflight: dict[str, tuple[Path | None, Directives, str | None]] = {}
    for case in cases:
        path, directives, reason = case_preflight(rust_root, case)
        preflight[case.case_id] = (path, directives, reason)
        listing.append(
            {
                "case_id": case.case_id,
                "path": case.path.as_posix(),
                "category": case.category,
                "expectation": case.expectation,
                "mode": case.mode,
                "source_present": path is not None,
                "preflight_reason": reason,
            }
        )
    if args.list:
        print(json.dumps({"toolchain": TOOLCHAIN, "target": TARGET, "cases": listing}, indent=2))
        return 0

    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    run_id = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ") + "-" + uuid.uuid4().hex[:12]
    cargo_dotnet = ROOT / "target" / "release" / "cargo-dotnet"
    if os.name == "nt":
        backend = ROOT / "target" / "release" / "rustc_codegen_clr.dll"
        linker = ROOT / "target" / "release" / "linker.exe"
    elif os.uname().sysname == "Darwin":
        backend = ROOT / "target" / "release" / "librustc_codegen_clr.dylib"
        linker = ROOT / "target" / "release" / "linker"
    else:
        backend = ROOT / "target" / "release" / "librustc_codegen_clr.so"
        linker = ROOT / "target" / "release" / "linker"
    target_spec = ROOT / f"{TARGET}.json"
    base_env = dict(os.environ)
    base_env.update(
        {
            "CARGO_TERM_COLOR": "never",
            "CARGO_NET_GIT_FETCH_WITH_CLI": "true",
            "DOTNET_CLI_TELEMETRY_OPTOUT": "1",
            "DOTNET_NOLOGO": "1",
            "CARGO_DOTNET_BACKEND": "native",
        }
    )
    results: list[dict[str, object]] = []
    # Keep the generated crate paths and target directories stable between invocations.  Cargo and
    # cargo-dotnet can then reuse their content-addressed release artifacts; source hashes and the
    # backend/linker identities still force a rebuild whenever semantics change.
    workspace = args.out / "_workspace"
    managed_target = args.out / (
        "_managed-target-"
        + managed_target_key(
            cargo_dotnet=cargo_dotnet,
            backend=backend,
            linker=linker,
            target_spec=target_spec,
        )
    )
    native_target = args.out / "_native-target"
    workspace.mkdir(parents=True, exist_ok=True)
    for case in cases:
        path, directives, reason = preflight[case.case_id]
        started = time.monotonic()
        result: dict[str, object] = {
            "case_id": case.case_id,
            "path": case.path.as_posix(),
            "category": case.category,
            "expectation": case.expectation,
            "mode": case.mode,
            "source_sha256": sha256_file(path) if path is not None else None,
        }
        if reason:
            result.update({"status": "unsupported", "reason": reason})
        elif case.mode != "managed":
            result.update(
                {
                    "status": "unsupported",
                    "reason": (
                        "selected backend-neutral case needs its specialised compiletest "
                        "runner; it is not counted as managed compatibility"
                    ),
                }
            )
        elif not cargo_dotnet.is_file() or not backend.is_file() or not linker.is_file():
            result.update(
                {
                    "status": "unsupported",
                    "reason": "release cargo-dotnet/backend/linker artifacts are not built",
                }
            )
        else:
            result.update(
                run_managed_case(
                    case=case,
                    source=path,
                    directives=directives,
                    staging=workspace,
                    managed_target=managed_target,
                    native_target=native_target,
                    cargo_dotnet=cargo_dotnet,
                    timeout=args.timeout,
                    base_env=base_env,
                )
            )
        result["duration_seconds"] = time.monotonic() - started
        results.append(result)

    counts = Counter(result["status"] for result in results)
    rust_version = run_command(
        ["rustup", "run", TOOLCHAIN, "rustc", "-vV"],
        cwd=ROOT,
        env=base_env,
        timeout=30,
    )
    receipt = {
        "schema_version": 1,
        "suite": "rustc-allowlist",
        "run_id": run_id,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "terminal_status": "complete",
        "counts": dict(sorted(counts.items())),
        "selection": {
            "cases": listing,
            "source_root": str(rust_root),
            "source_commit": git_output(rust_root, "rev-parse", "HEAD"),
            "source_dirty": git_output(rust_root, "status", "--porcelain") not in (None, ""),
            "execution": "native-differential-then-managed-cargo-dotnet",
            "timeout_seconds": args.timeout,
        },
        "toolchain": TOOLCHAIN,
        "rustc_version": rust_version.stdout.strip() if rust_version.returncode == 0 else None,
        "target": {
            "triple": TARGET,
            "spec": artifact_identity(target_spec),
        },
        "artifacts": {
            "cargo_dotnet": artifact_identity(cargo_dotnet),
            "backend": artifact_identity(backend),
            "linker": artifact_identity(linker),
            "runner": {"path": str(Path(__file__).resolve()), "sha256": sha256_file(Path(__file__))},
        },
        "cases": results,
    }
    output = args.out / f"{run_id}.json"
    output.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"receipt": str(output), "counts": dict(sorted(counts.items()))}, indent=2))
    return 0 if counts.get("failed", 0) == 0 and counts.get("timeout", 0) == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
