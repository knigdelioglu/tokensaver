use std::ffi::OsString;
use std::path::PathBuf;

use super::config::{
    connect_config_text, connection_state_text, disconnect_config_text, CodexConfigError,
    CodexConnectionState, OriginalOpenAiBaseUrl,
};
use super::path::resolve_codex_config_path;

const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const ENDPOINT: &str =
    "http://127.0.0.1:43117/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/v1";

#[test]
fn connect_preserves_unrelated_config_and_installs_owned_native_overrides() {
    let source = "model = \"gpt-5\"\n[mcp_servers.demo]\ncommand = \"demo\"\n";
    let (connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");

    assert!(connected.contains("model = \"gpt-5\""));
    assert!(connected.contains("[mcp_servers.demo]"));
    assert!(connected.contains(&format!("openai_base_url = \"{ENDPOINT}\"")));
    assert!(connected.contains(
        "experimental_realtime_webrtc_call_base_url = \"https://chatgpt.com/backend-api/codex\""
    ));
    assert!(connected.contains(
        "experimental_realtime_ws_base_url = \"https://api.openai.com/v1\""
    ));
    assert_eq!(snapshot.original_openai_base_url, OriginalOpenAiBaseUrl::Absent);
    assert_eq!(
        snapshot.installed_realtime_call_base_url.as_deref(),
        Some("https://chatgpt.com/backend-api/codex")
    );
    assert_eq!(
        snapshot.installed_realtime_ws_base_url.as_deref(),
        Some("https://api.openai.com/v1")
    );
}

#[test]
fn existing_openai_base_url_is_restored() {
    let source = "openai_base_url = \"https://example.invalid/v1\"\nmodel = \"gpt-5\"\n";
    let (connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");
    let restored = disconnect_config_text(&connected, &snapshot).expect("disconnect config");

    assert!(restored.contains("openai_base_url = \"https://example.invalid/v1\""));
    assert!(restored.contains("model = \"gpt-5\""));
    assert!(!restored.contains("experimental_realtime_webrtc_call_base_url"));
    assert!(!restored.contains("experimental_realtime_ws_base_url"));
}

#[test]
fn inserted_openai_and_realtime_values_are_removed_on_disconnect() {
    let source = "model = \"gpt-5\"\n";
    let (connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");
    let restored = disconnect_config_text(&connected, &snapshot).expect("disconnect config");

    assert!(!restored.contains("openai_base_url"));
    assert!(!restored.contains("experimental_realtime_webrtc_call_base_url"));
    assert!(!restored.contains("experimental_realtime_ws_base_url"));
    assert!(restored.contains("model = \"gpt-5\""));
}

#[test]
fn existing_user_realtime_values_are_never_owned_or_changed() {
    let source = concat!(
        "model = \"gpt-5\"\n",
        "experimental_realtime_webrtc_call_base_url = \"https://voice.example/calls\"\n",
        "experimental_realtime_ws_base_url = \"https://voice.example/ws\"\n",
    );
    let (connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");

    assert!(connected.contains(
        "experimental_realtime_webrtc_call_base_url = \"https://voice.example/calls\""
    ));
    assert!(connected.contains(
        "experimental_realtime_ws_base_url = \"https://voice.example/ws\""
    ));
    assert!(snapshot.installed_realtime_call_base_url.is_none());
    assert!(snapshot.installed_realtime_ws_base_url.is_none());

    let restored = disconnect_config_text(&connected, &snapshot).expect("disconnect config");
    assert!(restored.contains(
        "experimental_realtime_webrtc_call_base_url = \"https://voice.example/calls\""
    ));
    assert!(restored.contains(
        "experimental_realtime_ws_base_url = \"https://voice.example/ws\""
    ));
}

#[test]
fn custom_chatgpt_base_url_drives_managed_realtime_call_override() {
    let source = "chatgpt_base_url = \"https://chatgpt.example/backend-api/\"\n";
    let (connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");

    assert!(connected.contains(
        "experimental_realtime_webrtc_call_base_url = \"https://chatgpt.example/backend-api/codex\""
    ));
    assert_eq!(
        snapshot.installed_realtime_call_base_url.as_deref(),
        Some("https://chatgpt.example/backend-api/codex")
    );
}

#[test]
fn drift_refuses_to_overwrite_newer_user_openai_value() {
    let source = "model = \"gpt-5\"\n";
    let (_connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");
    let drifted = concat!(
        "openai_base_url = \"https://newer-user-value.example/v1\"\n",
        "experimental_realtime_webrtc_call_base_url = \"https://chatgpt.com/backend-api/codex\"\n",
        "experimental_realtime_ws_base_url = \"https://api.openai.com/v1\"\n",
        "model = \"gpt-5\"\n",
    );

    let error = disconnect_config_text(drifted, &snapshot).expect_err("must detect drift");
    assert!(matches!(error, CodexConfigError::Drift { .. }));
    assert_eq!(
        connection_state_text(drifted, &snapshot).expect("state"),
        CodexConnectionState::Drifted
    );
}

#[test]
fn realtime_drift_refuses_disconnect_overwrite() {
    let source = "model = \"gpt-5\"\n";
    let (connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");
    let drifted = connected.replace(
        "experimental_realtime_ws_base_url = \"https://api.openai.com/v1\"",
        "experimental_realtime_ws_base_url = \"https://newer-user-value.example/realtime\"",
    );

    let error = disconnect_config_text(&drifted, &snapshot).expect_err("must detect drift");
    assert!(matches!(error, CodexConfigError::Drift { .. }));
}

#[test]
fn config_error_display_never_exposes_capability_or_drift_values() {
    let unsafe_error = CodexConfigError::UnsafeLoopbackUrl(ENDPOINT.to_owned()).to_string();
    assert!(!unsafe_error.contains(SECRET));
    assert!(!unsafe_error.contains(ENDPOINT));

    let replacement_error = CodexConfigError::ActiveSnapshotDifferentEndpoint {
        installed: ENDPOINT.to_owned(),
        requested: format!("http://127.0.0.1:43118/{SECRET}/v1"),
    }
    .to_string();
    assert!(!replacement_error.contains(SECRET));
    assert!(!replacement_error.contains("43117"));
    assert!(!replacement_error.contains("43118"));

    let expected = "https://private-user-value.example/original".to_owned();
    let actual = "https://private-user-value.example/changed".to_owned();
    let drift_error = CodexConfigError::Drift {
        key: "openai_base_url",
        expected: Some(expected.clone()),
        actual: Some(actual.clone()),
    }
    .to_string();
    assert!(!drift_error.contains(&expected));
    assert!(!drift_error.contains(&actual));
}

#[test]
fn config_parse_error_display_does_not_echo_parser_context() {
    let sensitive = "secret-parser-context";
    let error = CodexConfigError::InvalidToml(sensitive.to_owned()).to_string();
    assert!(!error.contains(sensitive));

    let snapshot_error = CodexConfigError::SnapshotFormat(sensitive.to_owned()).to_string();
    assert!(!snapshot_error.contains(sensitive));
}

#[test]
fn non_string_owned_key_is_rejected() {
    let source = "openai_base_url = 42\n";
    let error = connect_config_text(source, ENDPOINT).expect_err("invalid field type");
    assert!(matches!(
        error,
        CodexConfigError::UnsupportedOpenAiBaseUrlType
    ));
}

#[test]
fn non_loopback_endpoint_is_rejected() {
    let error = connect_config_text("", "https://example.com/proxy").expect_err("unsafe URL");
    assert!(matches!(error, CodexConfigError::UnsafeLoopbackUrl(_)));
}

#[test]
fn loopback_endpoint_requires_64_hex_capability_and_v1_suffix() {
    let missing_v1 = format!("http://127.0.0.1:43117/{SECRET}");
    let short_secret = "http://127.0.0.1:43117/abcd/v1";
    assert!(matches!(
        connect_config_text("", &missing_v1),
        Err(CodexConfigError::UnsafeLoopbackUrl(_))
    ));
    assert!(matches!(
        connect_config_text("", short_secret),
        Err(CodexConfigError::UnsafeLoopbackUrl(_))
    ));
}

#[test]
fn codex_home_environment_matches_codex_resolution_rule() {
    let temp = std::env::temp_dir().join(format!("tokensaver-path-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp home");

    let resolved = resolve_codex_config_path(Some(OsString::from(&temp)), None)
        .expect("resolve CODEX_HOME");
    let canonical = temp.canonicalize().expect("canonical temp");
    assert_eq!(resolved, canonical.join("config.toml"));

    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn default_codex_home_is_dot_codex_under_user_home() {
    let home = PathBuf::from("/Users/example");
    let resolved = resolve_codex_config_path(None, Some(home.clone())).expect("default path");
    assert_eq!(resolved, home.join(".codex").join("config.toml"));
}
