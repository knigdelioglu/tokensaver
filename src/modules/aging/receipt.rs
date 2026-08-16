#![allow(dead_code)]

use std::fmt;

use sha2::{Digest, Sha256};

const METADATA_PREFIX: &str = "[tokensaver-receipt:v1 ";
const BEGIN_MARKER: &str = "--- beginning of original result ---";
const OMITTED_MARKER: &str = "--- omitted middle of original result ---";
const END_MARKER: &str = "--- end of original result ---";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReceiptEvidence {
    pub(crate) original_bytes: usize,
    pub(crate) sha256: String,
    pub(crate) head: String,
    pub(crate) tail: String,
}

impl ReceiptEvidence {
    pub(crate) fn visible_bytes(&self) -> usize {
        self.head.len().saturating_add(self.tail.len())
    }

    pub(crate) fn omitted_bytes(&self) -> usize {
        self.original_bytes.saturating_sub(self.visible_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReceiptParseError {
    MissingMetadata,
    UnsupportedVersion,
    InvalidMetadata,
    InvalidDigest,
    InvalidLayout,
}

impl fmt::Display for ReceiptParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMetadata => write!(formatter, "TokenSaver receipt metadata is missing"),
            Self::UnsupportedVersion => {
                write!(formatter, "TokenSaver receipt version is unsupported")
            }
            Self::InvalidMetadata => write!(formatter, "TokenSaver receipt metadata is invalid"),
            Self::InvalidDigest => write!(formatter, "TokenSaver receipt digest is invalid"),
            Self::InvalidLayout => write!(formatter, "TokenSaver receipt layout is invalid"),
        }
    }
}

impl std::error::Error for ReceiptParseError {}

pub(super) fn build_receipt(
    value: &str,
    tool_name: Option<&str>,
    preview_code_units: usize,
) -> String {
    let bytes = value.len();
    let digest = sha256_hex(value);
    let head = safe_head(value, preview_code_units);
    let tail = safe_tail(value, preview_code_units);
    let recovery = tool_name.map_or_else(
        || "the preceding tool call".to_owned(),
        |name| format!("the preceding {name} call"),
    );

    [
        format!(
            "[Older tool result compacted by TokenSaver after the model acted on it: {bytes} bytes, sha256:{digest}.]"
        ),
        format!(
            "{METADATA_PREFIX}original_bytes={bytes} sha256={digest} head_bytes={} tail_bytes={}]",
            head.len(),
            tail.len()
        ),
        "[Evidence boundary: only the beginning and tail below are verbatim. The omitted middle is not present in this receipt and must not be inferred.]"
            .to_string(),
        format!(
            "[Recovery: if exact omitted content is required, repeat {recovery} with the same arguments only when it is safe to repeat; otherwise obtain the exact source through the normal Codex workflow. Exact recovery is trusted only when UTF-8 byte length and SHA-256 both match this receipt.]"
        ),
        String::new(),
        BEGIN_MARKER.to_owned(),
        head.to_owned(),
        OMITTED_MARKER.to_owned(),
        tail.to_owned(),
        END_MARKER.to_owned(),
    ]
    .join("\n")
}

pub(crate) fn parse_receipt(receipt: &str) -> Result<ReceiptEvidence, ReceiptParseError> {
    let metadata_line = receipt
        .lines()
        .find(|line| line.starts_with("[tokensaver-receipt:"))
        .ok_or(ReceiptParseError::MissingMetadata)?;
    if !metadata_line.starts_with(METADATA_PREFIX) {
        return Err(ReceiptParseError::UnsupportedVersion);
    }
    let metadata = metadata_line
        .strip_prefix(METADATA_PREFIX)
        .and_then(|value| value.strip_suffix(']'))
        .ok_or(ReceiptParseError::InvalidMetadata)?;

    let mut original_bytes = None;
    let mut digest = None;
    let mut head_bytes = None;
    let mut tail_bytes = None;
    for field in metadata.split_whitespace() {
        let Some((key, value)) = field.split_once('=') else {
            return Err(ReceiptParseError::InvalidMetadata);
        };
        match key {
            "original_bytes" => {
                original_bytes = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| ReceiptParseError::InvalidMetadata)?,
                );
            }
            "sha256" => digest = Some(value.to_owned()),
            "head_bytes" => {
                head_bytes = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| ReceiptParseError::InvalidMetadata)?,
                );
            }
            "tail_bytes" => {
                tail_bytes = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| ReceiptParseError::InvalidMetadata)?,
                );
            }
            _ => return Err(ReceiptParseError::InvalidMetadata),
        }
    }

    let original_bytes = original_bytes.ok_or(ReceiptParseError::InvalidMetadata)?;
    let digest = digest.ok_or(ReceiptParseError::InvalidMetadata)?;
    let head_bytes = head_bytes.ok_or(ReceiptParseError::InvalidMetadata)?;
    let tail_bytes = tail_bytes.ok_or(ReceiptParseError::InvalidMetadata)?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ReceiptParseError::InvalidDigest);
    }
    if head_bytes.saturating_add(tail_bytes) > original_bytes {
        return Err(ReceiptParseError::InvalidMetadata);
    }

    let head_start = section_start(receipt, BEGIN_MARKER)?;
    let head_end = head_start
        .checked_add(head_bytes)
        .filter(|end| *end <= receipt.len())
        .ok_or(ReceiptParseError::InvalidLayout)?;
    if !receipt.is_char_boundary(head_start) || !receipt.is_char_boundary(head_end) {
        return Err(ReceiptParseError::InvalidLayout);
    }
    let after_head = receipt
        .get(head_end..)
        .ok_or(ReceiptParseError::InvalidLayout)?;
    let omitted_prefix = format!("\n{OMITTED_MARKER}\n");
    if !after_head.starts_with(&omitted_prefix) {
        return Err(ReceiptParseError::InvalidLayout);
    }

    let tail_start = head_end + omitted_prefix.len();
    let tail_end = tail_start
        .checked_add(tail_bytes)
        .filter(|end| *end <= receipt.len())
        .ok_or(ReceiptParseError::InvalidLayout)?;
    if !receipt.is_char_boundary(tail_start) || !receipt.is_char_boundary(tail_end) {
        return Err(ReceiptParseError::InvalidLayout);
    }
    let after_tail = receipt
        .get(tail_end..)
        .ok_or(ReceiptParseError::InvalidLayout)?;
    let end_suffix = format!("\n{END_MARKER}");
    if !after_tail.starts_with(&end_suffix) {
        return Err(ReceiptParseError::InvalidLayout);
    }

    Ok(ReceiptEvidence {
        original_bytes,
        sha256: digest.to_ascii_lowercase(),
        head: receipt[head_start..head_end].to_owned(),
        tail: receipt[tail_start..tail_end].to_owned(),
    })
}

