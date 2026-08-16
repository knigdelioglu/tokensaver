use super::{
    OptimizationEvent, OptimizationMetrics, OptimizationOutcome, ProviderUsage, SavingsLedger,
};

fn metrics(saved: u64) -> OptimizationMetrics {
    OptimizationMetrics {
        tool_results_evaluated: 3,
        tool_results_eligible: 1,
        tool_results_compacted: 1,
        largest_tool_result_bytes: 100_000,
        bytes_before: 100_000,
        bytes_after: 4_000,
        bytes_saved: saved,
    }
}

#[test]
fn distinguishes_disabled_passthrough_and_no_eligible_result() {
    let mut ledger = SavingsLedger::default();
    ledger.record(OptimizationEvent::new(
        1_000,
        7,
        OptimizationOutcome::Disabled,
        OptimizationMetrics::default(),
    ));
    ledger.record(OptimizationEvent::new(
        1_500,
        7,
        OptimizationOutcome::NativePassthrough,
        OptimizationMetrics::default(),
    ));
    ledger.record(OptimizationEvent::new(
        2_000,
        7,
        OptimizationOutcome::EvaluatedNoEligibleResult,
        OptimizationMetrics {
            tool_results_evaluated: 4,
            largest_tool_result_bytes: 18_000,
            ..OptimizationMetrics::default()
        },
    ));

    let summary = ledger.all_time();
    assert_eq!(summary.disabled_requests, 1);
    assert_eq!(summary.native_passthrough_requests, 1);
    assert_eq!(summary.no_eligible_requests, 1);
    assert_eq!(summary.tool_results_evaluated, 4);
    assert_eq!(summary.largest_tool_result_bytes, 18_000);
}

#[test]
fn aggregates_session_and_time_ranges() {
    let mut ledger = SavingsLedger::default();
    ledger.record(OptimizationEvent::new(
        1_000,
        1,
        OptimizationOutcome::Aged,
        metrics(96_000),
    ));
    ledger.record(OptimizationEvent::new(
        2_000,
        2,
        OptimizationOutcome::Aged,
        metrics(96_000),
    ));

    assert_eq!(ledger.for_session(1).bytes_saved, 96_000);
    assert_eq!(ledger.between(1_500, 2_500).bytes_saved, 96_000);
    assert_eq!(ledger.all_time().bytes_saved, 192_000);
}

#[test]
fn provider_usage_stays_separate_from_estimated_savings() {
    let mut ledger = SavingsLedger::default();
    ledger.record(
        OptimizationEvent::new(1_000, 1, OptimizationOutcome::Aged, metrics(96_000))
            .with_provider_usage(ProviderUsage {
                input_tokens: 10_000,
                cached_input_tokens: 8_000,
            }),
    );

    let summary = ledger.all_time();
    assert_eq!(summary.estimated_tokens_saved, 24_000);
    assert_eq!(summary.provider_input_tokens, 10_000);
    assert_eq!(summary.provider_cached_input_tokens, 8_000);
    assert_eq!(summary.cache_rate_basis_points(), Some(8_000));
}

#[test]
fn empty_ledger_has_no_cache_rate() {
    let ledger = SavingsLedger::default();
    assert!(ledger.is_empty());
    assert_eq!(ledger.len(), 0);
    assert_eq!(ledger.all_time().cache_rate_basis_points(), None);
}
