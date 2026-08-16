#!/usr/bin/env python3
"""Refuse release packaging unless every TokenSaver release gate is evidenced.

This verifier intentionally does not run the gates. The final validation pass
creates a local validation/release-manifest.json after executing them. Release
packaging then verifies that the evidence belongs to the exact clean source
commit and the currently installed Codex CLI identity.
"""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "validation" / "release-manifest.json"
PINNED_CODEX_BASELINE = "9ded177ce7c1c0bd2047f902936c177612ab3434"
REQUIRED_GATES = (
    "deterministic_aging_suite",
    "architecture_contract_suite",
    "telemetry_benchmark_suite",
    "recovery_quality_structural_suite",
    "transport_integration_suite",
    "config_restoration_drift_suite",
    "desktop_runtime_tray_suite",
    "cli_doctor_suite",
    "real_codex_smoke_test",
    "compaction_bypass_test",
    "on_off_payload_diff_invariant",
    "tray_backend_state_consistency",
    "privacy_redaction_review",
    "install_uninstall_round_trip",
    "long_session_savings_quality_benchmark",
)


def fail(message: str) -> "NoReturn":
    print(f"RELEASE BLOCKED: {message}", file=sys.stderr)
    raise SystemExit(1)


def command_output(*args: str) -> str:
    try:
        completed = subprocess.run(
            args,
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        fail(f"cannot execute {' '.join(args)}: {error}")
    return completed.stdout.strip()


def cargo_version() -> str:
    source = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"\s*$', source, re.MULTILINE)
    if not match:
        fail("cannot determine TokenSaver version from Cargo.toml")
    return match.group(1)


def main() -> None:
    if not MANIFEST.is_file():
        fail(
            "validation/release-manifest.json is missing; execute the final validation pass first"
        )

    try:
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"validation manifest is unreadable/invalid: {error}")

    if manifest.get("schema_version") != 1:
        fail("validation manifest schema_version must be 1")

    # The package must be built from exactly the tree that was validated. A HEAD
    # SHA alone is insufficient because Cargo/Tauri build working-tree changes.
    dirty = command_output("git", "status", "--porcelain", "--untracked-files=normal")
    if dirty:
        fail("working tree is not clean; commit or discard source changes before release packaging")

    current_commit = command_output("git", "rev-parse", "HEAD")
    if manifest.get("source_commit") != current_commit:
        fail("validation evidence does not belong to the current source commit")

    current_version = cargo_version()
    if manifest.get("tokensaver_version") != current_version:
        fail("validation evidence does not match the current TokenSaver version")

    if manifest.get("validated_codex_baseline_commit") != PINNED_CODEX_BASELINE:
        fail("validation evidence was created for a different Codex protocol baseline")

    validated_codex = manifest.get("validated_codex_cli_version")
    if not isinstance(validated_codex, str) or not validated_codex.strip():
        fail("validated_codex_cli_version must contain the exact validated `codex --version` output")

    installed_codex = command_output("codex", "--version")
    if installed_codex != validated_codex:
        fail(
            "installed Codex CLI identity differs from the build used for final validation"
        )

    gates = manifest.get("gates")
    if not isinstance(gates, dict):
        fail("validation manifest gates must be an object")

    missing = [name for name in REQUIRED_GATES if gates.get(name) is not True]
    if missing:
        fail("release gates not proven: " + ", ".join(missing))

    print(
        f"Release gates verified for TokenSaver {current_version}, commit {current_commit[:12]}, Codex {installed_codex}."
    )


if __name__ == "__main__":
    main()
