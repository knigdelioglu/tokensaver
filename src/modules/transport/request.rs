use serde_json::Value;

use crate::modules::aging::{
    AgingPolicy, AgingResult, HistoryItem, ToolOutput, ToolResultKind, age_tool_results,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedRequestBody {
    pub(crate) bytes: Vec<u8>,
    pub(crate) outcome: PreparationOutcome,
    pub(crate) aging: AgingResult,
    pub(crate) body_changed: bool,
}

impl PreparedRequestBody {
    pub(crate) fn original(bytes: &[u8], outcome: PreparationOutcome) -> Self {
        Self {
            bytes: bytes.to_vec(),
            outcome,
            aging: AgingResult::default(),
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
    // Endpoint classification is independent of the saving toggle. Native
    // passthrough and explicit Codex compaction remain distinguishable in
    // diagnostics even when saving is disabled.
    if is_compaction_path(upstream_path) {
        return PreparedRequestBody::original(encoded_body, PreparationOutcome::CompactionBypass);
    }
    if !is_responses_path(upstream_path) {
        return PreparedRequestBody::original(encoded_body, PreparationOutcome::NativePassthrough);
    }
    if !policy.enabled {
        return PreparedRequestBody::original(encoded_body, PreparationOutcome::Disabled);
    }

    match try_prepare(encoded_body, content_encoding, policy) {
        Ok(prepared) => prepared,
        Err(()) => PreparedRequestBody::original(encoded_body, PreparationOutcome::FailOriginal),
    }
}

fn try_prepare(
    encoded_body: &[u8],
    content_encoding: Option<&str>,
    policy: AgingPolicy,
) -> Result<PreparedRequestBody, ()> {
    let encodings = EncodingChain::parse(content_encoding).map_err(|_| ())?;
    let decoded = encodings.decode(encoded_body).map_err(|_| ())?;
    let mut root: Value = serde_json::from_slice(&decoded).map_err(|_| ())?;

    let Some(input) = root.get("input").and_then(Value::as_array) else {
        return Ok(PreparedRequestBody::original(
            encoded_body,
            PreparationOutcome::EvaluatedNoEligibleResult,
        ));
    };

    let normalized = input.iter().map(normalize_history_item).collect::<Vec<_>>();
    let aging = age_tool_results(&normalized, policy);

    if aging.replacements.is_empty() {
        let outcome = if aging.stats.tool_results_eligible > 0 {
            PreparationOutcome::EvaluatedNoSavings
        } else {
            PreparationOutcome::EvaluatedNoEligibleResult
        };
        return Ok(PreparedRequestBody {
            bytes: encoded_body.to_vec(),
            outcome,
            aging,
            body_changed: false,
        });
    }

    let Some(items) = root.get_mut("input").and_then(Value::as_array_mut) else {
        return Err(());
    };

    for replacement in &aging.replacements {
        let Some(item) = items.get_mut(replacement.item_index) else {
            return Err(());
        };
        if !replacement_still_matches(
            item,
            replacement.source_kind,
            replacement.source_call_id.as_deref(),
        ) {
            return Err(());
        }
        let Some(object) = item.as_object_mut() else {
            return Err(());
        };
        object.insert(
            "output".to_owned(),
            Value::String(replacement.receipt.clone()),
        );
    }

    let rewritten = serde_json::to_vec(&root).map_err(|_| ())?;
    let reencoded = encodings.encode(&rewritten).map_err(|_| ())?;

    Ok(PreparedRequestBody {
        bytes: reencoded,
        outcome: PreparationOutcome::Aged,
        aging,
        body_changed: true,
    })
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
