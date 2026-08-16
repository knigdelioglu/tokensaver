# TokenSaver Architecture

## Decision

TokenSaver is a **modular monolith**: one local product with explicit internal ownership and dependency boundaries.

The architecture optimizes for:

1. independently testable deterministic aging
2. Codex transport/configuration details not leaking into aging rules
3. desktop/UI state not becoming a second backend truth
4. a small context optimizer rather than a general model router

`SCOPE.md` is authoritative for product boundaries. This document maps them to code.

## Current code layout

```text
src/
├── main.rs
├── lib.rs
├── application/
│   ├── mod.rs
│   ├── benchmark.rs
│   ├── measurement.rs
│   ├── codex_connection.rs
│   ├── desktop_runtime.rs
│   ├── recovery.rs
│   └── quality.rs
├── desktop/
│   └── mod.rs
├── modules/
│   ├── mod.rs
│   ├── aging/
│   │   ├── mod.rs
│   │   ├── model.rs
│   │   ├── policy.rs
│   │   ├── engine.rs
│   │   ├── receipt.rs
│   │   └── tests.rs
│   ├── transport/
│   │   ├── mod.rs
│   │   ├── capability.rs
│   │   ├── compression.rs
│   │   ├── headers.rs
│   │   ├── request.rs
│   │   ├── observation.rs
│   │   ├── server.rs
│   │   └── tests.rs
│   ├── codex_integration/
│   │   ├── mod.rs
│   │   ├── path.rs
│   │   ├── config.rs
│   │   └── tests.rs
│   ├── telemetry/
│   │   ├── mod.rs
│   │   ├── model.rs
│   │   ├── aggregate.rs
│   │   ├── store.rs
│   │   └── tests.rs
│   ├── runtime/
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   └── preferences.rs
│   └── diagnostics/
│       └── mod.rs
└── shared/
    ├── mod.rs
    ├── filesystem.rs
    └── security.rs
```

## Edge-to-core flow

```text
Tauri menu-bar shell
        ↓
application::desktop_runtime
        ↓
 ┌──────┼───────────────┐
 ↓      ↓               ↓
runtime telemetry  codex_connection
                         ↓
                 codex_integration
                         +
                     transport
                         ↓
                       aging
```

The shell does not open module storage, parse Codex config, or mutate transport state directly.

## Module ownership

### `aging`

Owns:

- transport-neutral normalized history types
- tool-result eligibility
- aging policy
- consumed-result detection
- deterministic receipt generation
- receipt evidence parsing/identity verification
- transformation byte accounting
- per-result decisions
- replacement instructions

Must not know about:

- Codex configuration
- HTTP/WebSocket transport
- protocol JSON objects
- authentication
- telemetry persistence
- runtime lifecycle
- Tauri/tray UI

The domain returns replacement instructions identified by `item index + result kind + call_id`; it does not mutate Codex JSON.

### `transport`

Owns:

- loopback HTTP server
- local capability authentication
- finite native route/method allow-list
- browser-origin rejection
- upstream header allow-list
- request decompression/recompression for aging inspection
- Responses JSON adapter
- WebSocket → HTTP fallback signal
- fixed first-party upstream relay
- streamed response relay
- real in-flight request count
- request drain gate
- content-free transport observations

Transport may call aging through its explicit contract. It does not own persistent settings, tray state, or Codex config files.

### `codex_integration`

Owns:

- Codex-home/config-path resolution
- reversible root `openai_base_url` management
- only-missing realtime bypass management
- versioned owner-private restoration snapshot
- crash-safe write ordering
- exact restoration/removal
- per-owned-key drift detection
- persisted endpoint recovery metadata

It does not choose models, own provider credentials, or implement aging.

### `telemetry`

Owns:

- content-free optimization events
- numeric metrics
- aggregation
- provider token/cache metadata when naturally available
- bounded persistent daily/all-time numeric savings state
- last-optimization numeric metadata

It never receives/persists original tool-result bodies or receipt bodies as telemetry.

### `runtime`

Owns local runtime state and user lifecycle preferences only:

- service state
- Codex connection presentation state
- saving preference
- reconnect-on-launch intent
- current active-request presentation count

It deliberately does **not** start transport, edit Codex config, or query telemetry itself. Those cross-module operations belong in `application`.

### `diagnostics`

Reserved for Phase 6 health/doctor/status reporting.

Diagnostics must consume explicit application/status contracts rather than inspecting private storage ad hoc.

### `application`

Owns cross-module use cases/orchestration.

Current use cases:

