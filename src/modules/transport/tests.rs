use axum::http::{HeaderMap, HeaderValue};
use serde_json::Value;

use crate::modules::aging::AgingPolicy;

use super::capability::CallerCapability;
use super::compression::EncodingChain;
use super::headers::{has_browser_origin, native_upstream_headers};
use super::request::{prepare_responses_body, PreparationOutcome};

fn large_output() -> String {
    "0123456789abcdef".repeat(3_000)
}

fn aging_policy() -> AgingPolicy {
    AgingPolicy {
        frontier: 0,
        ..AgingPolicy::default()
    }
}

fn consumed_request_json() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "model": "gpt-test",
        "input": [
            {
                "type": "function_call",
                "call_id": "call-1",
                "name": "shell",
                "arguments": "{}"
            },
            {
                "type": "function_call_output",
                "call_id": "call-1",
                "output": large_output()
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "done"}]
            }
        ],
        "stream": true
    }))
    .expect("serialize fixture")
}

#[test]
fn capability_authenticates_only_exact_secret_and_v1_prefix() {
    let capability = CallerCapability::from_secret("secret-value");
    assert_eq!(
        capability.authenticate_path("/secret-value/v1/responses"),
        Some("/v1/responses")
    );
    assert_eq!(capability.authenticate_path("/wrong/v1/responses"), None);
    assert_eq!(capability.authenticate_path("/secret-value-extra/v1/responses"), None);
    assert_eq!(capability.authenticate_path("/secret-value/responses"), None);
}

#[test]
fn capability_base_url_round_trips_port_secret_and_v1() {
    let secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let capability = CallerCapability::from_secret(secret);
    let url = capability.loopback_base_url(43117);
    assert_eq!(url, format!("http://127.0.0.1:43117/{secret}/v1"));

    let (port, recovered) = CallerCapability::from_loopback_base_url(&url).expect("recover URL");
    assert_eq!(port, 43117);
    assert_eq!(recovered.loopback_base_url(port), url);
}

#[test]
fn disabled_mode_keeps_responses_body_byte_for_byte() {
    let source = consumed_request_json();
    let policy = AgingPolicy {
        enabled: false,
        ..aging_policy()
    };
    let prepared = prepare_responses_body(&source, None, "/v1/responses", policy);

    assert_eq!(prepared.outcome, PreparationOutcome::Disabled);
    assert_eq!(prepared.bytes, source);
    assert!(!prepared.body_changed);
}

#[test]
fn compact_endpoint_bypasses_aging_byte_for_byte_even_when_saving_is_off() {
    let source = consumed_request_json();
    let policy = AgingPolicy {
        enabled: false,
        ..aging_policy()
    };
    let prepared = prepare_responses_body(&source, None, "/v1/responses/compact", policy);

    assert_eq!(prepared.outcome, PreparationOutcome::CompactionBypass);
    assert_eq!(prepared.bytes, source);
    assert!(!prepared.body_changed);
}

#[test]
fn native_endpoint_is_passthrough_independent_of_saving_toggle() {
    let source = br#"{"query":"hello"}"#.to_vec();
    let policy = AgingPolicy {
        enabled: false,
        ..aging_policy()
    };
    let prepared = prepare_responses_body(&source, None, "/v1/alpha/search", policy);

    assert_eq!(prepared.outcome, PreparationOutcome::NativePassthrough);
    assert_eq!(prepared.bytes, source);
    assert!(!prepared.body_changed);
}

#[test]
fn ordinary_responses_request_changes_only_eligible_output_semantically() {
    let source = consumed_request_json();
    let original: Value = serde_json::from_slice(&source).expect("original JSON");
    let prepared = prepare_responses_body(&source, None, "/v1/responses", aging_policy());
    let optimized: Value = serde_json::from_slice(&prepared.bytes).expect("optimized JSON");

    assert_eq!(prepared.outcome, PreparationOutcome::Aged);
    assert!(prepared.body_changed);

    let mut expected = original;
    let receipt = prepared.aging.replacements[0].receipt.clone();
    expected["input"][1]["output"] = Value::String(receipt);
    assert_eq!(optimized, expected);
}

#[test]
fn mixed_output_is_preserved() {
    let source = serde_json::to_vec(&serde_json::json!({
        "input": [
            {
                "type": "custom_tool_call_output",
                "call_id": "mixed",
                "output": [
                    {"type": "text", "text": large_output()},
                    {"type": "input_image", "image_url": "data:image/png;base64,AA=="}
                ]
            },
            {"type": "message", "role": "assistant", "content": []}
        ]
    }))
    .expect("serialize mixed fixture");

    let prepared = prepare_responses_body(&source, None, "/v1/responses", aging_policy());
    assert_eq!(prepared.outcome, PreparationOutcome::EvaluatedNoEligibleResult);
    assert_eq!(prepared.bytes, source);
}

#[test]
fn supported_content_encodings_round_trip_when_aging_changes_body() {
    let source = consumed_request_json();
    for encoding in ["gzip", "x-gzip", "deflate", "br", "zstd"] {
        let chain = EncodingChain::parse(Some(encoding)).expect("encoding");
        let encoded = chain.encode(&source).expect("encode fixture");
        let prepared =
            prepare_responses_body(&encoded, Some(encoding), "/v1/responses", aging_policy());
        assert_eq!(prepared.outcome, PreparationOutcome::Aged, "{encoding}");
        let decoded = chain.decode(&prepared.bytes).expect("decode optimized");
        let optimized: Value = serde_json::from_slice(&decoded).expect("optimized JSON");
        assert!(optimized["input"][1]["output"]
            .as_str()
            .is_some_and(|value| value.contains("compacted by TokenSaver")));
    }
}

#[test]
fn unsupported_encoding_fails_original() {
    let source = consumed_request_json();
    let prepared =
        prepare_responses_body(&source, Some("compress"), "/v1/responses", aging_policy());
    assert_eq!(prepared.outcome, PreparationOutcome::FailOriginal);
    assert_eq!(prepared.bytes, source);
}

#[test]
fn browser_origin_is_rejected_and_auth_headers_are_allowlisted() {
    let mut headers = HeaderMap::new();
    headers.insert("origin", HeaderValue::from_static("https://example.com"));
    headers.insert("authorization", HeaderValue::from_static("Bearer secret"));
    headers.insert("chatgpt-account-id", HeaderValue::from_static("account"));
    headers.insert("cookie", HeaderValue::from_static("private-cookie"));
    headers.insert("x-random-header", HeaderValue::from_static("nope"));
    headers.insert("content-type", HeaderValue::from_static("application/json; charset=utf-8"));

    assert!(has_browser_origin(&headers));
    let forwarded = native_upstream_headers(&headers);
    assert_eq!(forwarded.get("authorization").unwrap(), "Bearer secret");
    assert_eq!(forwarded.get("chatgpt-account-id").unwrap(), "account");
    assert_eq!(
        forwarded.get("content-type").unwrap(),
        "application/json; charset=utf-8"
    );
    assert!(forwarded.get("cookie").is_none());
    assert!(forwarded.get("x-random-header").is_none());
    assert!(forwarded.get("origin").is_none());
}
