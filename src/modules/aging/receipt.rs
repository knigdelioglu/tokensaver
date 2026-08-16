use sha2::{Digest, Sha256};

pub(super) fn build_receipt(
    value: &str,
    tool_name: Option<&str>,
    preview_code_units: usize,
) -> String {
    let bytes = value.len();
    let digest = format!("{:x}", Sha256::digest(value.as_bytes()));
    let recovery = tool_name.map_or_else(
        || "Repeat the preceding tool call with the same arguments".to_owned(),
        |name| format!("Repeat the preceding {name} call with the same arguments"),
    );

    [
        format!(
            "[Older tool result compacted by TokenSaver after the model acted on it: {bytes} bytes, sha256:{digest}."
        ),
        format!(
            "{recovery} if exact or omitted content is needed. TokenSaver compacted only the forwarded historical copy.]"
        ),
        String::new(),
        "--- beginning of original result ---".to_owned(),
        safe_head(value, preview_code_units).to_owned(),
        "--- omitted middle of original result ---".to_owned(),
        safe_tail(value, preview_code_units).to_owned(),
        "--- end of original result ---".to_owned(),
    ]
    .join("\n")
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
    use super::{build_receipt, safe_head, safe_tail};

    #[test]
    fn sha256_identity_is_stable() {
        let receipt = build_receipt("abc", None, 1_024);
        assert!(receipt.contains(
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
    }

    #[test]
    fn previews_follow_utf16_boundaries() {
        let value = format!("{}😀{}", "a".repeat(1_023), "z".repeat(1_023));
        let head = safe_head(&value, 1_024);
        let tail = safe_tail(&value, 1_024);

        assert_eq!(head, "a".repeat(1_023));
        assert_eq!(tail, "z".repeat(1_023));
    }
}
