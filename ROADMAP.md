# TokenSaver Roadmap

TokenSaver has one product goal: **reduce repeated input-token usage in Codex by compacting old, already-consumed tool results, while leaving the rest of Codex behavior unchanged.**

The project is intentionally not a model router. It does not add providers, replace Codex model selection, translate external model protocols, manage provider credentials, or orchestrate agents.

The target end state is:

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

The user should continue using Codex normally. TokenSaver should be visible primarily through a small tray/menu-bar application that proves it is connected and shows measured savings.

---

## Product acceptance target

TokenSaver is considered successful when all of the following are true:

1. A user can install and open TokenSaver without changing how they normally use Codex.
2. Codex continues to use its existing account, native model picker, MCP tools, skills, subagents, permissions, and task state.
3. TokenSaver transparently receives Codex Responses traffic through a local loopback transport.
4. With token saving enabled, only eligible historical tool-result bodies are rewritten.
5. With token saving disabled, TokenSaver behaves as an exact semantic pass-through.
6. Explicit Codex conversation-compaction requests are not aged before the summarizer reads the original history.
7. The tray shows whether Codex is connected and how many bytes / estimated tokens were saved.
8. Disabling or uninstalling TokenSaver restores the Codex configuration it changed.
9. TokenSaver never becomes a provider/model router.

A key integration invariant is:

> For the same Codex request, the optimized and pass-through payloads may differ only where the tool-result aging policy explicitly permits a historical tool-result body to change.

---

# Phase 0 — Project contract and upstream reference

**Goal:** lock the product boundary before implementation.

Planned work:

- Keep `README.md`, `SCOPE.md`, and this roadmap aligned.
- Document Codex Router's tool-result-aging behavior as the initial reference implementation.
- Record the safety invariants that TokenSaver must preserve.
- Keep attribution to the upstream inspiration explicit.
- Define the exact Codex request surfaces TokenSaver needs to intercept.
- Document what TokenSaver must never own:
  - model routing
  - external provider catalogs
  - provider/API credentials
  - LiteLLM or protocol translation to third-party models
  - model picker replacement
  - subagent orchestration
  - MCP execution
  - Codex task state

### Exit criteria

- `README.md`, `SCOPE.md`, and `ROADMAP.md` describe the same narrow product.
- No provider-routing feature is required for the MVP.
- The Codex transport integration contract is documented well enough to test.

---

# Phase 1 — Deterministic tool-result aging engine

**Goal:** reproduce the token-saving core as a pure, independently testable engine.

The engine must operate on request history without depending on networking, UI, Codex configuration, or provider code.

## Initial policy

Use conservative defaults matching the reference behavior:

- minimum textual result size: **32 KiB**
- protected newest-result frontier: **4 tool results**
- head preview: approximately **1024 code units**
- tail preview: approximately **1024 code units**
- identity: original UTF-8 byte length + **SHA-256** digest

## Eligibility rules

A result may be compacted only when:

- it is a recognized tool-result item
- its model-visible payload is entirely textual
- it is larger than the configured minimum threshold
- the model has already acted after receiving it
- it is outside the protected newest-result frontier
- the compact receipt is smaller than the original result

A result must remain exact when:

- it is still unconsumed
- it is one of the protected newest results
- it is below the size threshold
- it contains image/mixed/non-text content
- its structure is unknown or ambiguous
- parsing/classification fails
- compaction would not reduce size

## Receipt requirements

A deterministic receipt should retain:

- original UTF-8 byte length
- SHA-256 digest
- bounded head preview
- explicit omitted-middle marker
- bounded tail preview
- structural tool-call/result identifiers required by the Responses protocol
- enough recovery context to identify the source operation without inventing information

The same source result must produce the same receipt bytes every time.

## Required tests

- large consumed textual output is compacted
- unconsumed output remains exact
- newest four tool outputs remain byte-for-byte exact
- small outputs remain exact
- mixed/image-bearing results remain exact
- Unicode/surrogate boundaries are preserved
- digest generation is deterministic
- call IDs and required structural fields survive rewriting
- disabled mode is byte-preserving
- compact output never exceeds source size
- unknown item shapes fail original
- a later tool result alone does not falsely prove the model consumed an earlier result

### Exit criteria

- Aging is a pure function/library independent of transport.
- Every safety rule has automated coverage.
- Unknown inputs are preserved rather than guessed.

---

# Phase 2 — Measurement, telemetry, and benchmark harness

**Goal:** prove that TokenSaver reduces context rather than merely rewriting it.

## Per-request metrics

Record only metadata, never original result bodies:

- tool results evaluated
- results eligible
- results compacted
- largest result evaluated
- bytes before
- bytes after
- bytes saved
- estimated input tokens saved
- whether optimization ran but found nothing eligible

Estimated token savings must be clearly labeled as estimates unless a provider/client reports authoritative token usage.

## Offline benchmark tooling

Add a benchmark command that can process captured or synthetic histories without spending model quota.

Fixtures should include:

- large test logs
- build logs
- large diffs
- repository search output
- large file reads
- many medium-size tool results
- mixed text/image results
- histories where the model has not yet consumed the result

The report should explain why each candidate was or was not compacted.

## Cache observability

Where real token/cache telemetry is available, collect enough metadata to compare:

- compacted turns
- non-compacted turns
- input tokens
- cached input tokens

Do not claim prompt-cache preservation from byte estimates alone.

### Exit criteria

- Savings are deterministic and reproducible.
- The system can distinguish "optimizer disabled" from "optimizer ran but nothing qualified."
- Telemetry does not persist tool-result contents.

---

# Phase 3 — Native Codex transport integration

**Goal:** make TokenSaver perform the same practical interception needed for tool-result aging on real Codex traffic, without importing Codex Router's model-routing scope.

This phase is the critical bridge between a working algorithm and a usable Codex application.

## 3.1 Codex configuration integration

TokenSaver must integrate with Codex's existing native OpenAI path rather than introduce a new model provider experience.

Planned work:

- Detect the active Codex installation and supported configuration shape.
- Preserve the built-in/native OpenAI provider behavior.
- Point only the required native Codex base URL/transport setting to TokenSaver's loopback endpoint.
- Snapshot every configuration value TokenSaver changes before modifying it.
- Make configuration writes atomic where possible.
- Never overwrite unrelated user settings.
- Restore the exact previous values when TokenSaver is disconnected/uninstalled.
- Detect configuration drift instead of blindly overwriting a user's later edits.

TokenSaver must not change:

- selected model
- reasoning level
- MCP configuration
- skills
- project trust
- permissions
- subagent configuration
- unrelated Codex settings

## 3.2 Local loopback transport

Run a minimal local service bound to loopback only.

Requirements:

- no public network binding by default
- reject unsupported/browser-origin traffic where appropriate
- accept the Codex Responses request path needed for normal native inference
- preserve request semantics outside tool-result aging
- relay response streams without semantic transformation
- preserve cancellation/abort behavior
- keep request ordering and streaming behavior compatible with Codex

## 3.3 Native Codex authentication passthrough

TokenSaver should not ask the user for a separate OpenAI API key merely to optimize their native Codex traffic.

Requirements:

- use the authentication Codex already supplies on the native path
- forward only the headers required by the native Codex backend
- do not log access tokens, account IDs, capability secrets, or equivalent credentials
- do not expose credentials in status/tray payloads
- never replace a credential the caller explicitly supplied with another credential

The exact allow-list must be derived and tested against current Codex behavior rather than forwarding arbitrary headers.

## 3.4 Codex transport compatibility

TokenSaver must support the transport details Codex actually uses, including where applicable:

- HTTP Responses requests
- Codex's initial WebSocket attempt/fallback behavior
- compressed request bodies used by Codex, including supported gzip/deflate/Brotli/Zstandard forms
- correct decompression before inspection
- correct serialization/recompression/forwarding semantics
- streamed Responses events back to Codex
- request abort/cancellation

Transport compatibility must be validated against a real supported Codex build, not inferred only from unit tests.

## 3.5 Aging insertion point

For ordinary native Responses requests:

```text
Codex request
  ↓
decode/decompress
  ↓
normalize only what is required to inspect history
  ↓
age eligible historical tool results
  ↓
serialize
  ↓
native Codex/OpenAI upstream
```

No other semantic request rewrite is allowed unless required strictly for transparent transport compatibility and documented separately.

## 3.6 Compaction bypass

TokenSaver must recognize explicit Codex conversation-compaction traffic.

For `/responses/compact` or the supported equivalent compaction trigger:

- do not replace historical tool contents with aging receipts before the compaction summarizer reads them
- preserve the chaining/compaction semantics Codex expects

Tool-result aging and conversation compaction are complementary mechanisms; TokenSaver must not corrupt the latter to optimize the former.

## 3.7 Hard OFF / pass-through mode

When token saving is disabled:

- do not run the aging rewrite
- do not alter tool-result bodies
- preserve all request semantics
- continue serving as the local transport if the user wants the connection left installed

A diagnostic comparison test must prove that OFF mode does not introduce unintended request mutations.

