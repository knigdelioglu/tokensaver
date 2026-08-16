# TokenSaver Roadmap

TokenSaver has one product goal: **reduce repeated input-token usage in Codex by compacting old, already-consumed tool results while leaving the rest of Codex behavior unchanged.**

TokenSaver is a modular monolith, not a model router. It does not add providers, replace the native Codex model picker, translate third-party model protocols, manage provider credentials, or orchestrate agents.

Target flow:

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
relay response stream unchanged
  ↓
Codex
```

The user continues using Codex normally. TokenSaver is visible through a small tray/menu-bar application that proves connection state and measured savings.

---

## Product acceptance target

TokenSaver is successful when:

1. A user can install/open TokenSaver without changing normal Codex usage.
2. Codex keeps its existing account, native model picker, MCP tools, skills, subagents, permissions, and task state.
3. Native Responses traffic passes through a local TokenSaver loopback transport.
4. Aging ON rewrites only eligible historical tool-result bodies.
5. Aging OFF performs no context rewrite.
6. Explicit Codex conversation compaction reads original history rather than aged receipts.
7. The tray truthfully shows connection state, optimizer activity, measured bytes saved, and estimated tokens saved.
8. Disconnect/uninstall restores TokenSaver-owned Codex configuration safely.
9. TokenSaver never becomes a provider/model router.
10. Modular-monolith boundaries remain enforced.

Core integration invariant:

> For the same logical Codex request, optimized and pass-through payloads may differ only where the aging policy explicitly permits an eligible historical tool-result body to change.

---

# Phase 0 — Project contract and architecture

**Status: COMPLETE**

**Goal:** freeze product scope, architecture, upstream reference, and the Codex integration contract before behavior is implemented.

Completed:

- `README.md`, `SCOPE.md`, and this roadmap aligned to the same narrow product.
- Codex Router `v0.4.0-beta.4` pinned as the initial tool-result-aging reference.
- `docs/UPSTREAM_REFERENCE.md` records what TokenSaver adopts and explicitly rejects.
- `docs/CODEX_TRANSPORT_CONTRACT.md` defines the native Codex interception/pass-through contract.
- `docs/ARCHITECTURE.md` defines the modular-monolith module ownership and dependency direction.
- Rust project skeleton established with `application`, `aging`, `transport`, `codex_integration`, `telemetry`, `runtime`, `diagnostics`, and `shared` boundaries.
- `tests/architecture_contract.rs` protects critical forbidden dependencies.
- `AGENTS.md` records implementation guardrails and authority order.
- Public crate surface begins at `application`; product modules remain internal.

Phase 0 explicitly does **not** claim the token optimizer is implemented. That begins in Phase 1.

Exit criteria: **met**.

---

# Phase 1 — Deterministic tool-result aging engine

**Status: IMPLEMENTED — VALIDATION DEFERRED**

**Goal:** implement the token-saving core as a pure, independently testable domain module.

Initial policy:

- textual results only
- minimum size: **32 KiB**
- protected newest frontier: **4 tool results**
- model must have acted after the result
- head preview: approximately **1024 code units**
- tail preview: approximately **1024 code units**
- identity: original UTF-8 byte length + **SHA-256**
- replacement is applied only when smaller than the source

A result remains exact when:

- still unconsumed
- inside the protected frontier
- too small
- mixed/image/non-text
- malformed/unknown/ambiguous
- transformation validation fails
- receipt would not reduce size

Implemented:

- typed aging policy
- recognized tool-call/tool-result shapes
- consumed-result detection
- safe textual extraction
- Unicode-safe head/tail preview
- deterministic SHA-256 receipt generation
- structural call/result validation tuple (`index + kind + call_id`)
- measured byte-savings result object
- hard disabled behavior
- per-result decision metadata used by Phase 2 observability

Authored tests cover:

- large consumed textual result is aged
- unconsumed result remains exact
- newest four remain byte-for-byte exact
- small results remain exact
- mixed/image-bearing results remain exact
- Unicode boundaries are not corrupted
- digest/receipt are deterministic
- structural identifiers survive
- unknown shapes fail original
- disabled mode is byte-preserving
- compact result never exceeds source
- later tool output alone does not falsely prove consumption
- architecture contract

Exit criteria implementation is complete. Automated execution is intentionally deferred by project instruction; no test/build/lint/formatter/CI command has been run.

---

# Phase 2 — Measurement, telemetry, and benchmark harness

**Status: IMPLEMENTED — VALIDATION DEFERRED**

**Goal:** prove what was evaluated and how much context was actually removed.

Per-request metrics:

- results evaluated
- results eligible
- results compacted
- largest result evaluated
- bytes before
- bytes after
- bytes saved
- estimated tokens saved
- optimizer-ran-but-nothing-qualified state

Implemented:

- content-free `OptimizationEvent` and `OptimizationMetrics` models
- distinct outcomes for disabled, bypassed, evaluated/no eligible, evaluated/no savings, and aged
- exact byte accounting from aging results
- reference-compatible `round(bytes / 4)` token estimate, explicitly marked approximate
- provider-reported input/cache token fields kept separate from estimates
- session, arbitrary time-range, and all-retained-event aggregation
- cache-rate aggregation only from provider-reported token counters
- per-result content-free aging decision reasons
- application-layer mapper from aging results to telemetry events
- offline benchmark harness with deterministic synthetic fixtures for:
  - test logs
  - build logs
  - large diffs
  - repository search output
  - large file reads
  - many medium outputs
  - mixed/unsupported output
  - unconsumed histories
- `docs/MEASUREMENT.md` defining metric and privacy semantics

Rules:

- routine telemetry never stores original tool-result bodies
- byte savings are measured directly
- token estimates are explicitly labeled estimates
- provider-reported token/cache metrics are kept distinct when naturally available

Authored tests cover telemetry aggregation, disabled-vs-no-eligible distinction, provider usage separation, benchmark skip reasons, and deterministic savings behavior.

Exit criteria implementation is complete. Automated execution is intentionally deferred by project instruction; no test/build/lint/formatter/CI command has been run.

---

# Phase 3 — Native Codex transport integration

**Goal:** transparently run normal native Codex traffic through TokenSaver without importing Codex Router's model-routing scope.

`docs/CODEX_TRANSPORT_CONTRACT.md` is authoritative for this phase.

## 3.1 Codex configuration integration

Implement:

- detect supported Codex installation/config shape
- preserve built-in/native OpenAI behavior
- point only required native transport/base URL setting to TokenSaver loopback
- snapshot every TokenSaver-owned value before changing it
- atomic/safe writes where possible
- exact restore on disconnect/uninstall
- config drift detection

Must not change:

- model selection
- reasoning level
- MCP configuration
- skills
- project trust
- permissions
- subagent configuration
- unrelated Codex settings

## 3.2 Loopback transport

Implement:

- loopback-only local service
- safe local caller validation/capability as required
- native Responses request handling
- streamed response relay without semantic transformation
- cancellation propagation
- request ordering/lifecycle compatibility
- no general unauthenticated proxy behavior

## 3.3 Native authentication passthrough

Implement:

- use authentication Codex already supplies
- no separate OpenAI API key requirement for native Codex optimization
- explicit upstream header allow-list
- no credential logging/status exposure
- never replace a credential explicitly supplied by the caller

## 3.4 Codex transport compatibility

Verify against the supported real Codex build:

- HTTP Responses traffic
- current WebSocket attempt/fallback behavior
- required gzip/deflate/Brotli/Zstandard request-body handling
- decode before inspection
- correct forwarding/recompression semantics
- streamed Responses events
- abort/cancellation

Do not copy version-sensitive behavior blindly from Codex Router.

## 3.5 Aging insertion point

```text
receive
  ↓
