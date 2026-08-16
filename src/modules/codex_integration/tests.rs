use std::ffi::OsString;
use std::path::PathBuf;

use super::config::{
    connect_config_text, connection_state_text, disconnect_config_text, CodexConfigError,
    CodexConnectionState, OriginalOpenAiBaseUrl,
};
use super::path::resolve_codex_config_path;

const ENDPOINT: &str = "http://127.0.0.1:43117/0123456789abcdef/responses-root";

#[test]
fn connect_adds_only_owned_root_key() {
    let source = "model = \"gpt-5\"\n[mcp_servers.demo]\ncommand = \"demo\"\n";
    let (connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");

    assert!(connected.contains("model = \"gpt-5\""));
    assert!(connected.contains("[mcp_servers.demo]"));
    assert!(connected.contains(&format!("openai_base_url = \"{ENDPOINT}\"")));
    assert_eq!(snapshot.original_openai_base_url, OriginalOpenAiBaseUrl::Absent);
}

#[test]
fn existing_openai_base_url_is_restored() {
    let source = "openai_base_url = \"https://example.invalid/v1\"\nmodel = \"gpt-5\"\n";
    let (connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");
    let restored = disconnect_config_text(&connected, &snapshot).expect("disconnect config");

    assert!(restored.contains("openai_base_url = \"https://example.invalid/v1\""));
    assert!(restored.contains("model = \"gpt-5\""));
}

#[test]
fn inserted_openai_base_url_is_removed_on_disconnect() {
    let source = "model = \"gpt-5\"\n";
    let (connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");
    let restored = disconnect_config_text(&connected, &snapshot).expect("disconnect config");

    assert!(!restored.contains("openai_base_url"));
    assert!(restored.contains("model = \"gpt-5\""));
}

#[test]
fn drift_refuses_to_overwrite_newer_user_value() {
    let source = "model = \"gpt-5\"\n";
    let (_connected, snapshot) = connect_config_text(source, ENDPOINT).expect("connect config");
    let drifted = "openai_base_url = \"https://newer-user-value.example/v1\"\nmodel = \"gpt-5\"\n";

    let error = disconnect_config_text(drifted, &snapshot).expect_err("must detect drift");
    assert!(matches!(error, CodexConfigError::Drift { .. }));
    assert_eq!(
        connection_state_text(drifted, &snapshot).expect("state"),
        CodexConnectionState::Drifted
    );
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
