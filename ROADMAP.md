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

1. installation does not change normal Codex usage
2. Codex keeps its account, native model picker, MCP tools, skills, subagents, permissions, and task state
3. native Responses traffic passes through TokenSaver loopback
4. aging ON rewrites only eligible historical tool-result bodies
5. aging OFF performs no context rewrite
6. native conversation compaction reads original history
7. the tray truthfully reports connection and savings
8. disconnect/uninstall restores TokenSaver-owned Codex configuration safely
9. TokenSaver never becomes a provider/model router
10. modular-monolith boundaries remain enforced

Core integration invariant:

> For the same logical Codex request, optimized and pass-through payloads may differ only where the aging policy explicitly permits an eligible historical tool-result body to change.

---

# Phase 0 — Project contract and architecture

**Status: COMPLETE**

Completed:

- `README.md`, `SCOPE.md`, and roadmap aligned
- Codex Router `v0.4.0-beta.4` pinned as the initial aging reference
- `docs/UPSTREAM_REFERENCE.md`
- `docs/CODEX_TRANSPORT_CONTRACT.md`
- `docs/ARCHITECTURE.md`
- modular-monolith Rust skeleton
- architecture contract tests authored
- `AGENTS.md` implementation rules

Phase 0 established boundaries only; runtime optimization began in Phase 1.

---

# Phase 1 — Deterministic tool-result aging engine

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Implemented:

- textual tool results only
- strict `> 32 KiB` threshold
- newest four tool-result frontier
- model-action consumption detection
- `function_call_output` and `custom_tool_call_output`
- pure textual multipart support
- mixed/image/unknown outputs preserved
- UTF-16-safe ~1024-code-unit head/tail previews
- SHA-256 deterministic receipt identity
- receipt used only when smaller than source
- structural replacement tuple: `index + kind + call_id`
- measured byte statistics
- hard disabled mode
- per-result aging/skip decisions
- fail-original architecture

Authored test sources cover all safety invariants, including Unicode, deterministic receipts, frontier behavior, unconsumed results, unsupported outputs, exact threshold behavior, and architecture boundaries.

**Validation remains intentionally deferred. No tests/build/lint/formatter/CI have been executed.**

---

# Phase 2 — Measurement, telemetry, and benchmark harness

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Implemented:

- content-free optimization events and metrics
- outcomes:
  - disabled
  - conversation-compaction bypass
  - native Codex passthrough
  - fail-original
  - evaluated/no eligible result
  - evaluated/no savings
  - aged
- exact byte accounting
- explicit approximate `round(bytes / 4)` token estimate
- provider-reported input/cache usage kept separate
- session/time-range/all-retained aggregation
- provider cache-rate aggregation
- application-layer aging → telemetry mapping
- real transport observation → telemetry mapping
- deterministic offline benchmark fixtures:
  - test logs
  - build logs
  - large diffs
  - repository searches
  - large file reads
  - many medium outputs
  - mixed output
  - unconsumed history
- `docs/MEASUREMENT.md`

Routine telemetry contains no original tool-result body or receipt.

**Validation remains intentionally deferred. No tests/build/lint/formatter/CI have been executed.**

---

# Phase 3 — Native Codex transport integration

**Status: IMPLEMENTED — VALIDATION DEFERRED**

**Goal:** transparently run normal built-in Codex/OpenAI provider traffic through TokenSaver without importing Codex Router's provider-routing scope.

`docs/CODEX_TRANSPORT_CONTRACT.md` is authoritative.

## Verified Codex baseline

Phase 3 was designed against OpenAI Codex commit:

`9ded177ce7c1c0bd2047f902936c177612ab3434`

The implementation is based on verified current-source behavior rather than copying version-sensitive assumptions blindly from Codex Router.

Verified facts used by the implementation:

- root `openai_base_url` overrides the built-in OpenAI provider
- built-in provider retains Responses behavior, native auth, and WebSocket support
- `CODEX_HOME` is honored; otherwise Codex home is `~/.codex`
- HTTP `426 Upgrade Required` triggers Codex WebSocket → HTTP fallback
- current ChatGPT request compression can use Zstandard
- native remote compaction uses `responses/compact`
- the same provider base URL is also used by native `models`, memory summarization, standalone search, and image endpoints
- realtime/WebRTC request shape is base-URL-sensitive and therefore must not inherit TokenSaver's Responses loopback URL

## 3.1 Codex configuration integration — implemented

TokenSaver always owns the temporary Responses base URL while connected:

```toml
openai_base_url = "http://127.0.0.1:<port>/<64-hex-capability>/v1"
```

If the user has not already configured them, TokenSaver also temporarily installs native realtime bypasses:

```toml
experimental_realtime_webrtc_call_base_url = "<native ChatGPT Codex URL>"
experimental_realtime_ws_base_url = "https://api.openai.com/v1"
```

Implemented:

