//! Internal product modules.
//!
//! These modules form one deployable product but retain explicit ownership.
//! Cross-module orchestration belongs in `crate::application`.

pub(crate) mod aging;
pub(crate) mod codex_integration;
pub(crate) mod diagnostics;
pub(crate) mod runtime;
pub(crate) mod telemetry;
pub(crate) mod transport;
