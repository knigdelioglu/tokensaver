use crate::modules::aging::AgingStats;

use super::request::{PreparationOutcome, RequestDiagnostics};
use super::response_usage::ProviderUsageObservation;

/// Content-free runtime evidence emitted after TokenSaver evaluates one request
/// and, for completed Responses streams, observes provider usage metadata.
///
/// Receipts, original tool-result text, prompt text, response IDs and
/// credentials deliberately never cross this boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransportObservation {
    pub(crate) outcome: PreparationOutcome,
    pub(crate) aging_stats: AgingStats,
    pub(crate) request: Option<RequestDiagnostics>,
    pub(crate) provider_usage: Option<ProviderUsageObservation>,
}
