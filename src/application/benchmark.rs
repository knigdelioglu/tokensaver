use crate::modules::aging::{
    AgingDecision, AgingPolicy, AgingSkipReason, AgingStats, HistoryItem, ToolOutput,
    ToolResultKind, age_tool_results,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BenchmarkFixtureKind {
    TestLog,
    BuildLog,
    LargeDiff,
    RepositorySearch,
    LargeFileRead,
    ManyMediumOutputs,
    MixedOutput,
    UnconsumedHistory,
}

impl BenchmarkFixtureKind {
    pub(crate) const ALL: [Self; 8] = [
        Self::TestLog,
        Self::BuildLog,
        Self::LargeDiff,
        Self::RepositorySearch,
        Self::LargeFileRead,
        Self::ManyMediumOutputs,
        Self::MixedOutput,
        Self::UnconsumedHistory,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::TestLog => "test-log",
            Self::BuildLog => "build-log",
            Self::LargeDiff => "large-diff",
            Self::RepositorySearch => "repository-search",
            Self::LargeFileRead => "large-file-read",
            Self::ManyMediumOutputs => "many-medium-outputs",
            Self::MixedOutput => "mixed-output",
            Self::UnconsumedHistory => "unconsumed-history",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BenchmarkDecision {
    Aged,
    ProtectedFrontier,
    UnsupportedOutput,
    AtOrBelowThreshold,
    Unconsumed,
    ReceiptNotSmaller,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BenchmarkEvaluation {
    pub(crate) item_index: usize,
    pub(crate) decision: BenchmarkDecision,
    pub(crate) source_bytes: Option<usize>,
    pub(crate) receipt_bytes: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BenchmarkReport {
    pub(crate) fixture: &'static str,
    pub(crate) stats: AgingStats,
    pub(crate) evaluations: Vec<BenchmarkEvaluation>,
}

pub(crate) fn run_builtin_fixture(
    fixture: BenchmarkFixtureKind,
    policy: AgingPolicy,
) -> BenchmarkReport {
    let history = fixture_history(fixture);
    let result = age_tool_results(&history, policy);
    let evaluations = result
        .evaluations
        .iter()
        .map(|evaluation| BenchmarkEvaluation {
            item_index: evaluation.item_index,
            decision: map_decision(evaluation.decision),
            source_bytes: evaluation.source_bytes,
            receipt_bytes: evaluation.receipt_bytes,
        })
        .collect();

    BenchmarkReport {
        fixture: fixture.name(),
        stats: result.stats,
        evaluations,
    }
}

pub(crate) fn run_builtin_suite(policy: AgingPolicy) -> Vec<BenchmarkReport> {
    BenchmarkFixtureKind::ALL
        .into_iter()
        .map(|fixture| run_builtin_fixture(fixture, policy))
        .collect()
}

fn map_decision(decision: AgingDecision) -> BenchmarkDecision {
    match decision {
        AgingDecision::Aged => BenchmarkDecision::Aged,
        AgingDecision::Skipped(AgingSkipReason::ProtectedFrontier) => {
            BenchmarkDecision::ProtectedFrontier
        }
        AgingDecision::Skipped(AgingSkipReason::UnsupportedOutput) => {
            BenchmarkDecision::UnsupportedOutput
        }
        AgingDecision::Skipped(AgingSkipReason::AtOrBelowThreshold) => {
            BenchmarkDecision::AtOrBelowThreshold
        }
        AgingDecision::Skipped(AgingSkipReason::Unconsumed) => BenchmarkDecision::Unconsumed,
        AgingDecision::Skipped(AgingSkipReason::ReceiptNotSmaller) => {
            BenchmarkDecision::ReceiptNotSmaller
        }
    }
}

fn fixture_history(fixture: BenchmarkFixtureKind) -> Vec<HistoryItem> {
    match fixture {
        BenchmarkFixtureKind::TestLog => consumed_large_history(
            "cargo-test",
            repeat_lines("test module::case ... ok\n", 2_000),
        ),
        BenchmarkFixtureKind::BuildLog => consumed_large_history(
            "build",
            repeat_lines("Compiling dependency v1.2.3\n", 2_000),
        ),
        BenchmarkFixtureKind::LargeDiff => consumed_large_history(
            "git-diff",
            repeat_lines("+pub fn generated_line() { /* changed */ }\n", 1_500),
        ),
        BenchmarkFixtureKind::RepositorySearch => consumed_large_history(
            "search",
            repeat_lines("src/module/file.rs:123: matching symbol\n", 1_500),
        ),
        BenchmarkFixtureKind::LargeFileRead => consumed_large_history(
            "read-file",
            repeat_lines("fn representative_source_line() {}\n", 2_000),
        ),
        BenchmarkFixtureKind::ManyMediumOutputs => many_medium_outputs(),
        BenchmarkFixtureKind::MixedOutput => mixed_output_history(),
        BenchmarkFixtureKind::UnconsumedHistory => unconsumed_history(),
    }
}

fn repeat_lines(line: &str, count: usize) -> String {
    line.repeat(count)
}

/// The first result is large and consumed. Four newer small results deliberately
/// exercise the default hot frontier, leaving the old large result eligible.
fn consumed_large_history(tool_name: &str, output: String) -> Vec<HistoryItem> {
    let mut history = vec![
        HistoryItem::FunctionCall {
            call_id: Some("old-call".to_owned()),
            name: Some(tool_name.to_owned()),
        },
        HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: Some("old-call".to_owned()),
            output: ToolOutput::Text(output),
        },
        HistoryItem::AssistantMessage,
    ];

    for index in 0..4 {
        history.push(HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: Some(format!("hot-{index}")),
            output: ToolOutput::Text("recent result".to_owned()),
        });
    }
    history
}

fn many_medium_outputs() -> Vec<HistoryItem> {
    let mut history = Vec::new();
    for index in 0..12 {
        history.push(HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: Some(format!("medium-{index}")),
            output: ToolOutput::Text("m".repeat(20 * 1024)),
        });
        history.push(HistoryItem::AssistantMessage);
    }
    history
}

fn mixed_output_history() -> Vec<HistoryItem> {
    let mut history = vec![
        HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: Some("mixed".to_owned()),
            output: ToolOutput::Unsupported,
        },
        HistoryItem::AssistantMessage,
    ];
    for index in 0..4 {
        history.push(HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: Some(format!("hot-{index}")),
            output: ToolOutput::Text("recent result".to_owned()),
        });
    }
    history
}

