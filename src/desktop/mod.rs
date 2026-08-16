use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use tauri::menu::{CheckMenuItem, Menu, MenuBuilder, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, RunEvent, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartManagerExt};

use crate::application::desktop_runtime::{
    DesktopCodexState, DesktopRuntimeController, DesktopRuntimeSnapshot, DesktopServiceState,
    LastOptimizationView, SavingsView,
};
use crate::shared::security::redact_local_secrets;

const MENU_SAVING: &str = "saving-toggle";
const MENU_CONNECT: &str = "connect-toggle";
const MENU_AUTOSTART: &str = "autostart-toggle";
const MENU_PREPARE_UNINSTALL: &str = "prepare-uninstall";
const MENU_QUIT: &str = "quit";

#[derive(Clone)]
struct TrayUi {
    _menu: Menu<Wry>,
    tray: TrayIcon<Wry>,
    status: MenuItem<Wry>,
    codex: MenuItem<Wry>,
    request: MenuItem<Wry>,
    traffic: MenuItem<Wry>,
    health: MenuItem<Wry>,
    optimizer: MenuItem<Wry>,
    routing: MenuItem<Wry>,
    tool_results: MenuItem<Wry>,
    session: MenuItem<Wry>,
    today: MenuItem<Wry>,
    all_time: MenuItem<Wry>,
    last_optimization: MenuItem<Wry>,
    saving: CheckMenuItem<Wry>,
    connect: MenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
    prepare_uninstall: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

impl TrayUi {
    fn build(app: &tauri::App<Wry>) -> tauri::Result<Self> {
        let status = MenuItem::with_id(app, "status", "Status: Starting", false, None::<&str>)?;
        let codex = MenuItem::with_id(app, "codex", "Codex config: Checking", false, None::<&str>)?;
        let request = MenuItem::with_id(app, "request", "Request: Idle", false, None::<&str>)?;
        let traffic = MenuItem::with_id(
            app,
            "traffic",
            "Traffic: Not seen this session",
            false,
            None::<&str>,
        )?;
        let health = MenuItem::with_id(app, "health", "Health: OK", false, None::<&str>)?;
        let optimizer = MenuItem::with_id(
            app,
            "optimizer-diagnostics",
            "Responses: 0 · aged 0 · no eligible 0 · no savings 0",
            false,
            None::<&str>,
        )?;
        let routing = MenuItem::with_id(
            app,
            "routing-diagnostics",
            "Other traffic: 0 passthrough · 0 compaction bypass · 0 fail-original · 0 saving-off",
            false,
            None::<&str>,
        )?;
        let tool_results = MenuItem::with_id(
            app,
            "tool-result-diagnostics",
            "Tool results: 0 evaluated · 0 eligible · 0 compacted",
            false,
            None::<&str>,
        )?;
        let session = MenuItem::with_id(
            app,
            "session-savings",
            "This session: 0 B saved · 0 est. tokens · 0 compacted / 0 observed",
            false,
            None::<&str>,
        )?;
        let today = MenuItem::with_id(
            app,
            "today-savings",
            "Today: 0 B saved · 0 est. tokens · 0 compacted / 0 observed",
            false,
            None::<&str>,
        )?;
        let all_time = MenuItem::with_id(
            app,
            "all-time-savings",
            "All time: 0 B saved · 0 est. tokens · 0 compacted / 0 observed",
            false,
            None::<&str>,
        )?;
        let last_optimization = MenuItem::with_id(
            app,
            "last-optimization",
            "Last optimization: —",
            false,
            None::<&str>,
        )?;
        let saving = CheckMenuItem::with_id(
            app,
            MENU_SAVING,
            "Token Saving Enabled",
            true,
            true,
            None::<&str>,
        )?;
        let connect = MenuItem::with_id(app, MENU_CONNECT, "Connect to Codex", true, None::<&str>)?;
        let autostart = CheckMenuItem::with_id(
            app,
            MENU_AUTOSTART,
            "Start at Login",
            true,
            false,
            None::<&str>,
        )?;
        let prepare_uninstall = MenuItem::with_id(
            app,
            MENU_PREPARE_UNINSTALL,
            "Prepare for Uninstall…",
            true,
            None::<&str>,
        )?;
        let quit = MenuItem::with_id(app, MENU_QUIT, "Quit TokenSaver", true, None::<&str>)?;

        let menu = MenuBuilder::new(app)
            .item(&status)
            .item(&codex)
            .item(&request)
            .item(&traffic)
            .item(&health)
            .separator()
            .item(&optimizer)
            .item(&routing)
            .item(&tool_results)
            .separator()
            .item(&session)
            .item(&today)
            .item(&all_time)
            .item(&last_optimization)
            .separator()
            .item(&saving)
            .item(&connect)
            .item(&autostart)
            .separator()
            .item(&prepare_uninstall)
            .item(&quit)
            .build()?;

        let tray = TrayIconBuilder::with_id("tokensaver-tray")
            .title("TS")
            .tooltip("TokenSaver")
            .menu(&menu)
            .show_menu_on_left_click(true)
            .build(app)?;

        Ok(Self {
            _menu: menu,
            tray,
            status,
            codex,
            request,
            traffic,
            health,
            optimizer,
            routing,
            tool_results,
            session,
            today,
            all_time,
            last_optimization,
            saving,
            connect,
            autostart,
            prepare_uninstall,
            quit,
        })
    }

