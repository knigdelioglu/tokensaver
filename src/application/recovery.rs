use crate::modules::aging::{
    parse_receipt, verify_exact_candidate, ReceiptEvidence, ReceiptParseError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryNeed {
    /// The caller only needs the verbatim preview evidence already carried by
    /// the receipt. No claim is made about omitted bytes.
    VisibleEvidenceOnly,
    /// The caller needs exact original content, including the omitted middle.
    ExactContent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryDisposition {
    /// Head/tail evidence can be used exactly as shown, while omitted bytes stay
    /// explicitly unknown.
    ReceiptEvidenceAvailable,
    /// The receipt cannot satisfy this request. The exact source must be
    /// obtained again through a trusted normal workflow; TokenSaver must not
    /// reconstruct or guess it.
    ExactSourceRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecoveryAssessment {
    pub(crate) disposition: RecoveryDisposition,
    pub(crate) original_bytes: usize,
    pub(crate) visible_bytes: usize,
    pub(crate) omitted_bytes: usize,
    pub(crate) sha256: String,
    pub(crate) head: String,
    pub(crate) tail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateVerification {
    VerifiedExact,
    Rejected,
}

pub(crate) fn assess_receipt(
    receipt: &str,
    need: RecoveryNeed,
) -> Result<RecoveryAssessment, ReceiptParseError> {
    let evidence = parse_receipt(receipt)?;
    Ok(assessment_from_evidence(evidence, need))
}

pub(crate) fn verify_recovered_candidate(
    receipt: &str,
    candidate: &str,
) -> Result<CandidateVerification, ReceiptParseError> {
    let evidence = parse_receipt(receipt)?;
    Ok(if verify_exact_candidate(&evidence, candidate) {
        CandidateVerification::VerifiedExact
    } else {
        CandidateVerification::Rejected
    })
}

fn assessment_from_evidence(
    evidence: ReceiptEvidence,
    need: RecoveryNeed,
) -> RecoveryAssessment {
    let disposition = match need {
        RecoveryNeed::VisibleEvidenceOnly => RecoveryDisposition::ReceiptEvidenceAvailable,
        RecoveryNeed::ExactContent => RecoveryDisposition::ExactSourceRequired,
    };

    RecoveryAssessment {
        disposition,
        original_bytes: evidence.original_bytes,
        visible_bytes: evidence.visible_bytes(),
        omitted_bytes: evidence.omitted_bytes(),
        sha256: evidence.sha256,
        head: evidence.head,
        tail: evidence.tail,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        assess_receipt, verify_recovered_candidate, CandidateVerification, RecoveryDisposition,
        RecoveryNeed,
    };
    use crate::modules::aging::{age_tool_results, AgingPolicy, HistoryItem, ToolOutput, ToolResultKind};

    fn receipt_and_source() -> (String, String) {
        let source = format!(
            "HEAD-SENTINEL\n{}\nMIDDLE-SENTINEL\n{}\nTAIL-SENTINEL",
            "a".repeat(40_000),
            "b".repeat(40_000)
        );
        let history = vec![
            HistoryItem::FunctionCall {
                call_id: Some("call-1".to_owned()),
                name: Some("read-file".to_owned()),
            },
            HistoryItem::ToolResult {
                kind: ToolResultKind::Function,
                call_id: Some("call-1".to_owned()),
                output: ToolOutput::Text(source.clone()),
            },
            HistoryItem::AssistantMessage,
        ];
        let result = age_tool_results(
            &history,
            AgingPolicy {
                frontier: 0,
                ..AgingPolicy::default()
            },
        );
        (result.replacements[0].receipt.clone(), source)
    }

    #[test]
    fn exact_need_never_claims_receipt_is_full_source() {
        let (receipt, _) = receipt_and_source();
        let assessment = assess_receipt(&receipt, RecoveryNeed::ExactContent).expect("assessment");

        assert_eq!(assessment.disposition, RecoveryDisposition::ExactSourceRequired);
        assert!(assessment.omitted_bytes > 0);
        assert!(!assessment.head.contains("MIDDLE-SENTINEL"));
        assert!(!assessment.tail.contains("MIDDLE-SENTINEL"));
    }

    #[test]
    fn visible_evidence_can_be_used_without_claiming_omitted_bytes() {
        let (receipt, _) = receipt_and_source();
        let assessment =
            assess_receipt(&receipt, RecoveryNeed::VisibleEvidenceOnly).expect("assessment");

        assert_eq!(
            assessment.disposition,
            RecoveryDisposition::ReceiptEvidenceAvailable
        );
        assert!(assessment.head.contains("HEAD-SENTINEL"));
        assert!(assessment.tail.contains("TAIL-SENTINEL"));
        assert!(assessment.omitted_bytes > 0);
    }

    #[test]
    fn exact_candidate_requires_digest_and_length_identity() {
        let (receipt, source) = receipt_and_source();
        assert_eq!(
            verify_recovered_candidate(&receipt, &source).expect("verify exact"),
            CandidateVerification::VerifiedExact
        );
        assert_eq!(
            verify_recovered_candidate(&receipt, &source.replace("MIDDLE-SENTINEL", "OTHER"))
                .expect("verify changed"),
            CandidateVerification::Rejected
        );
    }
}
