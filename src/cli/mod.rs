use std::error::Error;
use std::fmt;

use crate::application::control::{
    ControlRequest, ControlResponse, ControlSavings, ControlSnapshot,
};
use crate::application::doctor::{DoctorSeverity, run_doctor};
use crate::application::maintenance::purge_owned_state;
use crate::application::runtime_client::send_runtime_request;
use crate::application::settings::{
    SettingsSnapshot, load_product_settings, set_product_numeric_setting,
};
use crate::application::stats::{StoredSavingsView, load_product_stats};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
struct CliError(String);

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CliError {}

pub(crate) fn is_cli_invocation(args: &[String]) -> bool {
    args.first()
        .is_some_and(|argument| !argument.starts_with("-psn_"))
}

pub(crate) fn run(args: Vec<String>) -> Result<i32, Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_async(args))
}

async fn run_async(args: Vec<String>) -> Result<i32, Box<dyn Error>> {
    let command = args.first().map(String::as_str).unwrap_or("help");
    match command {
        "help" | "--help" | "-h" => {
            print_help();
            Ok(0)
        }
        "version" | "--version" | "-V" => {
            println!("TokenSaver {VERSION}");
            Ok(0)
        }
        "status" => status().await,
        "connect" => runtime_mutation(ControlRequest::Connect).await,
        "disconnect" => runtime_mutation(ControlRequest::Disconnect).await,
        "saving" => saving(&args[1..]).await,
        "stats" => stats().await,
        "config" => config(&args[1..]).await,
        "doctor" => doctor().await,
        "uninstall" => uninstall(&args[1..]).await,
        other => Err(Box::new(CliError(format!(
            "unknown command {other:?}; run `tokensaver help`"
        )))),
    }
}

async fn status() -> Result<i32, Box<dyn Error>> {
    match runtime_request(ControlRequest::Status).await {
        Ok(response) if response.ok => {
            if let Some(snapshot) = response.snapshot {
                print_status(&snapshot);
            } else {
                println!("Runtime: reachable");
            }
            Ok(0)
        }
        Ok(response) => {
            eprintln!(
                "Runtime error: {}",
                response
                    .message
                    .unwrap_or_else(|| "unknown error".to_owned())
            );
            Ok(1)
        }
        Err(_) => {
            println!("Runtime: not running");
            if let Ok(settings) = load_product_settings() {
                println!(
                    "Token saving preference: {}",
                    on_off(settings.saving_enabled)
                );
                println!(
                    "Reconnect on launch: {}",
                    yes_no(settings.connect_on_launch)
                );
            }
            Ok(1)
        }
    }
}

async fn runtime_mutation(request: ControlRequest) -> Result<i32, Box<dyn Error>> {
    let response = runtime_request(request).await.map_err(|_| {
        Box::new(CliError(
            "TokenSaver menu-bar runtime is not running; open TokenSaver first".to_owned(),
        )) as Box<dyn Error>
    })?;
    print_response_message(&response);
    if let Some(snapshot) = response.snapshot.as_ref() {
        print_status(snapshot);
    }
    Ok(if response.ok { 0 } else { 1 })
}

async fn saving(args: &[String]) -> Result<i32, Box<dyn Error>> {
    let enabled = match args {
        [value] if value == "on" => true,
        [value] if value == "off" => false,
        _ => {
            return Err(Box::new(CliError(
                "usage: tokensaver saving <on|off>".to_owned(),
            )));
        }
    };
    runtime_mutation(ControlRequest::Saving { enabled }).await
}

async fn stats() -> Result<i32, Box<dyn Error>> {
    if let Ok(ControlResponse {
        ok: true,
        snapshot: Some(snapshot),
        ..
    }) = runtime_request(ControlRequest::Stats).await
    {
        println!("This session");
        print_savings_control(&snapshot.session);
        println!("Today");
        print_savings_control(&snapshot.today);
        println!("All time");
        print_savings_control(&snapshot.all_time);
        return Ok(0);
    }

    let stored = load_product_stats()?;
    println!("Runtime: not running; showing persisted counters");
    println!("Today");
    print_savings_stored(stored.today);
    println!("All time");
    print_savings_stored(stored.all_time);
    Ok(0)
}

