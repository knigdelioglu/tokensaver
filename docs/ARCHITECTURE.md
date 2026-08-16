# TokenSaver Architecture

## Decision

TokenSaver is a **modular monolith**: one local product with explicit internal ownership and dependency boundaries.

The architecture optimizes for:

1. independently testable deterministic aging
2. Codex transport/configuration details not leaking into aging rules
3. desktop/CLI presentation not becoming a second backend truth
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
│   ├── codex_connection.rs
│   ├── control.rs
│   ├── desktop_runtime.rs
│   ├── doctor.rs
│   ├── measurement.rs
│   ├── quality.rs
│   ├── recovery.rs
│   ├── runtime_client.rs
│   ├── settings.rs
│   └── stats.rs
├── desktop/
│   └── mod.rs
├── cli/
│   └── mod.rs
├── modules/
│   ├── aging/
│   │   ├── model.rs
│   │   ├── policy.rs
│   │   ├── engine.rs
│   │   ├── receipt.rs
│   │   └── tests.rs
│   ├── transport/
│   │   ├── capability.rs
│   │   ├── compression.rs
│   │   ├── headers.rs
│   │   ├── request.rs
│   │   ├── observation.rs
│   │   ├── server.rs
│   │   └── tests.rs
│   ├── codex_integration/
│   │   ├── path.rs
│   │   ├── config.rs
│   │   └── tests.rs
│   ├── telemetry/
│   │   ├── model.rs
│   │   ├── aggregate.rs
│   │   ├── store.rs
│   │   └── tests.rs
│   ├── runtime/
│   │   ├── state.rs
│   │   └── preferences.rs
│   └── diagnostics/
│       └── mod.rs
└── shared/
    ├── filesystem.rs
    ├── paths.rs
    └── security.rs
```

## Edge-to-core flow

```text
Tauri menu-bar shell ─┐
                      ├────► application services
CLI shell ────────────┘              │
                         ┌────────────┼─────────────┐
                         ▼            ▼             ▼
                      runtime     telemetry   codex_integration
                                                  +
                                              transport
                                                  ▼
                                                aging
```

Both product edges express user intent. Neither opens module persistence, parses Codex config, or mutates transport internals directly.

## Module ownership

### `aging`

Owns:

- transport-neutral normalized history
- eligibility/consumption/frontier policy
- deterministic receipt generation
- receipt evidence parsing and exact-candidate verification
- transformation byte accounting
- replacement instructions

Must not know about Codex config, HTTP, protocol JSON, authentication, telemetry, runtime, desktop, or CLI.

### `transport`

Owns:

- loopback HTTP server
- local capability authentication
- finite native route/method allow-list
- browser-origin rejection
- upstream header allow-list
- request compression adaptation required for aging
- Responses JSON adapter
- WebSocket → HTTP fallback signal
- fixed first-party relay
- full-stream in-flight request count and drain gate
- content-free transport observations

Transport may call aging through its explicit domain contract. It does not own persistent user settings or Codex config files.

### `codex_integration`

Owns:

- Codex-home/config-path resolution
- reversible `openai_base_url` management
- only-missing realtime bypass management
- versioned owner-private restoration snapshot
- crash-safe write ordering
- exact restore/removal and drift detection
- persisted endpoint recovery metadata

It does not choose models, own credentials, or implement aging.

### `telemetry`

Owns:

- content-free optimization events
- numeric aggregation
- provider token/cache metadata when naturally available
- bounded persistent daily/all-time numeric savings state
- last-optimization numeric metadata

It never receives/persists original tool-result or receipt bodies as telemetry.

### `runtime`

Owns local runtime state and persisted lifecycle/optimizer preferences only:

- service/Codex presentation state
- saving preference
- reconnect-on-launch intent
- `min_bytes`
- `frontier`
- `preview_code_units`
- active-request presentation count

Runtime deliberately does **not** import the aging domain. Persistence defaults are duplicated at this boundary and an application-layer authored contract test keeps them aligned with `AgingPolicy::default()` without reversing the dependency.

Runtime does not start transport, edit Codex config, or query telemetry; application orchestration does that.

### `diagnostics`

Owns redacted primitive checks such as:

- Codex executable/version discovery
- owner-private file permission checks
- file readability
- bounded first-party host reachability probes

It does not expose credentials, transport capability, result bodies, or receipt bodies.

### `application`

Owns all cross-module orchestration and product-edge DTOs.

Current use cases:

- `benchmark` — offline aging fixtures
- `measurement` — transport/aging metrics → telemetry
- `codex_connection` — safe Codex connection transaction
- `desktop_runtime` — lifecycle, transport supervision, telemetry and presentation snapshot
- `control` — finite owner-local runtime control protocol/server
- `runtime_client` — CLI-facing live runtime client
- `doctor` — diagnostics composition into redacted application DTOs
- `settings` — owner-private optimization preference access
- `stats` — persisted content-free stats access
- `recovery` — explicit receipt recovery assessment
- `quality` — deterministic quality fixtures

This is the only layer allowed to compose multiple product modules.

### `desktop`

Owns native menu-bar presentation and user intent:

- Tauri lifecycle
- tray menu
- periodic refresh
- Connect/Disconnect
- saving toggle
- Start at Login
- safe Quit
- formatting/redaction

It imports application services only, not product modules.

### `cli`

Owns terminal parsing/formatting and user intent only:

- command selection
- human-readable output
- process exit codes

It imports application services/DTOs only. Architecture-contract source rejects `cli -> modules` and `cli -> shared` dependencies.

Live mutation does not start another proxy. It reaches the single menu-bar runtime through `application::runtime_client` and the finite owner-local control protocol.

### `shared`

Contains genuinely cross-cutting low-level primitives only:

- atomic owner-private file replacement
- canonical TokenSaver per-user data/control paths
- conservative outward local-secret redaction

It contains no domain policy or orchestration.

## Dependency direction

```text
desktop ─┐
         ├────────► application ─────────► modules