- no replacement `model_providers.openai` table
- same Codex-home resolution rule
- TOML-preserving edit with `toml_edit`
- pre-existing `openai_base_url` preservation
- pre-existing user realtime overrides left untouched
- exact restore/removal on disconnect
- per-owned-key drift detection before restoration
- owner-only versioned restoration snapshot
- atomic same-directory config writes
- snapshot written before Codex config mutation
- stale/unsafe snapshot detection
- crash/restart endpoint reuse
- custom `chatgpt_base_url` respected when deriving the managed WebRTC bypass

TokenSaver does not modify model selection, reasoning settings, MCP configuration, skills, permissions, project trust, subagent configuration, or unrelated Codex settings.

## 3.2 Local loopback transport — implemented

Implemented:

- IPv4 loopback-only listener
- OS-selected free port support
- 256-bit random caller capability encoded in URL path
- configured capability base ends in `/v1`
- constant-time capability comparison
- browser-origin rejection
- no permissive CORS behavior
- fixed first-party upstreams only; no arbitrary forward-proxy target
- redirects disabled on upstream HTTP client
- streamed upstream response relay without model-response rewriting
- hop-by-hop response header filtering
- raw query-string preservation
- finite native path/method allow-list

Allowed native routes:

| Local path | Method | TokenSaver behavior |
|---|---:|---|
| `/v1/responses` | POST | eligible for aging |
| `/v1/responses/compact` | POST | exact aging bypass |
| `/v1/models` | GET | native passthrough |
| `/v1/memories/trace_summarize` | POST | native passthrough |
| `/v1/alpha/search` | POST | native passthrough |
| `/v1/images/generations` | POST | native passthrough |
| `/v1/images/edits` | POST | native passthrough |

Unknown paths are rejected rather than becoming a generic authenticated proxy.

The local `/v1` prefix is stripped exactly once before joining the native first-party provider base URL, preventing duplicate `/v1` segments.

## 3.3 Native authentication and upstream preservation — implemented

Implemented:

- Codex-provided credentials are relayed rather than replaced
- no separate TokenSaver OpenAI key requirement
- explicit upstream header allow-list
- arbitrary cookies/browser/proxy headers excluded
- capability is local transport auth only and is never forwarded upstream
- no credential fields in transport observations
- account-scoped requests carrying `ChatGPT-Account-ID`/FedRAMP routing are sent to the ChatGPT Codex backend
- API-key-style requests without account routing are sent to the OpenAI API base
- TokenSaver does not parse bearer-token contents to make this decision

## 3.4 WebSocket, realtime, and compression compatibility — implemented

Implemented:

- Responses WebSocket Upgrade attempt receives `426 Upgrade Required`
- HTTP Responses path handles subsequent Codex fallback
- realtime/WebRTC traffic is deliberately kept off the TokenSaver loopback via native Codex config overrides when the user has no existing override
- request encoding support:
  - identity
  - zstd
  - gzip
  - x-gzip
  - deflate
  - Brotli
- encoded and decoded body limits
- multi-encoding decode/encode ordering
- original encoded bytes retained when no rewrite is required
- same declared encoding chain used after a successful aging rewrite
- malformed/unsupported compression on an aging-eligible Responses request fails original
- native passthrough/compact bodies are not decoded merely for TokenSaver

## 3.5 Responses aging adapter — implemented

Processing path:

```text
receive
  ↓
authenticate capability
  ↓
validate finite native route + method/content type
  ↓
classify responses / compact / native passthrough
  ↓
responses only: decode if aging inspection is enabled
  ↓
normalize Responses input
  ↓
run pure aging engine
  ↓
validate index + kind + call_id
  ↓
replace only eligible output fields
  ↓
serialize/re-encode
  ↓
forward to fixed first-party upstream
```

Implemented guarantees:

- unknown protocol items normalize to `Other`
- mixed output remains unsupported/ineligible
- replacement is validated against the original JSON item
- JSON object insertion order is preserved during rewritten serialization
- any unsafe decode/parse/replacement/serialization/re-encoding condition causes whole-request fail-original
- native non-Responses endpoints never enter the aging parser

## 3.6 Conversation-compaction bypass — implemented

Aging is bypassed for:

- `/responses/compact`
- `/v1/responses/compact`

The original encoded request body is forwarded untouched by aging.

## 3.7 Hard OFF mode — implemented

With token saving disabled:

- loopback may remain connected
- ordinary Responses history is not parsed or rewritten
- original encoded Responses body is forwarded
- native passthrough and explicit compaction retain their own diagnostic classifications

## 3.8 Lifecycle composition — implemented

Application-layer connection ordering:

```text
resolve/recover endpoint
  ↓
bind loopback successfully
  ↓
durably snapshot every TokenSaver-owned Codex value
  ↓
install openai_base_url + only-missing realtime bypasses
```

Restart:

