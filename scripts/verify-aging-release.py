#!/usr/bin/env python3
"""Release gate for TokenSaver's context-saving path.

This gate consumes evidence produced by the explicit live probes plus the
content-free cache export. It does not itself spend provider quota. The normal
Rust/CI validation is a separate gate and must also pass.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--token-ab", required=True, type=Path)
    parser.add_argument("--quality", required=True, type=Path)
    parser.add_argument("--cache", required=True, type=Path)
    parser.add_argument("--tokensaver-bin", default="tokensaver")
    parser.add_argument("--min-cache-events", type=int, default=5)
    parser.add_argument(
        "--max-cache-regression-bp",
        type=int,
        default=500,
        help="maximum aged cache-rate regression in basis points (default: 500 = 5pp)",
    )
    parser.add_argument("--skip-doctor", action="store_true")
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def load_json(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return payload


def cache_basis_points(bucket: dict[str, Any]) -> int | None:
    value = bucket.get("cache_rate")
    if not isinstance(value, (int, float)):
        return None
    return round(float(value) * 10_000)


def record(checks: list[dict[str, Any]], name: str, passed: bool, detail: str) -> None:
    checks.append({"name": name, "pass": bool(passed), "detail": detail})


def run_doctor(binary: str) -> tuple[bool, str]:
    try:
        completed = subprocess.run(
            [binary, "doctor"],
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return False, f"doctor could not run: {error}"
    lines = (completed.stdout + "\n" + completed.stderr).strip().splitlines()
    tail = " | ".join(line.strip() for line in lines[-4:] if line.strip())
    return completed.returncode == 0, tail or f"doctor exit={completed.returncode}"


def main() -> int:
    args = parse_args()
    token_ab = load_json(args.token_ab)
    quality = load_json(args.quality)
    cache = load_json(args.cache)
    checks: list[dict[str, Any]] = []

    record(
        checks,
        "token-ab-schema",
        token_ab.get("schema") == "tokensaver-live-token-ab:v1",
        str(token_ab.get("schema")),
    )
    off = token_ab.get("off") if isinstance(token_ab.get("off"), dict) else {}
    on = token_ab.get("on") if isinstance(token_ab.get("on"), dict) else {}
    off_input = off.get("input_tokens")
    on_input = on.get("input_tokens")
    saved = token_ab.get("actual_provider_input_tokens_saved")
    token_pass = (
        token_ab.get("pass") is True
        and isinstance(off_input, int)
        and isinstance(on_input, int)
        and isinstance(saved, int)
        and off_input > 0
        and on_input >= 0
        and on_input < off_input
        and saved == off_input - on_input
        and saved > 0
    )
    record(
        checks,
        "provider-token-reduction",
        token_pass,
        f"off={off_input} on={on_input} saved={saved}",
    )

    record(
        checks,
        "quality-schema",
        quality.get("schema") == "tokensaver-live-aging-quality:v1",
        str(quality.get("schema")),
    )
    record(
        checks,
        "omitted-middle-quality",
        quality.get("pass") is True,
        str(quality.get("verdict")),
    )
    same_model = (
        isinstance(token_ab.get("model"), str)
        and token_ab.get("model") == quality.get("model")
    )
    record(
        checks,
        "same-live-model",
        same_model,
        f"token={token_ab.get('model')} quality={quality.get('model')}",
    )

    record(
        checks,
        "cache-schema",
        cache.get("schema") == "tokensaver-cache-evidence:v1",
        str(cache.get("schema")),
    )
    aged = cache.get("aged") if isinstance(cache.get("aged"), dict) else {}
    unaged = cache.get("unaged") if isinstance(cache.get("unaged"), dict) else {}
    aged_events = int(aged.get("usage_events", 0) or 0)
    unaged_events = int(unaged.get("usage_events", 0) or 0)
    record(
        checks,
        "cache-sample-size",
        aged_events >= args.min_cache_events and unaged_events >= args.min_cache_events,
        f"aged_n={aged_events} unaged_n={unaged_events} min={args.min_cache_events}",
    )
    aged_bp = cache_basis_points(aged)
    unaged_bp = cache_basis_points(unaged)
    cache_ok = (
        aged_bp is not None
        and unaged_bp is not None
        and aged_bp + args.max_cache_regression_bp >= unaged_bp
    )
    record(
        checks,
        "cache-regression",
        cache_ok,
        f"aged_bp={aged_bp} unaged_bp={unaged_bp} allowed_regression_bp={args.max_cache_regression_bp}",
    )
    record(
        checks,
        "observed-runtime-evidence",
        int(cache.get("requests_observed", 0) or 0) > 0
        and int(cache.get("aged_requests", 0) or 0) > 0
        and int(cache.get("provider_usage_events", 0) or 0) > 0,
        (
            f"requests={cache.get('requests_observed')} "
            f"aged={cache.get('aged_requests')} "
            f"usage_events={cache.get('provider_usage_events')}"
        ),
    )

    if args.skip_doctor:
        record(checks, "tokensaver-doctor", True, "skipped by explicit --skip-doctor")
    else:
        doctor_ok, doctor_detail = run_doctor(args.tokensaver_bin)
        record(checks, "tokensaver-doctor", doctor_ok, doctor_detail)

    passed = all(check["pass"] for check in checks)
    result = {
        "schema": "tokensaver-aging-release-gate:v1",
        "pass": passed,
        "checks": checks,
        "note": (
            "This evidence gate complements, and does not replace, the repository's full "
            "Rust/format/clippy/architecture/packaging validation."
        ),
    }
    encoded = json.dumps(result, indent=2, sort_keys=True)
    print(encoded)
    if args.output:
        args.output.write_text(encoded + "\n", encoding="utf-8")
    return 0 if passed else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, json.JSONDecodeError, OSError) as error:
        print(f"release gate error: {error}", file=sys.stderr)
        raise SystemExit(2) from error
