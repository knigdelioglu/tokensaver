use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use chrono::Local;

use crate::modules::telemetry::{DurableSavingsStore, SavingsStoreError, SavingsSummary};
use crate::shared::paths::ensure_product_data_dir;

const SAVINGS_FILE: &str = "savings.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StoredSavingsView {
    pub(crate) requests_observed: u64,
    pub(crate) responses_requests: u64,
    pub(crate) compaction_bypass_requests: u64,
    pub(crate) native_passthrough_requests: u64,
    pub(crate) disabled_requests: u64,
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
    pub(crate) bytes_saved: u64,
    pub(crate) estimated_tokens_saved: u64,
    pub(crate) provider_usage_events: u64,
    pub(crate) provider_input_tokens: u64,
    pub(crate) provider_cached_input_tokens: u64,
    pub(crate) provider_output_tokens: u64,
    pub(crate) aged_cache_events: u64,
    pub(crate) aged_cache_rate_basis_points: Option<u64>,
    pub(crate) unaged_cache_events: u64,
    pub(crate) unaged_cache_rate_basis_points: Option<u64>,
    pub(crate) aged_requests: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StoredStats {
    pub(crate) today: StoredSavingsView,
    pub(crate) all_time: StoredSavingsView,
}

#[derive(Debug)]
pub(crate) enum StatsError {
    Io(io::Error),
    Store(SavingsStoreError),
}

impl fmt::Display for StatsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "stats I/O failed: {error}"),
            Self::Store(error) => write!(formatter, "stats store failed: {error}"),
        }
    }
}

impl std::error::Error for StatsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Store(error) => Some(error),
        }
    }
}

impl From<io::Error> for StatsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<SavingsStoreError> for StatsError {
    fn from(error: SavingsStoreError) -> Self {
        Self::Store(error)
    }
}

pub(crate) fn load_product_stats() -> Result<StoredStats, StatsError> {
    let data_dir = ensure_product_data_dir()?;
    load_stored_stats(&data_dir)
}

fn load_stored_stats(data_dir: &Path) -> Result<StoredStats, StatsError> {
    fs::create_dir_all(data_dir)?;
    let store = DurableSavingsStore::open(data_dir.join(SAVINGS_FILE))?;
    let today_key = Local::now().format("%Y-%m-%d").to_string();
    Ok(StoredStats {
        today: view(store.for_day(&today_key)),
        all_time: view(store.all_time()),
    })
}

fn view(summary: SavingsSummary) -> StoredSavingsView {
    let responses_requests = summary
        .aged_requests
        .saturating_add(summary.disabled_requests)
        .saturating_add(summary.fail_original_requests)
        .saturating_add(summary.no_eligible_requests)
        .saturating_add(summary.no_savings_requests);

    StoredSavingsView {
        requests_observed: summary.events,
        responses_requests,
        compaction_bypass_requests: summary.bypassed_requests,
        native_passthrough_requests: summary.native_passthrough_requests,
        disabled_requests: summary.disabled_requests,
        fail_original_requests: summary.fail_original_requests,
        no_eligible_requests: summary.no_eligible_requests,
        no_savings_requests: summary.no_savings_requests,
        tool_results_evaluated: summary.tool_results_evaluated,
        tool_results_eligible: summary.tool_results_eligible,
        tool_results_compacted: summary.tool_results_compacted,
        largest_tool_result_bytes: summary.largest_tool_result_bytes,
        protected_frontier: summary.protected_frontier,
        unsupported_output: summary.unsupported_output,
        at_or_below_threshold: summary.at_or_below_threshold,
        unconsumed: summary.unconsumed,
        receipt_not_smaller: summary.receipt_not_smaller,
        responses_with_previous_response_id: summary.responses_with_previous_response_id,
        responses_without_previous_response_id: summary.responses_without_previous_response_id,
        previous_response_id_preserved: summary.previous_response_id_preserved,
        aging_pass_ran: summary.aging_pass_ran,
        input_items: summary.input_items,
        function_call_outputs: summary.function_call_outputs,
        custom_tool_call_outputs: summary.custom_tool_call_outputs,
        textual_tool_result_bytes_seen: summary.textual_tool_result_bytes_seen,
        bytes_saved: summary.bytes_saved,
        estimated_tokens_saved: summary.estimated_tokens_saved,
        provider_usage_events: summary.provider_usage_events,
        provider_input_tokens: summary.provider_input_tokens,
        provider_cached_input_tokens: summary.provider_cached_input_tokens,
        provider_output_tokens: summary.provider_output_tokens,
        aged_cache_events: summary.aged_cache.usage_events,
        aged_cache_rate_basis_points: summary.aged_cache.rate_basis_points(),
        unaged_cache_events: summary.unaged_cache.usage_events,
        unaged_cache_rate_basis_points: summary.unaged_cache.rate_basis_points(),
        aged_requests: summary.aged_requests,
    }
}
