# Desktop Runtime and Tray Contract

## Purpose

Phase 5 turns TokenSaver from a library/local transport into a visible macOS menu-bar application without widening the product into a general Codex UI.

The desktop shell is an operational surface only. It may show connection state, token-saving state, request activity, measured savings, estimated token savings, configuration health, and lifecycle controls.

It must not choose models, manage provider credentials, host MCP, or expose original tool-result bodies.

## Shell architecture

TokenSaver uses one Tauri 2 process with no webview window:

```text
macOS menu bar / Tauri shell
          ↓
application::desktop_runtime
          ↓
 ┌────────┼───────────┐
 ↓        ↓           ↓
runtime telemetry  Codex connection
                     ↓
                  transport
                     ↓
                   aging
```

The tray never reads module internals directly. `application::desktop_runtime` is the composition boundary and returns presentation-safe snapshots.

On macOS the app uses accessory activation policy so the menu-bar utility does not require a normal application window.

## Single process and single instance

Only one TokenSaver desktop instance may run for the configured application identifier.

A second launch must not create a second loopback server, rotate the caller capability, or race the Codex config snapshot. The Tauri single-instance plugin owns this process-level guard.

## First launch and desired connection state

`runtime-preferences.json` stores two user choices:

- `saving_enabled`
- `connect_on_launch`

Default first-launch state:

```text
TokenSaver       Active
Codex            Disconnected
Token Saving     Enabled
```

Selecting **Connect to Codex** sets `connect_on_launch = true` and starts the Phase 3 connection transaction.

Selecting **Disconnect from Codex** performs the safe Phase 3 restoration and then sets `connect_on_launch = false`.

A normal safe application Quit is different from explicit Disconnect:

1. temporary Codex configuration is restored
2. TokenSaver transport stops
3. numeric telemetry is flushed
4. `connect_on_launch` is preserved

Therefore a user who normally keeps TokenSaver connected can quit/reboot/login and have a later TokenSaver launch reconnect rather than leaving Codex permanently pointed at a dead localhost service.

If an unclean shutdown leaves the Phase 3 config snapshot behind, startup first follows crash-recovery rules and attempts to reuse the exact persisted endpoint.

## Start at Login

The tray exposes **Start at Login** through Tauri's autostart plugin using the macOS LaunchAgent mode.

Autostart is operating-system state; the tray reads the real plugin state instead of keeping a separate UI boolean.

Start-at-login and Codex connection intent are deliberately separate:

- Start at Login controls whether TokenSaver launches
- `connect_on_launch` controls whether a launched TokenSaver should connect Codex

## Tray truth model

The tray refreshes from an application snapshot. It does not infer backend state from which menu item the user last clicked.

Displayed state includes:

- service: Starting / Active / Error
- Codex: Disconnected / Connecting / Connected / Configuration Drift / Error
- request: Idle / Active (`n` concurrent native requests)
- health/error text
- Token Saving Enabled/Disabled
- Start at Login real state
- session savings
- current local-day savings
- all-time savings
- latest successful optimization

### Savings truthfulness

For every time scope the tray distinguishes:

- directly measured serialized bytes saved
- approximate token savings derived from the configured byte heuristic
- compacted tool-result count
- requests in which aging occurred

Estimated token values are always marked with `~` or `est.`. They are not presented as provider-billed token counts.

Example:

```text
This session: 720 KB saved · ~184K tokens · 12 results / 7 requests
Today: 2.8 MB saved · ~742K tokens · 41 results / 24 requests
All time: 10.1 MB saved · ~2.6M tokens · 143 results / 82 requests
Last optimization 16:12: 84 KB → 3 KB · 81 KB saved · ~20K tokens · 2 results
```

## Durable local state

TokenSaver's application-data directory contains only bounded operational state:

- `codex-config-snapshot.json` — temporary owner-only Phase 3 restore state while connected/crash-recoverable
- `runtime-preferences.json` — saving/connection intent
- `savings.json` — numeric content-free savings aggregates

`savings.json` contains no prompt text, tool-result text, receipt text, bearer token, account ID, or capability secret.

Daily numeric buckets are bounded to 120 retained local dates. All-time aggregates remain numeric counters.

## Request activity and safe disconnect

Request activity comes from the real loopback transport rather than a UI timer.

A request remains active until its relayed response stream is exhausted or dropped. Receiving upstream response headers is not enough to mark it complete.

Disconnect uses a request-drain gate:

```text
stop admitting new requests
        ↓
check in-flight requests
        ↓
0 requests ──► restore Codex config ──► stop transport
        │
        └── >0 ──► resume admission and refuse disconnect
```

The second gate check in the request handler closes the race between a request entering and shutdown beginning.

The tray disables Disconnect and Quit while it observes active requests. The backend still enforces the same rule so stale UI state cannot bypass it.

## Safe Quit

Every normal Tauri exit request passes through TokenSaver shutdown logic.

When connected and idle:

1. begin transport drain
2. restore TokenSaver-owned Codex configuration
3. stop the loopback server
4. allow the content-free observation queue a bounded drain interval
5. flush savings state
6. exit

If restoration fails, configuration has drifted, or a Codex request is still active, TokenSaver prevents the normal exit and surfaces the problem instead of knowingly leaving Codex configured to a dead TokenSaver endpoint.

## Configuration drift

Tray connection state is periodically re-proven from the Phase 3 snapshot/config relationship.

If a TokenSaver-owned Codex value changed unexpectedly:

- state becomes `Configuration Drift`
- automatic destructive restoration is refused
- Disconnect is disabled in the tray
- the app remains available for later diagnostics rather than guessing what the user intended

Phase 6 `doctor` will provide the richer recovery workflow.

## Secret redaction

The local caller capability is operationally sensitive. UI/diagnostic text must never display a managed loopback capability URL.

The desktop shell applies conservative loopback URL redaction before error text is shown. Raw capability values remain limited to owner-only restoration state and the active Codex config value required for local routing.

## Deferred validation

Implementation is authored but intentionally not executed during development.

Final validation must include:

- Rust compile/test/lint/format pass
- actual macOS menu-bar visibility and menu behavior
- single-instance behavior
- first Connect / explicit Disconnect round trip
- normal Quit / relaunch preserving `connect_on_launch`
- Start at Login LaunchAgent behavior
- crash snapshot restart recovery
- saving toggle persistence and live transport update
- real request Active → Idle lifecycle through streamed responses
- Disconnect/Quit refusal while a request is active
- telemetry persistence across restart and local-day rollover
- tray/backend state consistency
- capability redaction in every surfaced error

Per project instruction, none of those commands or live validation cases are run during implementation.
