# Upstream Reference — Codex Router Tool-Result Aging

## Initial reference point

TokenSaver's initial token-saving behavior is derived from the tool-result aging implementation in:

- repository: `duolahypercho/codex-router`
- release: `v0.4.0-beta.4`

This reference remains pinned so later development can distinguish intentional TokenSaver behavior from unrelated changes in Codex Router.

## Current follow-up reference

A 2026-08 follow-up review also examined Codex Router's newer aging work, especially:

- `60100aa835fe3a0d6856b0b414223c84db672efa` — initial tool-result aging setting
- `fde723100d5af957ecd9ec45a240aa6b733c1318` — published live token benchmark evidence
- `64277304780b62976cb9d12dec5d6afcdea313e0` — native GPT aging plus provider token/cache telemetry
- `38b49fb8f44a9b81ed620df0c2c3dae1a0789431` — quality probe and removal of the experimental label
- `397f455b73a40f509e47be092e6f96cbaf5ffe96` — observability proving the aging pass ran even when it changed nothing

Relevant current files include:

- `src/tool-result-aging.mjs`
- `src/tool-result-aging-state.mjs`
- `src/router.mjs`
- `src/response-usage.mjs`
- `src/usage-events.mjs`
- `test/tool-result-aging.test.mjs`
- `scripts/live-test-tool-result-aging.mjs`
- `scripts/live-test-aging-quality.mjs`
- `docs/tool-result-aging-benchmark.md`

The follow-up is a selective compatibility/evidence review, not a new wholesale product baseline.

## Behavior TokenSaver adopts as its aging policy

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

The receipt retains:

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

## Current upstream evidence TokenSaver now mirrors structurally

The follow-up review found three valuable validation patterns that are directly relevant to TokenSaver without adopting router scope:

1. **Provider-reported live token A/B.** Codex Router published a controlled tool-heavy case where an identical request reported 22,071 input tokens with aging off and 2,991 with aging on. TokenSaver now ships its own explicit `scripts/live-token-ab.py` instead of treating byte estimates as proof of provider token reduction.
2. **Omitted-middle quality probe.** Codex Router tests whether a model honestly recovers/refuses missing exact content rather than inventing it. TokenSaver now ships `scripts/live-aging-quality.py` using the stronger TokenSaver receipt/recovery contract.
3. **Prompt-cache comparison.** Current upstream records cached-input telemetry for aged and unaged traffic. TokenSaver now observes provider usage side-band and aggregates aged-vs-unaged cache evidence.

These scripts/evidence paths are not automatically executed because live probes can consume provider/account quota.

## Intentional TokenSaver recovery hardening

TokenSaver does not copy the beta.4 or current recovery wording verbatim.

TokenSaver deliberately strengthens the receipt contract:

- a machine-readable `tokensaver-receipt:v1` identity line carries original byte length, SHA-256, and preview byte lengths
- the omitted middle is explicitly declared unavailable and must not be inferred
- replay of a historical tool call is suggested only when repeating that operation is safe
- an externally recovered candidate is accepted as exact only when UTF-8 byte length and SHA-256 both match the receipt
- TokenSaver does not create a broad persistent store of complete historical tool-result bodies in MVP

These are intentional safety/privacy extensions of the upstream idea, not attempts to preserve upstream output text byte-for-byte.

See `docs/RECOVERY.md` for the authoritative TokenSaver recovery contract.

## Intentional native `previous_response_id` divergence

Current Codex Router's native route removes `previous_response_id` on ordinary non-compaction turns because that router can construct and forward a stateless full-conversation request across a provider-routing boundary.

TokenSaver does **not** copy that behavior blindly.

TokenSaver forwards the built-in Codex/OpenAI provider to the same first-party upstream family. Removing native chaining without proof could change an incremental request's semantics. The TokenSaver P0/P1 contract is therefore:

- observe only whether `previous_response_id` exists, never its value
- preserve it on ordinary Responses turns
- confirm aging changed only approved historical tool-result outputs
- preserve exact chaining on conversation-compaction turns
- fail original if structural invariants cannot be proven

This divergence is intentional and documented in `docs/NATIVE_AGING_VALIDATION.md`.

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

Current Codex Router keeps tool-result aging off by default even after publishing validation evidence, because enabling it changes what the model sees mid-conversation.

TokenSaver now uses the same conservative **product default: OFF for a fresh install**. Existing persisted user choices are preserved and are not silently re-defaulted.

This is distinct from the pure aging domain's default policy, which remains enabled when directly invoked by deterministic tests/fixtures.

## Metrics interpretation

The reference implementation measures actual serialized **bytes saved** and derives approximate token savings from a bytes-per-token heuristic when authoritative token counts are unavailable.

TokenSaver adopts the same truthfulness rule:

- bytes saved can be measured directly
- estimated tokens must be labeled as estimates
- provider-reported token/cache/output telemetry, when available, is authoritative for those provider metrics
- provider usage is observed without semantically rewriting the response stream
- cache evidence is separated into aged and ordinary-unaged Responses buckets

## Upstream update rule

Future Codex Router releases may improve aging. TokenSaver may study and selectively port such changes only when they pass all of these checks:

1. The change directly improves token/context reduction, correctness, recovery, measurement, or required native Codex compatibility.
2. It fits TokenSaver's modular-monolith boundaries.
3. It does not introduce provider/model-routing scope.
4. It preserves fail-original and hard pass-through guarantees.
5. It receives TokenSaver-specific test sources rather than being copied without verification.
6. A provider-router transport assumption is not applied to native first-party TokenSaver traffic without direct evidence.
