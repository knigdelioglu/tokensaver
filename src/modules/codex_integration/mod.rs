//! Codex configuration integration boundary.
//!
//! This module owns detection of supported Codex configuration, reversible
//! connect/disconnect changes, configuration snapshots, restoration, and drift
//! detection. It must not choose models, own provider credentials, or implement
//! token aging itself.
