use crate::modules::{
    aging::AgingResult,
    telemetry::{
        OptimizationEvent, OptimizationMetrics, OptimizationOutcome, ProviderUsage,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OptimizationRunState {
    Disabled,
    Bypassed,
    Evaluated,
}

/// Cross-module mapper. Aging stays telemetry-agnostic; telemetry receives only
/// content-free numeric metrics selected here by the application layer.
pub(crate) fn event_from_aging(
    observed_at_epoch_ms: u64,
    session_id: u64,
    run_state: OptimizationRunState,
    result: &AgingResult,
    provider_usage: Option<ProviderUsage>,
) -> OptimizationEvent {
    let stats = &result.stats;
    let metrics = OptimizationMetrics {
        tool_results_evaluated: stats.tool_results_evaluated as u64,
        tool_results_eligible: stats.tool_results_eligible as u64,
        tool_results_compacted: stats.tool_results_aged as u64,
        largest_tool_result_bytes: stats.largest_tool_result_bytes as u64,
        bytes_before: stats.tool_result_bytes_before as u64,
        bytes_after: stats.tool_result_bytes_after as u64,
        bytes_saved: stats.tool_result_bytes_saved as u64,
    };

    let outcome = match run_state {
        OptimizationRunState::Disabled => OptimizationOutcome::Disabled,
        OptimizationRunState::Bypassed => OptimizationOutcome::Bypassed,
        OptimizationRunState::Evaluated if stats.tool_results_aged > 0 => {
            OptimizationOutcome::Aged
        }
        OptimizationRunState::Evaluated if stats.tool_results_eligible > 0 => {
            OptimizationOutcome::EvaluatedNoSavings
        }
        OptimizationRunState::Evaluated => OptimizationOutcome::EvaluatedNoEligibleResult,
    };

    let event = OptimizationEvent::new(observed_at_epoch_ms, session_id, outcome, metrics);
    provider_usage.map_or(event, |usage| event.with_provider_usage(usage))
}
