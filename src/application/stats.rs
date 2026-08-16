use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use chrono::Local;

use crate::modules::telemetry::{DurableSavingsStore, SavingsStoreError, SavingsSummary};

const SAVINGS_FILE: &str = "savings.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StoredSavingsView {
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

pub(crate) fn load_stored_stats(data_dir: &Path) -> Result<StoredStats, StatsError> {
    fs::create_dir_all(data_dir)?;
    let store = DurableSavingsStore::open(data_dir.join(SAVINGS_FILE))?;
    let today_key = Local::now().format("%Y-%m-%d").to_string();
    Ok(StoredStats {
        today: view(store.for_day(&today_key)),
        all_time: view(store.all_time()),
    })
}

fn view(summary: SavingsSummary) -> StoredSavingsView {
    StoredSavingsView {
        bytes_saved: summary.bytes_saved,
        estimated_tokens_saved: summary.estimated_tokens_saved,
        tool_results_compacted: summary.tool_results_compacted,
        aged_requests: summary.aged_requests,
    }
}
