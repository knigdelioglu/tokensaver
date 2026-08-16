# TokenSaver Roadmap

TokenSaver has one product goal: **reduce repeated input-token usage in Codex by compacting old, already-consumed tool results while leaving the rest of Codex behavior unchanged.**

TokenSaver is a modular monolith, not a model router. The user continues using normal Codex; TokenSaver sits transparently on the native request path and exposes operation/savings through a small macOS menu-bar application.

```text
Codex
  ↓
TokenSaver loopback
  ↓
inspect ordinary Responses history
  ↓
age only eligible old tool results
  ↓
forward to the same first-party upstream
  ↓
relay response stream unchanged
  ↓
Codex
```

## Product acceptance target

TokenSaver succeeds when:

1. normal Codex account/model/MCP/skills/subagent behavior remains intact
2. aging ON changes only eligible historical tool-result bodies
3. aging OFF performs no context rewrite
4. native conversation compaction sees original history
5. exact omitted content is never guessed
6. native non-Responses provider traffic remains functional
7. tray state is backed by runtime truth and measured savings
8. disconnect/uninstall restores TokenSaver-owned Codex configuration safely
9. TokenSaver never becomes a provider/model router
10. modular-monolith boundaries remain enforced

Core invariant:

> For the same logical ordinary Responses request, ON and OFF semantic payloads may differ only where the aging policy explicitly permits an eligible historical tool-result `output` to change.

---

# Phase 0 — Project contract and architecture

**Status: COMPLETE**

Implemented:

- aligned `README.md`, `SCOPE.md`, and roadmap
- Codex Router `v0.4.0-beta.4` pinned as the initial aging reference
- `docs/UPSTREAM_REFERENCE.md`
- `docs/CODEX_TRANSPORT_CONTRACT.md`
- `docs/ARCHITECTURE.md`
- modular-monolith Rust skeleton
- architecture-contract test source
- `AGENTS.md` implementation rules

Architecture rule: the aging domain remains Codex-, transport-, persistence-, telemetry-, and UI-agnostic.

---

# Phase 1 — Deterministic tool-result aging engine

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Implemented:

- textual tool results only
- strict `> 32 KiB` threshold
- newest four tool-result frontier
- later model-action consumption detection
- `function_call_output` / `custom_tool_call_output`
- pure textual multipart support
- mixed/image/unknown output preservation
- UTF-16-safe bounded head/tail previews
- SHA-256 source identity
- receipt only when smaller than source
- structural replacement tuple: `index + kind + call_id`
- exact byte-savings statistics
- hard disabled mode
- fail-original behavior
- per-result aging/skip reasons

Test sources cover threshold/frontier/consumption, Unicode, determinism, unsupported shapes, structural identity, disabled behavior, and receipt-size safety.

**No tests/build/lint/formatter/CI have been executed.**

---

# Phase 2 — Measurement, telemetry, and benchmark harness

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Implemented content-free outcomes:

- disabled
- conversation-compaction bypass
- native Codex passthrough
- fail-original
- evaluated/no eligible result
- evaluated/no savings
- aged

Implemented metrics:

- tool results evaluated/eligible/compacted
- largest result
- bytes before/after/saved
- approximate `round(bytes / 4)` tokens saved
- provider-reported input/cache usage kept separate from estimates
- session/time-range/all-retained aggregation

Offline deterministic fixtures cover logs, builds, diffs, searches, large reads, many medium results, mixed output, and unconsumed history.

Routine telemetry stores no original tool-result body or receipt.

**No tests/build/lint/formatter/CI have been executed.**

---

# Phase 3 — Native Codex transport integration

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Authoritative document: `docs/CODEX_TRANSPORT_CONTRACT.md`.

Pinned implementation baseline:

`openai/codex@9ded177ce7c1c0bd2047f902936c177612ab3434`

## Codex config ownership

While connected TokenSaver installs:

```toml
openai_base_url = "http://127.0.0.1:<port>/<64-hex-capability>/v1"
```

When missing, it also temporarily installs native realtime bypasses so voice/WebRTC does not inherit the Responses loopback URL.

Implemented:

- TOML-preserving config edits
- pre-existing values preserved
- owner-only versioned snapshot
- snapshot-before-mutation transaction
- atomic writes
- per-owned-key drift detection
- exact disconnect restoration
- crash/restart recovery of the same port + capability
- `CODEX_HOME` / `~/.codex` resolution

