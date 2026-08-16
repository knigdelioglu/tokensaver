# TokenSaver

TokenSaver is a focused context-optimization layer for coding agents such as Codex. Its purpose is deliberately narrow: **reduce repeated input-token usage caused by large historical tool results without changing the agent's task, model, provider, or normal workflow.**

The project is inspired by the tool-result aging mechanism used by [duolahypercho/codex-router](https://github.com/duolahypercho/codex-router), especially the behavior present around `v0.4.0-beta.4`. TokenSaver is not intended to reproduce Codex Router. It extracts the token-saving idea into a small, independent application.

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

## Core idea: tool-result aging

TokenSaver will detect old tool results that are safe candidates for compaction and replace the model-visible copy with a deterministic receipt.

A candidate is compacted only when all required safety conditions are satisfied. The initial policy follows these principles:

- only textual tool results are eligible
- the model must already have acted after seeing the result
- small results stay untouched
- a configurable number of the newest tool results stay byte-for-byte intact
- mixed/image-bearing results stay untouched
- compaction must never make a result larger

The initial reference policy is:

- minimum result size: **32 KiB**
- protected newest-result frontier: **4 results**
- retained preview: bounded head + bounded tail
- receipt identity: original byte length + SHA-256 digest

Conceptually:

```text
large historical tool result
        |
        | model already consumed it
        v
safety / eligibility checks
        |
        v
deterministic compact receipt
  - original size
  - SHA-256
  - bounded head preview
  - omitted-middle marker
  - bounded tail preview
        |
        v
smaller context on subsequent requests
```

## What TokenSaver does not do

TokenSaver is **not** a model router and will not become a general Codex replacement.

The project does not aim to provide:

- model/provider routing
- API-key or subscription management
- model catalogs or model pickers
- multi-agent orchestration
- vision bridges
- desktop tray features unrelated to token savings
- quota management
- provider retry logic
- prompt rewriting unrelated to context reduction
- generic conversation summarization by another LLM

See [SCOPE.md](./SCOPE.md) for the project boundary.

## Design principles

### 1. Preserve recent context

The newest tool results are considered hot context and remain exact.

### 2. Compact only consumed results

A result must not be shortened before the model has had an opportunity to use it.

### 3. Deterministic output

The same source result and policy should produce the same compact receipt. This makes behavior auditable and helps avoid unnecessary prompt-cache churn.

### 4. Fail original

If TokenSaver cannot confidently classify or compact an item, the original result should pass through unchanged.

### 5. Measure real savings

TokenSaver should record at least:

- results evaluated
- results compacted
- bytes before
- bytes after
- bytes saved
- estimated tokens saved

When an upstream provider reports actual token usage, provider-reported counts are authoritative over estimates.

### 6. No hidden semantic summarization

The first implementation should use deterministic structural compaction rather than asking another LLM to summarize tool output. This keeps the transformation cheap, reproducible, and inspectable.

## Planned architecture

The implementation will be kept small and separated into a few responsibilities:

```text
agent/client request
      |
      v
request adapter
      |
      v
tool-result classifier
      |
      v
aging policy
      |
      +---- ineligible ----> original result
      |
      v
receipt generator
      |
      v
optimized request
      |
      v
original upstream endpoint
```

The core aging engine should remain independent from any one client transport so that adapters can later be added for Codex or other compatible agents without duplicating the optimization logic.

## Safety invariants for v0.1

The first usable version should guarantee:

1. A tool result that the model has not consumed is never compacted.
2. The newest protected results are never compacted.
3. Non-text or mixed-media outputs are never compacted.
4. Results at or below the configured minimum size are never compacted.
5. Every compacted receipt includes a deterministic digest of the original content.
6. If the compact representation is not smaller, the original is retained.
7. Disabling TokenSaver results in pass-through behavior.
8. Optimization telemetry never contains the original large result body.

## Status

The repository is currently in the **design/bootstrap** stage. The immediate goal is to implement and validate the smallest reliable tool-result-aging engine before adding convenience UI or broader integrations.

See [ROADMAP.md](./ROADMAP.md) for the planned sequence.

## Attribution

The project concept is inspired by the open-source [Codex Router](https://github.com/duolahypercho/codex-router) project and its tool-result-aging work. TokenSaver is a separate, intentionally narrower project focused only on context/token reduction.
