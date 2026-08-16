use super::{
    age_tool_results, AgingPolicy, HistoryItem, ToolOutput, ToolResultKind,
    DEFAULT_FRONTIER, DEFAULT_MIN_BYTES,
};

fn large_text(seed: &str) -> String {
    let repeat = (DEFAULT_MIN_BYTES / seed.len()) + 128;
    seed.repeat(repeat)
}

fn function_call(call_id: &str, name: &str) -> HistoryItem {
    HistoryItem::FunctionCall {
        call_id: Some(call_id.to_owned()),
        name: Some(name.to_owned()),
    }
}

fn output(call_id: &str, value: String) -> HistoryItem {
    HistoryItem::ToolResult {
        kind: ToolResultKind::Function,
        call_id: Some(call_id.to_owned()),
        output: ToolOutput::Text(value),
    }
}

fn custom_output(call_id: &str, value: ToolOutput) -> HistoryItem {
    HistoryItem::ToolResult {
        kind: ToolResultKind::Custom,
        call_id: Some(call_id.to_owned()),
        output: value,
    }
}

#[test]
fn large_consumed_textual_output_is_aged() {
    let source = large_text("0123456789abcdef");
    let history = vec![
        function_call("call-1", "shell"),
        output("call-1", source.clone()),
        HistoryItem::AssistantMessage,
    ];
    let policy = AgingPolicy {
        frontier: 0,
        ..AgingPolicy::default()
    };

    let result = age_tool_results(&history, policy);

    assert_eq!(result.replacements.len(), 1);
    let replacement = &result.replacements[0];
    assert_eq!(replacement.item_index, 1);
    assert_eq!(replacement.source_call_id.as_deref(), Some("call-1"));
    assert_eq!(replacement.bytes_before, source.len());
    assert!(replacement.bytes_after < replacement.bytes_before);
    assert!(replacement.receipt.contains("the preceding shell call"));
    assert!(replacement.receipt.contains("only when it is safe to repeat"));
    assert!(replacement.receipt.contains("must not be inferred"));
    assert!(replacement.receipt.contains("[tokensaver-receipt:v1 "));
    assert!(replacement.receipt.contains("sha256:"));
    assert!(replacement.receipt.contains("--- beginning of original result ---"));
    assert!(replacement.receipt.contains("--- omitted middle of original result ---"));
    assert!(replacement.receipt.contains("--- end of original result ---"));
    assert_eq!(result.stats.tool_results_aged, 1);
    assert_eq!(
        result.stats.tool_result_bytes_saved,
        replacement.bytes_before - replacement.bytes_after
    );
}

#[test]
fn unconsumed_output_remains_exact_even_with_zero_frontier() {
    let history = vec![
        function_call("call-1", "shell"),
        output("call-1", large_text("unconsumed")),
    ];
    let policy = AgingPolicy {
        frontier: 0,
        ..AgingPolicy::default()
    };

    let result = age_tool_results(&history, policy);

    assert!(result.replacements.is_empty());
    assert_eq!(result.stats.tool_results_aged, 0);
}

#[test]
fn later_tool_result_alone_does_not_prove_consumption() {
    let history = vec![
        function_call("call-1", "first"),
        output("call-1", large_text("first-output")),
        output("call-2", large_text("later-output")),
    ];
    let policy = AgingPolicy {
        frontier: 0,
        ..AgingPolicy::default()
    };

    let result = age_tool_results(&history, policy);

    assert!(result.replacements.is_empty());
}

#[test]
fn newest_four_tool_results_are_protected_byte_for_byte() {
    let mut history = Vec::new();
    for index in 0..6 {
        let call_id = format!("call-{index}");
        history.push(function_call(&call_id, "shell"));
        history.push(output(&call_id, large_text(&format!("result-{index}"))));
        history.push(HistoryItem::AssistantMessage);
    }

    let result = age_tool_results(&history, AgingPolicy::default());

    assert_eq!(DEFAULT_FRONTIER, 4);
    assert_eq!(result.replacements.len(), 2);
    assert_eq!(result.replacements[0].item_index, 1);
    assert_eq!(result.replacements[1].item_index, 4);
}

