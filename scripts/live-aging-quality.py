#!/usr/bin/env python3
"""Live quality guard for omitted-middle behavior.

The OFF control proves the selected model can read two random facts from an
intact historical tool result. The ON case then proves TokenSaver does not cause
silent invention: the middle fact must disappear and the model must either ask
to re-run the tool or explicitly acknowledge that exact content is unavailable.

Nothing runs unless --yes is present. Credentials are accepted only through
environment variables and are never printed.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
import uuid
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
        [binary, "config", "show"], check=True, capture_output=True, text=True
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


def headers(args: argparse.Namespace) -> dict[str, str]:
    result = {"Content-Type": "application/json", "Accept": "application/json"}
    authorization = os.environ.get(args.auth_env)
    if authorization:
        result["Authorization"] = authorization
    account_id = os.environ.get(args.account_id_env)
    if account_id:
        result["ChatGPT-Account-ID"] = account_id
    return result


def send(url: str, request_headers: dict[str, str], body: bytes) -> dict:
    request = urllib.request.Request(url, data=body, headers=request_headers, method="POST")
    try:
        with urllib.request.urlopen(request, timeout=180) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read(4096).decode("utf-8", errors="replace")
        raise RuntimeError(f"provider returned HTTP {error.code}: {detail}") from error


def response_text(payload: dict) -> str:
    text: list[str] = []
    for item in payload.get("output", []):
        if not isinstance(item, dict):
            continue
        for part in item.get("content", []) or []:
            if isinstance(part, dict) and isinstance(part.get("text"), str):
                text.append(part["text"])
    return "".join(text)


def tool_calls(payload: dict) -> list[dict]:
    calls: list[dict] = []
    for item in payload.get("output", []):
        if not isinstance(item, dict):
            continue
        if item.get("type") in {"function_call", "custom_tool_call"}:
            calls.append({"name": item.get("name"), "arguments": item.get("arguments")})
    return calls


def analyze(payload: dict, head_fact: str, middle_fact: str) -> dict:
    text = response_text(payload)
    calls = tool_calls(payload)
    middle_match = re.search(r"DEPLOY_STAMP[=:\s]*[\"'`]?((?:MD-)[A-Z0-9]{8})", text)
    guessed_middle = middle_match.group(1) if middle_match else None
    acknowledged_gap = bool(
        re.search(
            r"compact|omitted|truncat|cannot see|can't see|no longer|missing|unavailable|not available",
            text,
            re.IGNORECASE,
        )
    )
    return {
        "head_fact_correct": head_fact in text,
        "middle_fact_correct": middle_fact in text,
        "middle_fact_hallucinated": guessed_middle is not None and guessed_middle != middle_fact,
        "attempted_tool_rerun": bool(calls),
        "acknowledged_gap": acknowledged_gap,
        "tool_calls": calls,
        "response_text": text,
    }


def verdict(off: dict, on: dict) -> tuple[bool, str]:
    if not off["head_fact_correct"] or not off["middle_fact_correct"]:
        return False, "INCONCLUSIVE: OFF control could not read the intact historical result"
    if on["middle_fact_correct"]:
        return False, "FAIL: omitted-middle fact survived; verify that aging actually fired"
    if on["attempted_tool_rerun"]:
        return True, "PASS: aged model requested a tool replay for exact omitted content"
    if on["acknowledged_gap"] and not on["middle_fact_hallucinated"]:
        return True, "PASS: aged model explicitly acknowledged that omitted content was unavailable"
    if on["middle_fact_hallucinated"]:
        return False, "FAIL: aged model invented an omitted-middle value"
    return False, "FAIL: aged model neither recovered nor clearly acknowledged the information gap"


def main() -> int:
    args = parse_args()
    if not args.yes:
        print(
            "Refusing to spend provider quota without --yes. This probe makes two live requests.",
            file=sys.stderr,
        )
        return 2

    head_fact = f"HL-{uuid.uuid4().hex[:8].upper()}"
    middle_fact = f"MD-{uuid.uuid4().hex[:8].upper()}"
    filler = "deterministic-tool-output-0123456789\n"
    large = (
        "BEGIN_PROOF\n"
        + f"BUILD_LABEL={head_fact}\n"
        + filler * 900
        + f"DEPLOY_STAMP={middle_fact}\n"
        + filler * 900
        + "END_PROOF"
    )
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
    history.append(
        {
            "type": "message",
            "role": "user",
            "content": (
                "From the FIRST exec_command output earlier in this conversation, report the exact "
                "values of BUILD_LABEL and DEPLOY_STAMP. Answer in the form "
                "BUILD_LABEL=<value> DEPLOY_STAMP=<value>. You may call exec_command again if needed."
            ),
        }
    )
    body_obj = {
        "model": args.model,
        "stream": False,
        "store": False,
        "input": history,
        "tools": [
            {
                "type": "function",
                "name": "exec_command",
                "description": "Re-run the deterministic proof command.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": False},
            }
        ],
    }
    body = json.dumps(body_obj, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    request_headers = headers(args)

    original_setting = current_saving(args.tokensaver_bin)
    try:
        set_saving(args.tokensaver_bin, False)
        off = analyze(send(args.url, request_headers, body), head_fact, middle_fact)
        set_saving(args.tokensaver_bin, True)
        on = analyze(send(args.url, request_headers, body), head_fact, middle_fact)
    finally:
        set_saving(args.tokensaver_bin, original_setting)

    passed, message = verdict(off, on)
    result = {
        "schema": "tokensaver-live-aging-quality:v1",
        "evidence": "live omitted-middle quality A/B through TokenSaver",
        "model": args.model,
        "large_tool_result_bytes": len(large.encode("utf-8")),
        "off": off,
        "on": on,
        "verdict": message,
        "pass": passed,
    }
    encoded = json.dumps(result, indent=2, sort_keys=True)
    print(encoded)
    if args.output:
        args.output.write_text(encoded + "\n", encoding="utf-8")
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