## Transport

Implemented:

- IPv4 loopback-only listener
- 256-bit capability path
- browser-origin rejection
- no permissive CORS
- fixed first-party upstreams only
- no arbitrary forward proxy
- response streaming without semantic rewrite
- raw query preservation
- redirects disabled upstream
- hop-by-hop response-header filtering
- Responses WebSocket `426` → Codex HTTP fallback compatibility
- zstd/gzip/x-gzip/deflate/Brotli request handling for aging inspection
- fail-original on unsafe Responses decode/parse/rewrite

Finite native route allow-list:

| Local route | Method | Behavior |
|---|---:|---|
| `/v1/responses` | POST | aging eligible |
| `/v1/responses/compact` | POST | exact aging bypass |
| `/v1/models` | GET | native passthrough |
| `/v1/memories/trace_summarize` | POST | native passthrough |
| `/v1/alpha/search` | POST | native passthrough |
| `/v1/images/generations` | POST | native passthrough |
| `/v1/images/edits` | POST | native passthrough |

Account-scoped Codex requests are preserved toward the ChatGPT Codex backend; API-key-style requests are preserved toward the OpenAI API without TokenSaver parsing bearer-token contents.

Still deferred for the final executed pass:

- compile/test/lint/format
- installed Codex smoke
- streamed tool-call turn
- cancellation
- live auth/header behavior
- native passthrough smoke cases
- captured ON/OFF semantic comparison

**No tests/build/lint/formatter/CI have been executed.**

---

# Phase 4 — Recovery and quality guardrails

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Authoritative document: `docs/RECOVERY.md`.

Goal: make information loss explicit and exact recovery verifiable without creating a broad private store of original tool outputs.

## Verifiable receipt v1

Receipts contain machine-readable identity:

```text
[tokensaver-receipt:v1 original_bytes=<n> sha256=<hex> head_bytes=<n> tail_bytes=<n>]
```

Implemented rules:

- head/tail are explicitly verbatim evidence
- omitted middle is explicitly unavailable and must not be inferred
- exact candidates require matching UTF-8 byte length + SHA-256
- unsupported receipt versions/layouts are rejected
- no persistent exact-result vault exists in MVP

Application recovery outcomes distinguish:

- `ReceiptEvidenceAvailable`
- `ExactSourceRequired`
- `VerifiedExact`
- `Rejected`

Deterministic quality fixtures cover evidence boundaries, many aged results, long history distance, exact identity verification, and modified-source rejection.

Still deferred for final A/B validation:

- task success ON vs OFF
- tool-call correctness ON vs OFF
- omitted-middle behavior in real tasks
- actual tool replay/recovery safety
- aging + native conversation compaction in live sessions
- authoritative token/cache comparison
- latency overhead

**No tests/build/lint/formatter/CI or live A/B validation have been executed.**

---

# Phase 5 — macOS runtime and tray/menu-bar application

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Authoritative document: `docs/DESKTOP_RUNTIME.md`.

Goal: let the user see, control, and safely operate TokenSaver without a terminal while keeping all truth in backend/application state.

## 5.1 Windowless Tauri shell — implemented

Implemented:

- Tauri 2 desktop entry point
- no webview/application window
- macOS accessory activation policy
- menu-bar tray title + tooltip
- single-instance protection
- tray menu built entirely from native Tauri menu items

The shell reaches core behavior only through `application::desktop_runtime`; it does not access aging/transport/config persistence directly.

## 5.2 Real runtime states — implemented

Tray state is refreshed from backend evidence and distinguishes:

- service Starting / Active / Error
- Codex Disconnected / Connecting / Connected / Configuration Drift / Error
- Token Saving enabled/disabled
- request Idle / Active with concurrent request count
- health/error state

Connection health is periodically re-proven against the Phase 3 config snapshot rather than inferred from a UI toggle.

## 5.3 Savings visibility — implemented

Tray shows:

- this session
- current local day
- all time
- last successful optimization

Each scope displays separately:

- **measured serialized bytes saved**
- **estimated tokens saved** (`~` / `est.` labeled)
- compacted tool-result count
- aging-request count

The menu-bar title may show the current-day estimated saving, always prefixed with `~`.

