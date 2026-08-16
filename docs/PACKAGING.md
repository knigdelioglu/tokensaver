# TokenSaver Packaging, Update, and Uninstall Contract

Phase 7 keeps packaging operationally separate from TokenSaver's context-optimization domain. A package/update/uninstall action must never weaken Codex configuration restoration or delete user state it cannot prove TokenSaver owns.

## macOS package target

The current desktop target is macOS 12+.

Base bundle configuration produces:

- `TokenSaver.app`
- a direct-distribution `.dmg`

`tauri.conf.json` keeps normal development/test configuration independent from generated release icons. `tauri.release.conf.json` is a release-only overlay that points the bundle at generated branded icon files.

The source of truth for the application icon is:

```text
assets/app-icon.svg
```

Generated PNG/ICNS files live in `icons/` and are intentionally gitignored.

## Packaging command

Use:

```bash
bash scripts/package-macos.sh
```

The script deliberately performs these release steps in order:

1. require macOS
2. require Cargo and a compatible Tauri 2 CLI
3. generate platform icons from `assets/app-icon.svg`
4. merge `tauri.release.conf.json`
5. build `.app` and `.dmg` bundles

The script does not run tests, linters, formatters, benchmarks, or application smoke tests. Those remain part of the explicit final validation pass.

## Signing and notarization

No Apple signing identity, certificate, notarization credential, private key, or updater signing key is committed to this repository.

Release signing/notarization must therefore be supplied by the release environment. A local unsigned/ad-hoc package must not be represented as a production-signed release.

TokenSaver does not hard-code an ad-hoc signing identity in the base configuration because that would blur the distinction between a local package and a distributable notarized build.

## Updater policy

`bundle.createUpdaterArtifacts` is intentionally `false`.

TokenSaver does **not** ship a self-updater in the current MVP. An in-app updater is allowed only after all of the following exist:

1. a trusted release endpoint
2. an updater public key embedded in the application
3. protected release signing material outside the repository
4. signed update artifacts
5. downgrade/version policy
6. failure/recovery tests proving Codex is not left pointing at a dead TokenSaver endpoint

Until then, updates use normal macOS application replacement.

## Safe manual update lifecycle

User preferences and numeric savings are stored outside the `.app` bundle in TokenSaver's per-user application-data directory. Replacing the application bundle must not delete them.

Safe update sequence:

```text
normal Quit TokenSaver
        ↓
safe request drain
        ↓
restore TokenSaver-owned Codex config
        ↓
flush content-free telemetry
        ↓
replace TokenSaver.app
        ↓
launch new TokenSaver.app
        ↓
reconnect if connect_on_launch was preserved
```

Normal Quit is intentionally different from uninstall preparation: it preserves `connect_on_launch` so a replacement build can resume the user's prior connection intent.

Never replace/delete a running TokenSaver application while Codex is still connected to its loopback endpoint.

## Prepare for Uninstall

The tray exposes:

```text
Prepare for Uninstall…
```

This operation is non-destructive with respect to preferences/statistics. It performs:

1. explicit Codex disconnect using the normal request-drain and config-restoration transaction
2. clears reconnect-on-launch intent
3. disables Start at Login through the registered Tauri autostart manager
4. flushes persisted numeric telemetry
5. exits only after the preceding steps succeed

If a request is active, Codex configuration drift exists, restoration fails, or autostart cannot be disabled, TokenSaver stays running/disconnected as appropriate and reports the error rather than claiming uninstall preparation succeeded.

## Optional state purge

After **Prepare for Uninstall…** has exited the menu-bar runtime, the user may run:

```bash
tokensaver uninstall --purge-state
```

Run it before deleting `TokenSaver.app` if the executable is being invoked from inside that bundle.

The purge operation has deliberately narrow ownership:

- `runtime-preferences.json`
- `savings.json`
- `control.sock`
- atomic temporary files matching those exact TokenSaver-owned filenames

It does **not** recursively delete the application-data directory.

An active `codex-config-snapshot.json` blocks the entire purge. The snapshot is never deleted by the generic purge path because it may be the only proof needed to restore Codex configuration.

Unknown files/directories are preserved and reported. The state directory is removed only if it is empty after known owned entries are removed.

The purge command never edits `~/.codex/config.toml`; Codex restoration belongs exclusively to the existing disconnect transaction.

## Full uninstall sequence

Recommended sequence:

```text
TokenSaver tray
  ↓
Prepare for Uninstall…
  ↓
TokenSaver exits safely
  ↓
optional: tokensaver uninstall --purge-state
  ↓
remove TokenSaver.app
```

If the user wants to keep savings/preferences for a later reinstall, skip `--purge-state` and remove only the application bundle.

## Upgrade compatibility

Persistent state is versioned independently from the application bundle.

Current versioned state includes:

- runtime preferences schema
- savings store schema
- Codex restoration snapshot schema

A new application version must migrate or reject unsupported state explicitly; it must not silently reinterpret an unknown schema.

## Deferred validation

Still deferred to the user's final executed validation pass:

- generated icon output
- `.app` bundle generation
- `.dmg` generation
- bundle metadata/icon inspection
- Apple code-signing/notarization with real release credentials
- normal Quit → replace app → relaunch round trip
- Prepare for Uninstall behavior
- Start at Login removal during uninstall preparation
- state purge ownership tests
- snapshot-blocks-purge test
- install/update/uninstall round trip

No packaging/build/test/lint/formatter/CI/notarization command has been executed while implementing this phase.
