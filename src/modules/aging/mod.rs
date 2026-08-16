//! Deterministic tool-result aging domain.
//!
//! This module owns eligibility rules, aging policy, deterministic receipts,
//! and aging results. It must remain independent of Codex configuration,
//! network transport, telemetry persistence, runtime lifecycle, and UI code.
//!
//! Phase 1 introduces the implementation. No transport-specific type may be
//! required by the aging domain contract.