validate/decode
  ↓
detect conversation compaction
  ↓
ordinary request → aging engine
  ↓
serialize/forward
```

No unrelated semantic rewrite is permitted.

## 3.6 Conversation-compaction bypass

At minimum `/responses/compact` must bypass aging when that is the verified native compaction path. Any equivalent trigger is added only after current-Codex verification.

## 3.7 Hard OFF mode

With saving disabled:

- transport may remain connected
- aging does not run/rewrite
- request semantics remain transparent

Required integration tests:

- native Codex turn succeeds
- streamed tool-call turn succeeds
- cancellation works
- required compression formats round-trip
- auth is forwarded but absent from logs/state
- conversation compaction bypasses aging
- ON/OFF semantic diff isolates eligible result bodies only
- connect/disconnect restores config
- config drift fails safely
- unsupported shapes/config fail original or fail closed

Exit criteria:

- real Codex works normally through TokenSaver
- no new model/provider picker exists
- ON changes only eligible historical tool-result bodies
- OFF is transparent
- disconnect restores TokenSaver-owned configuration

---

# Phase 4 — Recovery and quality validation

**Goal:** verify that saved context does not materially damage coding-agent behavior.

Quality cases:

- later need for head-preview content
- later need for tail-preview content
- later need for a fact only in omitted middle
- many aged results in one long task
- many turns after aging
- aging plus native conversation compaction in the same session

Recovery rules:

- never hallucinate omitted bytes
- clearly distinguish receipt evidence from exact content
- do not expose broad private-result retrieval to arbitrary model text
- do not leak originals via telemetry
- if rerunning a prior tool is offered, verify real Codex/tool behavior before promising it in receipts

A/B validation:

- task success
- tool-call correctness
- recovery behavior
- input tokens where authoritative data exists
- cache rate where available
- TokenSaver latency overhead

Exit criteria:

- no known silent data-loss path in the validation suite
- omitted-middle requests fail/recover safely rather than inventing values
- material context savings demonstrated in realistic coding workloads

---

# Phase 5 — macOS runtime and tray/menu-bar application

**Goal:** let the user see and control TokenSaver without a terminal.

The tray is part of MVP observability, not decorative UI.

Required real states:

- TokenSaver running/stopped
- Codex connected/waiting/configuration problem
- token saving enabled/disabled
- request active/idle
- config drift/error

Required tray information/actions:

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
```