- existing snapshot is loaded first
- exact prior port and capability are recovered from the managed `/v1` URL
- TokenSaver attempts to bind that same endpoint
- a fresh endpoint is not silently substituted while Codex points to the old one

Disconnect:

- every TokenSaver-owned key is drift-checked
- prior OpenAI base URL is restored or the TokenSaver-owned key removed
- only realtime keys TokenSaver itself installed are removed
- pre-existing user realtime settings remain untouched
- snapshot is removed only after successful restoration

## 3.9 Observability bridge — implemented

Transport emits only:

- preparation outcome
- content-free aging statistics

Outcomes distinguish native passthrough, explicit compaction bypass, fail-original, disabled Responses, no-eligible/no-savings evaluation, and actual aging.

It does not emit original tool-result text, receipt text, bearer credentials, account IDs, or capability values. The application layer maps these observations into Phase 2 telemetry.

## Authored Phase 3 test sources

Tests have been written for:

- Codex config connect/disconnect round-trip
- preservation of unrelated config
- pre-existing base URL restoration
- managed realtime override installation/removal
- pre-existing realtime override preservation
- custom `chatgpt_base_url` realtime derivation
- OpenAI and realtime drift refusal
- Codex-home path resolution
- strict `/<64-hex>/v1` loopback URL validation
- exact capability authentication and restart recovery
- OFF Responses byte preservation
- native passthrough classification independent of saving toggle
- compact byte preservation independent of saving toggle
- ON semantic-diff invariant at adapter level
- mixed-output preservation
- gzip/x-gzip/deflate/Brotli/Zstandard round trips
- unsupported compression fail-original
- browser-origin rejection
- header allow-listing
- ChatGPT-vs-API upstream selection helper behavior
- `/v1` upstream-path normalization
- finite native route/method allow-list
- prepare/disconnect lifecycle
- crash/restart endpoint reuse
- native passthrough telemetry aggregation

Still requiring final executed/live validation:

- compile/test/lint/format pass
- real installed Codex smoke test
- real streamed tool-call turn
- live cancellation behavior
- live ChatGPT and API-key auth/header behavior
- native models/search/images/memory passthrough smoke cases
- full ON/OFF captured-request comparison

**Phase 3 implementation is complete. Automated and live validation are intentionally deferred by project instruction; no test/build/lint/formatter/CI command has been run.**

---

# Phase 4 — Recovery and quality validation

**Status: NOT STARTED**

**Goal:** verify that saved context does not materially damage coding-agent behavior and define safe exact-content recovery behavior.

Planned quality cases:

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
- do not promise rerun recovery until real Codex/tool behavior has been validated

Planned A/B validation:

- task success
- tool-call correctness
- recovery behavior
- authoritative input tokens where available
- cache rate where available
- TokenSaver latency overhead

Because Phase 4 is validation-heavy, no live/A-B validation will be executed until the user explicitly requests the final validation pass. Any recovery infrastructure that can be implemented safely without executing tests may still be authored before then.

---

# Phase 5 — macOS runtime and tray/menu-bar application

**Status: NOT STARTED**

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

---

# Phase 6 — CLI and diagnostics

**Status: NOT STARTED**

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

`doctor` should verify:

- Codex installation/version support
- configuration shape
- loopback service
- expected Codex → TokenSaver connection
- upstream reachability through transport
- token-saving state
- config snapshot/restoration state
- last optimizer activity
- local state permissions

Diagnostics must redact credentials, capability secrets, and original tool-result bodies.

---

# Phase 7 — Packaging, update safety, and uninstall

**Status: NOT STARTED**

Planned:

- macOS application packaging
- signing/notarization when appropriate
- deterministic install/state paths
- safe runtime lifecycle
- update mechanism preserving user choices/state
- first-class disconnect/uninstall
- exact restoration of TokenSaver-owned Codex config
- cleanup limited to TokenSaver files

---

# Phase 8 — Hardening and release gates

**Status: NOT STARTED**

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

- loopback-only
- strict local caller capability
- no browser/CORS proxy surface
- owner-only sensitive state
- no auth tokens in logs
- no original result bodies in routine telemetry
- bounded logs/state
- safe temporary files

Performance:

- optimizer overhead materially lower than saved context
- avoid unnecessary copies of huge results
- benchmark compression/serialization overhead
- tray/status polling remains lightweight

Compatibility:

- explicit supported Codex version/config baseline
- unsupported builds detected rather than guessed
- unsafe automatic config changes refused

Final release gates:

1. deterministic aging suite
2. architecture-contract suite
3. telemetry/benchmark suite
4. transport integration suite
5. config restoration/drift suite
6. real Codex smoke test
7. compaction-bypass test
8. ON/OFF payload-diff invariant test
9. tray/backend state-consistency test
10. privacy/log-redaction test
11. install/uninstall round-trip
12. realistic long-session savings benchmark

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
