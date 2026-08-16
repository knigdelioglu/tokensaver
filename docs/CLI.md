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
- limits request/response payloads to 64 KiB
- allows at most 16 simultaneous control clients
- applies a 5-second connect/read/write timeout

Excess clients are dropped instead of creating unbounded runtime tasks.

## Commands

```text
tokensaver status
tokensaver connect
tokensaver disconnect
tokensaver saving on
tokensaver saving off
tokensaver stats
tokensaver diagnostics
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

The runtime DTO also carries a content-free dropped-telemetry-observation count. That value is primarily interpreted by `doctor`: non-zero means inference continued but savings telemetry may be incomplete because its bounded queue saturated.

When the runtime is not running, the command reports that condition and may show persisted user intent such as token-saving and reconnect-on-launch preferences.

### `connect` / `disconnect`

These commands require the menu-bar runtime to be running. They never start a short-lived second proxy process.

`disconnect` uses the same request drain, config restoration, and drift safeguards as the tray action. An active streamed request can therefore cause a safe refusal rather than an abrupt transport shutdown.

### `saving on|off`

Requires the live runtime. The setting is persisted and the current transport's `enabled` policy flag is updated through the application controller.

A fresh TokenSaver installation defaults saving **off**. An existing persisted choice is preserved. Enabling aging is therefore an explicit operator action.

### `stats`

When the runtime is live, reports session, today, and all-time values. When the runtime is closed, it reads the persisted content-free aggregate store and reports today/all-time values.

Output distinguishes:

- directly measured bytes saved
- approximate tokens saved, always marked with `~`
- compacted tool-result count
- aged request count
- persisted provider usage when available

Provider usage is emitted after the response stream reaches a terminal state. If the provider did not expose recognized usage fields, the optimization event remains valid and its provider-usage portion is simply absent.

### `diagnostics`

Reads the persisted content-free aggregate store and reports **why** savings did or did not occur.

It includes:

- ordinary Responses count with / without `previous_response_id`
- count proving the chaining field was preserved
- count of requests where the aging pass actually ran
- input item count
- function/custom tool-result item counts
- textual tool-result bytes observed and largest result
- skip reasons: protected frontier, at/below threshold, unconsumed, unsupported, receipt not smaller
- provider input/cached/output token counts and usage-event count
- aged and ordinary-unaged cache rates with sample counts

It never prints the value of `previous_response_id`, prompt text, tool-result text, receipt text, model response text, credentials, account IDs, or capability secrets.

The detailed diagnostic surface intentionally lives in the CLI so the menu-bar menu can remain compact. The tray continues to show high-level evaluated / eligible / compacted and savings counters.

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

Schema v1 preference files remain readable; missing policy fields receive the original conservative structural defaults and the explicit saving choice is preserved.

Fresh-install product defaults:

```text
saving = off
connect_on_launch = false
min_bytes = 32768
frontier = 4
preview_code_units = 1024
```

The pure aging-domain default remains enabled when directly invoked; product opt-in and engine policy are deliberately separate concerns.

Guardrails:

- `min_bytes > 0`
- `frontier <= 256`
- `preview_code_units` between 64 and 16384

## `doctor`

`tokensaver doctor` produces redacted PASS/WARN/FAIL checks.

Current authored checks cover:

- Codex CLI discovery/version and release-validation identity
- TokenSaver data-directory permissions
- runtime-preference file permissions
- persistent savings file permissions
- restoration snapshot privacy and snapshot/config coherence
- Codex config path/readability
- menu-bar runtime control-channel reachability
- bounded telemetry queue drop state
- first-party ChatGPT Codex host reachability
- first-party OpenAI API host reachability

### Codex compatibility status

TokenSaver's implementation baseline is pinned to:

```text
openai/codex@9ded177ce7c1c0bd2047f902936c177612ab3434
```

The pinned Rust workspace reports version `0.0.0`, so doctor does not invent a semantic-version compatibility range.

Instead:

- an exact `codex --version` identity explicitly release-validated by TokenSaver may PASS
- an installed but not explicitly validated identity WARNs
- an unavailable/empty version identity WARNs

The source validation allow-list is intentionally empty until the final executed validation pass proves an exact Codex build. Unknown does not mean incompatible; it means **not yet proven compatible for a TokenSaver release**.

### Telemetry health

If the bounded content-free observation queue drops entries, `doctor` returns a warning containing only the drop count. Inference is not blocked, but session/day/all-time savings may undercount real optimization activity.

A first-party reachability PASS means an HTTP response was obtained; it does not claim that a specific authenticated Codex request succeeded.

The authoritative Start-at-Login state remains the Tauri autostart plugin state shown by the running tray. The CLI doctor does not infer LaunchAgent state from guessed plist names or undocumented plugin internals.

## Live validation helpers

Live token/quality probes are scripts rather than ordinary CLI commands because they can spend provider/account quota and require an explicit `--yes` acknowledgement:

```text
scripts/live-token-ab.py
scripts/live-aging-quality.py
scripts/cache-evidence.py
scripts/verify-aging-release.py
```

See `docs/NATIVE_AGING_VALIDATION.md` for their contracts and execution order.

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
- `previous_response_id` values
- arbitrary Codex configuration contents
- private expected/actual config values from drift errors

Redaction is also enforced at the `CodexConfigError` outward display layer, not only in CLI presentation.

The CLI receives only application DTOs and must not directly inspect module persistence or transport internals.

## Validation status

P0–P6 implementation and test/architecture sources were authored before execution by explicit project instruction. Compile, test, lint, format, Tauri build/run, CI, live doctor, live provider A/B, cache evidence, and release-gate claims remain unproven until the final requested validation phase executes them.