fn unconsumed_history() -> Vec<HistoryItem> {
    vec![HistoryItem::ToolResult {
        kind: ToolResultKind::Function,
        call_id: Some("unconsumed".to_owned()),
        output: ToolOutput::Text("u".repeat(64 * 1024)),
    }]
}

#[cfg(test)]
mod tests {
    use super::{BenchmarkDecision, BenchmarkFixtureKind, run_builtin_fixture};
    use crate::modules::aging::AgingPolicy;

    #[test]
    fn large_consumed_fixture_reports_aging() {
        let report = run_builtin_fixture(BenchmarkFixtureKind::TestLog, AgingPolicy::default());
        assert!(
            report
                .evaluations
                .iter()
                .any(|evaluation| { evaluation.decision == BenchmarkDecision::Aged })
        );
        assert!(report.stats.tool_result_bytes_saved > 0);
    }

    #[test]
    fn medium_fixture_explains_threshold_skips() {
        let report = run_builtin_fixture(
            BenchmarkFixtureKind::ManyMediumOutputs,
            AgingPolicy::default(),
        );
        assert!(
            report
                .evaluations
                .iter()
                .any(|evaluation| { evaluation.decision == BenchmarkDecision::AtOrBelowThreshold })
        );
    }

    #[test]
    fn unconsumed_fixture_explains_consumption_skip_when_frontier_disabled() {
        let policy = AgingPolicy {
            frontier: 0,
            ..AgingPolicy::default()
        };
        let report = run_builtin_fixture(BenchmarkFixtureKind::UnconsumedHistory, policy);
        assert_eq!(
            report.evaluations[0].decision,
            BenchmarkDecision::Unconsumed
        );
    }
}
