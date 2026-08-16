use serde_json::Value;

use crate::modules::aging::{
    AgingDecision, AgingPolicy, AgingResult, AgingSkipReason, HistoryItem, ToolOutput,
    ToolResultKind, age_tool_results,
};

use super::compression::EncodingChain;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreparationOutcome {
    Disabled,
    CompactionBypass,
    NativePassthrough,
    EvaluatedNoEligibleResult,
    EvaluatedNoSavings,
    Aged,
    FailOriginal,
}

/// Content-free description of the Responses history TokenSaver actually saw.
///
/// No prompt text, tool output, call ID, model name, response ID, or credential
/// crosses this boundary. The purpose is to distinguish "the optimizer did not
/// run" from "Codex sent no eligible historical tool result" and, in
/// particular, to make native chaining (`previous_response_id`) visible without
/// ever recording the identifier itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RequestDiagnostics {
    pub(crate) has_previous_response_id: bool,
    pub(crate) previous_response_id_preserved: bool,
    pub(crate) input_items: usize,
    pub(crate) function_calls: usize,
    pub(crate) custom_tool_calls: usize,
    pub(crate) function_call_outputs: usize,
    pub(crate) custom_tool_call_outputs: usize,
    pub(crate) assistant_messages: usize,
    pub(crate) user_messages: usize,
    pub(crate) system_messages: usize,
    pub(crate) developer_messages: usize,
    pub(crate) reasoning_items: usize,
    pub(crate) textual_tool_result_bytes: usize,
    pub(crate) largest_textual_tool_result_bytes: usize,
    pub(crate) aging_pass_ran: bool,
    pub(crate) protected_frontier: usize,
    pub(crate) unsupported_output: usize,
    pub(crate) at_or_below_threshold: usize,
    pub(crate) unconsumed: usize,
    pub(crate) receipt_not_smaller: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedRequestBody {
    pub(crate) bytes: Vec<u8>,
    pub(crate) outcome: PreparationOutcome,
    pub(crate) aging: AgingResult,
    pub(crate) diagnostics: Option<RequestDiagnostics>,
    pub(crate) body_changed: bool,
}

impl PreparedRequestBody {
    pub(crate) fn original(bytes: &[u8], outcome: PreparationOutcome) -> Self {
        Self::original_with_diagnostics(bytes, outcome, None)
    }

    fn original_with_diagnostics(
        bytes: &[u8],
        outcome: PreparationOutcome,
        diagnostics: Option<RequestDiagnostics>,
    ) -> Self {
        Self {
            bytes: bytes.to_vec(),
            outcome,
            aging: AgingResult::default(),
            diagnostics,
            body_changed: false,
        }
    }
}