#[test]
fn frontier_counts_all_tool_results_even_when_some_are_unsupported() {
    let old = large_text("old");
    let history = vec![
        function_call("old", "shell"),
        output("old", old),
        HistoryItem::AssistantMessage,
        custom_output("mixed", ToolOutput::Unsupported),
        output("new-1", large_text("new-1")),
        output("new-2", large_text("new-2")),
        output("new-3", large_text("new-3")),
        HistoryItem::AssistantMessage,
    ];

    let result = age_tool_results(&history, AgingPolicy::default());

    assert_eq!(result.replacements.len(), 1);
    assert_eq!(result.replacements[0].item_index, 1);
}

#[test]
fn result_at_exact_threshold_is_not_aged() {
    let history = vec![
        function_call("call-1", "shell"),
        output("call-1", "x".repeat(DEFAULT_MIN_BYTES)),
        HistoryItem::AssistantMessage,
    ];
    let policy = AgingPolicy {
        frontier: 0,
        ..AgingPolicy::default()
    };

    assert!(age_tool_results(&history, policy).replacements.is_empty());
}

#[test]
fn mixed_or_unknown_output_is_never_aged() {
    let history = vec![
        function_call("call-1", "vision_tool"),
        custom_output("call-1", ToolOutput::Unsupported),
        HistoryItem::AssistantMessage,
    ];
    let policy = AgingPolicy {
        frontier: 0,
        min_bytes: 0,
        ..AgingPolicy::default()
    };

    assert!(age_tool_results(&history, policy).replacements.is_empty());
}

#[test]
fn purely_textual_parts_are_joined_for_aging() {
    let left = large_text("left");
    let right = large_text("right");
    let expected_bytes = left.len() + right.len();
    let history = vec![
        function_call("call-1", "read"),
        custom_output("call-1", ToolOutput::TextParts(vec![left, right])),
        HistoryItem::Reasoning,
    ];
    let policy = AgingPolicy {
        frontier: 0,
        ..AgingPolicy::default()
    };

    let result = age_tool_results(&history, policy);

    assert_eq!(result.replacements.len(), 1);
    assert_eq!(result.replacements[0].bytes_before, expected_bytes);
}

#[test]
fn disabled_mode_returns_no_replacements_or_stats() {
    let history = vec![
        function_call("call-1", "shell"),
        output("call-1", large_text("disabled")),
        HistoryItem::AssistantMessage,
    ];
    let policy = AgingPolicy {
        enabled: false,
        frontier: 0,
        ..AgingPolicy::default()
    };

    let result = age_tool_results(&history, policy);

    assert!(result.replacements.is_empty());
    assert_eq!(result.stats, Default::default());
}

#[test]
fn compaction_is_rejected_when_receipt_would_be_larger() {
    let history = vec![
        function_call("call-1", "shell"),
        output("call-1", "tiny".to_owned()),
        HistoryItem::AssistantMessage,
    ];
    let policy = AgingPolicy {
        min_bytes: 0,
        frontier: 0,
        ..AgingPolicy::default()
    };

    assert!(age_tool_results(&history, policy).replacements.is_empty());
}

#[test]
fn same_history_and_policy_produce_identical_receipt() {
    let history = vec![
        function_call("call-1", "shell"),
        output("call-1", large_text("deterministic")),
        HistoryItem::AssistantMessage,
    ];
    let policy = AgingPolicy {
        frontier: 0,
        ..AgingPolicy::default()
    };

    let first = age_tool_results(&history, policy);
    let second = age_tool_results(&history, policy);

    assert_eq!(first, second);
}

#[test]
fn missing_call_metadata_does_not_block_safe_aging() {
    let history = vec![
        HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: None,
            output: ToolOutput::Text(large_text("anonymous")),
        },
        HistoryItem::AssistantMessage,
    ];
    let policy = AgingPolicy {
        frontier: 0,
        ..AgingPolicy::default()
    };

    let result = age_tool_results(&history, policy);

    assert_eq!(result.replacements.len(), 1);
    assert_eq!(result.replacements[0].source_call_id, None);
    assert!(result.replacements[0].receipt.contains("the preceding tool call"));
    assert!(result.replacements[0].receipt.contains("only when it is safe to repeat"));
}
