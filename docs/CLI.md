# TokenSaver CLI and diagnostics

## Purpose

The CLI is a secondary operational surface for the same TokenSaver runtime used by the macOS menu-bar application. It is not a second daemon, a second proxy, or an alternate configuration owner.

Running the TokenSaver binary with no CLI command starts the menu-bar application. Running it with a recognized command uses CLI mode without constructing the Tauri UI.

## Runtime ownership

Commands that mutate live connection state use an owner-local Unix control socket:

```text
TokenSaver CLI
      ↓
application runtime client
      ↓
owner-only control.sock
      ↓
single live menu-bar runtime
      ↓
application controller
```

The socket:

- lives in TokenSaver's per-user application-data directory
- is Unix owner-only (`0600`)
- sits inside an owner-only parent directory (`0700`)
- accepts only the finite TokenSaver control protocol
- never accepts arbitrary shell commands
- never carries tool-result bodies, receipts, bearer credentials, account IDs, or the Codex transport capability
- is treated as stale and replaced on launch only when no live runtime answers it

## Commands

```text
tokensaver status
tokensaver connect
tokensaver disconnect
tokensaver saving on
tokensaver saving off
tokensaver stats
tokensaver config show
tokensaver config set min-bytes <bytes>
tokensaver config set frontier <count>
tokensaver config set preview-code-units <count>
tokensaver doctor
tokensaver uninstall [--purge-state]
tokensaver version
```

### `status`

Reads the live menu-bar runtime through the control channel and reports:

- runtime state
- Codex connection state
- token-saving state
- active request count
- redacted health state

When the runtime is not running, the command reports that condition and may show persisted user intent such as token-saving and reconnect-on-launch preferences.

### `connect` / `disconnect`

These commands require the menu-bar runtime to be running. They never start a short-lived second proxy process.

`disconnect` uses the same request drain, config restoration, and drift safeguards as the tray action. An active streamed request can therefore cause a safe refusal rather than an abrupt transport shutdown.

### `saving on|off`

Requires the live runtime. The setting is persisted and the current transport's `enabled` policy flag is updated through the application controller.

### `stats`

When the runtime is live, reports session, today, and all-time values. When the runtime is closed, it reads the persisted content-free aggregate store and reports today/all-time values.

Output distinguishes:

- directly measured bytes saved
- approximate tokens saved, always marked with `~`
- compacted tool-result count
- aged request count

### `config show`

Reports the effective persisted optimization policy:

- `saving`
- `connect_on_launch`
- `min_bytes`
- `frontier`
- `preview_code_units`

### `config set`

Supported numeric keys:

```text
min-bytes
frontier
preview-code-units
```

When the runtime is closed, changes are written to owner-private preferences and apply on the next Codex connection.

When the runtime is running but Codex is connected, structural aging-policy changes are refused. Disconnect first, change the policy, then reconnect. This avoids a request being evaluated under a policy that changes midway through the connected session.

The simple saving on/off flag remains live-switchable.

### `uninstall`

Without arguments, `tokensaver uninstall` prints the safe uninstall sequence.

The menu-bar action **Prepare for Uninstall…** is the authoritative detach step because it can use the real Tauri autostart manager. It:

1. safely disconnects Codex through the normal request-drain/restoration transaction
2. clears reconnect-on-launch intent
3. disables Start at Login
4. flushes persisted numeric telemetry
5. exits only after those steps succeed

After the menu-bar runtime has exited, optional destructive cleanup is available through:

```text
tokensaver uninstall --purge-state
```

The purge command refuses to run while the live control runtime is reachable. It also refuses all cleanup when `codex-config-snapshot.json` exists, because deleting restoration state could strand Codex on a dead TokenSaver endpoint.

Only known TokenSaver-owned state is removed. Unknown files/directories are preserved and reported; cleanup is non-recursive. The purge command never edits Codex configuration.

See `docs/PACKAGING.md` for the full install/update/uninstall contract.

## Persistent policy

Runtime preferences schema v2 persists:

- saving enabled
- reconnect-on-launch intent
- minimum eligible bytes
- protected result frontier
- preview code-unit count

Schema v1 preference files remain readable; missing policy fields receive the original conservative defaults and are upgraded on a later write.

Default policy remains:

```text
min_bytes = 32768
frontier = 4
preview_code_units = 1024
```

Guardrails:

- `min_bytes > 0`
- `frontier <= 256`
- `preview_code_units` between 64 and 16384

## `doctor`

`tokensaver doctor` produces redacted PASS/WARN/FAIL checks.

Current authored checks cover:

- Codex CLI discovery/version when available
- TokenSaver data-directory permissions
- runtime-preference file permissions
- persistent savings file permissions
- restoration snapshot privacy and snapshot/config coherence
- Codex config path/readability
- menu-bar runtime control-channel reachability
- first-party ChatGPT Codex host reachability
- first-party OpenAI API host reachability

A first-party reachability PASS means an HTTP response was obtained; it does not claim that a specific authenticated Codex request succeeded.

The authoritative Start-at-Login state remains the Tauri autostart plugin state shown by the running tray. The CLI doctor does not infer LaunchAgent state from guessed plist names or undocumented plugin internals.

## Exit codes

- `0` — requested operation/report completed without a failing doctor check
- `1` — operational state is unsuccessful, a runtime action was safely refused, or doctor found a FAIL check
- `2` — CLI syntax/setup error raised by the top-level binary entrypoint

Warnings in `doctor` do not by themselves make the doctor exit non-zero.

## Privacy rules

CLI and doctor output must never print:

- bearer/API credentials
- `ChatGPT-Account-ID`
- TokenSaver's 256-bit Codex transport capability
- original tool-result bodies
- compact receipt bodies
- arbitrary Codex configuration contents

The CLI receives only application DTOs and must not directly inspect module persistence or transport internals.

## Validation status

Implementation and test/architecture sources may be authored during development, but project instruction defers execution. No compile, test, lint, format, Tauri run/build, CI, live doctor, or CLI smoke command is considered passed until the final user-requested validation phase.
