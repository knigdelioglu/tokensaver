use crate::modules::aging::AgingStats;

use super::request::PreparationOutcome;

/// Content-free runtime evidence emitted after TokenSaver evaluates one request.
/// Receipts and original tool-result text deliberately never cross this boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransportObservation {
    pub(crate) outcome: PreparationOutcome,
    pub(crate) aging_stats: AgingStats,
}
