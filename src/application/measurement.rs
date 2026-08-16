#![allow(dead_code)]

use crate::modules::{
    aging::{AgingResult, AgingStats},
    telemetry::{OptimizationEvent, OptimizationMetrics, OptimizationOutcome, ProviderUsage},
    transport::{PreparationOutcome, RequestDiagnostics, TransportObservation},
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
        None,
        provider_usage,
    )
}

/// Cross-module mapper used by the real loopback transport. Transport emits no
/// body/receipt content; telemetry receives only content-free request shape,
/// numeric aging statistics, and provider-reported token counters observed from
/// the unchanged response stream.
///
/// `provider_usage` remains as a compatibility/fallback argument for offline
/// callers. Live transport usage, when present, is authoritative and wins.
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
    let live_provider_usage = observation.provider_usage.map(|usage| ProviderUsage {
        input_tokens: usage.input_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        output_tokens: usage.output_tokens,
    });

    build_event(
        observed_at_epoch_ms,
        session_id,
        outcome,
        &observation.aging_stats,
        observation.request.as_ref(),
        live_provider_usage.or(provider_usage),
    )
}

fn build_event(
    observed_at_epoch_ms: u64,
    session_id: u64,
    outcome: OptimizationOutcome,
    stats: &AgingStats,
    diagnostics: Option<&RequestDiagnostics>,
    provider_usage: Option<ProviderUsage>,
) -> OptimizationEvent {
    let diagnostics_largest =
        diagnostics.map_or(0, |value| value.largest_textual_tool_result_bytes as u64);
    let metrics = OptimizationMetrics {
        tool_results_evaluated: stats.tool_results_evaluated as u64,
        tool_results_eligible: stats.tool_results_eligible as u64,
        tool_results_compacted: stats.tool_results_aged as u64,
        largest_tool_result_bytes: (stats.largest_tool_result_bytes as u64)
            .max(diagnostics_largest),
        protected_frontier: diagnostics.map_or(0, |value| value.protected_frontier as u64),
        unsupported_output: diagnostics.map_or(0, |value| value.unsupported_output as u64),
        at_or_below_threshold: diagnostics.map_or(0, |value| value.at_or_below_threshold as u64),
        unconsumed: diagnostics.map_or(0, |value| value.unconsumed as u64),
        receipt_not_smaller: diagnostics.map_or(0, |value| value.receipt_not_smaller as u64),
        responses_with_previous_response_id: diagnostics
            .is_some_and(|value| value.has_previous_response_id)
            as u64,
        responses_without_previous_response_id: diagnostics
            .is_some_and(|value| !value.has_previous_response_id)
            as u64,
        previous_response_id_preserved: diagnostics
            .is_some_and(|value| value.previous_response_id_preserved)
            as u64,
        aging_pass_ran: diagnostics.is_some_and(|value| value.aging_pass_ran) as u64,
        input_items: diagnostics.map_or(0, |value| value.input_items as u64),
        function_call_outputs: diagnostics.map_or(0, |value| value.function_call_outputs as u64),
        custom_tool_call_outputs: diagnostics
            .map_or(0, |value| value.custom_tool_call_outputs as u64),
        textual_tool_result_bytes_seen: diagnostics
            .map_or(0, |value| value.textual_tool_result_bytes as u64),
        bytes_before: stats.tool_result_bytes_before as u64,
        bytes_after: stats.tool_result_bytes_after as u64,
        bytes_saved: stats.tool_result_bytes_saved as u64,
    };

    let event = OptimizationEvent::new(observed_at_epoch_ms, session_id, outcome, metrics);
    provider_usage.map_or(event, |usage| event.with_provider_usage(usage))
}
