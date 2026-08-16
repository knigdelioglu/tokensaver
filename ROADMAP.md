# TokenSaver Roadmap

TokenSaver has one product goal: **reduce repeated input-token usage in Codex by compacting old, already-consumed tool results while leaving the rest of Codex behavior unchanged.**

TokenSaver is a modular monolith, not a model router. The user continues using normal Codex; TokenSaver sits transparently on the native request path and exposes operation/savings through a small macOS tray application.

```text
Codex
  ↓
TokenSaver loopback
  ↓
inspect ordinary Responses history
  ↓
age only eligible old tool results
  ↓
forward to the same first-party upstream
  ↓
relay response stream unchanged
  ↓
Codex
```

## Product acceptance target

TokenSaver succeeds when:

1. normal Codex account/model/MCP/skills/subagent behavior remains intact
2. aging ON changes only eligible historical tool-result bodies
3. aging OFF performs no context rewrite
4. native conversation compaction sees original history
5. exact omitted content is never guessed
6. native non-Responses provider traffic remains functional
7. tray state is backed by runtime truth and measured savings
8. disconnect/uninstall restores TokenSaver-owned Codex configuration safely
9. TokenSaver never becomes a provider/model router
10. modular-monolith boundaries remain enforced

Core invariant:

> For the same logical ordinary Responses request, ON and OFF semantic payloads may differ only where the aging policy explicitly permits an eligible historical tool-result `output` to change.

---

# Phase 0 — Project contract and architecture

**Status: COMPLETE**

Implemented:

- aligned `README.md`, `SCOPE.md`, and roadmap
- Codex Router `v0.4.0-beta.4` pinned as the initial aging reference
- `docs/UPSTREAM_REFERENCE.md`
- `docs/CODEX_TRANSPORT_CONTRACT.md`
- `docs/ARCHITECTURE.md`
- modular-monolith Rust skeleton
- architecture-contract test source
- `AGENTS.md` implementation rules

Architecture rule: the aging domain remains Codex-, transport-, persistence-, telemetry-, and UI-agnostic.

---

# Phase 1 — Deterministic tool-result aging engine

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Implemented:

- textual tool results only
- strict `> 32 KiB` threshold
- newest four tool-result frontier
- later model-action consumption detection
- `function_call_output` / `custom_tool_call_output`
- pure textual multipart support
- mixed/image/unknown output preservation
- UTF-16-safe bounded head/tail previews
- SHA-256 source identity
- receipt only when smaller than source
- structural replacement tuple: `index + kind + call_id`
- exact byte-savings statistics
- hard disabled mode
- fail-original behavior
- per-result aging/skip reasons

Test sources cover threshold/frontier/consumption, Unicode, determinism, unsupported shapes, structural identity, disabled behavior, and receipt-size safety.

**No tests/build/lint/formatter/CI have been executed.**

---

# Phase 2 — Measurement, telemetry, and benchmark harness

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Implemented content-free outcomes:

- disabled
- conversation-compaction bypass
- native Codex passthrough
- fail-original
- evaluated/no eligible result
- evaluated/no savings
- aged

Implemented metrics:

- tool results evaluated/eligible/compacted
- largest result
- bytes before/after/saved
- approximate `round(bytes / 4)` tokens saved
- provider-reported input/cache usage kept separate from estimates
- session/time-range/all-retained aggregation

Offline deterministic fixtures cover logs, builds, diffs, searches, large reads, many medium results, mixed output, and unconsumed history.

Routine telemetry stores no original tool-result body or receipt.

**No tests/build/lint/formatter/CI have been executed.**

---

# Phase 3 — Native Codex transport integration

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Authoritative document: `docs/CODEX_TRANSPORT_CONTRACT.md`.

Pinned implementation baseline:

`openai/codex@9ded177ce7c1c0bd2047f902936c177612ab3434`

## Codex config ownership

While connected TokenSaver installs:

```toml
openai_base_url = "http://127.0.0.1:<port>/<64-hex-capability>/v1"
```