    fn apply(
        &self,
        snapshot: &DesktopRuntimeSnapshot,
        autostart_enabled: bool,
        shell_error: Option<&str>,
    ) -> tauri::Result<()> {
        self.status
            .set_text(format!("Status: {}", service_text(snapshot.service)))?;
        self.codex
            .set_text(format!("Codex config: {}", codex_text(snapshot.codex)))?;
        self.request.set_text(if snapshot.active_requests == 0 {
            "Request: Idle".to_owned()
        } else {
            format!("Request: Active ({})", snapshot.active_requests)
        })?;
        self.traffic.set_text(format_traffic(snapshot.session))?;

        let health_error = snapshot.last_error.as_deref().or(shell_error);
        self.health.set_text(match health_error {
            Some(error) => format!("Health: {}", truncate_single_line(error, 92)),
            None if snapshot.dropped_telemetry_observations > 0 => format!(
                "Health: Telemetry incomplete — {} observations dropped",
                snapshot.dropped_telemetry_observations
            ),
            None => "Health: OK".to_owned(),
        })?;

        self.optimizer
            .set_text(format_optimizer_diagnostics(snapshot.session))?;
        self.routing
            .set_text(format_routing_diagnostics(snapshot.session))?;
        self.tool_results
            .set_text(format_tool_result_diagnostics(snapshot.session))?;
        self.session
            .set_text(format_savings("This session", snapshot.session))?;
        self.today
            .set_text(format_savings("Today", snapshot.today))?;
        self.all_time
            .set_text(format_savings("All time", snapshot.all_time))?;
        self.last_optimization.set_text(
            snapshot
                .last_optimization
                .map(format_last_optimization)
                .unwrap_or_else(|| "Last optimization: —".to_owned()),
        )?;

        self.saving.set_checked(snapshot.saving_enabled)?;
        self.autostart.set_checked(autostart_enabled)?;

        let (connect_text, connect_enabled) = match snapshot.codex {
            DesktopCodexState::Disconnected => ("Connect to Codex", true),
            DesktopCodexState::Connecting => ("Connecting to Codex…", false),
            DesktopCodexState::Connected if snapshot.active_requests > 0 => {
                ("Disconnect from Codex — Request Active", false)
            }
            DesktopCodexState::Connected => ("Disconnect from Codex", true),
            DesktopCodexState::Drifted => ("Configuration Drift — Fix Before Disconnect", false),
            DesktopCodexState::Error => ("Reconnect Codex", snapshot.active_requests == 0),
        };
        self.connect.set_text(connect_text)?;
        self.connect.set_enabled(connect_enabled)?;

        let uninstall_enabled = snapshot.active_requests == 0
            && !matches!(
                snapshot.codex,
                DesktopCodexState::Connecting | DesktopCodexState::Drifted
            );
        self.prepare_uninstall.set_enabled(uninstall_enabled)?;
        self.quit.set_enabled(snapshot.active_requests == 0)?;

        let title = match snapshot.codex {
            DesktopCodexState::Drifted | DesktopCodexState::Error => "TS !".to_owned(),
            _ if !snapshot.saving_enabled => "TS · Off".to_owned(),
            _ if snapshot.today.estimated_tokens_saved > 0 => format!(
                "TS · ~{}",
                format_compact_count(snapshot.today.estimated_tokens_saved)
            ),
            _ => "TS".to_owned(),
        };
        self.tray.set_title(Some(title))?;
        self.tray.set_tooltip(Some(format!(
            "TokenSaver — config {} — {} requests observed this session — {} saved today ({} measured)",
            codex_text(snapshot.codex),
            snapshot.session.requests_observed,
            format_estimated_tokens(snapshot.today.estimated_tokens_saved),
            format_bytes(snapshot.today.bytes_saved)
        )))?;
        Ok(())
    }
}

struct DesktopManagedState {
    controller: DesktopRuntimeController,
    tray: TrayUi,
    shell_error: Arc<RwLock<Option<String>>>,
    allow_exit: Arc<AtomicBool>,
    shutdown_in_progress: Arc<AtomicBool>,
}

pub(crate) fn run() -> Result<(), Box<dyn Error>> {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;

            let data_dir = app.path().app_data_dir()?;
            let controller = DesktopRuntimeController::open(data_dir)?;
            let tray = TrayUi::build(app)?;
            let shell_error = Arc::new(RwLock::new(None));
            let allow_exit = Arc::new(AtomicBool::new(false));
            let shutdown_in_progress = Arc::new(AtomicBool::new(false));

            register_menu_handler(
                &tray,
                controller.clone(),
                shell_error.clone(),
                allow_exit.clone(),
                shutdown_in_progress.clone(),
            );

            app.manage(DesktopManagedState {
                controller: controller.clone(),
                tray: tray.clone(),
                shell_error: shell_error.clone(),
                allow_exit,
                shutdown_in_progress,
            });

            let app_handle = app.handle().clone();
            let controller_for_init = controller.clone();
            let tray_for_init = tray.clone();
            let shell_error_for_init = shell_error.clone();
            tauri::async_runtime::spawn(async move {
                controller_for_init.initialize().await;
                refresh_tray(
                    &app_handle,
                    &controller_for_init,
                    &tray_for_init,
                    &shell_error_for_init,
                )
                .await;
            });

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                let mut ticks = 0u64;
                loop {
                    interval.tick().await;
                    refresh_tray(&app_handle, &controller, &tray, &shell_error).await;
                    ticks = ticks.wrapping_add(1);
                    if ticks.is_multiple_of(5)
                        && let Err(error) = controller.flush_persistent().await
                    {
                        set_shell_error(&shell_error, Some(error.to_string()));
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(|app_handle, event| {
        if let RunEvent::ExitRequested { api, .. } = event {
            let state = app_handle.state::<DesktopManagedState>();
            if state.allow_exit.load(Ordering::Acquire) {
                return;
            }

            api.prevent_exit();
            if state.shutdown_in_progress.swap(true, Ordering::AcqRel) {
                return;
            }

            let controller = state.controller.clone();
            let tray = state.tray.clone();
            let shell_error = state.shell_error.clone();
            let allow_exit = state.allow_exit.clone();
            let shutdown_in_progress = state.shutdown_in_progress.clone();
            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                match controller.safe_shutdown().await {
                    Ok(()) => {
                        allow_exit.store(true, Ordering::Release);
                        app_handle.exit(0);
                    }
                    Err(error) => {
                        set_shell_error(&shell_error, Some(error.to_string()));
                        shutdown_in_progress.store(false, Ordering::Release);
                        refresh_tray(&app_handle, &controller, &tray, &shell_error).await;
                    }
                }
            });
        }
    });

    #[allow(unreachable_code)]
    Ok(())
}

fn register_menu_handler(
    tray: &TrayUi,
    controller: DesktopRuntimeController,
    shell_error: Arc<RwLock<Option<String>>>,
    allow_exit: Arc<AtomicBool>,
    shutdown_in_progress: Arc<AtomicBool>,
) {
    let tray_ui = tray.clone();
    tray.tray.on_menu_event(move |app, event| {
        let event_id = event.id().as_ref().to_owned();
        if event_id == MENU_QUIT {
            app.exit(0);
            return;
        }

        if event_id == MENU_PREPARE_UNINSTALL {
            if shutdown_in_progress.swap(true, Ordering::AcqRel) {
                return;
            }

            let app = app.clone();
            let controller = controller.clone();
            let tray_ui = tray_ui.clone();
            let shell_error = shell_error.clone();
            let allow_exit = allow_exit.clone();
            let shutdown_in_progress = shutdown_in_progress.clone();
            tauri::async_runtime::spawn(async move {
                match prepare_for_uninstall(&app, &controller).await {
                    Ok(()) => {
                        allow_exit.store(true, Ordering::Release);
                        app.exit(0);
                    }
                    Err(error) => {
                        set_shell_error(&shell_error, Some(error));
                        shutdown_in_progress.store(false, Ordering::Release);
                        refresh_tray(&app, &controller, &tray_ui, &shell_error).await;
                    }
                }
            });
            return;
        }

        let app = app.clone();
        let controller = controller.clone();
        let tray_ui = tray_ui.clone();
        let shell_error = shell_error.clone();
        tauri::async_runtime::spawn(async move {
            let result = match event_id.as_str() {
                MENU_SAVING => {
                    let snapshot = controller.snapshot().await;
                    controller
                        .set_saving_enabled(!snapshot.saving_enabled)
                        .await
                        .map_err(|error| error.to_string())
                }
                MENU_CONNECT => {
                    let snapshot = controller.snapshot().await;
                    match snapshot.codex {
                        DesktopCodexState::Disconnected => controller
                            .connect()
                            .await
                            .map_err(|error| error.to_string()),
                        DesktopCodexState::Connecting => Ok(()),
                        DesktopCodexState::Connected => controller
                            .disconnect()
                            .await
                            .map_err(|error| error.to_string()),
                        DesktopCodexState::Drifted => Err(
                            "Codex configuration drift must be resolved before disconnect"
                                .to_owned(),
                        ),
                        DesktopCodexState::Error => match controller.disconnect().await {
                            Ok(()) => controller
                                .connect()
                                .await
                                .map_err(|error| error.to_string()),
                            Err(error) => Err(error.to_string()),
                        },
                    }
                }
                MENU_AUTOSTART => toggle_autostart(&app).map_err(|error| error.to_string()),
                _ => Ok(()),
            };

            match result {
                Ok(()) => set_shell_error(&shell_error, None),
                Err(error) => set_shell_error(&shell_error, Some(error)),
            }
            refresh_tray(&app, &controller, &tray_ui, &shell_error).await;
        });
    });
}

async fn prepare_for_uninstall(
    app: &AppHandle<Wry>,
    controller: &DesktopRuntimeController,
) -> Result<(), String> {
    // Explicit disconnect clears reconnect-on-launch and uses the same request
    // drain + Codex restoration transaction as the normal tray action.
    controller
        .disconnect()
        .await
        .map_err(|error| error.to_string())?;

    let autostart = app.autolaunch();
    if autostart.is_enabled().map_err(|error| error.to_string())? {
        autostart.disable().map_err(|error| error.to_string())?;
    }

    controller
        .flush_persistent()
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn refresh_tray(
    app: &AppHandle<Wry>,
    controller: &DesktopRuntimeController,
    tray: &TrayUi,
    shell_error: &Arc<RwLock<Option<String>>>,
) {
    let snapshot = controller.snapshot().await;
    let autostart_enabled = match app.autolaunch().is_enabled() {
        Ok(enabled) => enabled,
        Err(error) => {
            set_shell_error(
                shell_error,
                Some(format!("Start at Login status failed: {error}")),
            );
            false
        }
    };
    let shell_error_value = shell_error
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Err(error) = tray.apply(&snapshot, autostart_enabled, shell_error_value.as_deref()) {
        set_shell_error(shell_error, Some(format!("Tray update failed: {error}")));
    }
}

fn toggle_autostart(app: &AppHandle<Wry>) -> Result<(), tauri_plugin_autostart::Error> {
    let manager = app.autolaunch();
    if manager.is_enabled()? {
        manager.disable()
    } else {
        manager.enable()
    }
}

fn set_shell_error(target: &Arc<RwLock<Option<String>>>, value: Option<String>) {
    let mut slot = target
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = value.map(|message| redact_local_secrets(&message));
}

fn service_text(state: DesktopServiceState) -> &'static str {
    match state {
        DesktopServiceState::Starting => "Starting",
        DesktopServiceState::Running => "Active",
        DesktopServiceState::Error => "Error",
    }
}

fn codex_text(state: DesktopCodexState) -> &'static str {
    match state {
        DesktopCodexState::Disconnected => "Disconnected",
        DesktopCodexState::Connecting => "Connecting",
        DesktopCodexState::Connected => "Connected",
        DesktopCodexState::Drifted => "Configuration Drift",
        DesktopCodexState::Error => "Error",
    }
}

