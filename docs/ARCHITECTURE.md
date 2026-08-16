# TokenSaver Architecture

## Decision

TokenSaver is a **modular monolith**: one local product with explicit internal module ownership and dependency boundaries.

The architecture optimizes for three properties:

1. The tool-result aging domain stays independently testable.
2. Codex transport/configuration details cannot leak into the aging rules.
3. The product can remain a small desktop utility instead of evolving into a general model router.

`SCOPE.md` is authoritative for product boundaries. This document describes how those boundaries map to code.

## Current code layout

```text
src/
├── lib.rs
├── application/
│   └── mod.rs
├── modules/
│   ├── mod.rs
│   ├── aging/
│   │   └── mod.rs
│   ├── transport/
│   │   └── mod.rs
│   ├── codex_integration/
│   │   └── mod.rs
│   ├── telemetry/
│   │   └── mod.rs
│   ├── runtime/
│   │   └── mod.rs
│   └── diagnostics/
│       └── mod.rs
└── shared/
    └── mod.rs
```

The future macOS tray shell belongs at the product edge and will call the application layer. It is intentionally not implemented during Phase 0.

## Module ownership

### `aging`

Owns:

- tool-result eligibility
- aging policy
- consumed-result detection
- deterministic receipt generation
- byte accounting intrinsic to the transformation
- aging result types

Must not know about:

- Codex configuration files
- HTTP/WebSocket transport
- authentication
- telemetry persistence
- process lifecycle
- tray/UI

This is the strongest boundary in the system.

### `transport`

Owns:

- loopback request handling
- request decompression/recompression where required
- Responses request/stream compatibility
- cancellation/abort propagation
- transport-level security controls

It may invoke the aging behavior through an explicit contract, but it does not own aging policy.

### `codex_integration`

Owns:

- supported Codex installation/configuration detection
- connect/disconnect configuration changes
- snapshots of TokenSaver-owned changes
- exact restoration
- drift detection

It does not choose models and does not own provider credentials.

### `telemetry`

Owns:

- non-content savings events
- aggregation
- session/day/all-time statistics
- provider-reported token/cache metadata when naturally available

It must not persist full tool-result bodies in routine telemetry.

### `runtime`

Owns:

- local service lifecycle
- startup/shutdown state
- later start-at-login behavior
- local runtime supervision

### `diagnostics`

Owns:

- health checks
- doctor/status reporting
- redacted diagnostic snapshots

Diagnostics should consume explicit status interfaces rather than bypass module ownership and inspect private storage.

### `application`

Owns cross-module use cases and orchestration.

Examples in later phases:

- connect TokenSaver to Codex
- enable/disable token saving
- process one native Responses request
- obtain a savings/status snapshot
- disconnect and restore Codex configuration

UI and CLI call the application layer rather than module internals.

### `shared`

Contains only genuinely cross-cutting low-level primitives. It is not a generic place for domain helpers.

Appropriate examples:

- common error primitives
- filesystem safety helpers
- security primitives

Inappropriate examples:

- aging eligibility logic
- Codex config parsing
- telemetry business rules
- tray view models

## Dependency direction

Target dependency shape:

```text
tray / CLI
    │
    ▼
application
    ├──────────────► codex_integration
    ├──────────────► runtime
    ├──────────────► telemetry
    ├──────────────► diagnostics
    └──────────────► transport ─────► aging
```

The application layer may coordinate modules. A module must not use another module's private persistence or internal types as an informal API.

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

## Public API rule

The crate-level public surface starts at `application`. Internal product modules are crate-internal. This is intentional: TokenSaver is an application, not a grab bag of unrelated public libraries.

A future need for an independently reusable aging library may be handled by an explicit architecture decision and extraction, not by casually making all internals public.

## Process model

Modular does not mean distributed.

The initial product may run these capabilities in one process:

- local Codex transport
- aging engine
- telemetry
- runtime state
- configuration integration
- tray shell

If platform lifecycle constraints later justify a helper/service process, module contracts should be preserved across that physical split.

## Failure rule

Module boundaries must reinforce TokenSaver's core safety rule:

> **When the optimizer is uncertain, preserve the original model-visible content.**

Transport failures, parsing ambiguity, unsupported result shapes, and configuration drift must not be converted into guessed context rewrites.

## Architecture change rule

A change that introduces a new module, reverses a dependency, lets the aging domain depend on an adapter, or creates a second source of configuration truth requires a documented architecture decision before implementation.