async fn config(args: &[String]) -> Result<i32, Box<dyn Error>> {
    match args {
        [command] if command == "show" => config_show().await,
        [command, key, value] if command == "set" => {
            let value = value.parse::<usize>().map_err(|_| {
                Box::new(CliError(format!("{key} requires a non-negative integer")))
                    as Box<dyn Error>
            })?;
            config_set(key, value).await
        }
        _ => Err(Box::new(CliError(
            "usage: tokensaver config show | tokensaver config set <min-bytes|frontier|preview-code-units> <value>"
                .to_owned(),
        ))),
    }
}

async fn config_show() -> Result<i32, Box<dyn Error>> {
    if let Ok(ControlResponse {
        ok: true,
        snapshot: Some(snapshot),
        ..
    }) = runtime_request(ControlRequest::ConfigShow).await
    {
        println!("saving = {}", on_off(snapshot.saving_enabled));
        println!("connect_on_launch = {}", snapshot.connect_on_launch);
        println!("min_bytes = {}", snapshot.policy.min_bytes);
        println!("frontier = {}", snapshot.policy.frontier);
        println!(
            "preview_code_units = {}",
            snapshot.policy.preview_code_units
        );
        return Ok(0);
    }

    print_settings(load_product_settings()?);
    Ok(0)
}

async fn config_set(key: &str, value: usize) -> Result<i32, Box<dyn Error>> {
    match runtime_request(ControlRequest::ConfigSet {
        key: key.to_owned(),
        value,
    })
    .await
    {
        Ok(response) => {
            print_response_message(&response);
            Ok(if response.ok { 0 } else { 1 })
        }
        Err(_) => {
            let settings = set_product_numeric_setting(key, value)?;
            println!("Runtime: not running; persisted setting for next connection");
            print_settings(settings);
            Ok(0)
        }
    }
}

async fn doctor() -> Result<i32, Box<dyn Error>> {
    let report = run_doctor().await;
    for check in &report.checks {
        let label = match check.severity {
            DoctorSeverity::Pass => "PASS",
            DoctorSeverity::Warning => "WARN",
            DoctorSeverity::Failure => "FAIL",
        };
        println!("[{label}] {} — {}", check.name, check.detail);
    }
    Ok(if report.has_failures() { 1 } else { 0 })
}

async fn uninstall(args: &[String]) -> Result<i32, Box<dyn Error>> {
    match args {
        [] => {
            println!("Safe uninstall preparation is a two-step operation:");
            println!("  1. In the TokenSaver menu-bar menu choose ‘Prepare for Uninstall…’.");
            println!(
                "     This safely disconnects Codex, clears reconnect intent, disables Start at Login, and exits."
            );
            println!(
                "  2. Optionally run `tokensaver uninstall --purge-state` before removing TokenSaver.app to delete TokenSaver-owned preferences/statistics."
            );
            Ok(0)
        }
        [flag] if flag == "--purge-state" => purge_state_for_uninstall().await,
        _ => Err(Box::new(CliError(
            "usage: tokensaver uninstall [--purge-state]".to_owned(),
        ))),
    }
}

async fn purge_state_for_uninstall() -> Result<i32, Box<dyn Error>> {
    if runtime_request(ControlRequest::Status).await.is_ok() {
        return Err(Box::new(CliError(
            "TokenSaver runtime is still running; choose ‘Prepare for Uninstall…’ from the menu-bar app first"
                .to_owned(),
        )));
    }

    let report = purge_owned_state()?;
    if report.removed_files.is_empty() {
        println!("No TokenSaver-owned state files needed removal.");
    } else {
        println!("Removed TokenSaver-owned state:");
        for file in &report.removed_files {
            println!("  {file}");
        }
    }

    if report.removed_data_directory {
        println!("TokenSaver state directory is removed/absent.");
    } else if !report.preserved_entries.is_empty() {
        println!("Preserved unknown entries (not owned by the uninstaller):");
        for entry in &report.preserved_entries {
            println!("  {entry}");
        }
    }
    println!(
        "You can now remove TokenSaver.app. Codex configuration was not modified by this purge command."
    );
    Ok(0)
}

async fn runtime_request(request: ControlRequest) -> Result<ControlResponse, Box<dyn Error>> {
    Ok(send_runtime_request(&request).await?)
}