fn format_traffic(savings: SavingsView) -> String {
    if savings.requests_observed == 0 {
        "Traffic: Not seen this session".to_owned()
    } else {
        format!(
            "Traffic: Seen · {} requests this session",
            savings.requests_observed
        )
    }
}

fn format_optimizer_diagnostics(savings: SavingsView) -> String {
    format!(
        "Responses: {} · aged {} · no eligible {} · no savings {}",
        savings.responses_requests,
        savings.aged_requests,
        savings.no_eligible_requests,
        savings.no_savings_requests
    )
}

fn format_routing_diagnostics(savings: SavingsView) -> String {
    format!(
        "Other traffic: {} passthrough · {} compaction bypass · {} fail-original · {} saving-off",
        savings.native_passthrough_requests,
        savings.compaction_bypass_requests,
        savings.fail_original_requests,
        savings.disabled_requests
    )
}

fn format_tool_result_diagnostics(savings: SavingsView) -> String {
    format!(
        "Tool results: {} evaluated · {} eligible · {} compacted",
        savings.tool_results_evaluated,
        savings.tool_results_eligible,
        savings.tool_results_compacted
    )
}

fn format_savings(label: &str, savings: SavingsView) -> String {
    format!(
        "{label}: {} saved · {} · {} compacted / {} observed",
        format_bytes(savings.bytes_saved),
        format_estimated_tokens(savings.estimated_tokens_saved),
        savings.tool_results_compacted,
        savings.requests_observed
    )
}

