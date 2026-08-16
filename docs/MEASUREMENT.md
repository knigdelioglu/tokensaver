# Measurement and telemetry contract

TokenSaver measures context reduction without persisting original tool-result bodies.

## Metric semantics

- **tool results evaluated** — recognized tool-result items inspected by the aging engine, including protected or unsupported results.
- **tool results eligible** — textual results that are outside the hot frontier, strictly larger than the minimum byte threshold, and already consumed by a later model action.
- **tool results compacted** — eligible results whose deterministic receipt is smaller than the original payload and therefore produces a replacement decision.
- **largest tool result bytes** — largest exact UTF-8 byte size observed among textual tool results in the evaluated history.
- **bytes before** — exact UTF-8 bytes of results that were actually compacted.
- **bytes after** — exact UTF-8 bytes of the receipts replacing those results.
- **bytes saved** — `bytes before - bytes after`; this is directly measured and authoritative.
- **estimated tokens saved** — display-only estimate using the reference heuristic `round(bytes saved / 4)`. It is not provider billing data.

## Outcome states

Telemetry distinguishes:

- `Disabled` — token saving was explicitly disabled.
- `Bypassed` — application policy intentionally skipped aging, for example a future verified conversation-compaction path.
- `EvaluatedNoEligibleResult` — aging ran but every candidate was protected, unsupported, too small, or unconsumed.
- `EvaluatedNoSavings` — at least one result passed eligibility but its receipt was not smaller than the source.
- `Aged` — at least one historical tool result produced a replacement.

This distinction is required so tray/diagnostics can tell the difference between “TokenSaver is off” and “TokenSaver is working but this request had nothing worth compacting.”

## Provider usage

Provider-reported input-token and cached-input-token values, when naturally available, are stored separately from TokenSaver estimates.

Provider-reported values are authoritative for provider accounting. TokenSaver must not infer provider billing from byte savings.

## Privacy boundary

Routine telemetry models contain no field for original tool-result content, receipt previews, prompts, model responses, credentials, or tool-call arguments.

The application layer maps aging statistics to telemetry. The telemetry module does not import aging or transport internals.

## Time and session aggregation

Telemetry accepts an opaque local session identifier and an event timestamp. It can aggregate:

- all retained events,
- one session,
- a caller-defined time range.

Timezone policy is intentionally outside telemetry. A future runtime/tray layer supplies local-day boundaries when it needs a “Today” view.

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

Each report exposes the decision reason for every recognized result without exposing the result body. Current reasons are:

- aged,
- protected by frontier,
- unsupported output,
- at or below threshold,
- unconsumed,
- receipt not smaller.

The benchmark harness is offline and does not call a model provider.
