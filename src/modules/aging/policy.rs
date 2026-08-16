/// Default minimum UTF-8 payload size required before a textual tool result is
/// eligible for aging. The comparison is strict: a result must be larger than
/// this value.
pub(crate) const DEFAULT_MIN_BYTES: usize = 32 * 1024;

/// Number of newest tool-result items that remain byte-for-byte hot context.
pub(crate) const DEFAULT_FRONTIER: usize = 4;

/// Number of UTF-16 code units retained from each edge of an aged tool result.
/// This matches the reference implementation's JavaScript slicing semantics
/// without ever emitting a split surrogate pair.
pub(crate) const DEFAULT_PREVIEW_CODE_UNITS: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgingPolicy {
    pub(crate) enabled: bool,
    pub(crate) min_bytes: usize,
    pub(crate) frontier: usize,
    pub(crate) preview_code_units: usize,
}

impl Default for AgingPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            min_bytes: DEFAULT_MIN_BYTES,
            frontier: DEFAULT_FRONTIER,
            preview_code_units: DEFAULT_PREVIEW_CODE_UNITS,
        }
    }
}