- offline benchmark orchestration (`benchmark`)
- transport/aging → telemetry mapping (`measurement`)
- safe native Codex connection transaction (`codex_connection`)
- desktop lifecycle + runtime snapshot composition (`desktop_runtime`)
- explicit receipt recovery assessment (`recovery`)
- deterministic quality fixtures (`quality`)

`desktop_runtime` composes runtime, Codex connection, transport control, and telemetry while returning presentation-safe DTOs to the desktop shell.

### `desktop`

Owns only native presentation and user intent:

- Tauri application lifecycle integration
- menu-bar menu construction
- periodic presentation refresh
- Connect / Disconnect intent
- saving toggle intent
- Start at Login intent
- safe Quit request
- formatting measured/estimated savings
- outward error redaction

It must not import module persistence or aging/transport internals directly.

### `shared`

Contains genuinely cross-cutting low-level primitives only.

Current responsibilities:

- atomic owner-private file replacement
- conservative local secret redaction for outward text

It must not contain aging policy, Codex parsing, telemetry business rules, or UI state.

## Dependency direction

```text
desktop ───────────────► application
                              │
           ┌──────────────────┼──────────────────┐
           ▼                  ▼                  ▼
        runtime          telemetry       codex_integration
           ▲                  ▲                  ▲
           │                  │                  │
           └──────── application ────────────────┤
                                                 ▼
                                             transport
                                                 ▼
                                               aging

application::benchmark ────────────────────────► aging
application::recovery / quality ───────────────► aging
shared ◄──── low-level filesystem/security users only
```

### Explicitly forbidden

```text
aging -> transport
aging -> codex_integration
aging -> telemetry
aging -> runtime
aging -> desktop/UI
telemetry -> transport internals
codex_integration -> aging internals
desktop -> module persistence
desktop -> aging/transport internals
runtime -> transport/codex persistence
```

The content-free `TransportObservation` is an explicit contract; `application::measurement` performs the mapping into telemetry.

## External/public API rule

TokenSaver is an application, not an accidental Rust SDK.

The crate's intentional external surface is currently the desktop entry function used by the binary:

```text
run_desktop()
```

`application`, `modules`, and `shared` remain crate-internal.

A reusable aging library would require an explicit extraction/API decision.

## Process model

Modular does not mean distributed.

The macOS MVP runs in one process:

- Tauri menu-bar shell
- desktop runtime controller
- local Codex transport
- aging engine
- telemetry aggregation/persistence
- runtime preferences/state
- Codex config integration

Single-instance protection prevents a second desktop process from creating a competing loopback/config transaction.

If a helper/service process is later justified, module contracts should survive that physical split.

## Lifecycle contracts

### Connect ordering

```text
recover/generate local endpoint
        ↓
bind transport successfully
        ↓
durably write restoration snapshot
        ↓
install TokenSaver-owned Codex values
        ↓
start supervised runtime state
```

Codex is never intentionally pointed at an endpoint that did not bind successfully.

### Disconnect ordering

```text
begin request drain
        ↓
active requests? ── yes ──► refuse + resume admission
        │
        no
        ↓
restore owned Codex config
        ↓
stop server
        ↓
drain content-free observations (bounded)
        ↓
flush numeric telemetry
```

### Normal Quit

Normal exit passes through safe disconnect logic. `connect_on_launch` is preserved so a later user launch/autostart may reconnect, while the temporary Codex base URL is still restored before this process exits.

Explicit Disconnect additionally clears reconnect-on-launch intent.

## Failure rules

### Aging uncertainty

> **Preserve the original model-visible content.**

Decode/parse/normalization/replacement/serialization/re-encoding uncertainty produces fail-original behavior.

### Recovery uncertainty

> **Never present omitted middle content as exact without exact-source verification.**

### Configuration uncertainty

> **Preserve the user's Codex configuration.**

Drift refuses automatic destructive restoration.

### Runtime uncertainty

> **Do not knowingly exit into a broken Codex configuration.**

A normal exit is prevented when active requests, config drift, or restoration failure makes safe detach impossible.

### UI uncertainty

> **Backend evidence wins over UI intent.**

Tray checkmarks/menu text are refreshed from application state and OS autostart state rather than treated as truth themselves.

## Sensitive state

The caller capability may exist only where routing/recovery requires it:

- active managed Codex config
- owner-private restoration snapshot
- in-memory transport state

It must not enter routine telemetry or outward status text. Shared redaction is applied to surfaced local URL errors.

## Architecture change rule

A change that introduces a new module, reverses a dependency, lets aging depend on an adapter, creates a second source of configuration truth, or broadens transport into a general upstream proxy requires a documented architecture decision before implementation.
