# TokenSaver Engineering Rules

These rules apply to implementation work in this repository.

## Authority order

For product and architecture decisions, use this order:

1. `SCOPE.md`
2. `docs/ARCHITECTURE.md`
3. `docs/CODEX_TRANSPORT_CONTRACT.md`
4. `ROADMAP.md`
5. `README.md`

If documents disagree, stop the conflicting implementation and reconcile the documents before expanding behavior.

## Product boundary

TokenSaver is a context optimizer for native Codex traffic. It is not a model/provider router.

Do not add, unless scope is explicitly revised:

- provider catalogs
- model aliases or model switching
- LiteLLM/provider protocol translation
- provider API-key management
- OAuth login systems
- subagent orchestration
- MCP hosting
- vision/OCR bridges
- unrelated Codex configuration management

## Modular monolith rule

Keep the module boundaries defined in `docs/ARCHITECTURE.md`.

Most importantly:

- `aging` must not depend on Codex, transport, persistence, telemetry, runtime, or UI types
- UI/CLI must call application services, not module storage
- telemetry consumes explicit metrics, not transport internals
- Codex configuration integration must not implement aging policy
- `shared` must not become a dumping ground for domain logic

Do not make internal modules public merely to avoid designing a proper application interface.

## Aging safety rules

When aging is implemented:

- unconsumed results remain exact
- protected recent results remain exact
- unsupported/mixed/image-bearing results remain exact
- small results remain exact
- structural call/result identity is preserved
- deterministic receipts are byte-stable for the same input/policy
- a replacement must never be larger than the source
- uncertainty means preserve the original

## Transport safety rules

Transport code must follow `docs/CODEX_TRANSPORT_CONTRACT.md`.

In particular:

- preserve native Codex model/account behavior
- change only TokenSaver-owned configuration
- keep connect/disconnect reversible
- bypass explicit conversation compaction
- do not log credentials
- do not expose a generic unauthenticated upstream proxy
- hard OFF mode must perform no aging rewrite

## Test execution policy

During the current implementation sequence, **do not run tests, builds, linters, formatters, CI workflows, or validation commands unless the user explicitly asks for the final test/validation pass**.

This does not prohibit writing test sources alongside the phase that introduces behavior. Tests should be authored now and executed only when the user later requests the final validation run.

Do not mark an unexecuted test suite as passing. Distinguish implementation completion from validation completion in reports and roadmap status.

## Implementation discipline

- Implement only behavior owned by the current roadmap phase unless a prerequisite is unavoidable.
- Do not leave production TODO/placeholder behavior that pretends a feature is implemented.
- Prefer explicit typed boundaries over shared mutable state.
- Keep transformation functions deterministic where practical.
- Keep sensitive content out of routine logs and fixtures.
- Add tests with the phase that introduces behavior, but follow the deferred test-execution policy above.
- When a phase's implementation is complete but tests are intentionally deferred, record that distinction rather than claiming validation passed.

## Upstream usage

Codex Router is a reference, not a dependency contract.

When porting an upstream idea:

- identify the exact source behavior
- keep attribution
- adapt it to TokenSaver's narrow scope
- add TokenSaver-specific tests
- do not import unrelated router/provider machinery