pub(crate) fn verify_exact_candidate(evidence: &ReceiptEvidence, candidate: &str) -> bool {
    candidate.len() == evidence.original_bytes && sha256_hex(candidate) == evidence.sha256
}

fn section_start(receipt: &str, marker: &str) -> Result<usize, ReceiptParseError> {
    let prefix = format!("{marker}\n");
    receipt
        .find(&prefix)
        .map(|index| index + prefix.len())
        .ok_or(ReceiptParseError::InvalidLayout)
}

fn sha256_hex(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn safe_head(value: &str, max_utf16_units: usize) -> &str {
    if max_utf16_units == 0 {
        return "";
    }

    let mut used_units = 0usize;
    let mut end = 0usize;

    for (index, character) in value.char_indices() {
        let character_units = character.len_utf16();
        if used_units + character_units > max_utf16_units {
            break;
        }
        used_units += character_units;
        end = index + character.len_utf8();
    }

    &value[..end]
}

fn safe_tail(value: &str, max_utf16_units: usize) -> &str {
    if max_utf16_units == 0 {
        return "";
    }

    let mut used_units = 0usize;
    let mut start = value.len();

    for (index, character) in value.char_indices().rev() {
        let character_units = character.len_utf16();
        if used_units + character_units > max_utf16_units {
            break;
        }
        used_units += character_units;
        start = index;
    }

    &value[start..]
}

#[cfg(test)]
mod tests {
    use super::{
        ReceiptParseError, build_receipt, parse_receipt, safe_head, safe_tail,
        verify_exact_candidate,
    };

    #[test]
    fn sha256_identity_is_stable() {
        let receipt = build_receipt("abc", None, 1_024);
        assert!(
            receipt.contains(
                "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            )
        );
    }

    #[test]
    fn previews_follow_utf16_boundaries() {
        let value = format!("{}😀{}", "a".repeat(1_023), "z".repeat(1_023));
        let head = safe_head(&value, 1_024);
        let tail = safe_tail(&value, 1_024);

        assert_eq!(head, "a".repeat(1_023));
        assert_eq!(tail, "z".repeat(1_023));
    }

    #[test]
    fn receipt_round_trip_exposes_only_declared_evidence() {
        let source = format!("HEAD{}TAIL", "middle\n".repeat(8_000));
        let receipt = build_receipt(&source, Some("shell"), 32);
        let evidence = parse_receipt(&receipt).expect("parse receipt");

        assert_eq!(evidence.original_bytes, source.len());
        assert!(source.starts_with(&evidence.head));
        assert!(source.ends_with(&evidence.tail));
        assert!(evidence.omitted_bytes() > 0);
        assert!(verify_exact_candidate(&evidence, &source));
        assert!(!verify_exact_candidate(&evidence, &format!("{source}x")));
    }

    #[test]
    fn parser_uses_declared_lengths_even_when_preview_contains_marker_text() {
        let source = format!(
            "prefix {marker} {} suffix",
            "x".repeat(40_000),
            marker = super::OMITTED_MARKER
        );
        let receipt = build_receipt(&source, None, 2_048);
        let evidence = parse_receipt(&receipt).expect("parse receipt");
        assert!(verify_exact_candidate(&evidence, &source));
    }

    #[test]
    fn unknown_receipt_version_is_rejected() {
        let receipt = "[tokensaver-receipt:v2 original_bytes=1 sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa head_bytes=0 tail_bytes=0]";
        assert_eq!(
            parse_receipt(receipt).expect_err("unsupported version"),
            ReceiptParseError::UnsupportedVersion
        );
    }
}