When missing, it also temporarily installs native realtime bypasses so voice/WebRTC does not inherit the Responses loopback URL.

Implemented:

- TOML-preserving config edits
- pre-existing values preserved
- owner-only versioned snapshot
- snapshot-before-mutation transaction
- atomic writes
- per-owned-key drift detection
- exact disconnect restoration
- crash/restart recovery of the same port + capability
- `CODEX_HOME` / `~/.codex` resolution

## Transport

Implemented:

- IPv4 loopback-only listener
- 256-bit capability path
- browser-origin rejection
- no permissive CORS
- fixed first-party upstreams only
- no arbitrary forward proxy
- response streaming without semantic rewrite
- raw query preservation
- redirects disabled upstream
- hop-by-hop response-header filtering
- Responses WebSocket `426` → Codex HTTP fallback compatibility
- zstd/gzip/x-gzip/deflate/Brotli request handling for aging inspection
- fail-original on unsafe Responses decode/parse/rewrite

Finite native route allow-list:

| Local route | Method | Behavior |
|---|---:|---|
| `/v1/responses` | POST | aging eligible |
| `/v1/responses/compact` | POST | exact aging bypass |
| `/v1/models` | GET | native passthrough |
| `/v1/memories/trace_summarize` | POST | native passthrough |
| `/v1/alpha/search` | POST | native passthrough |
| `/v1/images/generations` | POST | native passthrough |
| `/v1/images/edits` | POST | native passthrough |

Account-scoped Codex requests are preserved toward the ChatGPT Codex backend; API-key-style requests are preserved toward the OpenAI API without TokenSaver parsing bearer-token contents.

Still deferred for the final executed pass:

- compile/test/lint/format
- installed Codex smoke
- streamed tool-call turn
- cancellation
- live auth/header behavior
- native passthrough smoke cases
- captured ON/OFF semantic comparison

**No tests/build/lint/formatter/CI have been executed.**

---

# Phase 4 — Recovery and quality guardrails

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Authoritative document: `docs/RECOVERY.md`.

Goal: make information loss explicit and make exact recovery verifiable without creating a broad private store of original tool outputs.

## Verifiable receipt v1

New receipts contain machine-readable identity:

```text
[tokensaver-receipt:v1 original_bytes=<n> sha256=<hex> head_bytes=<n> tail_bytes=<n>]
```

Receipt rules:

- head and tail are explicitly verbatim evidence
- omitted middle is explicitly marked unavailable
- the model is told **not to infer** omitted bytes
- replay of the previous tool is suggested only when that operation is safe to repeat
- exact recovery is accepted only when UTF-8 byte length and SHA-256 both match

Implemented aging-domain APIs:

- parse/validate TokenSaver receipt evidence
- expose original byte count, digest, head, tail, and omitted byte count
- verify an externally recovered exact candidate
- reject unsupported receipt versions and malformed layouts

## Application recovery contract

`src/application/recovery.rs` implements explicit recovery intent rather than inferring intent from arbitrary model text.

Outcomes distinguish:

- `ReceiptEvidenceAvailable`
- `ExactSourceRequired`
- `VerifiedExact`
- `Rejected`

If exact omitted content is required, TokenSaver never reconstructs it. The normal Codex workflow must obtain the source again; a returned candidate becomes exact only after identity verification.

## Privacy boundary

MVP deliberately has **no persistent exact-result vault**.

TokenSaver therefore does not create a second store containing complete shell output, file reads, diffs, or search results merely for recovery. A future bounded owner-local cache would require a separate architecture/privacy decision.

## Deterministic quality harness

`src/application/quality.rs` provides authored fixtures for:

- head/middle/tail evidence boundary
- many aged results in one history
- old consumed result after a long history distance
- receipt parsing for every aged fixture result
- exact-source digest verification
- same-length modified-source rejection

Existing Phase 3 coverage supplies the explicit conversation-compaction bypass side of the combined behavior.

Still deferred for the final executed/live A/B pass:

