use crate::modules::{
    aging::{AgingResult, AgingStats},
    telemetry::{OptimizationEvent, OptimizationMetrics, OptimizationOutcome, ProviderUsage},
    transport::{PreparationOutcome, TransportObservation},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OptimizationRunState {
    Disabled,
    Bypassed,
    Evaluated,
}

/// Cross-module mapper used by offline/direct aging use cases.
pub(crate) fn event_from_aging(
    observed_at_epoch_ms: u64,
    session_id: u64,
    run_state: OptimizationRunState,
    result: &AgingResult,
    provider_usage: Option<ProviderUsage>,
) -> OptimizationEvent {
    let outcome = match run_state {
        OptimizationRunState::Disabled => OptimizationOutcome::Disabled,
        OptimizationRunState::Bypassed => OptimizationOutcome::Bypassed,
        OptimizationRunState::Evaluated if result.stats.tool_results_aged > 0 => {
            OptimizationOutcome::Aged
        }
        OptimizationRunState::Evaluated if result.stats.tool_results_eligible > 0 => {
            OptimizationOutcome::EvaluatedNoSavings
        }
        OptimizationRunState::Evaluated => OptimizationOutcome::EvaluatedNoEligibleResult,
    };

    build_event(
        observed_at_epoch_ms,
        session_id,
        outcome,
        &result.stats,
        provider_usage,
    )
}

/// Cross-module mapper used by the real loopback transport. Transport emits no
/// body/receipt content; telemetry receives only the content-free outcome and
/// numeric aging statistics.
pub(crate) fn event_from_transport_observation(
    observed_at_epoch_ms: u64,
    session_id: u64,
    observation: &TransportObservation,
    provider_usage: Option<ProviderUsage>,
) -> OptimizationEvent {
    let outcome = match observation.outcome {
        PreparationOutcome::Disabled => OptimizationOutcome::Disabled,
        PreparationOutcome::CompactionBypass => OptimizationOutcome::Bypassed,
        PreparationOutcome::NativePassthrough => OptimizationOutcome::NativePassthrough,
        PreparationOutcome::FailOriginal => OptimizationOutcome::FailOriginal,
        PreparationOutcome::EvaluatedNoEligibleResult => {
            OptimizationOutcome::EvaluatedNoEligibleResult
        }
        PreparationOutcome::EvaluatedNoSavings => OptimizationOutcome::EvaluatedNoSavings,
        PreparationOutcome::Aged => OptimizationOutcome::Aged,
    };

    build_event(
        observed_at_epoch_ms,
        session_id,
        outcome,
        &observation.aging_stats,
        provider_usage,
    )
}

fn build_event(
    observed_at_epoch_ms: u64,
    session_id: u64,
    outcome: OptimizationOutcome,
    stats: &AgingStats,
    provider_usage: Option<ProviderUsage>,
) -> OptimizationEvent {
    let metrics = OptimizationMetrics {
        tool_results_evaluated: stats.tool_results_evaluated as u64,
        tool_results_eligible: stats.tool_results_eligible as u64,
        tool_results_compacted: stats.tool_results_aged as u64,
        largest_tool_result_bytes: stats.largest_tool_result_bytes as u64,
        bytes_before: stats.tool_result_bytes_before as u64,
        bytes_after: stats.tool_result_bytes_after as u64,
        bytes_saved: stats.tool_result_bytes_saved as u64,
    };

    let event = OptimizationEvent::new(observed_at_epoch_ms, session_id, outcome, metrics);
    provider_usage.map_or(event, |usage| event.with_provider_usage(usage))
}
