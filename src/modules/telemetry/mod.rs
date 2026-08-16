//! Savings telemetry and aggregation boundary.
//!
//! This module owns non-content optimization events, aggregation, statistics,
//! and durable numeric savings state. It must never persist original tool-result
//! bodies, receipts, prompts, credentials, or caller capability secrets.

mod aggregate;
mod model;
mod store;

pub(crate) use aggregate::{SavingsLedger, SavingsSummary};
pub(crate) use model::{
    OptimizationEvent, OptimizationMetrics, OptimizationOutcome, ProviderUsage,
};
pub(crate) use store::{DurableSavingsStore, LastOptimization, SavingsStoreError};

#[cfg(test)]
mod tests;