## 5.4 Durable content-free telemetry — implemented

`src/modules/telemetry/store.rs` persists only numeric aggregates:

- all-time summary
- bounded daily summaries (maximum 120 local-day buckets)
- last optimization numeric metadata

It persists no prompt, result body, receipt body, credential, account ID, or capability secret.

Writes use the existing atomic private-file primitive and are periodically flushed rather than synchronously blocking every Codex request.

## 5.5 Runtime preferences — implemented

`runtime-preferences.json` schema v2 persists:

- `saving_enabled`
- `connect_on_launch`
- `min_bytes`
- `frontier`
- `preview_code_units`

Behavior:

- first launch defaults to Codex disconnected, saving enabled
- initial aging policy remains 32 KiB / frontier 4 / preview 1024 code units
- schema v1 preference files receive the original defaults for new policy fields
- explicit **Connect to Codex** sets `connect_on_launch = true`
- explicit **Disconnect from Codex** safely restores config and sets it false
- normal safe Quit restores config but preserves connection intent
- later app launch / Start at Login reconnects when that intent is true
- crash snapshot recovery remains authoritative when a Phase 3 snapshot survived

Saving toggle changes are persisted and also update the live transport policy when connected. Structural threshold/frontier/preview changes are applied only while disconnected so one connected session cannot silently change its aging policy midstream.

## 5.6 Start at Login — implemented

Tray exposes **Start at Login** through the Tauri autostart plugin using macOS LaunchAgent mode.

The checked state is read from the real operating-system/plugin state. Start-at-login and desired Codex connection are separate controls.

## 5.7 Request-aware safe disconnect and quit — implemented

Transport now tracks real request lifetime through the entire relayed response stream, not merely until upstream headers arrive.

A drain gate protects disconnect:

```text
stop new request admission
        ↓
check in-flight responses
        ↓
0 ──► restore Codex config ──► stop transport
│
└─ >0 ──► resume admission + refuse disconnect
```

The request handler performs a second drain check after incrementing the active counter, closing the shutdown/admission race before upstream forwarding.

Tray disables Disconnect and Quit while requests are active, and the backend independently enforces the same condition in case UI state is stale.

Every normal app exit request is intercepted. The process is allowed to exit only after safe restore + transport shutdown + telemetry flush succeeds. Drift/restoration errors leave TokenSaver running instead of knowingly stranding Codex on a dead loopback URL.

## 5.8 Shutdown/relaunch behavior — implemented

Normal safe shutdown:

1. refuse if a Codex request is active
2. begin transport drain
3. restore TokenSaver-owned Codex config
4. stop the loopback server
5. allow the content-free observation receiver a bounded drain period
6. flush numeric telemetry
7. exit while preserving `connect_on_launch`

Relaunch reconnects automatically when the user previously chose to stay connected.

## 5.9 Privacy and outward error handling — implemented

- app data files are owner-private through atomic private writes
- routine telemetry is content-free
- tray errors never intentionally expose request/result bodies
- conservative local-loopback redaction helper is present for capability-bearing diagnostics
- capability values remain limited to the active Codex config and owner-only restoration snapshot required for routing/recovery

## Phase 5 deferred validation

Still requiring the user's final validation pass:

- Rust compile/test/lint/format
- actual macOS tray visibility/layout
- native menu action behavior
- single-instance behavior
- first Connect / explicit Disconnect round trip
- normal Quit / relaunch preserving `connect_on_launch`
- Start at Login LaunchAgent behavior
- crash/restart snapshot recovery
- live saving-toggle behavior
- streamed Active → Idle request lifecycle
- Disconnect/Quit refusal during a real active request
- telemetry persistence + local-day rollover
- tray/backend state consistency
- surfaced error/capability redaction

**No test/build/lint/formatter/CI or live desktop validation command has been executed.**

---

# Phase 6 — CLI and diagnostics

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Authoritative document: `docs/CLI.md`.

Goal: expose a narrow terminal surface for status, control, persisted optimization settings/statistics, and redacted health diagnostics without creating a second proxy/runtime.

## 6.1 Unified binary dispatch — implemented

The same TokenSaver binary now has two product edges:

- no CLI command → windowless menu-bar application
- CLI command → terminal mode without constructing the Tauri UI

