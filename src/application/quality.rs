#![allow(dead_code)]

use crate::modules::aging::{
    AgingPolicy, HistoryItem, ToolOutput, ToolResultKind, age_tool_results, parse_receipt,
    verify_exact_candidate,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QualityFixtureKind {
    EvidenceBoundary,
    ManyAgedResults,
    LongDistanceAfterConsumption,
}

impl QualityFixtureKind {
    pub(crate) const ALL: [Self; 3] = [
        Self::EvidenceBoundary,
        Self::ManyAgedResults,
        Self::LongDistanceAfterConsumption,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::EvidenceBoundary => "evidence-boundary",
            Self::ManyAgedResults => "many-aged-results",
            Self::LongDistanceAfterConsumption => "long-distance-after-consumption",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QualityReport {
    pub(crate) fixture: &'static str,
    pub(crate) tool_results_aged: usize,
    pub(crate) bytes_saved: usize,
    pub(crate) receipts_parseable: bool,
    pub(crate) exact_sources_verify: bool,
    pub(crate) changed_sources_rejected: bool,
    pub(crate) head_sentinel_visible: Option<bool>,
    pub(crate) middle_sentinel_visible: Option<bool>,
    pub(crate) tail_sentinel_visible: Option<bool>,
}

pub(crate) fn run_quality_fixture(fixture: QualityFixtureKind) -> QualityReport {
    let (history, exact_sources) = fixture_history(fixture);
    let result = age_tool_results(&history, quality_policy());

    let mut receipts_parseable = true;
    let mut exact_sources_verify = true;
    let mut changed_sources_rejected = true;
    for replacement in &result.replacements {
        let Some(source) = exact_sources
            .get(replacement.item_index)
            .and_then(|source| source.as_deref())
        else {
            exact_sources_verify = false;
            changed_sources_rejected = false;
            continue;
        };
        match parse_receipt(&replacement.receipt) {
            Ok(evidence) => {
                exact_sources_verify &= verify_exact_candidate(&evidence, source);
                let changed = mutate_same_length(source);
                changed_sources_rejected &= !verify_exact_candidate(&evidence, &changed);
            }
            Err(_) => {
                receipts_parseable = false;
                exact_sources_verify = false;
                changed_sources_rejected = false;
            }
        }
    }

    let (head_sentinel_visible, middle_sentinel_visible, tail_sentinel_visible) =
        if fixture == QualityFixtureKind::EvidenceBoundary {
            let receipt = result
                .replacements
                .first()
                .map(|replacement| replacement.receipt.as_str());
            (
                receipt.map(|value| value.contains("HEAD-SENTINEL")),
                receipt.map(|value| value.contains("MIDDLE-SENTINEL")),
                receipt.map(|value| value.contains("TAIL-SENTINEL")),
            )
        } else {
            (None, None, None)
        };

    QualityReport {
        fixture: fixture.name(),
        tool_results_aged: result.stats.tool_results_aged,
        bytes_saved: result.stats.tool_result_bytes_saved,
        receipts_parseable,
        exact_sources_verify,
        changed_sources_rejected,
        head_sentinel_visible,
        middle_sentinel_visible,
        tail_sentinel_visible,
    }
}

pub(crate) fn run_quality_suite() -> Vec<QualityReport> {
    QualityFixtureKind::ALL
        .into_iter()
        .map(run_quality_fixture)
        .collect()
}

fn quality_policy() -> AgingPolicy {
    AgingPolicy {
        frontier: 0,
        ..AgingPolicy::default()
    }
}

fn fixture_history(fixture: QualityFixtureKind) -> (Vec<HistoryItem>, Vec<Option<String>>) {
    match fixture {
        QualityFixtureKind::EvidenceBoundary => evidence_boundary_history(),
        QualityFixtureKind::ManyAgedResults => many_aged_results_history(),
        QualityFixtureKind::LongDistanceAfterConsumption => long_distance_history(),
    }
}

fn evidence_boundary_history() -> (Vec<HistoryItem>, Vec<Option<String>>) {
    let source = format!(
        "HEAD-SENTINEL\n{}\nMIDDLE-SENTINEL\n{}\nTAIL-SENTINEL",
        "a".repeat(48 * 1024),
        "b".repeat(48 * 1024)
    );
    let history = vec![
        HistoryItem::FunctionCall {
            call_id: Some("boundary-call".to_owned()),
            name: Some("read-file".to_owned()),
        },
        HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: Some("boundary-call".to_owned()),
            output: ToolOutput::Text(source.clone()),
        },
        HistoryItem::AssistantMessage,
    ];
    let mut sources = vec![None; history.len()];
    sources[1] = Some(source);
    (history, sources)
}

fn many_aged_results_history() -> (Vec<HistoryItem>, Vec<Option<String>>) {
    let mut history = Vec::new();
    let mut sources = Vec::new();
    for index in 0..12 {
        history.push(HistoryItem::FunctionCall {
            call_id: Some(format!("many-{index}")),
            name: Some("shell".to_owned()),
        });
        sources.push(None);

        let source = format!("result-{index}\n{}\nend-{index}", "x".repeat(48 * 1024));
        history.push(HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: Some(format!("many-{index}")),
            output: ToolOutput::Text(source.clone()),
        });
        sources.push(Some(source));

        history.push(HistoryItem::AssistantMessage);
        sources.push(None);
    }
    (history, sources)
}

fn long_distance_history() -> (Vec<HistoryItem>, Vec<Option<String>>) {
    let source = format!(
        "long-distance-head\n{}\nlong-distance-tail",
        "y".repeat(64 * 1024)
    );
    let mut history = vec![
        HistoryItem::FunctionCall {
            call_id: Some("old-call".to_owned()),
            name: Some("repository-search".to_owned()),
        },
        HistoryItem::ToolResult {
            kind: ToolResultKind::Function,
            call_id: Some("old-call".to_owned()),
            output: ToolOutput::Text(source.clone()),
        },
        HistoryItem::AssistantMessage,
    ];
    let mut sources = vec![None, Some(source), None];

    for _ in 0..128 {
        history.push(HistoryItem::Other);
        sources.push(None);
    }
    history.push(HistoryItem::Reasoning);
    sources.push(None);

    (history, sources)
}

fn mutate_same_length(source: &str) -> String {
    if source.is_empty() {
        return source.to_owned();
    }
    let mut bytes = source.as_bytes().to_vec();
    let index = bytes
        .iter()
        .position(|byte| byte.is_ascii_alphabetic())
        .unwrap_or(0);
    bytes[index] = if bytes[index] == b'Z' { b'Y' } else { b'Z' };
    String::from_utf8(bytes).expect("fixture mutation remains UTF-8")
}

#[cfg(test)]
mod tests {
    use super::{QualityFixtureKind, run_quality_fixture};

    #[test]
    fn evidence_boundary_keeps_head_and_tail_but_not_middle() {
        let report = run_quality_fixture(QualityFixtureKind::EvidenceBoundary);
        assert_eq!(report.head_sentinel_visible, Some(true));
        assert_eq!(report.middle_sentinel_visible, Some(false));
        assert_eq!(report.tail_sentinel_visible, Some(true));
        assert!(report.receipts_parseable);
        assert!(report.exact_sources_verify);
        assert!(report.changed_sources_rejected);
    }

    #[test]
    fn many_old_results_can_be_verified_individually() {
        let report = run_quality_fixture(QualityFixtureKind::ManyAgedResults);
        assert_eq!(report.tool_results_aged, 12);
        assert!(report.bytes_saved > 0);
        assert!(report.receipts_parseable);
        assert!(report.exact_sources_verify);
        assert!(report.changed_sources_rejected);
    }

    #[test]
    fn old_consumed_result_remains_eligible_after_long_history_distance() {
        let report = run_quality_fixture(QualityFixtureKind::LongDistanceAfterConsumption);
        assert_eq!(report.tool_results_aged, 1);
        assert!(report.receipts_parseable);
        assert!(report.exact_sources_verify);
    }
}