fn format_last_optimization(last: LastOptimizationView) -> String {
    let local_time = i64::try_from(last.observed_at_epoch_ms)
        .ok()
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .map(|utc| utc.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_owned());
    format!(
        "Last optimization {local_time}: {} → {} · {} saved · {} · {} results",
        format_bytes(last.bytes_before),
        format_bytes(last.bytes_after),
        format_bytes(last.bytes_saved),
        format_estimated_tokens(last.estimated_tokens_saved),
        last.tool_results_compacted
    )
}

fn format_estimated_tokens(tokens: u64) -> String {
    if tokens == 0 {
        "0 est. tokens".to_owned()
    } else {
        format!("~{} tokens", format_compact_count(tokens))
    }
}

fn format_compact_count(value: u64) -> String {
    if value >= 1_000_000 {
        let whole = value / 1_000_000;
        let tenth = (value % 1_000_000) / 100_000;
        if tenth == 0 {
            format!("{whole}M")
        } else {
            format!("{whole}.{tenth}M")
        }
    } else if value >= 1_000 {
        let whole = value / 1_000;
        let tenth = (value % 1_000) / 100;
        if whole >= 100 || tenth == 0 {
            format!("{whole}K")
        } else {
            format!("{whole}.{tenth}K")
        }
    } else {
        value.to_string()
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    if bytes >= MIB {
        let whole = bytes / MIB;
        let tenth = (bytes % MIB) * 10 / MIB;
        if tenth == 0 {
            format!("{whole} MB")
        } else {
            format!("{whole}.{tenth} MB")
        }
    } else if bytes >= KIB {
        format!("{} KB", bytes / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn truncate_single_line(value: &str, max_chars: usize) -> String {
    let redacted = redact_local_secrets(value);
    let flattened = redacted.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() <= max_chars {
        return flattened;
    }
    let mut output = flattened
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

#[cfg(test)]
mod tests {
    use super::{
        SavingsView, format_optimizer_diagnostics, format_savings, format_tool_result_diagnostics,
        format_traffic,
    };

    fn observed_without_savings() -> SavingsView {
        SavingsView {
            requests_observed: 12,
            responses_requests: 10,
            compaction_bypass_requests: 0,
            native_passthrough_requests: 2,
            disabled_requests: 0,
            fail_original_requests: 0,
            no_eligible_requests: 10,
            no_savings_requests: 0,
            tool_results_evaluated: 42,
            tool_results_eligible: 0,
            bytes_saved: 0,
            estimated_tokens_saved: 0,
            tool_results_compacted: 0,
            aged_requests: 0,
        }
    }

    #[test]
    fn traffic_diagnostics_distinguish_zero_savings_from_zero_traffic() {
        let savings = observed_without_savings();
        assert_eq!(
            format_traffic(savings),
            "Traffic: Seen · 12 requests this session"
        );
        assert!(format_savings("This session", savings).contains("0 compacted / 12 observed"));
        assert!(format_optimizer_diagnostics(savings).contains("Responses: 10 · aged 0"));
        assert!(format_tool_result_diagnostics(savings).contains("42 evaluated · 0 eligible"));
    }

    #[test]
    fn zero_observed_requests_remain_explicitly_unproven() {
        assert_eq!(
            format_traffic(SavingsView::default()),
            "Traffic: Not seen this session"
        );
    }
}
