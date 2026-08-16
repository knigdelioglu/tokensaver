# TokenSaver Scope

## Mission

TokenSaver exists to reduce repeated input-token consumption in coding-agent conversations by compacting historical tool results after they have been safely consumed.

The product boundary is intentionally strict:

> TokenSaver optimizes context. It does not choose models, route providers, manage accounts, or orchestrate agents.

## In scope

### 1. Tool-result aging

TokenSaver may identify historical tool outputs that meet a conservative eligibility policy and replace only the model-visible historical copy with a smaller deterministic receipt.

The initial eligible class is:

- textual tool output
- larger than the configured minimum byte threshold
- already followed by evidence that the model acted after receiving it
- outside the protected newest-result frontier
- safely representable as deterministic text

### 2. Deterministic receipts

A compacted result may retain bounded evidence needed to identify and reason about the omitted result, including:

- original byte size
- SHA-256 digest
- bounded beginning preview
- bounded ending preview
- explicit omitted-middle marker
- structural call/result identifiers required by the protocol

The same source result under the same policy should produce the same receipt.

### 3. Hot/cold context policy

TokenSaver may distinguish recent tool results from older tool results.

Recent results remain exact. Older results become eligible only after all other safety checks pass.

The initial default frontier is the newest four tool results.

### 4. Configurable optimization policy

TokenSaver may expose settings directly related to context optimization, such as:

- minimum eligible byte size
- protected frontier size
- preview size
- optimization enabled/disabled state

Defaults must be conservative.

### 5. Transparent request integration

TokenSaver may provide the minimum local transport/proxy layer needed to receive a coding-agent request, optimize eligible historical tool results, and forward the request to the same intended upstream.

This layer must not become a general model router.

### 6. Savings telemetry

TokenSaver may record non-content optimization metrics, including:

- requests evaluated
- tool results evaluated
- tool results compacted
- largest evaluated result
- bytes before
- bytes after
- bytes saved
- estimated tokens saved
- provider-reported token/cache metrics when naturally available on the proxied request path

Telemetry must not persist original tool-result bodies by default.

### 7. Offline measurement

TokenSaver may include benchmark and fixture tooling that evaluates the aging policy without requiring paid provider calls.

### 8. Safety and regression testing

Tests for preservation, eligibility, Unicode safety, deterministic hashing, pass-through behavior, and protocol structure are part of the core product.

## Required invariants

These rules are stronger than convenience or token savings and must not be bypassed silently.

### INV-1 — Unconsumed results remain exact

If the model has not yet acted after a tool result, TokenSaver must not compact it.

### INV-2 — Protected recent results remain exact

Tool results inside the configured hot frontier must not be compacted.

### INV-3 — Unsupported output types remain exact

Image-bearing, mixed-media, binary, malformed, or otherwise ambiguous outputs must pass through unchanged unless a future format has its own explicitly safe policy.

### INV-4 — Small results remain exact

Results at or below the configured minimum threshold must pass through unchanged.

### INV-5 — Never expand context

If the compact receipt is not smaller than the original result, keep the original.

### INV-6 — Stable identity

A compacted result must retain a deterministic identity derived from the exact original content, initially SHA-256 plus original byte length.

### INV-7 — Preserve protocol structure

Compaction must preserve call/result pairing and all structural fields required by the client/upstream protocol.

### INV-8 — Fail original

If classification, transformation, or validation is uncertain or fails, TokenSaver must prefer the original request content over an invented or partial representation.

### INV-9 — Hard off means pass-through

There must be a reliable mode in which TokenSaver performs no context rewriting.

### INV-10 — No original content in routine telemetry

Savings logs and metrics must not contain full original tool-result bodies.

## Out of scope

The following features are explicitly outside TokenSaver's product mission unless the scope is intentionally revised in a future documented decision.

### Model and provider routing

Out of scope:

- choosing a different model
- model aliases
- model catalogs
- provider failover
- provider selection
- gateway-model translation
- API-provider registries

TokenSaver forwards to the user's already intended upstream.

### Credential and subscription management

Out of scope:

- storing provider API keys as a product feature
- OAuth login systems
- ChatGPT account/session discovery
- subscription switching
- quota/reset management
- billing dashboards unrelated to measured token savings

If a transport requires authentication headers, TokenSaver may relay them without becoming their owner.

### Multi-agent orchestration

Out of scope:

- spawning subagents
- selecting subagent models
- agent registries
- task delegation
- collaboration runtimes

### General context rewriting

Out of scope for the initial product:

- arbitrary removal of user or assistant messages
- rewriting system prompts
- rewriting user prompts
- rewriting assistant reasoning
- LLM-generated summaries of the conversation
- semantic compression of arbitrary prose

TokenSaver starts with tool-result aging because it has a narrow, auditable eligibility boundary.

### General-purpose response transformation

Out of scope:

- modifying model answers
- changing tool-call arguments
- repairing arbitrary upstream schemas
- converting one provider protocol to another as a product feature
- response quality enhancement unrelated to context reduction

Minimal protocol adaptation is acceptable only when strictly required to preserve the same request semantics while applying TokenSaver.

### Vision and media processing

Out of scope:

- OCR
- image understanding
- vision-model bridges
- image compression for model reasoning

Mixed/image-bearing tool results pass through unchanged in the initial implementation.

### Generic observability platform

Out of scope:

- full provider analytics
- model speed leaderboards
- unrelated latency dashboards
- quota dashboards
- system tray status unrelated to optimization

TokenSaver observability should answer: what was evaluated, what was compacted, why, and how much context was saved?

### Tool execution platform

TokenSaver does not execute arbitrary coding tools on behalf of the agent merely to become a general tool host.

Any future exact-result recovery mechanism must remain narrowly tied to safely recovering content omitted by TokenSaver.

## MVP definition

The MVP is complete when TokenSaver can do all of the following:

1. Accept a representative Codex-style request/history.
2. Detect eligible historical textual tool results.
3. Compact them using a deterministic receipt.
4. Preserve ineligible and recent results exactly.
5. Forward the optimized request to the same intended upstream path.
6. Run in a hard pass-through mode.
7. Report byte and estimated-token savings without logging original result bodies.
8. Pass automated tests for every required invariant above.

The MVP does **not** require a desktop GUI, tray application, provider catalog, model selector, multi-agent support, or account integration.

## Scope-change rule

Before adding a substantial feature, evaluate it against this test:

1. Does it directly reduce repeated context/token usage, improve the correctness of that reduction, measure it, or safely integrate it?
2. Can it be implemented without turning TokenSaver into a general router or agent platform?
3. Does it preserve the fail-original and pass-through guarantees?

If the answer to any of these is no, the feature should be rejected or moved to a separate project.

## Upstream relationship

TokenSaver is inspired by the tool-result-aging work in `duolahypercho/codex-router`, but Codex Router has a much broader mission. TokenSaver should study relevant upstream improvements while selectively adopting only mechanisms that fit this scope.

Upstream changes related to routing, provider support, account/session management, desktop UI, harness integrations, model catalogs, vision, or multi-agent behavior are not automatically relevant to TokenSaver.
