use serde::{Deserialize, Serialize};

use super::model::{OptimizationEvent, OptimizationOutcome};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProviderCacheSummary {
    pub(crate) usage_events: u64,
    pub(crate) input_tokens: u64,
    pub(crate) cached_input_tokens: u64,
}

impl ProviderCacheSummary {
    pub(crate) fn observe(&mut self, input_tokens: u64, cached_input_tokens: u64) {
        self.usage_events = self.usage_events.saturating_add(1);
        self.input_tokens = self.input_tokens.saturating_add(input_tokens);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(cached_input_tokens.min(input_tokens));
    }

    pub(crate) fn rate_basis_points(self) -> Option<u64> {
        if self.input_tokens == 0 {
            return None;
        }
        Some(self.cached_input_tokens.saturating_mul(10_000) / self.input_tokens)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SavingsSummary {
    pub(crate) events: u64,
    pub(crate) aged_requests: u64,
    pub(crate) disabled_requests: u64,
    pub(crate) bypassed_requests: u64,
    pub(crate) native_passthrough_requests: u64,
    pub(crate) fail_original_requests: u64,
    pub(crate) no_eligible_requests: u64,
    pub(crate) no_savings_requests: u64,
    pub(crate) tool_results_evaluated: u64,
    pub(crate) tool_results_eligible: u64,
    pub(crate) tool_results_compacted: u64,
    pub(crate) largest_tool_result_bytes: u64,
    pub(crate) protected_frontier: u64,
    pub(crate) unsupported_output: u64,
    pub(crate) at_or_below_threshold: u64,
    pub(crate) unconsumed: u64,
    pub(crate) receipt_not_smaller: u64,
    pub(crate) responses_with_previous_response_id: u64,
    pub(crate) responses_without_previous_response_id: u64,
    pub(crate) previous_response_id_preserved: u64,
    pub(crate) aging_pass_ran: u64,
    pub(crate) input_items: u64,
    pub(crate) function_call_outputs: u64,
    pub(crate) custom_tool_call_outputs: u64,
    pub(crate) textual_tool_result_bytes_seen: u64,
    pub(crate) bytes_before: u64,
    pub(crate) bytes_after: u64,
    pub(crate) bytes_saved: u64,
    pub(crate) estimated_tokens_saved: u64,
    pub(crate) provider_input_tokens: u64,
    pub(crate) provider_cached_input_tokens: u64,
    pub(crate) provider_output_tokens: u64,
    pub(crate) provider_usage_events: u64,
    pub(crate) aged_cache: ProviderCacheSummary,
    pub(crate) unaged_cache: ProviderCacheSummary,
}

impl SavingsSummary {
    pub(crate) fn observe(&mut self, event: OptimizationEvent) {
        self.events = self.events.saturating_add(1);
        match event.outcome {
            OptimizationOutcome::Disabled => {
                self.disabled_requests = self.disabled_requests.saturating_add(1)
            }
            OptimizationOutcome::Bypassed => {
                self.bypassed_requests = self.bypassed_requests.saturating_add(1)
            }
            OptimizationOutcome::NativePassthrough => {
                self.native_passthrough_requests =
                    self.native_passthrough_requests.saturating_add(1)
            }
            OptimizationOutcome::FailOriginal => {
                self.fail_original_requests = self.fail_original_requests.saturating_add(1)
            }
            OptimizationOutcome::EvaluatedNoEligibleResult => {
                self.no_eligible_requests = self.no_eligible_requests.saturating_add(1)
            }
            OptimizationOutcome::EvaluatedNoSavings => {
                self.no_savings_requests = self.no_savings_requests.saturating_add(1)
            }
            OptimizationOutcome::Aged => self.aged_requests = self.aged_requests.saturating_add(1),
        }

        let metrics = event.metrics;
        self.tool_results_evaluated = self
            .tool_results_evaluated
            .saturating_add(metrics.tool_results_evaluated);
        self.tool_results_eligible = self
            .tool_results_eligible
            .saturating_add(metrics.tool_results_eligible);
        self.tool_results_compacted = self
            .tool_results_compacted
            .saturating_add(metrics.tool_results_compacted);
        self.largest_tool_result_bytes = self
            .largest_tool_result_bytes
            .max(metrics.largest_tool_result_bytes);
        self.protected_frontier = self
            .protected_frontier
            .saturating_add(metrics.protected_frontier);
        self.unsupported_output = self
            .unsupported_output
            .saturating_add(metrics.unsupported_output);
        self.at_or_below_threshold = self
            .at_or_below_threshold
            .saturating_add(metrics.at_or_below_threshold);
        self.unconsumed = self.unconsumed.saturating_add(metrics.unconsumed);
        self.receipt_not_smaller = self
            .receipt_not_smaller
            .saturating_add(metrics.receipt_not_smaller);
        self.responses_with_previous_response_id = self
            .responses_with_previous_response_id
            .saturating_add(metrics.responses_with_previous_response_id);
        self.responses_without_previous_response_id = self
            .responses_without_previous_response_id
            .saturating_add(metrics.responses_without_previous_response_id);
        self.previous_response_id_preserved = self
            .previous_response_id_preserved
            .saturating_add(metrics.previous_response_id_preserved);
        self.aging_pass_ran = self.aging_pass_ran.saturating_add(metrics.aging_pass_ran);
        self.input_items = self.input_items.saturating_add(metrics.input_items);
        self.function_call_outputs = self
            .function_call_outputs
            .saturating_add(metrics.function_call_outputs);
        self.custom_tool_call_outputs = self
            .custom_tool_call_outputs
            .saturating_add(metrics.custom_tool_call_outputs);
        self.textual_tool_result_bytes_seen = self
            .textual_tool_result_bytes_seen
            .saturating_add(metrics.textual_tool_result_bytes_seen);
        self.bytes_before = self.bytes_before.saturating_add(metrics.bytes_before);
        self.bytes_after = self.bytes_after.saturating_add(metrics.bytes_after);
        self.bytes_saved = self.bytes_saved.saturating_add(metrics.bytes_saved);
        self.estimated_tokens_saved = self
            .estimated_tokens_saved
            .saturating_add(metrics.estimated_tokens_saved());

        if let Some(usage) = event.provider_usage {
            self.provider_usage_events = self.provider_usage_events.saturating_add(1);
            self.provider_input_tokens = self
                .provider_input_tokens
                .saturating_add(usage.input_tokens);
            self.provider_cached_input_tokens = self
                .provider_cached_input_tokens
                .saturating_add(usage.cached_input_tokens.min(usage.input_tokens));
            self.provider_output_tokens = self
                .provider_output_tokens
                .saturating_add(usage.output_tokens);

            match event.outcome {
                OptimizationOutcome::Aged => self
                    .aged_cache
                    .observe(usage.input_tokens, usage.cached_input_tokens),
                OptimizationOutcome::Disabled
                | OptimizationOutcome::FailOriginal
                | OptimizationOutcome::EvaluatedNoEligibleResult
                | OptimizationOutcome::EvaluatedNoSavings => self
                    .unaged_cache
                    .observe(usage.input_tokens, usage.cached_input_tokens),
                OptimizationOutcome::Bypassed | OptimizationOutcome::NativePassthrough => {}
            }
        }
    }

    pub(crate) fn cache_rate_basis_points(self) -> Option<u64> {
        if self.provider_input_tokens == 0 {
            return None;
        }
        Some(self.provider_cached_input_tokens.saturating_mul(10_000) / self.provider_input_tokens)
    }
}

/// In-memory content-free event ledger used for the current process/session.
#[derive(Debug, Default)]
pub(crate) struct SavingsLedger {
    events: Vec<OptimizationEvent>,
}

impl SavingsLedger {
    pub(crate) fn record(&mut self, event: OptimizationEvent) {
        self.events.push(event);
    }

    #[allow(dead_code)]
    pub(crate) fn all_time(&self) -> SavingsSummary {
        summarize(self.events.iter().copied())
    }

    pub(crate) fn for_session(&self, session_id: u64) -> SavingsSummary {
        summarize(
            self.events
                .iter()
                .copied()
                .filter(|event| event.session_id == session_id),
        )
    }

    #[allow(dead_code)]
    pub(crate) fn between(&self, start_epoch_ms: u64, end_epoch_ms: u64) -> SavingsSummary {
        summarize(self.events.iter().copied().filter(|event| {
            event.observed_at_epoch_ms >= start_epoch_ms
                && event.observed_at_epoch_ms < end_epoch_ms
        }))
    }

    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.events.len()
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

fn summarize(events: impl Iterator<Item = OptimizationEvent>) -> SavingsSummary {
    let mut summary = SavingsSummary::default();
    for event in events {
        summary.observe(event);
    }
    summary
}
