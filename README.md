# TokenSaver

TokenSaver is a focused local context optimizer for Codex. Its purpose is deliberately narrow: **reduce repeated input-token usage caused by large historical tool results without changing Codex's task, native model selection, account flow, tools, or normal workflow.**

The project is inspired by the tool-result aging mechanism in [`duolahypercho/codex-router`](https://github.com/duolahypercho/codex-router), with `v0.4.0-beta.4` pinned as the initial behavioral reference and later upstream aging/measurement work reviewed selectively. TokenSaver does not reproduce Codex Router's provider/model-routing product.

## The problem

Coding agents repeatedly produce large terminal outputs, test/build logs, file reads, diffs, searches, and repository-inspection results. After the model has already consumed a large result, later requests may continue carrying the same full historical payload.

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

Initial conservative structural policy:

- minimum result size: **32 KiB**
- protected newest-result frontier: **4 results**
- head preview: approximately **1024 UTF-16 code units**
- tail preview: approximately **1024 UTF-16 code units**
- identity: original UTF-8 byte length + **SHA-256**

A fresh product installation starts **Token Saving off** because enabling aging changes historical context. An existing persisted user choice is preserved. The user can opt in from the tray or with `tokensaver saving on`.

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
│   ├── maintenance.rs
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
- the presence of native `previous_response_id` is observable without logging its value, and TokenSaver preserves native chaining rather than copying a provider router's stateless-history assumption

See [docs/CODEX_TRANSPORT_CONTRACT.md](./docs/CODEX_TRANSPORT_CONTRACT.md) and [docs/NATIVE_AGING_VALIDATION.md](./docs/NATIVE_AGING_VALIDATION.md).

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

  Token Saving Enabled   ← off on a fresh install until explicitly enabled
  Disconnect from Codex
✓ Start at Login
  Prepare for Uninstall…

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
- uninstall preparation through the same safe disconnect transaction

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
tokensaver diagnostics
tokensaver config show
tokensaver config set min-bytes <bytes>
tokensaver config set frontier <count>
tokensaver config set preview-code-units <count>
tokensaver doctor
tokensaver uninstall [--purge-state]
tokensaver version
```

Behavior:

- `connect`, `disconnect`, and `saving` require the live menu-bar runtime
- `stats` can report persisted content-free counters while the runtime is closed
- `diagnostics` explains request shape, skip reasons, observed provider tokens, and aged-vs-unaged cache evidence without printing request/result content
- `config show/set` can use persisted owner-private preferences while offline
- structural policy changes require Codex to be disconnected
- saving on/off remains live-switchable
- doctor reports redacted PASS/WARN/FAIL health checks
- uninstall purge is blocked while runtime/restoration state says cleanup is unsafe
- measured bytes, estimated tokens, and provider-reported usage are always distinguished

Runtime preferences schema v2 persists `saving_enabled`, `connect_on_launch`, `min_bytes`, `frontier`, and `preview_code_units`. Legacy v1 preferences preserve their explicit saving choice and receive conservative defaults for missing structural policy fields.

See [docs/CLI.md](./docs/CLI.md).

## Native aging validation and release evidence

The current P0–P6 remediation track adds:

- content-free native request-shape diagnostics
- explicit preservation proof for `previous_response_id`
- provider input/cache/output usage extraction from unchanged responses
- aggregate skip reasons
- aged-vs-unaged prompt-cache evidence
- explicit live provider token A/B
- omitted-middle recovery/hallucination quality A/B
- a fail-closed aging evidence gate

The live probes never run implicitly because they may consume provider/account quota:

```bash
python3 scripts/live-token-ab.py --yes ...
python3 scripts/live-aging-quality.py --yes ...
python3 scripts/cache-evidence.py ...
python3 scripts/verify-aging-release.py ...
```

See [docs/NATIVE_AGING_VALIDATION.md](./docs/NATIVE_AGING_VALIDATION.md) and [docs/MEASUREMENT.md](./docs/MEASUREMENT.md).

## Packaging, update, and uninstall

Phase 7 defines a macOS `.app` + `.dmg` path without adding an untrusted self-updater.

Release assets are source-first:

```text
assets/app-icon.svg
  ↓
cargo tauri icon
  ↓
generated icons/
  ↓
release-only Tauri config overlay
  ↓
TokenSaver.app + DMG
```

Development/local packaging:

```bash
bash scripts/package-macos.sh
```

Validated project release packaging:

```bash
bash scripts/release-macos.sh
```

The release path is fail-closed: it requires a local validation manifest tied to the exact source commit, TokenSaver version, pinned Codex protocol baseline, exact validated `codex --version` identity, and all 15 release gates. The completed manifest is gitignored; the repository ships only an all-false example template.

`bundle.createUpdaterArtifacts` is intentionally disabled until TokenSaver has a trusted update endpoint, updater public key, protected signing material, signed artifacts, and tested recovery behavior.

Safe manual update uses normal Quit so Codex config is restored while `connect_on_launch` is preserved; replacing the `.app` does not delete external per-user preferences/savings.

Safe uninstall uses **Prepare for Uninstall…** to disconnect Codex, clear reconnect intent, disable Start at Login, flush telemetry, and exit. Optional `tokensaver uninstall --purge-state` then deletes only known TokenSaver-owned state, refuses an active restoration snapshot, and preserves unknown entries.

See [docs/PACKAGING.md](./docs/PACKAGING.md).

## Phase 8 hardening

Phase 8 adds finite resource limits and evidence-based release/compatibility behavior without changing TokenSaver's product scope.

Runtime bounds currently authored:

- encoded native request: **64 MiB max**
- decoded inspection body: **256 MiB max**
- native concurrent requests: **16 max**
- upstream connect timeout: **15 seconds**
- content-free telemetry observation queue: **1024 max**
- owner-local control clients: **16 max**
- control message/response: **64 KiB max**
- control connect/read/write timeout: **5 seconds**

A saturated telemetry queue never blocks inference; it increments a dropped-observation health counter and doctor warns that savings may be incomplete. Provider usage parsing is bounded and side-band; telemetry inability must not rewrite or fail inference.

Codex compatibility is tied to pinned protocol baseline `openai/codex@9ded177ce7c1c0bd2047f902936c177612ab3434` and an explicitly validated exact `codex --version` identity. Unknown builds WARN rather than being silently declared supported. The validation allow-list remains empty until the final executed validation pass proves a build.

Source-level error formatting also redacts capability-bearing endpoints, drift values, and parser context rather than relying only on UI/CLI wrappers.

See [docs/HARDENING.md](./docs/HARDENING.md).

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
Directly measured bytes, estimated tokens, provider-reported usage, and telemetry completeness are distinct facts.

### Preserve native semantics
Provider-router transport assumptions are not copied onto TokenSaver's first-party native path without direct evidence.

### Safe lifecycle over convenience
TokenSaver refuses unsafe config restoration, disconnect, normal process exit, or state purge.

### Bound degradation
Resource saturation is bounded and surfaced rather than translated into unbounded memory/task growth.

### Evidence before release claims
Unknown Codex builds and missing validation evidence remain unproven rather than being guessed safe.

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
16. uninstall cleanup never deletes restoration proof or unknown user-owned entries
17. runtime request/queue/control growth is explicitly bounded
18. unknown Codex builds are not silently release-certified
19. release packaging fails closed without exact validation evidence
20. native `previous_response_id` values are never persisted by optimizer diagnostics and their presence is preserved across aging
21. provider usage observation never changes upstream response semantics

The complete invariant set is in [SCOPE.md](./SCOPE.md).

## Phase status

- **Phase 0 — Project contract and architecture:** complete
- **Phase 1 — Deterministic aging engine:** implemented, validation deferred
- **Phase 2 — Measurement/benchmark:** implemented, validation deferred
- **Phase 3 — Native Codex transport:** implemented, validation deferred
- **Phase 4 — Recovery/quality guardrails:** implemented, validation deferred
- **Phase 5 — macOS runtime/tray:** implemented, validation deferred
- **Phase 6 — CLI/doctor:** implemented, validation deferred
- **Phase 7 — Packaging/update/uninstall:** implemented, validation deferred
- **Phase 8 — Hardening/release gates:** implemented, validation deferred
- **Native aging P0–P6 remediation:** implemented, final repository/live validation deferred

Per project instruction, P0–P6 implementation was authored **without running tests, builds, cargo checks, linters, formatters, CI, live benchmarks, CLI/doctor smoke tests, package builds, release verification, or live Codex validation**. The collective repository test pass is intentionally deferred until P6 implementation is complete; live provider probes remain separately opt-in because they spend quota.

See [ROADMAP.md](./ROADMAP.md) and [docs/NATIVE_AGING_VALIDATION.md](./docs/NATIVE_AGING_VALIDATION.md).

## Engineering documents

- [SCOPE.md](./SCOPE.md) — authoritative product boundary and invariants
- [ROADMAP.md](./ROADMAP.md) — phased implementation plan and release gates
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — modular-monolith boundaries
- [docs/CODEX_TRANSPORT_CONTRACT.md](./docs/CODEX_TRANSPORT_CONTRACT.md) — native Codex integration contract
- [docs/NATIVE_AGING_VALIDATION.md](./docs/NATIVE_AGING_VALIDATION.md) — current P0–P6 native aging validation/remediation contract
- [docs/MEASUREMENT.md](./docs/MEASUREMENT.md) — byte/token/cache telemetry semantics
- [docs/RECOVERY.md](./docs/RECOVERY.md) — receipt/recovery evidence rules
- [docs/DESKTOP_RUNTIME.md](./docs/DESKTOP_RUNTIME.md) — macOS runtime/tray lifecycle contract
- [docs/CLI.md](./docs/CLI.md) — CLI/control-channel/doctor contract
- [docs/PACKAGING.md](./docs/PACKAGING.md) — packaging/update/uninstall contract
- [docs/HARDENING.md](./docs/HARDENING.md) — runtime bounds, compatibility, and fail-closed release contract
- [docs/UPSTREAM_REFERENCE.md](./docs/UPSTREAM_REFERENCE.md) — pinned initial and selectively reviewed current Codex Router aging behavior
- [AGENTS.md](./AGENTS.md) — implementation guardrails

## Attribution

TokenSaver is a separate, intentionally narrower project inspired by the open-source [Codex Router](https://github.com/duolahypercho/codex-router) project and its tool-result-aging work. Upstream behavior is studied selectively; routing/provider features are outside TokenSaver's mission.
