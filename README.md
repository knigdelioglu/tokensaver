# TokenSaver

TokenSaver is a focused local context optimizer for Codex. Its purpose is deliberately narrow: **reduce repeated input-token usage caused by large historical tool results without changing Codex's task, native model selection, account flow, tools, or normal workflow.**

The project is inspired by the tool-result aging mechanism in [`duolahypercho/codex-router`](https://github.com/duolahypercho/codex-router), with `v0.4.0-beta.4` pinned as the initial behavioral reference. TokenSaver does not reproduce Codex Router's provider/model-routing product. It extracts the token-saving mechanism into a small independent desktop utility.

## The problem

Coding agents repeatedly produce large tool outputs:

- terminal command output
- test and build logs
- file reads
- diffs and patches
- search results
- repository inspection results

After the model has already consumed one of these results, the same large payload may continue to be included in later requests as conversation history. A single large result can therefore consume input tokens many times.

TokenSaver targets that repetition.

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

The user continues to use normal Codex. TokenSaver stays out of the model/tool workflow and exposes only a small macOS menu-bar control surface.

## Core idea: tool-result aging

TokenSaver detects historical tool results that are safe candidates for compaction and replaces only the model-visible historical copy with a deterministic receipt.

A candidate is compacted only when all required safety conditions are satisfied:

- only textual tool results are eligible
- the model must already have acted after seeing the result
- small results stay untouched
- a configurable number of newest tool results stay byte-for-byte intact
- mixed/image-bearing results stay untouched
- unknown/ambiguous structures stay untouched
- compaction must never make a result larger

Initial policy:

- minimum result size: **32 KiB**
- protected newest-result frontier: **4 results**
- head preview: approximately **1024 UTF-16 code units**
- tail preview: approximately **1024 UTF-16 code units**
- receipt identity: original UTF-8 byte length + **SHA-256** digest

Conceptually:

```text
large historical tool result
        │
        │ model already consumed it
        ▼
safety / eligibility checks
        │
        ├── ineligible ──► original result
        │
        ▼
deterministic compact receipt
  - original size
  - SHA-256
  - bounded head preview
  - explicit omitted middle
  - bounded tail preview
        │
        ▼
smaller context on later requests
```

Receipt v1 also carries machine-readable size/digest/preview-length metadata so an externally recovered exact source can be verified. TokenSaver does **not** keep a persistent vault of complete original tool outputs in MVP.

## Product boundary

TokenSaver is **not** a model router and will not become a general Codex replacement.

It does not provide:

- model/provider routing
- API-key or subscription management
- model catalogs or model pickers
- LiteLLM/provider translation
- multi-agent orchestration
- MCP hosting
- vision/OCR bridges
- provider quota management
- unrelated response rewriting
- generic conversation summarization by another LLM

The macOS tray/menu-bar application is part of the product because it answers whether TokenSaver is connected and whether it is actually saving context. It is restricted to TokenSaver operation/observability.

See [SCOPE.md](./SCOPE.md) for the authoritative product boundary.

## Architecture

TokenSaver is implemented as a **modular monolith**.

Current layout:

```text
src/
├── application/
│   ├── codex_connection.rs
│   ├── desktop_runtime.rs
│   ├── measurement.rs
│   ├── benchmark.rs
│   ├── recovery.rs
│   └── quality.rs
├── desktop/
│   └── mod.rs
├── modules/
│   ├── aging/
│   ├── transport/
│   ├── codex_integration/
│   ├── telemetry/
│   ├── runtime/
│   └── diagnostics/
└── shared/
```

The strongest dependency rule is:

> **The aging domain must remain transport-, Codex-, persistence-, and UI-agnostic.**

The Tauri shell also does not reach into modules directly; it calls the application-layer desktop runtime controller.

See [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) for module ownership and dependency rules.

## Native Codex integration

TokenSaver temporarily points the built-in Codex/OpenAI provider at a capability-protected loopback base URL, while preserving native account/model behavior and fixed first-party upstreams.

Important rules:

- use Codex's existing authentication path; no separate TokenSaver OpenAI key
- snapshot TokenSaver-owned Codex config before mutation
- restore/remove only values TokenSaver owns
- detect drift rather than overwriting newer user changes
- bypass explicit conversation compaction so it sees original history
- pass verified native models/search/images/memory endpoints without aging
- keep realtime/WebRTC off the Responses optimizer path
- return WebSocket `426` so supported Codex builds fall back to HTTP Responses
- relay upstream response streams without semantic rewrite
- provide hard OFF mode with no aging rewrite

The detailed contract is in [docs/CODEX_TRANSPORT_CONTRACT.md](./docs/CODEX_TRANSPORT_CONTRACT.md).

## macOS menu-bar runtime

Phase 5 provides a windowless Tauri 2 menu-bar application.

The tray exposes backend-derived state such as:

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

Key lifecycle behavior:

- first launch is disconnected unless prior connection intent/crash state says to reconnect
- Connect/Disconnect uses the reversible Phase 3 config transaction
- saving ON/OFF persists and updates live transport policy
- Start at Login uses the macOS autostart integration
- only one desktop instance is allowed
- request activity is counted through the complete streamed response lifetime
- Disconnect/Quit is refused while a Codex request is active
- normal Quit restores Codex config before process exit
- safe Quit preserves the user's desire to reconnect on a later launch

Only numeric content-free savings aggregates are persisted for tray statistics.

See [docs/DESKTOP_RUNTIME.md](./docs/DESKTOP_RUNTIME.md) for the lifecycle and tray contract.

## Design principles

### Preserve recent context

The newest tool results are hot context and remain exact.

### Compact only consumed results

A result must not be shortened before the model has acted after receiving it.

### Deterministic output

The same source result and policy should produce the same compact receipt.

### Fail original

If TokenSaver cannot confidently classify or transform an item, the original content passes through unchanged.

### Never expand context

If the receipt is not smaller than the source result, keep the source result.

### Make missing evidence explicit

Receipt head/tail content is verbatim evidence; omitted middle bytes are unavailable and must not be inferred.

### Measure truthfully

TokenSaver distinguishes:

- directly measured bytes saved
- estimated tokens saved
- provider-reported token/cache telemetry when naturally available

Estimated values are labeled as estimates.

### Safe lifecycle over convenience

TokenSaver refuses destructive config restoration or process exit when it cannot prove that doing so is safe.

## Required invariants

The project contract requires, among other rules:

1. Unconsumed tool results remain exact.
2. Protected recent results remain exact.
3. Unsupported/mixed/image-bearing outputs remain exact.
4. Small results remain exact.
5. Receipts have deterministic identity.
6. Compaction never expands context.
7. Call/result protocol structure is preserved.
8. Uncertainty fails original.
9. Hard OFF mode performs no context rewriting.
10. Routine telemetry does not contain original large result bodies.
11. Modular-monolith boundaries are enforced.
12. Explicit conversation compaction receives original history.
13. Exact omitted content is never fabricated.
14. Normal desktop shutdown does not intentionally strand Codex on a dead local endpoint.

The complete invariant set lives in [SCOPE.md](./SCOPE.md).

## Phase status

- **Phase 0 — Project contract and architecture:** complete
- **Phase 1 — Deterministic aging engine:** implemented, validation deferred
- **Phase 2 — Measurement/benchmark:** implemented, validation deferred
- **Phase 3 — Native Codex transport:** implemented, validation deferred
- **Phase 4 — Recovery/quality guardrails:** implemented, validation deferred
- **Phase 5 — macOS runtime/tray:** implemented, validation deferred
- **Phase 6 — CLI/doctor:** not started
- **Phase 7 — Packaging/update/uninstall:** not started
- **Phase 8 — Hardening/release gates:** not started

Per project instruction, implementation phases have been authored **without running tests, builds, linters, formatters, CI, benchmarks, or live Codex validation**. Final execution is intentionally deferred.

See [ROADMAP.md](./ROADMAP.md) for the complete implementation sequence and release gates.

## Engineering documents

- [SCOPE.md](./SCOPE.md) — authoritative product boundary and invariants
- [ROADMAP.md](./ROADMAP.md) — phased implementation plan and release gates
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — modular-monolith boundaries
- [docs/CODEX_TRANSPORT_CONTRACT.md](./docs/CODEX_TRANSPORT_CONTRACT.md) — native Codex integration contract
- [docs/RECOVERY.md](./docs/RECOVERY.md) — receipt/recovery evidence rules
- [docs/DESKTOP_RUNTIME.md](./docs/DESKTOP_RUNTIME.md) — macOS runtime/tray lifecycle contract
- [docs/UPSTREAM_REFERENCE.md](./docs/UPSTREAM_REFERENCE.md) — pinned Codex Router behavior adopted/rejected
- [AGENTS.md](./AGENTS.md) — repository implementation guardrails

## Attribution

TokenSaver is a separate, intentionally narrower project inspired by the open-source [Codex Router](https://github.com/duolahypercho/codex-router) project and its tool-result-aging work. Upstream behavior is studied selectively; routing/provider features are outside TokenSaver's mission.
