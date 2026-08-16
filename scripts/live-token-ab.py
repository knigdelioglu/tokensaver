#!/usr/bin/env python3
"""Explicit paid/live A/B probe for TokenSaver tool-result aging.

The script never discovers or prints credentials. The caller supplies the exact
TokenSaver Responses URL (including its local capability path) and, when the
upstream requires it, an Authorization header through an environment variable.
It toggles the already-running TokenSaver runtime OFF/ON, sends an identical
request body twice, and compares provider-reported input tokens.

Nothing runs unless --yes is present.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--yes", action="store_true", help="confirm two live provider requests")
    parser.add_argument("--url", required=True, help="full TokenSaver /v1/responses URL")
    parser.add_argument("--model", required=True)
    parser.add_argument("--tokensaver-bin", default="tokensaver")
    parser.add_argument("--auth-env", default="TOKENSAVER_LIVE_AUTHORIZATION")
    parser.add_argument("--account-id-env", default="TOKENSAVER_LIVE_ACCOUNT_ID")
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def current_saving(binary: str) -> bool:
    result = subprocess.run(
        [binary, "config", "show"],
        check=True,
        capture_output=True,
        text=True,
    )
    for line in result.stdout.splitlines():
        if line.strip() == "saving = on":
            return True
        if line.strip() == "saving = off":
            return False
    raise RuntimeError("could not determine current TokenSaver saving setting")


def set_saving(binary: str, enabled: bool) -> None:
    subprocess.run(
        [binary, "saving", "on" if enabled else "off"],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )


def request_headers(args: argparse.Namespace) -> dict[str, str]:
    headers = {"Content-Type": "application/json", "Accept": "application/json"}
    authorization = os.environ.get(args.auth_env)
    if authorization:
        headers["Authorization"] = authorization
    account_id = os.environ.get(args.account_id_env)
    if account_id:
        headers["ChatGPT-Account-ID"] = account_id
    return headers


def provider_usage(payload: dict) -> tuple[int, int | None]:
    candidates = [payload.get("usage")]
    response = payload.get("response")
    if isinstance(response, dict):
        candidates.append(response.get("usage"))
    for usage in candidates:
        if not isinstance(usage, dict):
            continue
        value = usage.get("input_tokens", usage.get("prompt_tokens"))
        if isinstance(value, int) and value >= 0:
            cached = None
            details = usage.get("input_tokens_details") or usage.get("prompt_tokens_details")
            if isinstance(details, dict) and isinstance(details.get("cached_tokens"), int):
                cached = details["cached_tokens"]
            elif isinstance(usage.get("prompt_cache_hit_tokens"), int):
                cached = usage["prompt_cache_hit_tokens"]
            return value, cached
    raise RuntimeError("provider response did not report input tokens")


def send(url: str, headers: dict[str, str], body: bytes) -> dict:
    request = urllib.request.Request(url, data=body, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(request, timeout=180) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read(4096).decode("utf-8", errors="replace")
        raise RuntimeError(f"provider returned HTTP {error.code}: {detail}") from error


def main() -> int:
    args = parse_args()
    if not args.yes:
        print(
            "Refusing to spend provider quota without --yes. This probe makes two live requests.",
            file=sys.stderr,
        )
        return 2

    large = "BEGIN_PROOF\n" + ("deterministic-tool-output-0123456789\n" * 1800) + "END_PROOF"
    history: list[dict] = [
        {"type": "function_call", "call_id": "old-proof", "name": "exec_command", "arguments": "{}"},
        {"type": "function_call_output", "call_id": "old-proof", "output": large},
        {"type": "message", "role": "assistant", "content": "I consumed the old proof output."},
    ]
    for index in range(4):
        history.extend(
            [
                {"type": "function_call", "call_id": f"new-{index}", "name": "exec_command", "arguments": "{}"},
                {"type": "function_call_output", "call_id": f"new-{index}", "output": f"recent-{index}"},
                {"type": "message", "role": "assistant", "content": f"I consumed recent result {index}."},
            ]
        )
    history.append({"type": "message", "role": "user", "content": "Reply with exactly OK."})
    body_obj = {"model": args.model, "stream": False, "store": False, "input": history}
    body = json.dumps(body_obj, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    headers = request_headers(args)

    original_setting = current_saving(args.tokensaver_bin)
    try:
        set_saving(args.tokensaver_bin, False)
        off_payload = send(args.url, headers, body)
        off_tokens, off_cached = provider_usage(off_payload)

        set_saving(args.tokensaver_bin, True)
        on_payload = send(args.url, headers, body)
        on_tokens, on_cached = provider_usage(on_payload)
    finally:
        set_saving(args.tokensaver_bin, original_setting)

    saved = off_tokens - on_tokens
    percent = round((saved / off_tokens * 100), 2) if off_tokens else 0.0
    result = {
        "schema": "tokensaver-live-token-ab:v1",
        "evidence": "provider-reported live A/B through TokenSaver",
        "model": args.model,
        "identical_request_body_sha256": hashlib.sha256(body).hexdigest(),
        "request_body_bytes": len(body),
        "large_tool_result_bytes": len(large.encode("utf-8")),
        "off": {"input_tokens": off_tokens, "cached_input_tokens": off_cached},
        "on": {"input_tokens": on_tokens, "cached_input_tokens": on_cached},
        "actual_provider_input_tokens_saved": saved,
        "actual_provider_input_token_reduction_percent": percent,
        "pass": saved > 0 and on_tokens < off_tokens,
    }
    encoded = json.dumps(result, indent=2, sort_keys=True)
    print(encoded)
    if args.output:
        args.output.write_text(encoded + "\n", encoding="utf-8")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