Also:

- enable/disable saving
- Connect to Codex / Disconnect
- start at login
- diagnostics/status
- safe quit

Truthfulness rules:

- measured bytes and estimated tokens are distinguished
- no eligible result yet is shown explicitly
- UI toggle state never substitutes for backend state
- connection is proven from configuration/runtime evidence

Exit criteria:

- non-technical user can tell whether TokenSaver is working
- counters are backed by runtime telemetry
- connect/disconnect is reversible
- tray/backend state cannot silently disagree

---

# Phase 6 — CLI and diagnostics

**Goal:** provide engineering/diagnostic control over the same backend truth used by the tray.

Planned commands:

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

`doctor` verifies:

- Codex installation/version support
- configuration shape
- loopback service
- expected Codex → TokenSaver connection
- upstream reachability through transport
- token-saving state
- config snapshot/restoration state
- last optimizer activity
- local state permissions

Diagnostics redact credentials and original tool-result bodies.

Exit criteria:

- CLI and tray use the same state model
- common failures can be diagnosed without manual config-file inspection

---

# Phase 7 — Packaging, update safety, and uninstall

**Goal:** make TokenSaver a reversible desktop utility.

Implement:

- macOS application packaging
- signing/notarization when appropriate
- deterministic install/state paths
- safe runtime lifecycle
- update mechanism preserving user choices/state
- first-class disconnect/uninstall
- exact restoration of TokenSaver-owned Codex config
- cleanup limited to TokenSaver files

Exit criteria:

- install → connect → use → disconnect → uninstall leaves Codex usable with its prior configuration
- upgrades do not silently reset user choices

---

# Phase 8 — Hardening and release gates

**Goal:** make long-running production use safe and measurable.

Reliability:

- malformed requests
- fail-original parser/classifier errors
- huge outputs/request bodies
- memory pressure
- concurrent requests
- interrupted streams
- TokenSaver/Codex restart
- machine reboot/start-at-login

Security/privacy:

- loopback-only by default
- strict local caller protection as required
- no permissive browser/CORS access
- owner-only sensitive state
- no auth tokens in logs
- no original result bodies in telemetry by default
- bounded logs/state
- safe temporary files

Performance:

- optimizer overhead materially lower than saved context
- avoid unnecessary copies of huge results
- benchmark compression/serialization overhead
- tray/status polling must remain lightweight

Compatibility:

- explicit supported Codex version/config matrix
- unsupported builds are detected, not guessed
- unsafe automatic config changes are refused

Release candidate gates:

1. deterministic aging suite
2. architecture-contract suite
3. transport integration suite
4. config restoration/drift suite
5. real Codex smoke test
6. conversation-compaction bypass test
7. ON/OFF payload-diff invariant test
8. tray/backend state-consistency test
9. privacy/log-redaction test
10. install/uninstall round-trip
11. realistic long-session savings benchmark

---

# Post-MVP ideas — only if they remain in scope

Possible later improvements:

- per-tool thresholds
- adaptive thresholds based on real workload telemetry
- structured compaction for safely parsed repetitive logs
- tokenizer-aware estimates
- bounded owner-local exact-result retention with strict privacy controls
- adapters for other Responses-compatible coding agents
- richer local statistics/history
- Windows/Linux support

Every proposal must pass:

> Does this directly improve safe context/token reduction, transparent Codex integration, or operation/observability of that mechanism?

If not, it does not belong in TokenSaver.

---

# Explicit non-goals

TokenSaver will not become:

- a multi-provider model router
- an external-model catalog
- a provider API-key manager
- a LiteLLM replacement
- a model picker
- an agent orchestrator
- an MCP host
- a prompt marketplace
- a general Codex configuration manager

The intended product remains:

> **Run normal Codex through a small local context optimizer, safely compact old consumed tool results, show the user what was saved, and otherwise stay out of the way.**