fn print_status(snapshot: &ControlSnapshot) {
    println!("Runtime: {}", snapshot.service);
    println!("Codex config: {}", snapshot.codex);
    println!("Token saving: {}", on_off(snapshot.saving_enabled));
    println!("Active requests: {}", snapshot.active_requests);
    if snapshot.session.requests_observed == 0 {
        println!("Traffic: not seen this session");
    } else {
        println!(
            "Traffic: seen ({} requests this session)",
            snapshot.session.requests_observed
        );
    }
    if let Some(error) = snapshot.last_error.as_deref() {
        println!("Health: {}", single_line(error));
    } else if snapshot.dropped_telemetry_observations > 0 {
        println!(
            "Health: telemetry incomplete ({} observations dropped)",
            snapshot.dropped_telemetry_observations
        );
    } else {
        println!("Health: ok");
    }
}

fn print_response_message(response: &ControlResponse) {
    if let Some(message) = response.message.as_deref() {
        if response.ok {
            println!("{message}");
        } else {
            eprintln!("{message}");
        }
    }
}

fn print_settings(settings: SettingsSnapshot) {
    println!("saving = {}", on_off(settings.saving_enabled));
    println!("connect_on_launch = {}", settings.connect_on_launch);
    println!("min_bytes = {}", settings.min_bytes);
    println!("frontier = {}", settings.frontier);
    println!("preview_code_units = {}", settings.preview_code_units);
}

fn print_savings_control(savings: &ControlSavings) {
    println!("  observed requests: {}", savings.requests_observed);
    println!("  Responses requests: {}", savings.responses_requests);
    println!("  aged requests: {}", savings.aged_requests);
    println!("  no eligible result: {}", savings.no_eligible_requests);
    println!("  eligible but no savings: {}", savings.no_savings_requests);
    println!("  fail-original requests: {}", savings.fail_original_requests);
    println!("  saving-off requests: {}", savings.disabled_requests);
    println!(
        "  native passthrough / compaction bypass: {} / {}",
        savings.native_passthrough_requests, savings.compaction_bypass_requests
    );
    println!(
        "  tool results evaluated / eligible / compacted: {} / {} / {}",
        savings.tool_results_evaluated,
        savings.tool_results_eligible,
        savings.tool_results_compacted
    );
    println!(
        "  measured bytes saved: {}",
        format_bytes(savings.bytes_saved)
    );
    println!(
        "  estimated tokens saved: ~{}",
        format_count(savings.estimated_tokens_saved)
    );
}

fn print_savings_stored(savings: StoredSavingsView) {
    println!("  observed requests: {}", savings.requests_observed);
    println!("  Responses requests: {}", savings.responses_requests);
    println!("  aged requests: {}", savings.aged_requests);
    println!("  no eligible result: {}", savings.no_eligible_requests);
    println!("  eligible but no savings: {}", savings.no_savings_requests);
    println!("  fail-original requests: {}", savings.fail_original_requests);
    println!("  saving-off requests: {}", savings.disabled_requests);
    println!(
        "  native passthrough / compaction bypass: {} / {}",
        savings.native_passthrough_requests, savings.compaction_bypass_requests
    );
    println!(
        "  tool results evaluated / eligible / compacted: {} / {} / {}",
        savings.tool_results_evaluated,
        savings.tool_results_eligible,
        savings.tool_results_compacted
    );
    println!(
        "  measured bytes saved: {}",
        format_bytes(savings.bytes_saved)
    );
    println!(
        "  estimated tokens saved: ~{}",
        format_count(savings.estimated_tokens_saved)
    );
}

fn print_help() {
    println!(
        "TokenSaver {VERSION}\n\n\
Usage:\n  tokensaver status\n  tokensaver connect\n  tokensaver disconnect\n  tokensaver saving <on|off>\n  tokensaver stats\n  tokensaver config show\n  tokensaver config set min-bytes <bytes>\n  tokensaver config set frontier <count>\n  tokensaver config set preview-code-units <count>\n  tokensaver doctor\n  tokensaver uninstall [--purge-state]\n  tokensaver version\n\n\
`connect`, `disconnect`, and `saving` control the running menu-bar runtime.\n\
`stats` and `config` can also read persisted owner-local state while the runtime is closed.\n\
`uninstall` explains the safe detach flow; `--purge-state` is destructive and only works after the menu-bar runtime has exited."
    );
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn format_count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}
