# TokenSaver Roadmap

This roadmap keeps TokenSaver intentionally narrow. Every milestone must directly improve token/context reduction, correctness, observability, or safe integration of that mechanism.

## Phase 0 — Project contract

Status: **in progress**

- Define project scope and non-goals.
- Document the reference tool-result-aging policy.
- Establish safety invariants.
- Keep upstream inspiration and attribution explicit.
- Decide the smallest transport surface needed for Codex integration.

Exit criteria:

- `README.md`, `SCOPE.md`, and this roadmap agree on the same project boundary.
- No routing/provider-management features are part of the MVP.

## Phase 1 — Deterministic aging engine

Goal: implement the optimization as a pure, testable core.

Planned work:

- Parse a request history into recognizable tool calls, tool outputs, assistant messages, and reasoning/action items.
- Identify textual tool-result items.
- Track whether the model has acted after a result.
- Protect the newest configurable result frontier.
- Apply a configurable minimum-size threshold.
- Generate deterministic compact receipts containing:
  - original UTF-8 byte length
  - SHA-256 digest
  - bounded head preview
  - omitted-middle marker
  - bounded tail preview
- Preserve call/result pairing metadata.
- Return the original item whenever compaction would not reduce size.
- Implement a hard pass-through/off mode.

Initial defaults:

- minimum result size: `32 KiB`
- protected frontier: `4`
- head preview: approximately `1024` code units
- tail preview: approximately `1024` code units

Required tests:

- large consumed textual output is compacted
- unconsumed output remains exact
- newest four outputs remain exact
- small outputs remain exact
- mixed/image-bearing outputs remain exact
- Unicode surrogate boundaries are not corrupted
- digest is deterministic
- call IDs and required structural fields survive rewriting
- disabled mode is byte-preserving
- compact output is never larger than its source

Exit criteria:

- The aging engine is independent from any network proxy or client adapter.
- All safety invariants in `SCOPE.md` have automated tests.

## Phase 2 — Measurement and benchmark harness

Goal: prove that optimization saves context instead of merely changing payloads.

Planned work:

- Record per-request optimization statistics:
  - results evaluated
  - results compacted
  - largest result evaluated
  - bytes before
  - bytes after
  - bytes saved
  - estimated tokens saved
- Keep telemetry free of original result bodies.
- Add an offline benchmark command for captured/synthetic histories.
- Report per-history and cumulative savings.
- Separate estimated token savings from provider-reported token usage.
- Add representative fixtures:
  - test logs
  - build logs
  - large diffs
  - repository search output
  - large file reads
  - many medium-sized outputs

Exit criteria:

- A benchmark can show exactly why a result was or was not compacted.
- Reported byte savings are deterministic and reproducible.

## Phase 3 — Codex request integration

Goal: run the optimizer transparently between Codex and its existing upstream path without becoming a model router.

Planned work:

- Add a minimal local request adapter/proxy.
- Preserve model selection and upstream destination as supplied by Codex.
- Rewrite only eligible historical tool-result payloads.
- Leave unrelated request fields untouched.
- Exempt explicit conversation-compaction requests when the summarizer needs original history.
- Provide a kill switch that restores exact pass-through behavior.
- Confirm streamed responses are relayed without semantic transformation.
- Add integration tests comparing pass-through and optimized requests.

Exit criteria:

- Codex can operate normally through TokenSaver.
- With optimization disabled, the application behaves as a transparent pass-through.
- With optimization enabled, only eligible tool-result bodies differ.

## Phase 4 — Recovery and quality validation

Goal: reduce the risk that compacted history removes information the model later needs.

Planned work:

- Validate that receipts give the model enough evidence to recognize the original result.
- Define safe recovery behavior for exact omitted content.
- Test tasks that later ask for facts located only in an omitted middle section.
- Test repeated turns to confirm the same source produces a byte-stable receipt.
- Compare prompt-cache behavior where provider telemetry is available.
- Run A/B sessions with aging on and off.

Important constraint:

Recovery must not silently introduce a broad remote storage system or expose private original tool outputs through telemetry.

Exit criteria:

- No observed hallucinated recovery in the quality suite.
- Exact-content failures have a documented, explicit recovery path.

## Phase 5 — Configuration and small CLI

Goal: make the optimizer usable without expanding product scope.

Planned commands may include:

```text
tokensaver start
tokensaver stop
tokensaver status
tokensaver config show
tokensaver config set min-bytes ...
tokensaver config set frontier ...
tokensaver stats
```

Configuration should cover only optimization behavior and local integration necessities.

Exit criteria:

- Users can enable, disable, inspect, and measure TokenSaver without editing source files.

## Phase 6 — Hardening

Goal: make long-running use safe and predictable.

Planned work:

- Request-size and memory-pressure testing.
- Very large tool-output tests.
- Malformed input handling.
- Fail-original behavior on parser/classifier errors.
- Atomic local-state writes where state is required.
- Secure local file permissions for sensitive runtime state.
- Cross-platform path/process behavior if Windows/Linux support is added.
- Performance profiling to ensure optimization overhead is materially lower than the context it saves.

Exit criteria:

- Failure of TokenSaver does not corrupt the model-visible conversation.
- Optimization can be disabled immediately.
- No sensitive tool-result content is written to logs by default.

## Later ideas — only if they remain within scope

These are explicitly not MVP requirements:

- per-tool thresholds
- structured compaction for highly repetitive logs
- optional exact-result local retention with strict privacy controls
- adapters for other Responses-compatible coding agents
- tokenizer-aware estimates for selected models
- simple local dashboard for savings metrics

Any later feature must satisfy one question before being accepted:

> Does this directly improve safe context/token reduction or make that mechanism easier to operate?

If the answer is no, it belongs outside TokenSaver.
