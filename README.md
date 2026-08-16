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

## Target flow

```text
Codex
  ↓
TokenSaver local transport
  ↓
inspect request history
  ↓
age only eligible historical tool results
  ↓
forward to the same native Codex/OpenAI upstream
  ↓
relay the response stream unchanged
  ↓
Codex
```

The user continues to use normal Codex. TokenSaver should otherwise stay out of the way.

## Core idea: tool-result aging

TokenSaver detects historical tool results that are safe candidates for compaction and replaces only the model-visible historical copy with a deterministic receipt.

A candidate is compacted only when all required safety conditions are satisfied:

- only textual tool results are eligible
- the model must already have acted after seeing the result
- small results stay untouched
- a configurable number of the newest tool results stay byte-for-byte intact
- mixed/image-bearing results stay untouched
- unknown/ambiguous structures stay untouched
- compaction must never make a result larger

The initial reference policy is:

- minimum result size: **32 KiB**
- protected newest-result frontier: **4 results**
- head preview: approximately **1024 code units**
- tail preview: approximately **1024 code units**
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
  - omitted-middle marker
  - bounded tail preview
        │
        ▼
smaller context on later requests
```

## Product boundary

TokenSaver is **not** a model router and will not become a general Codex replacement.

It does not aim to provide:

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

A minimal tray/menu-bar application **is** part of the product because it answers whether TokenSaver is connected and whether it is actually saving context. The tray is restricted to TokenSaver operation/observability; it is not a model/provider management surface.

See [SCOPE.md](./SCOPE.md) for the authoritative product boundary.

## Architecture

TokenSaver is implemented as a **modular monolith**.

Current core layout:

```text
src/
├── application/
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

Cross-module use cases are coordinated through the application layer. Internal modules are not exposed as an accidental public library API.

See [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) for module ownership and dependency rules.

## Native Codex integration contract

TokenSaver's future Codex transport must preserve native Codex behavior while inserting aging at one narrow point in the request path.

Key requirements include:

- keep Codex's existing account/model experience
- change only the minimum TokenSaver-owned Codex configuration
- snapshot and restore those changes exactly
- use Codex's existing native authentication path rather than requiring a separate OpenAI API key
- support the transport details required by the supported Codex build
- bypass explicit conversation compaction so its summarizer can read original history
- relay responses without semantic transformation
- provide a hard OFF mode with no aging rewrite

The detailed contract is frozen in [docs/CODEX_TRANSPORT_CONTRACT.md](./docs/CODEX_TRANSPORT_CONTRACT.md).

## Design principles

### Preserve recent context

The newest tool results are hot context and remain exact.

### Compact only consumed results

A result must not be shortened before the model has acted after receiving it.

### Deterministic output

The same source result and policy should produce the same compact receipt. This keeps behavior auditable and reduces unnecessary prompt-prefix churn.

### Fail original

If TokenSaver cannot confidently classify or transform an item, the original content passes through unchanged.

### Never expand context

If the receipt is not smaller than the source result, keep the source result.

### Measure truthfully

TokenSaver distinguishes:

- directly measured bytes saved
- estimated tokens saved
- provider-reported token/cache telemetry when naturally available

Estimated values must remain labeled as estimates.

### No hidden semantic summarization

The initial mechanism uses deterministic structural compaction rather than another LLM to summarize tool output.

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

The complete invariant set lives in [SCOPE.md](./SCOPE.md).

## User-facing target

The macOS product will eventually expose a small menu-bar surface such as:

```text
TokenSaver
──────────────
Status            Active
Codex             Connected
Token Saving      On

This session
Saved             ~184K tokens
Compacted         12 results

Last optimization
84 KB → 3 KB
```

Counters must be backed by runtime telemetry rather than UI toggle state.

## Phase status

### Phase 0 — Project contract and architecture: complete

Phase 0 establishes:

- product scope and non-goals
- modular-monolith code skeleton
- module ownership/dependency rules
- architecture-contract tests for critical forbidden dependencies
- native Codex transport contract
- pinned Codex Router aging reference
- engineering guardrails for future implementation

No token-result transformation is claimed yet. The actual deterministic aging engine begins in **Phase 1**.

See [ROADMAP.md](./ROADMAP.md) for the complete implementation sequence.

## Engineering documents

- [SCOPE.md](./SCOPE.md) — authoritative product boundary and invariants
- [ROADMAP.md](./ROADMAP.md) — phased implementation plan and release gates
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — modular-monolith boundaries
- [docs/CODEX_TRANSPORT_CONTRACT.md](./docs/CODEX_TRANSPORT_CONTRACT.md) — native Codex integration contract
- [docs/UPSTREAM_REFERENCE.md](./docs/UPSTREAM_REFERENCE.md) — pinned Codex Router behavior adopted/rejected
- [AGENTS.md](./AGENTS.md) — repository implementation guardrails

## Attribution

TokenSaver is a separate, intentionally narrower project inspired by the open-source [Codex Router](https://github.com/duolahypercho/codex-router) project and its tool-result-aging work. Upstream behavior is studied selectively; routing/provider features are outside TokenSaver's mission.
