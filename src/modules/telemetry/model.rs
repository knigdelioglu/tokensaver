/// Conservative display-only conversion used when no provider-reported token
/// count is available. Measured byte counts remain authoritative.
pub(crate) const BYTES_PER_TOKEN_ESTIMATE: u64 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OptimizationOutcome {
    Disabled,
    Bypassed,
    FailOriginal,
    EvaluatedNoEligibleResult,
    EvaluatedNoSavings,
    Aged,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProviderUsage {
    pub(crate) input_tokens: u64,
    pub(crate) cached_input_tokens: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OptimizationMetrics {
    pub(crate) tool_results_evaluated: u64,
    pub(crate) tool_results_eligible: u64,
    pub(crate) tool_results_compacted: u64,
    pub(crate) largest_tool_result_bytes: u64,
    pub(crate) bytes_before: u64,
    pub(crate) bytes_after: u64,
    pub(crate) bytes_saved: u64,
}

impl OptimizationMetrics {
    /// Mirrors the reference router's `Math.round(bytes / 4)` estimate for
    /// non-negative byte counts. This value is deliberately approximate;
    /// provider-reported usage remains authoritative when available.
    pub(crate) fn estimated_tokens_saved(self) -> u64 {
        self.bytes_saved
            .saturating_add(BYTES_PER_TOKEN_ESTIMATE / 2)
            / BYTES_PER_TOKEN_ESTIMATE
    }
}

/// One content-free observation of optimizer behavior.
///
/// The event intentionally has no field capable of storing original tool-result
/// text. `session_id` is an opaque local identifier selected by runtime code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OptimizationEvent {
    pub(crate) observed_at_epoch_ms: u64,
    pub(crate) session_id: u64,
    pub(crate) outcome: OptimizationOutcome,
    pub(crate) metrics: OptimizationMetrics,
    pub(crate) provider_usage: Option<ProviderUsage>,
}

impl OptimizationEvent {
    pub(crate) fn new(
        observed_at_epoch_ms: u64,
        session_id: u64,
        outcome: OptimizationOutcome,
        metrics: OptimizationMetrics,
    ) -> Self {
        Self {
            observed_at_epoch_ms,
            session_id,
            outcome,
            metrics,
            provider_usage: None,
        }
    }

    pub(crate) fn with_provider_usage(mut self, provider_usage: ProviderUsage) -> Self {
        self.provider_usage = Some(provider_usage);
        self
    }
}
