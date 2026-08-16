# TokenSaver Scope

## Mission

TokenSaver exists to reduce repeated input-token consumption in coding-agent conversations by compacting historical tool results after they have been safely consumed.

The product boundary is intentionally strict:

> **TokenSaver optimizes context. It does not choose models, route providers, manage accounts, or orchestrate agents.**

## Architecture rule — modular monolith

TokenSaver is implemented as a **modular monolith**.

The application may run as one local product/runtime, but internal modules have explicit ownership and dependency boundaries. A module must not reach into another module's private state, persistence implementation, or internal types.

Module boundaries:

- **aging** — deterministic eligibility, receipts, receipt evidence/identity
- **transport** — Codex request/response transport, compression, streaming, request activity
- **codex integration** — reversible Codex configuration and drift/restoration state
- **telemetry** — content-free savings events, aggregation, numeric persistence
- **runtime** — process/service state and user runtime preferences
- **diagnostics** — health/doctor reporting
- **desktop/tray** — native menu-bar presentation and controls
- **application** — cross-module use-case composition

Strongest dependency rule:

> **The aging domain must remain transport-, Codex-, persistence-, telemetry-, and UI-agnostic.**

Conceptually:

```text
desktop / CLI
      ↓
application services
      ↓
 ┌────┼──────────────┐
 ↓    ↓              ↓
runtime telemetry  codex integration
                     ↓
                  transport
                     ↓
                   aging
```

Forbidden examples:

- `aging -> tray`
- `aging -> Codex configuration`
- `aging -> telemetry storage`
- `telemetry -> transport internals`
- `codex integration -> aging internals`
- desktop UI reading module persistence directly

`shared` may contain only genuinely cross-cutting low-level primitives such as filesystem safety and secret redaction. It must not become a domain dumping ground.

Physical process separation is not required. The MVP may run tray, transport, aging, telemetry, runtime, and configuration management in one process.

## In scope

### 1. Tool-result aging

TokenSaver may replace only the model-visible historical copy of a tool result when a conservative policy proves it eligible.

Initial eligible class:

- textual tool output
- larger than the configured minimum byte threshold
- followed by evidence that the model acted after receiving it
- outside the protected newest-result frontier
- safely representable as deterministic text

### 2. Deterministic receipts and evidence

A compacted result may retain bounded evidence including:

- original UTF-8 byte length
- SHA-256 digest
- bounded beginning preview
- bounded ending preview
- explicit omitted-middle marker
- machine-readable receipt version/preview lengths
- structural call/result identifiers required by protocol

Receipt evidence may be parsed and an externally recovered exact candidate may be verified against byte length + digest.

TokenSaver must not reconstruct omitted bytes from guesses.

### 3. Hot/cold context policy

Recent tool results remain exact. Older results become eligible only after all other safety checks pass.

Initial default frontier: newest **4** tool results.

### 4. Configurable optimization policy

TokenSaver may expose settings directly related to context optimization:

- minimum eligible byte size
- protected frontier size
- preview size
- optimization enabled/disabled state

Defaults must remain conservative.

### 5. Transparent native request integration

TokenSaver may provide the minimum local transport needed to receive supported Codex traffic, optimize ordinary Responses history, and forward requests to the same first-party upstream family.

This layer must not become a general model router or arbitrary forward proxy.

### 6. Native passthrough compatibility

TokenSaver may transparently relay native provider endpoints required for supported Codex operation when those endpoints share the overridden provider base URL.

Native passthrough payloads are not aging targets.

### 7. Savings telemetry

TokenSaver may record non-content metrics:

- requests evaluated
- tool results evaluated/eligible/compacted
- largest result
- bytes before/after/saved
- estimated tokens saved
- provider-reported token/cache metrics when naturally available
- session/day/all-time numeric aggregates
- latest optimization numeric metadata

Telemetry must not persist original tool-result bodies or receipt bodies.

### 8. Offline measurement and quality fixtures

TokenSaver may include deterministic fixtures for aging behavior, byte savings, receipt evidence boundaries, and exact-candidate identity verification without paid provider calls.

### 9. Safety and regression testing

Tests for preservation, eligibility, Unicode safety, hashing, pass-through behavior, protocol structure, recovery evidence, configuration restoration, lifecycle, and module-boundary enforcement are part of the product.

### 10. Minimal macOS tray/menu-bar control surface

The supported desktop build may provide a small TokenSaver-specific surface for:

- service/connection health
- request active/idle state
- saving on/off
- measured byte savings
- estimated token savings
- recent optimization activity
- Connect / Disconnect
- Start at Login
- safe Quit
- later diagnostics/doctor entry points

The tray is not a model/provider management surface.

### 11. Safe local lifecycle state

TokenSaver may persist bounded owner-local operational state required for correct lifecycle:

- reversible Codex config snapshot
- runtime preferences such as saving state / reconnect-on-launch intent
- numeric content-free savings aggregates

Normal process exit may be delayed/refused when immediate exit would knowingly strand Codex on a dead TokenSaver endpoint or interrupt an active Codex request.

## Required invariants

These rules are stronger than convenience or token savings.

### INV-1 — Unconsumed results remain exact

If the model has not acted after a tool result, TokenSaver must not compact it.

### INV-2 — Protected recent results remain exact

Tool results inside the configured hot frontier must not be compacted.

### INV-3 — Unsupported output types remain exact

Image-bearing, mixed-media, binary, malformed, or ambiguous outputs remain exact unless a future format receives its own explicitly safe policy.

### INV-4 — Small results remain exact

Results at or below the minimum threshold remain exact.

