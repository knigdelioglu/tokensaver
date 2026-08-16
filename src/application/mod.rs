//! Application composition boundary.
//!
//! Cross-module use cases belong here. UI, CLI, and future platform shells call
//! application services rather than reaching into module internals.
//!
//! Measurement/benchmark orchestration keeps telemetry independent from aging.
//! Native Codex connection orchestration binds transport before applying the
//! reversible Codex configuration change. Recovery use cases expose receipt
//! evidence without creating a broad store of original tool-result bodies.

pub(crate) mod benchmark;
pub(crate) mod codex_connection;
pub(crate) mod measurement;
pub(crate) mod quality;
pub(crate) mod recovery;
