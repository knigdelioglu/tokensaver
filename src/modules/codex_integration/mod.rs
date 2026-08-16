//! Codex configuration integration boundary.
//!
//! This module owns detection of supported Codex configuration, reversible
//! connect/disconnect changes, configuration snapshots, restoration, and drift
//! detection. It must not choose models, own provider credentials, or implement
//! token aging itself.
//!
//! Current supported Codex builds expose a root `openai_base_url` override for
//! the built-in OpenAI provider. TokenSaver owns only that key. It deliberately
//! does not create or replace `model_providers.openai`, which preserves Codex's
//! built-in authentication and provider capabilities.

mod config;
mod path;

pub(crate) use config::{
    connect_config_file, connection_state_file, disconnect_config_file, CodexConfigError,
    CodexConfigSnapshot, CodexConnectionState, OriginalOpenAiBaseUrl,
};
pub(crate) use path::{codex_config_path, CodexPathError};

#[cfg(test)]
mod tests;