### INV-5 — Never expand context

If a receipt is not smaller than its source, keep the source.

### INV-6 — Stable identity

A compacted result retains deterministic exact-content identity, initially SHA-256 + original UTF-8 byte length.

### INV-7 — Preserve protocol structure

Compaction preserves call/result pairing and structural fields required by client/upstream protocols.

### INV-8 — Fail original

If classification, transformation, or validation is uncertain or fails, TokenSaver prefers original request content over an invented/partial representation.

### INV-9 — Hard off means no aging rewrite

There is a reliable mode in which TokenSaver performs no context rewriting.

### INV-10 — No original content in routine telemetry

Savings state/logs must not contain full original tool-result bodies or receipt bodies.

### INV-11 — Module boundaries are enforced

Cross-module behavior uses explicit application services/interfaces. The aging domain never acquires dependencies on Codex transport/configuration, UI, or telemetry persistence.

### INV-12 — Native conversation compaction sees original history

Explicit supported conversation-compaction requests bypass TokenSaver aging so native compaction is not summarizing TokenSaver receipts instead of the original history.

### INV-13 — Omitted content is never fabricated

Receipt head/tail evidence may be used as shown, but omitted middle bytes must never be reconstructed or presented as exact without source identity verification.

### INV-14 — Native passthrough is not an optimization surface

Verified native models/search/images/memory endpoints may pass through the local transport but do not enter the tool-result aging parser.

### INV-15 — Safe desktop detach

Normal Disconnect/Quit must not intentionally leave Codex configured to a TokenSaver endpoint that is no longer serving. Active request streams must not be cut merely for menu convenience.

### INV-16 — Capability secrets stay local

The caller capability must not enter routine telemetry or outward status text. Owner-only config/snapshot storage may contain it only because local routing/recovery requires it.

### INV-17 — UI state is not backend truth

Tray toggle/checkmark state must be derived from application/runtime evidence. It must not substitute for actual Codex config state, transport state, OS autostart state, or measured telemetry.

## Out of scope

### Model/provider routing

Out of scope:

- choosing different models/providers
- aliases/catalogs
- provider failover
- protocol translation as a product feature
- API-provider registries

TokenSaver forwards to the already intended first-party upstream path.

### Credential/subscription management

Out of scope:

- provider API-key storage as a product feature
- OAuth/account systems
- ChatGPT account/session discovery
- subscription switching
- quota/reset management
- unrelated billing dashboards

Transport may relay authentication headers without owning credentials.

### Multi-agent orchestration

Out of scope:

- spawning subagents
- selecting subagent models
- agent registries
- task delegation
- collaboration runtimes

### General context rewriting

Out of scope:

- arbitrary removal of messages
- system/user prompt rewriting
- assistant reasoning rewriting
- LLM-generated whole-conversation summaries
- semantic compression of arbitrary prose

### General-purpose response transformation

Out of scope:

- modifying model answers
- changing tool-call arguments
- arbitrary upstream schema repair
- provider-protocol conversion
- response quality enhancement unrelated to context reduction

### Vision/media processing

Out of scope:

- OCR
- image understanding
- vision bridges
- image compression for reasoning

Mixed/image-bearing tool results remain exact.

### Persistent exact-result vault in MVP

MVP does not keep a second persistent store of complete shell output, file reads, search output, or diffs solely for recovery.

A future bounded owner-local cache requires a separate privacy/architecture decision.

### Generic observability/dashboard platform

Out of scope:

- model speed leaderboards
- provider analytics unrelated to TokenSaver
- quota dashboards
- large management dashboards
- tray state unrelated to TokenSaver operation

### Tool execution platform

TokenSaver does not become a general coding-tool host. Any future exact-result recovery execution must remain narrowly tied to safely recovering content omitted by TokenSaver.

## MVP definition

The MVP is complete when TokenSaver can:

1. Accept real supported Codex traffic through local transport.
2. Detect eligible historical textual tool results.
3. Compact them with deterministic verifiable receipts.
4. Preserve ineligible/recent results exactly.
5. Forward ordinary/native traffic to the same intended first-party path.
6. Run hard pass-through for aging.
7. Bypass explicit native conversation compaction.
8. Report measured byte + estimated-token savings without persisting result content.
9. Preserve Codex model/account/MCP/skills/subagents/permissions/task state.
10. Safely connect/disconnect and restore TokenSaver-owned Codex configuration.
11. Provide a minimal macOS tray showing backend-derived connection/request/savings state.
12. Persist saving/reconnect intent and bounded numeric savings state.
13. Safely detach on normal Quit without cutting active request streams.
14. Pass automated and live validation for all required invariants.

The MVP does **not** require a provider catalog, model selector, external-model routing, multi-agent orchestration, account-management system, or full dashboard.

## Scope-change rule

Before adding a substantial feature, evaluate it against:

1. Does it directly reduce repeated context/token usage, improve correctness/recovery/measurement, or safely operate that mechanism?
2. Can it be implemented without turning TokenSaver into a general router or agent platform?
3. Does it preserve fail-original/pass-through/recovery truthfulness?
4. Does it preserve modular-monolith boundaries, or is an explicit architecture decision required?

If any answer is no, reject the feature or move it to a separate project.

## Upstream relationship

TokenSaver is inspired by tool-result-aging work in `duolahypercho/codex-router`, but Codex Router has a broader mission. TokenSaver studies relevant upstream improvements and selectively adopts only mechanisms that fit this scope.

Routing, provider support, account/session management, broad management UI, harness integrations, model catalogs, vision, and multi-agent behavior are not automatically relevant to TokenSaver.