- real task success ON vs OFF
- real tool-call correctness ON vs OFF
- model behavior when a needed fact exists only in omitted middle
- safety of actual tool replay/recovery
- aging + native conversation compaction in live sessions
- authoritative input/cache token comparison
- latency overhead

**No tests/build/lint/formatter/CI or live A/B validation have been executed.**

---

# Phase 5 — macOS runtime and tray/menu-bar application

**Status: NOT STARTED**

Goal: let the user see and control TokenSaver without a terminal.

Required real states:

- TokenSaver running/stopped
- Codex connected/waiting/configuration problem
- token saving enabled/disabled
- request active/idle
- config drift/error

Required tray information/actions:

```text
TokenSaver
──────────────
Status            Active
Codex             Connected
Token Saving      On

This session
Saved             ~184K tokens
Compacted         12 results

Today
Saved             ~742K tokens

All time
Saved             ~2.6M tokens

Last optimization
84 KB → 3 KB
```

Also:

- enable/disable saving
- Connect to Codex / Disconnect
- Start at Login
- diagnostics/status
- safe quit

Tray truth must come from backend/runtime state rather than UI toggle state.

---

# Phase 6 — CLI and diagnostics

**Status: NOT STARTED**

Planned commands:

```text
tokensaver status
tokensaver connect
tokensaver disconnect
tokensaver saving on
tokensaver saving off
tokensaver stats
tokensaver config show
tokensaver config set min-bytes ...
tokensaver config set frontier ...
tokensaver doctor
```

`doctor` should verify Codex compatibility, loopback state, config ownership/drift, upstream reachability, saving state, last optimizer activity, and local file permissions while redacting credentials/capabilities/result bodies.

---

# Phase 7 — Packaging, update safety, and uninstall

**Status: NOT STARTED**

Planned:

- macOS application packaging
- signing/notarization when appropriate
- deterministic install/state paths
- safe runtime lifecycle
- update mechanism preserving user choices/state
- first-class disconnect/uninstall
- exact restoration of TokenSaver-owned Codex config
- cleanup limited to TokenSaver files

---

# Phase 8 — Hardening and release gates

**Status: NOT STARTED**

Reliability gates:

- malformed requests
- huge outputs/bodies
- memory pressure
- concurrency
- interrupted streams
- TokenSaver/Codex restart
- machine reboot/start-at-login

Security/privacy gates:

- loopback-only
- strict local capability
- no browser/CORS proxy surface
- owner-only sensitive state
- no credentials in logs
- no original result bodies in routine telemetry
- bounded logs/state
- safe temporary files

Performance/compatibility gates:

- optimizer overhead materially below saved context cost
- bounded copies of huge results
- compression/serialization benchmark
- explicit supported Codex baseline
- unsupported builds detected rather than guessed

Final release gates:

1. deterministic aging suite
2. architecture-contract suite
3. telemetry/benchmark suite
4. recovery/quality structural suite
5. transport integration suite
6. config restoration/drift suite
7. real Codex smoke test
8. compaction-bypass test
9. ON/OFF payload-diff invariant
10. tray/backend state consistency
11. privacy/log-redaction
12. install/uninstall round-trip
13. realistic long-session savings + quality benchmark

---

# Post-MVP ideas — only if they remain in scope

Possible later improvements:

- per-tool/adaptive thresholds
- safely parsed repetitive-log compaction
- tokenizer-aware estimates
- optional bounded owner-local exact-result retention after separate privacy review
- other Responses-compatible coding-agent adapters
- richer local statistics/history
- Windows/Linux support

Every proposal must pass:

> Does this directly improve safe context/token reduction, transparent Codex integration, or operation/observability of that mechanism?

If not, it does not belong in TokenSaver.

---

# Explicit non-goals

TokenSaver will not become:

- a multi-provider model router
- an external-model catalog
- a provider API-key manager
- a LiteLLM replacement
- a model picker
- an agent orchestrator
- an MCP host
- a prompt marketplace
- a general Codex configuration manager

The intended product remains:

> **Run normal Codex through a small local context optimizer, safely compact old consumed tool results, make missing evidence explicit, show the user what was saved, and otherwise stay out of the way.**