macOS Finder process-serial-number arguments remain treated as desktop launch rather than an accidental CLI command.

## 6.2 Owner-local runtime control channel — implemented

Live mutation commands reach the single menu-bar runtime through an owner-local Unix socket rather than starting another proxy process.

Implemented safeguards:

- finite JSON control protocol only
- no arbitrary shell-command execution
- per-user application-data location
- `0700` parent directory and `0600` socket on Unix
- bounded request/response size
- stale socket replacement only when no live runtime answers it
- runtime action responses contain only application DTOs
- no tool-result body, receipt body, bearer credential, account ID, or Codex capability in the control protocol

## 6.3 CLI commands — implemented

```text
tokensaver status
tokensaver connect
tokensaver disconnect
tokensaver saving on
tokensaver saving off
tokensaver stats
tokensaver config show
tokensaver config set min-bytes <bytes>
tokensaver config set frontier <count>
tokensaver config set preview-code-units <count>
tokensaver doctor
tokensaver uninstall [--purge-state]
tokensaver version
```

Behavior:

- `connect`, `disconnect`, and `saving` require the live menu-bar runtime
- `stats` uses live session data when available and persisted content-free counters while closed
- `config show` works live or offline
- numeric policy changes work offline or through the live runtime while Codex is disconnected
- structural policy changes are refused while Codex is connected
- saving on/off remains live-switchable
- uninstall state purge requires the runtime to be stopped and an absent restoration snapshot
- measured bytes and approximate token estimates remain labeled separately

## 6.4 Persistent policy schema — implemented

Runtime preferences schema v2 adds the optimization-policy values to the same owner-private preference source.

Guardrails:

- `min_bytes > 0`
- `frontier <= 256`
- `preview_code_units` in `64..=16384`
- v1 preferences remain readable and receive conservative defaults for new fields

## 6.5 Doctor — implemented

`tokensaver doctor` emits redacted PASS/WARN/FAIL checks for:

- Codex CLI discovery/version when available
- TokenSaver application-data privacy
- runtime-preference privacy
- savings-store privacy
- restoration snapshot privacy and snapshot/config coherence
- Codex config resolution/readability
- live TokenSaver runtime/control-channel reachability
- first-party ChatGPT Codex host reachability
- first-party OpenAI API host reachability

HTTP reachability means a first-party host returned an HTTP response; it does not claim authenticated inference success.

The authoritative Start-at-Login state remains the Tauri autostart plugin state surfaced by the tray. Doctor deliberately does not guess undocumented LaunchAgent plist naming or infer plugin state from filesystem heuristics.

## 6.6 Architecture boundary — implemented

CLI code imports application services only. Architecture-contract source now explicitly rejects `src/cli -> crate::modules::*` and `src/cli -> crate::shared::*` dependencies.

Cross-module diagnostics, settings, persisted stats, and control-socket path resolution remain behind application services.

## Phase 6 deferred validation

Still requiring the user's final validation pass:

- compile/test/lint/format
- CLI help/version smoke
- status with runtime running/stopped
- live connect/disconnect/saving commands
- active-request disconnect refusal through CLI
- live/offline stats consistency
- v1 → v2 preference migration
- config value validation
- connected structural-policy-change refusal
- stale control-socket recovery
- socket permission checks
- doctor with runtime running/stopped
- first-party reachability behavior under normal/offline network conditions
- redaction/privacy review of all CLI/doctor output
- CLI architecture-contract execution

**No test/build/lint/formatter/CI, CLI smoke, doctor, network-probe, or live runtime validation command has been executed.**

---

# Phase 7 — Packaging, update safety, and uninstall

**Status: IMPLEMENTED — VALIDATION DEFERRED**

Authoritative document: `docs/PACKAGING.md`.

Goal: produce a reproducible macOS package path and a safe update/uninstall lifecycle without adding an unsigned self-updater or weakening Codex restoration guarantees.

## 7.1 macOS bundle configuration — implemented

Implemented:

- `.app` and `.dmg` bundle targets
- Developer Tool category and product descriptions
- macOS 12 minimum target retained
- release-only icon overlay (`tauri.release.conf.json`)
- single SVG icon source (`assets/app-icon.svg`)
- generated `icons/` excluded from source control
- packaging helper `scripts/package-macos.sh`