pub(crate) fn prepare_responses_body(
    encoded_body: &[u8],
    content_encoding: Option<&str>,
    upstream_path: &str,
    policy: AgingPolicy,
) -> PreparedRequestBody {
    let is_responses = is_responses_path(upstream_path);
    let is_compaction = is_compaction_path(upstream_path);

    if !is_responses && !is_compaction {
        return PreparedRequestBody::original(encoded_body, PreparationOutcome::NativePassthrough);
    }

    // Inspect even when saving is off or this is an explicit Codex compaction.
    // Inspection is content-free and the exact encoded body is still forwarded.
    // That makes request-shape diagnostics useful without changing OFF/bypass
    // semantics.
    let (encodings, mut root) = match decode_root(encoded_body, content_encoding) {
        Ok(parsed) => parsed,
        Err(()) => {
            let outcome = if is_compaction {
                PreparationOutcome::CompactionBypass
            } else if !policy.enabled {
                PreparationOutcome::Disabled
            } else {
                PreparationOutcome::FailOriginal
            };
            return PreparedRequestBody::original(encoded_body, outcome);
        }
    };

    let mut diagnostics = inspect_request(&root);

    // Native conversation compaction must always read the original history.
    if is_compaction {
        return PreparedRequestBody::original_with_diagnostics(
            encoded_body,
            PreparationOutcome::CompactionBypass,
            Some(diagnostics),
        );
    }

    // Preserve native Responses chaining. Unlike a provider router, TokenSaver
    // forwards to the same first-party endpoint, so deleting
    // `previous_response_id` could turn an incremental request into an invalid
    // stateless history. P0 diagnostics tell us whether chaining is present;
    // P1 deliberately keeps it byte-for-byte unless future live evidence proves
    // a safe, explicit alternative.
    diagnostics.previous_response_id_preserved = diagnostics.has_previous_response_id;

    if !policy.enabled {
        return PreparedRequestBody::original_with_diagnostics(
            encoded_body,
            PreparationOutcome::Disabled,
            Some(diagnostics),
        );
    }

    let Some(input) = root.get("input").and_then(Value::as_array) else {
        diagnostics.aging_pass_ran = true;
        return PreparedRequestBody {
            bytes: encoded_body.to_vec(),
            outcome: PreparationOutcome::EvaluatedNoEligibleResult,
            aging: AgingResult::default(),
            diagnostics: Some(diagnostics),
            body_changed: false,
        };
    };

    let normalized = input.iter().map(normalize_history_item).collect::<Vec<_>>();
    let aging = age_tool_results(&normalized, policy);
    diagnostics.aging_pass_ran = true;
    apply_aging_diagnostics(&mut diagnostics, &aging);

    if aging.replacements.is_empty() {
        let outcome = if aging.stats.tool_results_eligible > 0 {
            PreparationOutcome::EvaluatedNoSavings
        } else {
            PreparationOutcome::EvaluatedNoEligibleResult
        };
        return PreparedRequestBody {
            bytes: encoded_body.to_vec(),
            outcome,
            aging,
            diagnostics: Some(diagnostics),
            body_changed: false,
        };
    }

    let Some(items) = root.get_mut("input").and_then(Value::as_array_mut) else {
        return PreparedRequestBody::original_with_diagnostics(
            encoded_body,
            PreparationOutcome::FailOriginal,
            Some(diagnostics),
        );
    };

    for replacement in &aging.replacements {
        let Some(item) = items.get_mut(replacement.item_index) else {
            return PreparedRequestBody::original_with_diagnostics(
                encoded_body,
                PreparationOutcome::FailOriginal,
                Some(diagnostics),
            );
        };
        if !replacement_still_matches(
            item,
            replacement.source_kind,
            replacement.source_call_id.as_deref(),
        ) {
            return PreparedRequestBody::original_with_diagnostics(
                encoded_body,
                PreparationOutcome::FailOriginal,
                Some(diagnostics),
            );
        }
        let Some(object) = item.as_object_mut() else {
            return PreparedRequestBody::original_with_diagnostics(
                encoded_body,
                PreparationOutcome::FailOriginal,
                Some(diagnostics),
            );
        };
        object.insert(
            "output".to_owned(),
            Value::String(replacement.receipt.clone()),
        );
    }

    // Re-check the chaining invariant on the exact rewritten JSON before it is
    // serialized. Aging may change only approved historical output fields.
    if diagnostics.has_previous_response_id != has_previous_response_id(&root) {
        return PreparedRequestBody::original_with_diagnostics(
            encoded_body,
            PreparationOutcome::FailOriginal,
            Some(diagnostics),
        );
    }

    let rewritten = match serde_json::to_vec(&root) {
        Ok(value) => value,
        Err(_) => {
            return PreparedRequestBody::original_with_diagnostics(
                encoded_body,
                PreparationOutcome::FailOriginal,
                Some(diagnostics),
            );
        }
    };
    let reencoded = match encodings.encode(&rewritten) {
        Ok(value) => value,
        Err(_) => {
            return PreparedRequestBody::original_with_diagnostics(
                encoded_body,
                PreparationOutcome::FailOriginal,
                Some(diagnostics),
            );
        }
    };

    PreparedRequestBody {
        bytes: reencoded,
        outcome: PreparationOutcome::Aged,
        aging,
        diagnostics: Some(diagnostics),
        body_changed: true,
    }
}

fn decode_root(
    encoded_body: &[u8],
    content_encoding: Option<&str>,
) -> Result<(EncodingChain, Value), ()> {
    let encodings = EncodingChain::parse(content_encoding).map_err(|_| ())?;
    let decoded = encodings.decode(encoded_body).map_err(|_| ())?;
    let root = serde_json::from_slice(&decoded).map_err(|_| ())?;
    Ok((encodings, root))
}

fn inspect_request(root: &Value) -> RequestDiagnostics {
    let mut diagnostics = RequestDiagnostics {
        has_previous_response_id: has_previous_response_id(root),
        ..RequestDiagnostics::default()
    };
    let Some(input) = root.get("input").and_then(Value::as_array) else {
        return diagnostics;
    };
    diagnostics.input_items = input.len();

    for item in input {
        let Some(object) = item.as_object() else {
            continue;
        };
        match object.get("type").and_then(Value::as_str) {
            Some("function_call") => diagnostics.function_calls += 1,
            Some("custom_tool_call") => diagnostics.custom_tool_calls += 1,
            Some("function_call_output") => {
                diagnostics.function_call_outputs += 1;
                observe_tool_output(&mut diagnostics, object.get("output"));
            }
            Some("custom_tool_call_output") => {
                diagnostics.custom_tool_call_outputs += 1;
                observe_tool_output(&mut diagnostics, object.get("output"));
            }
            Some("reasoning") => diagnostics.reasoning_items += 1,
            Some("message") => match object.get("role").and_then(Value::as_str) {
                Some("assistant") => diagnostics.assistant_messages += 1,
                Some("user") => diagnostics.user_messages += 1,
                Some("system") => diagnostics.system_messages += 1,
                Some("developer") => diagnostics.developer_messages += 1,
                _ => {}
            },
            _ => {}
        }
    }
    diagnostics
}

