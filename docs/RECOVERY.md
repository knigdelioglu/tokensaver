# TokenSaver Recovery Contract

## Purpose

Tool-result aging deliberately removes the middle of old, already-consumed textual tool results from the model-visible historical copy. Recovery therefore has one non-negotiable rule:

> **Missing bytes are unknown until an exact source is obtained and verified.**

TokenSaver must never reconstruct, summarize, interpolate, or guess omitted content and then present it as exact recovery.

## Receipt evidence

Every newly generated TokenSaver receipt carries:

- original UTF-8 byte length
- SHA-256 of the complete original textual result
- bounded verbatim head preview
- bounded verbatim tail preview
- exact byte lengths of the two preview regions
- an explicit statement that the omitted middle is not present

The machine-readable metadata line is:

```text
[tokensaver-receipt:v1 original_bytes=<n> sha256=<hex> head_bytes=<n> tail_bytes=<n>]
```

Human-readable receipt text and machine-readable evidence describe the same source identity.

## Evidence boundary

The receipt proves only:

1. the displayed head bytes are verbatim from the original beginning
2. the displayed tail bytes are verbatim from the original end
3. the original result had the recorded byte length and SHA-256 at aging time

The receipt does **not** prove the contents of the omitted middle.

A later answer may use a fact visible in the head/tail as receipt evidence. A fact that exists only in the omitted middle requires exact recovery before it can be treated as known from that old result.

## Exact candidate verification

An externally recovered candidate is accepted as exact only when both are true:

```text
candidate UTF-8 byte length == receipt original_bytes
candidate SHA-256          == receipt sha256
```

A same-length but modified value is rejected. A same-content value with a different byte representation is also rejected because exact recovery means byte-identical textual content.

## How exact source may be obtained

TokenSaver itself does not execute Codex tools and does not expose a hidden model-triggerable result store.

When exact omitted content is required, the normal Codex workflow must obtain the source again. For example, the preceding tool call may be repeated with the same arguments **only when repeating that operation is safe**.

This safety qualification matters because some shell/tool calls can have side effects. A receipt must never instruct the model to blindly replay an arbitrary historical operation.

If the original operation is not safe to repeat, the user/agent must obtain the exact source through another trusted normal workflow.

## No broad exact-result vault in MVP

Phase 4 intentionally does not persist complete original tool-result bodies merely to support recovery.

Reasons:

- it would create a second sensitive-content store
- it would expand file-permission and retention requirements
- it would create a model-access control surface unrelated to core token reduction
- the normal Codex tool path can often obtain exact data when truly needed

A future bounded owner-local exact-result cache is allowed only through a separate architecture/privacy decision and must remain optional.

## Application recovery API

`src/application/recovery.rs` exposes two explicit operations:

- assess a receipt as either preview-evidence use or exact-content need
- verify an externally recovered candidate against receipt identity

It deliberately does not infer recovery need from arbitrary model text.

Outcomes distinguish:

- `ReceiptEvidenceAvailable`
- `ExactSourceRequired`
- `VerifiedExact`
- `Rejected`

## Quality harness

`src/application/quality.rs` provides deterministic fixtures for the final validation pass:

- head/middle/tail evidence boundary
- many aged results in one history
- a consumed old result separated from later model activity by a long history distance

The harness can prove structural receipt properties and digest verification. It cannot prove LLM task quality.

## Deferred live/A-B validation

The following remain intentionally unexecuted until the user requests the final validation pass:

- whether Codex correctly reasons from head/tail receipts in realistic tasks
- whether it safely requests/repeats tools when omitted-middle content is needed
- task success with aging ON vs OFF
- tool-call correctness ON vs OFF
- prompt-cache/input-token impact
- latency overhead
- aging plus native conversation compaction in live sessions

No implementation-phase result should be described as having passed these quality gates before they are actually run.