## Required integration tests

- normal Codex native turn succeeds through TokenSaver
- streaming tool-call turn succeeds
- cancellation propagates correctly
- compressed request is decoded and forwarded correctly
- auth is forwarded correctly without appearing in logs/state
- `/responses/compact` bypasses aging
- ON/OFF comparison isolates differences to eligible tool-result bodies
- config connect/disconnect round-trip restores original values
- config drift fails safely
- unsupported request shapes fail original or pass through safely

### Exit criteria

- A real Codex session works normally through TokenSaver.
- No separate provider/model picker is introduced.
- With aging ON, only eligible historical tool-result payloads change.
- With aging OFF, behavior is transparent.
- Disconnect restores Codex configuration safely.

---

# Phase 4 — Recovery and quality validation

**Goal:** verify that context reduction does not materially degrade coding-agent behavior.

## Quality cases

Test tasks where:

- the model later asks about information retained in the head preview
- the model later asks about information retained in the tail preview
- the model later needs a fact that existed only in the omitted middle
- many large historical results have been aged
- the task continues for many turns after aging
- compaction and aging both occur in one long session

## Recovery behavior

Define an explicit safe recovery path for exact omitted content.

Recovery must:

- never hallucinate omitted bytes
- avoid exposing a broad file-retrieval capability to arbitrary model text
- avoid leaking private original outputs through telemetry
- clearly distinguish "receipt evidence" from exact original content

If the design supports rerunning a deterministic preceding tool call, validate that the protocol and client actually make this reliable before promising it in user-facing receipts.

Optional later design: owner-local exact-result retention may be considered only with strict privacy, bounded storage, secure permissions, and fail-original behavior. It is not required for the first release.

## A/B validation

Run controlled real Codex sessions with aging ON and OFF and compare:

- task success
- tool-call correctness
- model recovery behavior
- input-token usage where available
- prompt-cache rate where available
- latency overhead

### Exit criteria

- No known silent data-loss failure in the validation suite.
- Missing omitted information produces safe recovery behavior rather than invented values.
- The optimization produces material context savings in realistic coding workloads.

---

# Phase 5 — macOS runtime and tray/menu-bar application

**Goal:** make TokenSaver observable and controllable without requiring terminal commands.

The tray is part of the product, not decorative UI. It answers two essential questions:

1. **Is TokenSaver actually connected to Codex?**
2. **Is it actually saving context?**

## 5.1 Runtime ownership

For the first supported platform, package TokenSaver as a macOS application with a menu-bar/tray presence.

The app should own or supervise the local TokenSaver service and report its real state rather than infer it from a toggle.

Required states should distinguish at least:

- TokenSaver running / stopped
- Codex connected / waiting / configuration problem
- token saving enabled / disabled
- current request active / idle
- configuration drift/error

## 5.2 Minimal tray surface

The tray should stay intentionally small and token-focused.

Example information:

```text
TokenSaver
──────────────
Status            Active
Codex             Connected
Token Saving      On

This session
Saved             ~184K tokens
Compacted         12 results

Today
Saved             ~742K tokens

All time
Saved             ~2.6M tokens

Last optimization
84 KB → 3 KB

[✓] Token Saving Enabled
[ ] Start at Login

Open Statistics
Quit TokenSaver
```

Exact copy/design may change, but the following capabilities are required:

- enable/disable aging
- show real Codex connection state
- show whether the optimizer actually ran
- show session savings
- show cumulative savings
- show compacted result count
- show last optimization summary
- start at login
- expose diagnostics/status
- quit safely

## 5.3 Savings truthfulness

The tray must not display fabricated precision.

Distinguish clearly between:

- measured bytes saved
- estimated tokens saved
- provider-reported tokens/cache telemetry, if available

If no eligible result has yet appeared, say so rather than implying the optimizer is broken.

Examples:

- `Active · no eligible large result yet`
- `Largest result seen: 18 KB · threshold: 32 KB`
- `Saved 41.2 MB · ~10.3M estimated tokens`

## 5.4 Connect/disconnect UX

First-run experience should make the integration explicit:

1. User opens TokenSaver.
2. App detects Codex.
3. User chooses **Connect to Codex**.
4. TokenSaver snapshots and applies only the required Codex transport configuration.
5. Connection is verified with real local traffic/health evidence.
6. Tray reports `Codex: Connected`.

Disconnect should:

- stop intercepting Codex traffic
- restore TokenSaver-owned configuration changes
- leave unrelated Codex configuration untouched

## 5.5 Start at login

If enabled:

