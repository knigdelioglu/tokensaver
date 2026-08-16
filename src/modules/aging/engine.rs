use std::collections::{HashMap, HashSet};

use super::{
    model::{HistoryItem, ToolResultKind},
    policy::AgingPolicy,
    receipt::build_receipt,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgingSkipReason {
    ProtectedFrontier,
    UnsupportedOutput,
    AtOrBelowThreshold,
    Unconsumed,
    ReceiptNotSmaller,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgingDecision {
    Aged,
    Skipped(AgingSkipReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolResultEvaluation {
    pub(crate) item_index: usize,
    pub(crate) source_kind: ToolResultKind,
    pub(crate) source_call_id: Option<String>,
    pub(crate) decision: AgingDecision,
    pub(crate) source_bytes: Option<usize>,
    pub(crate) receipt_bytes: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AgingStats {
    pub(crate) tool_results_evaluated: usize,
    pub(crate) tool_results_eligible: usize,
    pub(crate) tool_results_aged: usize,
    pub(crate) largest_tool_result_bytes: usize,
    pub(crate) tool_result_bytes_before: usize,
    pub(crate) tool_result_bytes_after: usize,
    pub(crate) tool_result_bytes_saved: usize,
}

/// A transport-neutral instruction to replace only the model-visible output of
/// one normalized historical tool-result item.
///
/// The item index, result kind, and call ID form a validation tuple. A transport
/// adapter must confirm that tuple still identifies the same original item
/// before applying the receipt; otherwise it must fail original.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgedReplacement {
    pub(crate) item_index: usize,
    pub(crate) source_kind: ToolResultKind,
    pub(crate) source_call_id: Option<String>,
    pub(crate) receipt: String,
    pub(crate) bytes_before: usize,
    pub(crate) bytes_after: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AgingResult {
    pub(crate) replacements: Vec<AgedReplacement>,
    pub(crate) evaluations: Vec<ToolResultEvaluation>,
    pub(crate) stats: AgingStats,
}

/// Evaluate a normalized request history and return deterministic aging
/// decisions. The original history is never mutated by this domain function.
///
/// The caller owns protocol-specific application of replacements. If a caller
/// cannot prove a replacement still targets the same original item, it must
/// fail original and leave that item untouched.
pub(crate) fn age_tool_results(input: &[HistoryItem], policy: AgingPolicy) -> AgingResult {
    if !policy.enabled || input.is_empty() {
        return AgingResult::default();
    }

    let acted_after = acted_after_map(input);
    let protected_indexes = protected_tool_result_indexes(input, policy.frontier);
    let call_names = call_name_map(input);

    let mut replacements = Vec::new();
    let mut evaluations = Vec::new();
    let mut stats = AgingStats::default();

    for (index, item) in input.iter().enumerate() {
        let Some((kind, call_id, output)) = item.tool_result() else {
            continue;
        };

        stats.tool_results_evaluated += 1;
        let source_bytes = output.textual_byte_len();
        if let Some(bytes) = source_bytes {
            stats.largest_tool_result_bytes = stats.largest_tool_result_bytes.max(bytes);
        }

        let skipped = |reason| ToolResultEvaluation {
            item_index: index,
            source_kind: kind,
            source_call_id: call_id.map(str::to_owned),
            decision: AgingDecision::Skipped(reason),
            source_bytes,
            receipt_bytes: None,
        };

        if protected_indexes.contains(&index) {
            evaluations.push(skipped(AgingSkipReason::ProtectedFrontier));
            continue;
        }

        let Some(before) = source_bytes else {
            evaluations.push(skipped(AgingSkipReason::UnsupportedOutput));
            continue;
        };

        if before <= policy.min_bytes {
            evaluations.push(skipped(AgingSkipReason::AtOrBelowThreshold));
            continue;
        }

        if !acted_after[index] {
            evaluations.push(skipped(AgingSkipReason::Unconsumed));
            continue;
        }

        stats.tool_results_eligible += 1;

        let Some(text) = output.textual_value() else {
            evaluations.push(skipped(AgingSkipReason::UnsupportedOutput));
            continue;
        };

        let tool_name = call_id.and_then(|id| call_names.get(id).copied());
        let receipt = build_receipt(&text, tool_name, policy.preview_code_units);
        let after = receipt.len();

        if after >= before {
            evaluations.push(ToolResultEvaluation {
                item_index: index,
                source_kind: kind,
                source_call_id: call_id.map(str::to_owned),
                decision: AgingDecision::Skipped(AgingSkipReason::ReceiptNotSmaller),
                source_bytes: Some(before),
                receipt_bytes: Some(after),
            });
            continue;
        }

        evaluations.push(ToolResultEvaluation {
            item_index: index,
            source_kind: kind,
            source_call_id: call_id.map(str::to_owned),
            decision: AgingDecision::Aged,
            source_bytes: Some(before),
            receipt_bytes: Some(after),
        });

        replacements.push(AgedReplacement {
            item_index: index,
            source_kind: kind,
            source_call_id: call_id.map(str::to_owned),
            receipt,
            bytes_before: before,
            bytes_after: after,
        });

        stats.tool_results_aged += 1;
        stats.tool_result_bytes_before += before;
        stats.tool_result_bytes_after += after;
    }

    stats.tool_result_bytes_saved = stats
        .tool_result_bytes_before
        .saturating_sub(stats.tool_result_bytes_after);

    AgingResult {
        replacements,
        evaluations,
        stats,
    }
}

fn acted_after_map(input: &[HistoryItem]) -> Vec<bool> {
    let mut acted_after = vec![false; input.len()];
    let mut later_model_action = false;

    for index in (0..input.len()).rev() {
        acted_after[index] = later_model_action;
        if input[index].is_model_action() {
            later_model_action = true;
        }
    }

    acted_after
}

fn protected_tool_result_indexes(input: &[HistoryItem], frontier: usize) -> HashSet<usize> {
    if frontier == 0 {
        return HashSet::new();
    }

    let output_indexes = input
        .iter()
        .enumerate()
        .filter_map(|(index, item)| item.is_tool_result().then_some(index))
        .collect::<Vec<_>>();

    output_indexes.into_iter().rev().take(frontier).collect()
}

fn call_name_map(input: &[HistoryItem]) -> HashMap<&str, &str> {
    let mut names = HashMap::new();
    for item in input {
        if let Some((call_id, name)) = item.tool_call_identity() {
            names.insert(call_id, name);
        }
    }
    names
}
