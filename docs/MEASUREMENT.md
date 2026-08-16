# Measurement and telemetry contract

TokenSaver measures context reduction without persisting original tool-result bodies.

## Metric semantics

- **tool results evaluated** — recognized tool-result items inspected by the aging engine, including protected or unsupported results.
- **tool results eligible** — textual results that are outside the hot frontier, strictly larger than the minimum byte threshold, and already consumed by a later model action.
- **tool results compacted** — eligible results whose deterministic receipt is smaller than the original payload and therefore produces a replacement decision.
- **largest tool result bytes** — largest exact UTF-8 byte size observed among textual tool results in the request history. Request-shape inspection can populate this even while saving is off.
- **textual tool-result bytes seen** — total UTF-8 bytes of supported textual tool-result payloads observed at the Responses boundary before any rewrite.
- **bytes before** — exact UTF-8 bytes of results that were actually compacted.
- **bytes after** — exact UTF-8 bytes of the receipts replacing those results.
- **bytes saved** — `bytes before - bytes after`; this is directly measured and authoritative.
- **estimated tokens saved** — display-only estimate using the reference heuristic `round(bytes saved / 4)`. It is not provider billing data.

## Skip-reason metrics

Aging emits deterministic content-free reasons for results that were not replaced:

- protected frontier
- unsupported/mixed output
- at or below threshold
- unconsumed
- receipt not smaller

The detailed persisted CLI surface is `tokensaver diagnostics`.

## Native request-shape diagnostics

For ordinary Responses requests TokenSaver may retain aggregate counters for:

- request had / did not have `previous_response_id`
- `previous_response_id` was preserved
- aging pass ran
- input item count
- function/custom tool-result item count
- textual tool-result bytes and largest textual result

Only the **presence** of `previous_response_id` is observed. Its value is never put into TokenSaver telemetry.

These counters exist to diagnose cases where a long Codex task appears expensive but no compactable historical result reaches the optimizer.

## Outcome states

Telemetry distinguishes:

- `Disabled` — token saving was explicitly disabled.
- `Bypassed` — application policy intentionally skipped aging, including explicit native conversation compaction.
- `NativePassthrough` — a finite allow-listed native non-Responses route passed through unchanged.
- `FailOriginal` — Responses inspection/rewrite could not be proven safe, so the original encoded request was forwarded.
- `EvaluatedNoEligibleResult` — aging ran but every candidate was protected, unsupported, too small, or unconsumed.
- `EvaluatedNoSavings` — at least one result passed eligibility but its receipt was not smaller than the source.
- `Aged` — at least one historical tool result produced a replacement.

This distinction is required so tray/diagnostics can tell the difference between “TokenSaver is off” and “TokenSaver is working but this request had nothing worth compacting.”

## Provider usage

Provider-reported usage, when naturally available in the unchanged upstream response, is observed separately from TokenSaver estimates:

- input tokens
- cached input tokens
- output tokens

Recognized compatible field families include:

- `input_tokens` / `prompt_tokens`
- `output_tokens` / `completion_tokens`
- `input_tokens_details.cached_tokens`
- `prompt_tokens_details.cached_tokens`
- `prompt_cache_hit_tokens`

For SSE Responses the transport parses bounded `data:` lines side-band. For non-streaming JSON it uses a bounded capture. Failure to parse usage means provider usage is unavailable; it never changes or fails inference.

Provider-reported values are authoritative for provider accounting. TokenSaver must not infer provider billing from byte savings.

## Cache comparison

Ordinary Responses provider usage is split into:

- **aged cache** — request actually compacted at least one historical result
- **unaged cache** — ordinary Responses request did not compact a result

Conversation-compaction and unrelated native passthrough traffic are excluded from the aged-vs-unaged cache comparison.

Cache rate is computed from aggregate provider tokens:

```text
cached input tokens / input tokens
```

The report must include sample counts. A percentage without an aged/unaged sample count is not sufficient release evidence.

## Privacy boundary

Routine telemetry models contain no field for original tool-result content, receipt previews, prompts, model responses, credentials, account IDs, response IDs, capability secrets, or tool-call arguments.

The application layer maps aging/transport statistics to telemetry. The telemetry module does not import aging or transport internals.

## Time and session aggregation

Telemetry accepts an opaque local session identifier and an event timestamp. It can aggregate:

- all retained events,
- one session,
- a caller-defined time range,
- bounded persisted local-day buckets.

The runtime supplies local-day keys for “Today” views.

## Offline benchmark fixtures

The built-in benchmark suite includes synthetic representative histories for:

- test logs,
- build logs,
- large diffs,
- repository search output,
- large file reads,
- many medium outputs,
- mixed/unsupported output,
- unconsumed history.

Each report exposes the decision reason for every recognized result without exposing the result body.

The benchmark harness is offline and does not call a model provider.

## Explicit live evidence

Live evidence is opt-in and separate from normal test execution because it can consume provider/account quota:

- `scripts/live-token-ab.py` — identical OFF/ON request, provider-reported input-token delta
- `scripts/live-aging-quality.py` — omitted-middle recovery/hallucination guard
- `scripts/cache-evidence.py` — numeric aged/unaged cache export
- `scripts/verify-aging-release.py` — release evidence gate over the three artifacts

See `docs/NATIVE_AGING_VALIDATION.md`.