CLI ─────┘                │
                          └──────────────► shared low-level helpers

transport ───────────────► aging
application::benchmark ──► aging
application::recovery ───► aging
application::quality ────► aging
```

### Explicitly forbidden

```text
aging -> transport/codex/telemetry/runtime/desktop/CLI
telemetry -> transport internals
codex_integration -> aging internals
runtime -> aging/transport/codex/telemetry/application/edges
desktop -> product modules
CLI -> product modules
CLI -> shared persistence/path internals
shared -> product modules/application/edges
```

`TransportObservation` and application DTOs are explicit contracts; they are not permission to inspect another module's private state.

## Public API rule

TokenSaver is an application, not an accidental Rust SDK.

The external crate surface used by the binary is intentionally tiny:

```text
should_run_cli(args)
run_cli(args)
run_desktop()
```

`application`, `modules`, `shared`, `desktop`, and `cli` remain implementation details.

## Process model

Modular does not mean distributed.

The macOS MVP uses one long-lived menu-bar process for:

- Tauri tray
- application controller
- local Codex transport
- aging
- telemetry
- runtime preferences/state
- Codex configuration integration
- owner-local CLI control server

A CLI invocation is a short-lived client process only. It never becomes a second inference proxy.

Single-instance protection prevents competing desktop runtimes. The control socket independently prevents a second live control owner from silently replacing a responding socket.

## Lifecycle contracts

### Connect

```text
recover/generate endpoint
      ↓
bind transport
      ↓
write restoration snapshot
      ↓
install TokenSaver-owned Codex values
      ↓
publish connected runtime state
```

### Disconnect

```text
begin request drain
      ↓
active requests? ─ yes ─► refuse + resume admission
      │
      no
      ↓
restore owned Codex config
      ↓
stop server
      ↓
drain observations (bounded)
      ↓
flush numeric telemetry
```

### Normal Quit

Normal exit passes through safe disconnect but preserves `connect_on_launch`, allowing a later user launch/autostart to reconnect after Codex config was safely restored.

Explicit Disconnect additionally clears reconnect intent.

### CLI control

```text
CLI
 ↓
application::runtime_client
 ↓
owner-only control.sock
 ↓
application::control
 ↓
DesktopRuntimeController
```

The protocol is finite JSON, size bounded, and contains no arbitrary command execution.

## Failure rules

### Aging uncertainty
**Preserve original model-visible content.**

### Recovery uncertainty
**Never present omitted middle bytes as exact without source verification.**

### Configuration uncertainty
**Preserve the user's Codex configuration; drift refuses destructive restore.**

### Runtime uncertainty
**Do not knowingly exit into a broken Codex configuration.**

### UI/CLI uncertainty
**Backend/application evidence wins over presentation state.**

### Diagnostic uncertainty
**Warn rather than invent unsupported facts.** For example, doctor does not guess an undocumented autostart plist name; the running tray's official autostart plugin state remains authoritative.

## Sensitive state

The Codex transport capability may exist only where routing/recovery requires it:

- active managed Codex config
- owner-private restoration snapshot
- in-memory transport state

It must not enter routine telemetry, control-protocol DTOs, CLI output, doctor output, or tray status text.

The CLI control socket is owner-only and carries only operation/status DTOs.

## Architecture change rule

A change that introduces a module, reverses a dependency, lets aging depend on an adapter, creates another configuration source of truth, or broadens transport/control into a general proxy/command channel requires an explicit architecture decision before implementation.
