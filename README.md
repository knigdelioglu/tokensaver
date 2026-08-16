# TokenSaver

TokenSaver is a focused local context optimizer for Codex. Its purpose is deliberately narrow: **reduce repeated input-token usage caused by large historical tool results without changing Codex's task, native model selection, account flow, tools, or normal workflow.**

The project is inspired by the tool-result aging mechanism in [`duolahypercho/codex-router`](https://github.com/duolahypercho/codex-router), with `v0.4.0-beta.4` pinned as the initial behavioral reference. TokenSaver does not reproduce Codex Router's provider/model-routing product.

## The problem

Coding agents repeatedly produce large terminal outputs, test/build logs, file reads, diffs, searches, and repository-inspection results. After the model has already consumed a large result, stateless later requests may continue carrying the same full historical payload.

TokenSaver targets that repeated context cost.

## Runtime flow

```text
Codex
  ↓
TokenSaver local loopback
  ↓
inspect ordinary Responses history
  ↓
age only eligible historical tool results
  ↓
forward to the same first-party Codex/OpenAI upstream
  ↓
relay response stream unchanged
  ↓
Codex
```

The user continues to use normal Codex. TokenSaver stays out of model selection, account management, MCP, skills, subagents, permissions, and task state.

## Core idea: tool-result aging

A historical result is compacted only when the safety policy proves it eligible:

- text only
- model has acted after receiving it
- strictly larger than the configured threshold
- outside the protected newest-result frontier
- supported/unambiguous protocol shape
- replacement is smaller than the original

Initial conservative policy:

- minimum result size: **32 KiB**
- protected newest-result frontier: **4 results**
- head preview: approximately **1024 UTF-16 code units**
- tail preview: approximately **1024 UTF-16 code units**
- identity: original UTF-8 byte length + **SHA-256**

Receipt v1 carries verifiable metadata:

```text
[tokensaver-receipt:v1 original_bytes=<n> sha256=<hex> head_bytes=<n> tail_bytes=<n>]
```

The head/tail are verbatim evidence; the omitted middle is explicitly unavailable and must not be inferred. TokenSaver does **not** keep a persistent vault of complete original tool outputs in MVP.

## Product boundary

TokenSaver is **not** a model router or Codex replacement. It does not provide:

- provider/model routing or failover
- model catalogs/pickers
- provider API-key/subscription management
- LiteLLM/provider translation
- multi-agent orchestration
- MCP hosting
- vision/OCR bridges
- unrelated response rewriting
- generic LLM summarization of the conversation

The menu-bar app and CLI exist only to operate and observe TokenSaver itself.

See [SCOPE.md](./SCOPE.md) for the authoritative boundary.

## Architecture

TokenSaver is a **modular monolith**.

```text
src/
├── main.rs
├── lib.rs
├── application/
│   ├── benchmark.rs
│   ├── codex_connection.rs
│   ├── control.rs
│   ├── desktop_runtime.rs
│   ├── doctor.rs
│   ├── measurement.rs
│   ├── quality.rs
│   ├── recovery.rs
│   ├── runtime_client.rs
│   ├── settings.rs
│   └── stats.rs
├── desktop/
│   └── mod.rs
├── cli/
│   └── mod.rs
├── modules/
│   ├── aging/
│   ├── transport/
│   ├── codex_integration/
│   ├── telemetry/
│   ├── runtime/
│   └── diagnostics/
└── shared/
    ├── filesystem.rs
    ├── paths.rs
    └── security.rs
```

Strongest dependency rule:

> **The aging domain must remain transport-, Codex-, persistence-, telemetry-, runtime-, and UI-agnostic.**

Both product edges follow the same rule:

```text
desktop ─┐
         ├──► application services ───► modules
CLI ─────┘
```

Neither desktop nor CLI is allowed to reach into product modules or persistence directly. Architecture-contract sources enforce these boundaries.

See [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md).

## Native Codex integration

While connected, TokenSaver temporarily points the built-in Codex/OpenAI provider at a capability-protected loopback base URL:

```text
http://127.0.0.1:<port>/<64-hex-capability>/v1
```

Key rules:

- native Codex authentication is relayed; TokenSaver requires no separate OpenAI key
- TokenSaver-owned Codex values are snapshotted before mutation
- disconnect restores/removes only values TokenSaver owns
- configuration drift is refused rather than overwritten
- explicit conversation compaction bypasses aging
- verified native models/search/images/memory routes pass through without aging
- realtime/WebRTC stays off the Responses optimizer path
- Responses WebSocket receives `426` for supported Codex HTTP fallback
- upstream model responses are streamed without semantic rewrite
- hard OFF mode performs no context rewrite

See [docs/CODEX_TRANSPORT_CONTRACT.md](./docs/CODEX_TRANSPORT_CONTRACT.md).

## macOS menu-bar runtime

Phase 5 provides a windowless Tauri 2 menu-bar application. It exposes backend-derived state such as:

```text
TokenSaver
────────────────────────────────────────────
Status: Active
Codex: Connected
Request: Idle
Health: OK

This session: 720 KB saved · ~184K tokens · 12 results / 7 requests
Today: 2.8 MB saved · ~742K tokens · 41 results / 24 requests
All time: 10.1 MB saved · ~2.6M tokens · 143 results / 82 requests
Last optimization 16:12: 84 KB → 3 KB · 81 KB saved · ~20K tokens

✓ Token Saving Enabled
  Disconnect from Codex
✓ Start at Login

Quit TokenSaver
```

Lifecycle safeguards include:

- single desktop instance
- reversible Connect/Disconnect
- persistent saving and reconnect intent
- Start at Login
- full streamed-request Active/Idle tracking
- Disconnect/Quit refusal while a request is active
- Codex config restoration before normal process exit
- content-free persisted savings aggregates

See [docs/DESKTOP_RUNTIME.md](./docs/DESKTOP_RUNTIME.md).

## CLI and doctor

The same binary also exposes a narrow CLI. It does **not** start a second proxy. Mutating commands control the single running menu-bar runtime over an owner-only local Unix socket.

```text
tokensaver status
tokensaver connect
tokensaver disconnect
tokensaver saving on
tokensaver saving off
tokensaver stats
tokensaver config show
tokensaver config set min-bytes <bytes>
tokensaver config set frontier <count>
tokensaver config set preview-code-units <count>
tokensaver doctor
tokensaver version
```

Behavior:

- `connect`, `disconnect`, and `saving` require the live menu-bar runtime
- `stats` can report persisted content-free counters while the runtime is closed
- `config show/set` can use persisted owner-private preferences while offline
- structural policy changes require Codex to be disconnected
- saving on/off remains live-switchable
- doctor reports redacted PASS/WARN/FAIL health checks
- measured bytes and estimated tokens are always distinguished

Runtime preferences schema v2 persists `saving_enabled`, `connect_on_launch`, `min_bytes`, `frontier`, and `preview_code_units`. Legacy v1 preferences receive the original conservative policy defaults.

See [docs/CLI.md](./docs/CLI.md).

## Design principles

### Preserve recent context
The newest tool results remain exact.

### Compact only consumed results
A result is not shortened before the model has acted after receiving it.

### Deterministic output
The same source and policy produce the same receipt.

### Fail original
Classification/transformation uncertainty leaves the original content untouched.

### Never expand context
A receipt is used only when it is smaller than its source.

### Make missing evidence explicit
Omitted bytes are never presented as known content.

### Measure truthfully
Directly measured bytes, estimated tokens, and provider-reported usage are distinct metrics.

### Safe lifecycle over convenience
TokenSaver refuses unsafe config restoration, disconnect, or normal process exit.

## Required invariants

Among the project-wide invariants:

1. unconsumed results remain exact
2. protected recent results remain exact
3. unsupported/mixed/image-bearing outputs remain exact
4. small results remain exact
5. receipts have deterministic identity
6. compaction never expands context
7. call/result structure is preserved
8. uncertainty fails original
9. hard OFF performs no context rewriting
10. routine telemetry contains no original large result bodies
11. modular-monolith boundaries remain enforced
12. explicit conversation compaction receives original history
13. exact omitted content is never fabricated
14. normal desktop shutdown does not intentionally strand Codex on a dead local endpoint
15. CLI control never becomes arbitrary local command execution

The complete invariant set is in [SCOPE.md](./SCOPE.md).

## Phase status

- **Phase 0 — Project contract and architecture:** complete
- **Phase 1 — Deterministic aging engine:** implemented, validation deferred
- **Phase 2 — Measurement/benchmark:** implemented, validation deferred
- **Phase 3 — Native Codex transport:** implemented, validation deferred
- **Phase 4 — Recovery/quality guardrails:** implemented, validation deferred
- **Phase 5 — macOS runtime/tray:** implemented, validation deferred
- **Phase 6 — CLI/doctor:** implemented, validation deferred
- **Phase 7 — Packaging/update/uninstall:** not started
- **Phase 8 — Hardening/release gates:** not started

Per project instruction, implementation has been authored **without running tests, builds, linters, formatters, CI, benchmarks, CLI/doctor smoke tests, or live Codex validation**. Final execution is intentionally deferred.

See [ROADMAP.md](./ROADMAP.md).

## Engineering documents

- [SCOPE.md](./SCOPE.md) — authoritative product boundary and invariants
- [ROADMAP.md](./ROADMAP.md) — phased implementation plan and release gates
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — modular-monolith boundaries
- [docs/CODEX_TRANSPORT_CONTRACT.md](./docs/CODEX_TRANSPORT_CONTRACT.md) — native Codex integration contract
- [docs/RECOVERY.md](./docs/RECOVERY.md) — receipt/recovery evidence rules
- [docs/DESKTOP_RUNTIME.md](./docs/DESKTOP_RUNTIME.md) — macOS runtime/tray lifecycle contract
- [docs/CLI.md](./docs/CLI.md) — CLI/control-channel/doctor contract
- [docs/UPSTREAM_REFERENCE.md](./docs/UPSTREAM_REFERENCE.md) — pinned Codex Router behavior adopted/rejected
- [AGENTS.md](./AGENTS.md) — implementation guardrails

## Attribution

TokenSaver is a separate, intentionally narrower project inspired by the open-source [Codex Router](https://github.com/duolahypercho/codex-router) project and its tool-result-aging work. Upstream behavior is studied selectively; routing/provider features are outside TokenSaver's mission.