The packaging script generates the platform icon set first, then builds `.app` + `.dmg` with the release overlay. It does not run project validation suites.

## 7.2 Signing/notarization boundary — implemented

No Apple certificate, notarization credential, private key, or updater signing key is committed.

Release signing/notarization remains a release-environment responsibility. Local/ad-hoc packages must not be represented as production-signed releases.

## 7.3 Update safety — implemented

Self-updater artifacts are intentionally disabled:

```text
bundle.createUpdaterArtifacts = false
```

MVP updates use normal macOS application replacement:

```text
normal safe Quit
  ↓
restore Codex config + flush telemetry
  ↓
replace TokenSaver.app
  ↓
launch replacement
  ↓
reconnect when connect_on_launch was preserved
```

A future self-updater requires a trusted endpoint, updater public key, protected signing material, signed artifacts, version/downgrade policy, and recovery tests before scope is widened.

## 7.4 Prepare for Uninstall — implemented

Tray adds **Prepare for Uninstall…**.

It:

1. uses explicit disconnect, including request drain and exact Codex config restoration
2. clears reconnect-on-launch intent
3. disables Start at Login through the real Tauri autostart manager
4. flushes numeric telemetry
5. exits only if all preceding steps succeed

The action is disabled during active requests and configuration drift; backend disconnect safeguards remain authoritative if UI state is stale.

## 7.5 Optional owned-state purge — implemented

CLI adds:

```text
tokensaver uninstall
tokensaver uninstall --purge-state
```

The destructive form refuses to run while the menu-bar runtime is reachable.

`src/application/maintenance.rs` removes only known TokenSaver-owned state and known atomic temp files. It is non-recursive, preserves/reports unknown entries, and removes the state directory only when empty.

An active `codex-config-snapshot.json` blocks the entire purge. Generic purge never deletes the restoration snapshot and never edits `~/.codex/config.toml`.

## Phase 7 deferred validation

Still requiring the user's final validation pass:

- compile/test/lint/format
- icon generation
- `.app` generation
- `.dmg` generation
- bundle metadata/icon inspection
- real Apple signing/notarization with release credentials
- normal Quit → app replacement → relaunch state preservation
- Prepare for Uninstall flow
- Start at Login removal during uninstall preparation
- runtime-running purge refusal
- snapshot-blocks-purge behavior
- non-recursive/unknown-entry preservation
- complete install/update/uninstall round trip

**No test/build/lint/formatter/CI, icon generation, package build, signing, notarization, or install/uninstall validation command has been executed.**

---

# Phase 8 — Hardening and release gates

**Status: NOT STARTED**

Reliability gates:

- malformed requests
- huge outputs/bodies
- memory pressure
- concurrency
- interrupted streams
- TokenSaver/Codex restart
- machine reboot/start-at-login
- forced termination / power-loss recovery

Security/privacy gates:

- loopback-only
- strict local capability
- no browser/CORS proxy surface
- owner-only sensitive state
- no credentials/capability in logs or UI
- no original result bodies in routine telemetry
- bounded logs/state
- safe temporary files

Performance/compatibility gates:

- optimizer overhead materially below saved context cost
- bounded copies of huge results
- compression/serialization benchmark
- lightweight tray refresh
- explicit supported Codex baseline
- unsupported builds detected rather than guessed

Final release gates:

1. deterministic aging suite
2. architecture-contract suite
3. telemetry/benchmark suite
4. recovery/quality structural suite
5. transport integration suite
6. config restoration/drift suite
7. desktop runtime/tray suite
8. CLI/doctor suite
9. packaging/update/uninstall suite
10. real Codex smoke test
11. compaction-bypass test
12. ON/OFF payload-diff invariant
13. tray/backend state consistency
14. privacy/log/UI/CLI redaction
15. install/uninstall round trip
16. realistic long-session savings + quality benchmark

---

# Post-MVP ideas — only if they remain in scope

Possible later improvements:

- per-tool/adaptive thresholds
- safely parsed repetitive-log compaction
- tokenizer-aware estimates
- optional bounded owner-local exact-result retention after separate privacy review
- other Responses-compatible coding-agent adapters
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

> **Run normal Codex through a small local context optimizer, safely compact old consumed tool results, make missing evidence explicit, show the user what was saved, and otherwise stay out of the way.**
