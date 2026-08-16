use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::shared::filesystem::atomic_write_private;

use super::{OptimizationEvent, OptimizationOutcome, SavingsSummary};

const STORE_SCHEMA_VERSION: u32 = 1;
const MAX_DAILY_BUCKETS: usize = 120;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LastOptimization {
    pub(crate) observed_at_epoch_ms: u64,
    pub(crate) bytes_before: u64,
    pub(crate) bytes_after: u64,
    pub(crate) bytes_saved: u64,
    pub(crate) estimated_tokens_saved: u64,
    pub(crate) tool_results_compacted: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistentSavings {
    schema_version: u32,
    all_time: SavingsSummary,
    daily: BTreeMap<String, SavingsSummary>,
    last_optimization: Option<LastOptimization>,
}

impl Default for PersistentSavings {
    fn default() -> Self {
        Self {
            schema_version: STORE_SCHEMA_VERSION,
            all_time: SavingsSummary::default(),
            daily: BTreeMap::new(),
            last_optimization: None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum SavingsStoreError {
    Io(io::Error),
    InvalidJson(String),
    UnsupportedSchema(u32),
}

impl fmt::Display for SavingsStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "savings store I/O failed: {error}"),
            Self::InvalidJson(error) => write!(formatter, "savings store JSON is invalid: {error}"),
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported savings store schema version: {version}")
            }
        }
    }
}

impl std::error::Error for SavingsStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for SavingsStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub(crate) struct DurableSavingsStore {
    path: PathBuf,
    state: PersistentSavings,
    dirty: bool,
}

impl DurableSavingsStore {
    pub(crate) fn open(path: impl Into<PathBuf>) -> Result<Self, SavingsStoreError> {
        let path = path.into();
        let state = match fs::read_to_string(&path) {
            Ok(source) => {
                let state = serde_json::from_str::<PersistentSavings>(&source)
                    .map_err(|error| SavingsStoreError::InvalidJson(error.to_string()))?;
                if state.schema_version != STORE_SCHEMA_VERSION {
                    return Err(SavingsStoreError::UnsupportedSchema(state.schema_version));
                }
                state
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => PersistentSavings::default(),
            Err(error) => return Err(SavingsStoreError::Io(error)),
        };

        Ok(Self {
            path,
            state,
            dirty: false,
        })
    }

    pub(crate) fn record(&mut self, event: OptimizationEvent, local_day: &str) {
        self.state.all_time.observe(event);
        self.state
            .daily
            .entry(local_day.to_owned())
            .or_default()
            .observe(event);

        if event.outcome == OptimizationOutcome::Aged {
            let metrics = event.metrics;
            self.state.last_optimization = Some(LastOptimization {
                observed_at_epoch_ms: event.observed_at_epoch_ms,
                bytes_before: metrics.bytes_before,
                bytes_after: metrics.bytes_after,
                bytes_saved: metrics.bytes_saved,
                estimated_tokens_saved: metrics.estimated_tokens_saved(),
                tool_results_compacted: metrics.tool_results_compacted,
            });
        }

        while self.state.daily.len() > MAX_DAILY_BUCKETS {
            let Some(oldest) = self.state.daily.keys().next().cloned() else {
                break;
            };
            self.state.daily.remove(&oldest);
        }
        self.dirty = true;
    }

    pub(crate) fn all_time(&self) -> SavingsSummary {
        self.state.all_time
    }

    pub(crate) fn for_day(&self, local_day: &str) -> SavingsSummary {
        self.state.daily.get(local_day).copied().unwrap_or_default()
    }

    pub(crate) fn last_optimization(&self) -> Option<LastOptimization> {
        self.state.last_optimization
    }

    pub(crate) fn flush(&mut self) -> Result<(), SavingsStoreError> {
        if !self.dirty {
            return Ok(());
        }
        let serialized = serde_json::to_string_pretty(&self.state)
            .map_err(|error| SavingsStoreError::InvalidJson(error.to_string()))?;
        atomic_write_private(&self.path, &serialized)?;
        self.dirty = false;
        Ok(())
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
