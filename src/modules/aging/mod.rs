//! Deterministic tool-result aging domain.
//!
//! This module owns eligibility rules, aging policy, deterministic receipts,
//! receipt evidence verification, and aging decisions. It remains independent
//! of Codex configuration, network transport, telemetry persistence, runtime
//! lifecycle, and UI code.
//!
//! The domain intentionally returns replacement decisions rather than mutating
//! protocol objects. A transport adapter may apply a replacement only after it
//! validates that the indexed original item and call identity still match.

mod engine;
mod model;
mod policy;
mod receipt;

pub(crate) use engine::{
    age_tool_results, AgedReplacement, AgingDecision, AgingResult, AgingSkipReason, AgingStats,
    ToolResultEvaluation,
};
pub(crate) use model::{HistoryItem, ToolOutput, ToolResultKind};
pub(crate) use policy::{
    AgingPolicy, DEFAULT_FRONTIER, DEFAULT_MIN_BYTES, DEFAULT_PREVIEW_CODE_UNITS,
};
pub(crate) use receipt::{
    parse_receipt, verify_exact_candidate, ReceiptEvidence, ReceiptParseError,
};

#[cfg(test)]
mod tests;