- start the TokenSaver runtime automatically
- ensure the local endpoint is ready before reporting Active
- do not silently rewrite Codex configuration on every launch if it is already correct
- surface errors when the expected configuration no longer matches

### Exit criteria

- A non-technical user can tell whether TokenSaver is working without opening a terminal.
- Savings counters are backed by runtime telemetry.
- Toggle state and backend state cannot silently disagree.
- Connect/disconnect is reversible.

---

# Phase 6 — CLI and diagnostics

**Goal:** provide a small engineering/diagnostic interface without turning TokenSaver into a large management platform.

Possible commands:

```text
tokensaver status
tokensaver connect
tokensaver disconnect
tokensaver saving on
tokensaver saving off
tokensaver stats
tokensaver config show
tokensaver config set min-bytes ...
tokensaver config set frontier ...
tokensaver doctor
```

The CLI should expose the same underlying state used by the tray rather than maintain a second configuration model.

## `doctor` should verify

- Codex installation detected
- supported Codex configuration shape
- TokenSaver loopback service reachable
- Codex currently points to the expected TokenSaver endpoint
- native upstream reachable through the transport
- token-saving state
- config snapshot/restoration state
- last optimizer activity
- local state permissions

Diagnostics must redact credentials and private tool-result contents.

### Exit criteria

- Tray and CLI report the same backend truth.
- Common integration failures can be diagnosed without manually reading config files.

---

# Phase 7 — Packaging, update safety, and uninstall

**Goal:** make TokenSaver behave like a normal reversible desktop utility.

Planned work:

- macOS application packaging
- signed/notarized distribution when appropriate
- deterministic installation paths
- safe service lifecycle
- update mechanism that preserves user state
- explicit uninstall/disconnect path
- restoration of TokenSaver-owned Codex configuration
- cleanup of TokenSaver runtime files without deleting unrelated Codex data

Uninstall must be tested as a first-class workflow, not an afterthought.

### Exit criteria

- Install → connect → use → disconnect → uninstall leaves Codex usable with its previous configuration.
- Upgrading TokenSaver does not reset user choices unexpectedly.

---

# Phase 8 — Hardening and release gates

**Goal:** make long-running use safe, predictable, and measurable.

## Reliability

- malformed request handling
- fail-original behavior on classifier/parser errors
- very large result tests
- large request-body tests
- memory-pressure tests
- concurrent request tests
- interrupted stream tests
- service restart tests
- Codex restart tests
- machine reboot/start-at-login tests

## Security/privacy

- loopback-only service by default
- strict local request authentication/capability if required by the final transport design
- no arbitrary browser access
- sensitive state stored with owner-only permissions
- no auth tokens in logs
- no tool-result bodies in telemetry by default
- bounded log/state growth
- safe temporary-file handling

## Performance

- optimizer overhead materially below the context it saves
- avoid unnecessary whole-payload copies for very large results
- benchmark request serialization/decompression overhead
- ensure tray/status polling does not create meaningful CPU or disk load

## Compatibility

Define a supported Codex version matrix rather than assuming every future client uses the same transport/config schema.

On an unsupported Codex build:

- detect it
- refuse unsafe automatic configuration changes
- explain the compatibility problem
- never guess a replacement config layout

### Release gates

A release candidate must pass:

1. deterministic aging unit suite
2. transport integration suite
3. config connect/disconnect restoration suite
4. real Codex smoke test
5. compaction-bypass test
6. ON/OFF payload-diff invariant test
7. tray/backend state-consistency test
8. privacy/log-redaction test
9. install/uninstall round-trip
10. realistic long-session savings benchmark

---

# Post-MVP ideas — only if they remain inside TokenSaver's scope

Possible later improvements:

- per-tool thresholds
- adaptive thresholds informed by real workload telemetry
- structured compaction for safely parsed repetitive logs
- tokenizer-aware savings estimates for selected models
- bounded owner-local exact-result retention with strict privacy controls
- adapters for other Responses-compatible coding agents
- richer local statistics/history view
- Windows/Linux support

Every proposal must pass the same scope test:

> Does this directly improve safe context/token reduction, transparent Codex integration, or operation/observability of that mechanism?

If not, it does not belong in TokenSaver.

---

# Explicit non-goals

TokenSaver will not become:

- a multi-provider model router
- an external-model catalog
- an API-key manager for model providers
- a LiteLLM replacement
- a model picker
- an agent orchestrator
- an MCP host
- a prompt marketplace
- a general Codex configuration manager

The intended product remains simple:

> **Run normal Codex through a small local context optimizer, safely compact old consumed tool results, show the user what was saved, and otherwise stay out of the way.**
