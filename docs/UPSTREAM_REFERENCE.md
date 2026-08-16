# Upstream Reference — Codex Router Tool-Result Aging

## Reference point

TokenSaver's initial token-saving behavior is derived from the tool-result aging implementation in:

- repository: `duolahypercho/codex-router`
- release: `v0.4.0-beta.4`

This reference is pinned so later development can distinguish intentional TokenSaver behavior from unrelated changes in Codex Router.

## Relevant upstream files

The initial implementation study identified these files as the core reference set:

- `src/tool-result-aging.mjs` — aging algorithm
- `src/router.mjs` — request-path insertion and compaction bypass
- `src/tool-result-aging-state.mjs` — enable/disable state
- `src/usage-events.mjs` — measured byte savings and estimated tokens
- `test/tool-result-aging.test.mjs` — preservation/safety cases
- `docs/HOW-IT-WORKS.md` — Codex transport context

## Behavior TokenSaver adopts as its initial aging policy

### Eligibility floor

Only textual tool results larger than **32 KiB** are initially eligible.

### Protected frontier

The newest **4 tool-result items** remain exact.

### Consumed-result requirement

A large old result is not enough. There must be evidence that the model acted after receiving it.

The reference treats later model-authored activity such as an assistant message, reasoning item, or tool call as evidence. A later tool result by itself does not prove the earlier result was consumed.

### Text-only safety boundary

Mixed/image-bearing or otherwise unsupported output shapes remain exact.

### Deterministic receipt

The initial receipt retains:

- original UTF-8 byte length
- SHA-256 digest
- bounded beginning preview
- omitted-middle marker
- bounded ending preview
- call/result structure required by the protocol

The initial preview target is approximately 1024 code units from each end, with Unicode-safe boundaries.

### Never expand context

If the replacement would be equal to or larger than the original result, the original remains unchanged.

### Preserve call/result pairing

Aging changes the model-visible result payload, not the structural identity of the tool interaction.

### Fail original

Unknown, malformed, ambiguous, or unsupported shapes are not guessed into a compact representation.

### Conversation-compaction bypass

Explicit Codex conversation compaction must read the original history rather than already-aged receipts.

## Behavior TokenSaver does not adopt

TokenSaver does not inherit Codex Router's broader product responsibilities, including:

- external model routing
- provider catalogs
- LiteLLM translation
- provider API-key/OAuth management
- external-model aliases
- login-free provider behavior
- vision bridges
- model curation
- DeepSeek Harness integration
- subagent model routing/orchestration
- provider quota dashboards
- model speed analytics

If an upstream change is not directly required for safe token/context reduction or transparent native Codex integration, it is not automatically relevant to TokenSaver.

## Native aging default

The reference release treated native GPT aging as an opt-in behavior and its surrounding releases changed default/experimental status while the feature was being validated.

TokenSaver therefore does **not** infer its final shipping default solely from upstream. Enablement default is a TokenSaver release decision gated by Phase 4 quality validation and release evidence.

## Metrics interpretation

The reference implementation measures actual serialized **bytes saved** and derives approximate token savings from a bytes-per-token heuristic when authoritative token counts are unavailable.

TokenSaver adopts the same truthfulness rule:

- bytes saved can be measured directly
- estimated tokens must be labeled as estimates
- provider-reported token/cache telemetry, when available, is authoritative for those provider metrics

## Upstream update rule

Future Codex Router releases may improve aging. TokenSaver may study and selectively port such changes only when they pass all of these checks:

1. The change directly improves token/context reduction, correctness, recovery, measurement, or required native Codex compatibility.
2. It fits TokenSaver's modular-monolith boundaries.
3. It does not introduce provider/model-routing scope.
4. It preserves fail-original and hard pass-through guarantees.
5. It receives TokenSaver-specific tests rather than being copied without verification.
