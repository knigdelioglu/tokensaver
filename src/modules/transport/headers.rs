use axum::http::header::{ACCEPT_ENCODING, CONTENT_TYPE, ORIGIN};
use axum::http::{HeaderMap, HeaderName, HeaderValue};

/// Native Codex request headers that TokenSaver may relay to the fixed OpenAI
/// upstream. The list intentionally excludes browser/proxy/hop-by-hop headers.
const FORWARD_HEADERS: &[&str] = &[
    "authorization",
    "chatgpt-account-id",
    "openai-beta",
    "openai-organization",
    "openai-project",
    "originator",
    "session_id",
    "session-id",
    "thread-id",
    "user-agent",
    "version",
    "x-client-request-id",
    "x-codex-beta-features",
    "x-codex-installation-id",
    "x-codex-parent-thread-id",
    "x-codex-turn-metadata",
    "x-codex-turn-state",
    "x-codex-window-id",
    "x-oai-attestation",
    "x-openai-subagent",
    "x-responsesapi-include-timing-metrics",
];

pub(crate) fn has_browser_origin(headers: &HeaderMap) -> bool {
    headers.contains_key(ORIGIN)
}

pub(crate) fn native_upstream_headers(headers: &HeaderMap) -> HeaderMap {
    let mut forwarded = HeaderMap::new();
    forwarded.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    forwarded.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));

    for name in FORWARD_HEADERS {
        let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        for value in headers.get_all(&header_name) {
            forwarded.append(header_name.clone(), value.clone());
        }
    }

    forwarded
}
