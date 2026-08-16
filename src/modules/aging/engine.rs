use std::collections::{HashMap, HashSet};

use super::{
    model::{HistoryItem, ToolResultKind},
    policy::AgingPolicy,
    receipt::build_receipt,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AgingStats {
    pub(crate) tool_results_aged: usize,
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
    pub(crate) stats: AgingStats,
}

/// Evaluate a normalized request history and return only deterministic aging
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
    let mut stats = AgingStats::default();

    for (index, item) in input.iter().enumerate() {
        if protected_indexes.contains(&index) {
            continue;
        }

        let Some((kind, call_id, output)) = item.tool_result() else {
            continue;
        };
        let Some(text) = output.textual_value() else {
            continue;
        };

        let before = text.len();
        if before <= policy.min_bytes || !acted_after[index] {
            continue;
        }

        let tool_name = call_id.and_then(|id| call_names.get(id).copied());
        let receipt = build_receipt(&text, tool_name, policy.preview_code_units);
        let after = receipt.len();

        if after >= before {
            continue;
        }

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

    output_indexes
        .into_iter()
        .rev()
        .take(frontier)
        .collect()
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
