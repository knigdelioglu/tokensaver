/// Redact loopback URL material before it is surfaced through diagnostics or UI.
///
/// TokenSaver's caller capability is carried in a loopback URL path. Outward
/// status text never needs that URL, so the conservative rule is to redact the
/// complete loopback token rather than trying to preserve its port/path shape.
pub(crate) fn redact_local_secrets(input: &str) -> String {
    const PREFIX: &str = "http://127.0.0.1:";
    let mut remaining = input;
    let mut output = String::with_capacity(input.len());

    while let Some(start) = remaining.find(PREFIX) {
        output.push_str(&remaining[..start]);
        output.push_str("http://127.0.0.1:[REDACTED]");

        let after_prefix = &remaining[start + PREFIX.len()..];
        let end = after_prefix
            .find(|character: char| {
                character.is_whitespace()
                    || matches!(character, '"' | '\'' | ',' | ';' | ')' | ']' | '}')
            })
            .unwrap_or(after_prefix.len());
        remaining = &after_prefix[end..];
    }

    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::redact_local_secrets;

    #[test]
    fn loopback_capability_url_is_not_returned() {
        let source = "endpoint http://127.0.0.1:43117/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/v1 failed";
        let redacted = redact_local_secrets(source);
        assert!(!redacted.contains("aaaaaaaa"));
        assert!(redacted.contains("http://127.0.0.1:[REDACTED]"));
    }
}
