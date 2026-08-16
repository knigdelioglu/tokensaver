#!/usr/bin/env python3
"""Export a content-free cache comparison from TokenSaver's savings.json.

The caller supplies the owner-local savings file explicitly. No prompt, receipt,
tool-result body, credential, account ID, or capability is read by this script.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--savings-file", required=True, type=Path)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def rate(bucket: dict) -> float | None:
    input_tokens = bucket.get("input_tokens", 0)
    cached = bucket.get("cached_input_tokens", 0)
    if not isinstance(input_tokens, int) or input_tokens <= 0:
        return None
    if not isinstance(cached, int) or cached < 0:
        return None
    return min(cached, input_tokens) / input_tokens


def normalized_bucket(value: object) -> dict:
    bucket = value if isinstance(value, dict) else {}
    return {
        "usage_events": int(bucket.get("usage_events", 0) or 0),
        "input_tokens": int(bucket.get("input_tokens", 0) or 0),
        "cached_input_tokens": int(bucket.get("cached_input_tokens", 0) or 0),
        "cache_rate": rate(bucket),
    }


def main() -> int:
    args = parse_args()
    state = json.loads(args.savings_file.read_text(encoding="utf-8"))
    all_time = state.get("all_time") if isinstance(state, dict) else None
    if not isinstance(all_time, dict):
        raise SystemExit("savings file does not contain all_time telemetry")

    aged = normalized_bucket(all_time.get("aged_cache"))
    unaged = normalized_bucket(all_time.get("unaged_cache"))
    result = {
        "schema": "tokensaver-cache-evidence:v1",
        "source": "content-free persisted provider cache counters",
        "aged": aged,
        "unaged": unaged,
        "requests_observed": int(all_time.get("events", 0) or 0),
        "aged_requests": int(all_time.get("aged_requests", 0) or 0),
        "provider_usage_events": int(all_time.get("provider_usage_events", 0) or 0),
    }
    encoded = json.dumps(result, indent=2, sort_keys=True)
    print(encoded)
    if args.output:
        args.output.write_text(encoded + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
