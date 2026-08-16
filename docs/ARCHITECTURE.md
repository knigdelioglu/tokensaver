# TokenSaver Architecture

## Decision

TokenSaver is a **modular monolith**: one local product with explicit internal module ownership and dependency boundaries.

The architecture optimizes for three properties:

1. the tool-result aging domain stays independently testable
2. Codex transport/configuration details cannot leak into aging rules
3. the product remains a small context optimizer instead of evolving into a general model router

`SCOPE.md` is authoritative for product boundaries. This document maps those boundaries to code.

## Current code layout

```text
src/
├── lib.rs
├── application/
│   ├── mod.rs
│   ├── benchmark.rs
│   ├── measurement.rs
│   └── codex_connection.rs
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
│   │   └── tests.rs
│   ├── runtime/
│   │   └── mod.rs
│   └── diagnostics/
│       └── mod.rs
└── shared/
    ├── mod.rs
    └── filesystem.rs
```

The future macOS tray shell belongs at the product edge and calls application use cases. Runtime/tray implementation begins in Phase 5; the Phase 3 loopback server is already implemented but is deliberately not supervised by a UI/runtime shell yet.

## Module ownership

### `aging`

Owns:

- normalized transport-neutral history types
- tool-result eligibility
- aging policy
- consumed-result detection
- deterministic receipt generation
- byte accounting intrinsic to transformation
- per-result aging/skip decisions
- replacement instructions

Must not know about:

- Codex configuration files
- HTTP/WebSocket transport
- JSON protocol objects
- authentication
- telemetry persistence
- process lifecycle
- tray/UI

The domain does not mutate Codex JSON. It returns replacement instructions identified by `item index + tool-result kind + call_id`.

This is the strongest boundary in the system.

### `transport`

Owns:

- loopback HTTP listener
- caller capability authentication
- supported Responses path filtering
- browser-origin rejection
- native upstream header allow-listing
- request decompression/recompression
- Responses JSON normalization/application adapter
- WebSocket-to-HTTP fallback signal
- upstream streaming response relay
- content-free transport observations

Transport may call the aging domain, but does not own aging policy semantics.

Transport is not allowed to choose arbitrary upstreams at request time. The production connection use case supplies a fixed native upstream.

### `codex_integration`

Owns:

- Codex-home/config-path resolution
- root `openai_base_url` connect/disconnect change
- versioned local restoration snapshot
- crash-safe write ordering
- exact restoration
- config drift detection
- restart state recovery metadata

It does not choose models, replace the built-in OpenAI provider, own provider credentials, or implement aging policy.

### `telemetry`

Owns:

- content-free optimization events
- numeric metrics
- aggregation
- session/time-range/all-retained statistics
- provider-reported token/cache metadata when naturally available

It never receives original tool-result bodies or aging receipts from transport.

### `runtime`

Reserved for Phase 5 ownership of:

- server task supervision
- startup/shutdown state
- start-at-login behavior
- application lifecycle
- durable runtime state coordination

Transport owns the server mechanism; runtime will own whether/how long it runs.

### `diagnostics`

Reserved for health checks, doctor/status reporting, and redacted diagnostic snapshots.

Diagnostics must consume explicit status/application interfaces rather than bypassing ownership and inspecting module-private storage.

### `application`

Owns cross-module use cases and orchestration.

Current use cases include:

- offline benchmark orchestration (`benchmark`)
- aging/transport observation → telemetry mapping (`measurement`)
- safe native Codex connection preparation/restoration (`codex_connection`)

The native connection use case deliberately orders operations as:

```text
recover/generate endpoint
  ↓
bind transport
  ↓
durably snapshot Codex config
  ↓
install openai_base_url
```

UI and CLI call application use cases rather than module internals.

### `shared`

Contains only genuinely cross-cutting low-level primitives.

Currently:

- atomic same-directory private file replacement

`shared` must not contain aging logic, Codex parsing, telemetry business rules, or UI models.

## Dependency direction

Current intended dependency shape:

```text
future tray / CLI
       │
       ▼
 application
   ├──────────────► codex_integration ──► shared filesystem
   ├──────────────► telemetry
   └──────────────► transport ──────────► aging
                                      
 benchmark ─────────────────────────────► aging
 measurement ───────────────► transport + telemetry + aging metrics
```

`application` may coordinate modules. A module must not use another module's private persistence or implementation types as an informal API.

### Explicitly forbidden examples

```text
aging -> transport
aging -> codex_integration
aging -> telemetry
aging -> runtime
aging -> tray/UI
telemetry -> transport internals
codex_integration -> aging internals
UI -> module persistence
```

The content-free `TransportObservation` is an explicit contract, not permission for telemetry to inspect transport state directly; `application::measurement` performs that mapping.

## Public API rule

The crate-level public surface starts at `application`. Internal product modules are crate-internal. TokenSaver is an application, not a collection of accidental public libraries.

A future reusable aging library requires an explicit extraction decision rather than casually exposing all internals.

## Process model

Modular does not mean distributed.

The intended macOS product may run these capabilities in one process:

- tray shell
- local Codex transport
- aging engine
- telemetry
- runtime state
- Codex configuration integration
- diagnostics

If platform lifecycle constraints later justify a helper/service process, the same module contracts should survive that physical split.

## Failure rules

### Aging uncertainty

> **When the optimizer is uncertain, preserve the original model-visible content.**

Decode, parse, normalization, replacement-validation, serialization, or re-encoding uncertainty produces fail-original behavior rather than a partial rewrite.

### Configuration uncertainty

> **When ownership is uncertain, preserve the user's Codex configuration.**

If `openai_base_url` no longer equals the value TokenSaver installed, disconnect reports drift and refuses to overwrite it.

### Runtime ordering

Codex is never intentionally pointed to an endpoint that has not already been bound successfully.

### Sensitive state

The caller capability may be persisted only as part of owner-private TokenSaver restoration state. It must not be logged or surfaced as routine telemetry.

## Architecture change rule

A change that introduces a new module, reverses a dependency, lets aging depend on an adapter, creates a second source of configuration truth, or broadens transport into a general upstream proxy requires a documented architecture decision before implementation.
