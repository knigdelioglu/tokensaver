//! Local runtime and lifecycle boundary.
//!
//! This module owns process/service state and durable runtime preferences. It
//! deliberately does not start Codex transport or read telemetry directly;
//! cross-module lifecycle composition belongs in the application layer.

mod preferences;
mod state;

pub(crate) use preferences::{
    RuntimePreferences, RuntimePreferencesError, RuntimePreferencesStore,
};
pub(crate) use state::{CodexStatus, RuntimeStatus, RuntimeStatusStore, ServiceStatus};
