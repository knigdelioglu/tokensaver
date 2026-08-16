use std::{fs, path::Path};

fn rust_sources(root: &Path) -> Vec<String> {
    let mut sources = Vec::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(path) = pending.pop() {
        let entries = fs::read_dir(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

        for entry in entries {
            let entry = entry.expect("failed to read directory entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                sources.push(
                    fs::read_to_string(&path).unwrap_or_else(|error| {
                        panic!("failed to read {}: {error}", path.display())
                    }),
                );
            }
        }
    }

    sources
}

fn code_without_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_no_dependency(module_path: &str, forbidden: &[&str]) {
    for source in rust_sources(Path::new(module_path)) {
        let code = code_without_line_comments(&source);
        for dependency in forbidden {
            assert!(
                !code.contains(dependency),
                "{module_path} must not depend on {dependency}"
            );
        }
    }
}

#[test]
fn aging_domain_is_adapter_agnostic() {
    assert_no_dependency(
        "src/modules/aging",
        &[
            "crate::modules::transport",
            "crate::modules::codex_integration",
            "crate::modules::telemetry",
            "crate::modules::runtime",
            "crate::modules::diagnostics",
            "crate::application",
            "crate::desktop",
            "crate::cli",
        ],
    );
}

#[test]
fn shared_does_not_depend_on_product_modules() {
    assert_no_dependency(
        "src/shared",
        &[
            "crate::modules",
            "crate::application",
            "crate::desktop",
            "crate::cli",
        ],
    );
}

#[test]
fn telemetry_does_not_reach_into_transport() {
    assert_no_dependency("src/modules/telemetry", &["crate::modules::transport"]);
}

#[test]
fn codex_integration_does_not_own_aging() {
    assert_no_dependency("src/modules/codex_integration", &["crate::modules::aging"]);
}

#[test]
fn runtime_is_not_a_cross_module_orchestrator() {
    assert_no_dependency(
        "src/modules/runtime",
        &[
            "crate::modules::aging",
            "crate::modules::transport",
            "crate::modules::codex_integration",
            "crate::modules::telemetry",
            "crate::application",
            "crate::desktop",
            "crate::cli",
        ],
    );
}

#[test]
fn desktop_shell_uses_application_boundary_not_product_modules() {
    assert_no_dependency("src/desktop", &["crate::modules::"]);
}

#[test]
fn cli_shell_uses_application_boundary_not_product_modules_or_shared_state() {
    assert_no_dependency("src/cli", &["crate::modules::", "crate::shared::"]);
}
