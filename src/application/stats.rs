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
    pub(crate) bytes_saved: u64,
    pub(crate) estimated_tokens_saved: u64,
    pub(crate) tool_results_compacted: u64,
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
        bytes_saved: summary.bytes_saved,
        estimated_tokens_saved: summary.estimated_tokens_saved,
        tool_results_compacted: summary.tool_results_compacted,
        aged_requests: summary.aged_requests,
    }
}