fn observe_tool_output(diagnostics: &mut RequestDiagnostics, output: Option<&Value>) {
    let normalized = normalize_output(output);
    if let Some(bytes) = normalized.textual_byte_len() {
        diagnostics.textual_tool_result_bytes =
            diagnostics.textual_tool_result_bytes.saturating_add(bytes);
        diagnostics.largest_textual_tool_result_bytes =
            diagnostics.largest_textual_tool_result_bytes.max(bytes);
    }
}

fn apply_aging_diagnostics(diagnostics: &mut RequestDiagnostics, aging: &AgingResult) {
    for evaluation in &aging.evaluations {
        match evaluation.decision {
            AgingDecision::Aged => {}
            AgingDecision::Skipped(AgingSkipReason::ProtectedFrontier) => {
                diagnostics.protected_frontier += 1;
            }
            AgingDecision::Skipped(AgingSkipReason::UnsupportedOutput) => {
                diagnostics.unsupported_output += 1;
            }
            AgingDecision::Skipped(AgingSkipReason::AtOrBelowThreshold) => {
                diagnostics.at_or_below_threshold += 1;
            }
            AgingDecision::Skipped(AgingSkipReason::Unconsumed) => {
                diagnostics.unconsumed += 1;
            }
            AgingDecision::Skipped(AgingSkipReason::ReceiptNotSmaller) => {
                diagnostics.receipt_not_smaller += 1;
            }
        }
    }
}

fn has_previous_response_id(root: &Value) -> bool {
    root.get("previous_response_id")
        .is_some_and(|value| !value.is_null())
}

fn normalize_history_item(item: &Value) -> HistoryItem {
    let Some(object) = item.as_object() else {
        return HistoryItem::Other;
    };
    let item_type = object.get("type").and_then(Value::as_str);

    match item_type {
        Some("function_call") => HistoryItem::FunctionCall {
            call_id: string_field(object.get("call_id")),
            name: string_field(object.get("name")),
        },
        Some("custom_tool_call") => HistoryItem::CustomToolCall {
            call_id: string_field(object.get("call_id")),
            name: string_field(object.get("name")),
        },
        Some("function_call_output") => HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: string_field(object.get("call_id")),
            output: normalize_output(object.get("output")),
        },
        Some("custom_tool_call_output") => HistoryItem::ToolResult {
            kind: ToolResultKind::Custom,
            call_id: string_field(object.get("call_id")),
            output: normalize_output(object.get("output")),
        },
        Some("reasoning") => HistoryItem::Reasoning,
        Some("message") if object.get("role").and_then(Value::as_str) == Some("assistant") => {
            HistoryItem::AssistantMessage
        }
        _ => HistoryItem::Other,
    }
}

fn normalize_output(output: Option<&Value>) -> ToolOutput {
    match output {
        Some(Value::String(text)) => ToolOutput::Text(text.clone()),
        Some(Value::Array(parts)) => {
            let mut text = Vec::with_capacity(parts.len());
            for part in parts {
                let Some(object) = part.as_object() else {
                    return ToolOutput::Unsupported;
                };
                let part_type = object.get("type").and_then(Value::as_str);
                if part_type != Some("input_text") && part_type != Some("text") {
                    return ToolOutput::Unsupported;
                }
                let Some(value) = object.get("text").and_then(Value::as_str) else {
                    return ToolOutput::Unsupported;
                };
                text.push(value.to_owned());
            }
            ToolOutput::TextParts(text)
        }
        _ => ToolOutput::Unsupported,
    }
}

fn replacement_still_matches(
    item: &Value,
    expected_kind: ToolResultKind,
    expected_call_id: Option<&str>,
) -> bool {
    let Some(object) = item.as_object() else {
        return false;
    };
    let expected_type = match expected_kind {
        ToolResultKind::Function => "function_call_output",
        ToolResultKind::Custom => "custom_tool_call_output",
    };
    if object.get("type").and_then(Value::as_str) != Some(expected_type) {
        return false;
    }
    object.get("call_id").and_then(Value::as_str) == expected_call_id
}

fn string_field(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_owned)
}

pub(crate) fn is_responses_path(path: &str) -> bool {
    matches!(path, "/responses" | "/v1/responses")
}

pub(crate) fn is_compaction_path(path: &str) -> bool {
    matches!(path, "/responses/compact" | "/v1/responses/compact")
}
