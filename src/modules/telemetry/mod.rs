//! Savings telemetry and aggregation boundary.
//!
//! This module owns non-content optimization events, aggregation, and statistics.
//! Routine telemetry must never persist original tool-result bodies. It consumes
//! explicit metrics produced by application use cases rather than reaching into
//! transport internals.
