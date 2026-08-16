//! Application composition boundary.
//!
//! Cross-module use cases belong here. UI, CLI, and future platform shells call
//! application services rather than reaching into module internals.
//!
//! Phase 2 adds measurement and offline benchmark orchestration here so the
//! aging domain remains telemetry-agnostic and telemetry never reaches into
//! aging or transport internals.

pub(crate) mod benchmark;
pub(crate) mod measurement;
