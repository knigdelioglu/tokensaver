//! Application composition boundary.
//!
//! Cross-module use cases belong here. UI, CLI, and platform shells call
//! application services rather than reaching into module internals.
//!
//! Measurement/benchmark orchestration keeps telemetry independent from aging.
//! Native Codex connection orchestration binds transport before applying the
//! reversible Codex configuration change. Desktop runtime composition joins
//! lifecycle, telemetry, and Codex transport without weakening module borders.

pub(crate) mod benchmark;
pub(crate) mod codex_connection;
pub(crate) mod desktop_runtime;
pub(crate) mod measurement;
pub(crate) mod quality;
pub(crate) mod recovery;
