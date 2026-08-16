# Native Aging Validation — P0–P6

This document is the authoritative remediation/validation track for TokenSaver's native Codex tool-result aging path after reviewing the current `duolahypercho/codex-router` implementation beyond the originally pinned `v0.4.0-beta.4` reference.

The work remains deliberately narrower than Codex Router: TokenSaver does not adopt provider routing, model catalogs, third-party credentials, failover, vision bridges, or harness orchestration.

## P0 — Prove the native request shape

Status: **IMPLEMENTED — EXECUTION DEFERRED**

Ordinary `/v1/responses` requests are now inspected into content-free diagnostics. TokenSaver records only numeric/boolean shape evidence:

- whether `previous_response_id` is present
- whether that field was preserved by the optimizer
- input item count
- function/custom tool-result counts
- textual tool-result bytes seen
- largest textual tool result
- whether the aging pass ran
- deterministic skip-reason counts

No prompt text, result text, response ID value, call ID value, model name, credential, account ID, or capability secret is persisted in this diagnostic path.

Use:

```bash
tokensaver diagnostics
```

The key diagnostic for a “large Codex task but zero savings” case is now explicit: the command can distinguish no traffic, no historical tool results, results below threshold, protected-frontier results, unconsumed results, unsupported output, and eligible results whose receipts would not save bytes.

## P1 — Native chaining policy

Status: **IMPLEMENTED — EXECUTION DEFERRED**

TokenSaver intentionally does **not** copy Codex Router's native `previous_response_id` stripping behavior.

Codex Router can convert its routed native path into a stateless full-conversation request because it owns a provider-routing boundary. TokenSaver is different: it forwards the built-in Codex/OpenAI path to the same first-party upstream family. Removing native chaining without proof could change request semantics or make an incremental request incomplete.

Therefore the TokenSaver rule is:

1. detect whether `previous_response_id` is present without recording its value
2. preserve it byte/semantic-equivalently
3. permit aging to change only explicitly approved historical tool-result `output` fields
4. re-check the chaining-field presence after rewrite
5. fail original if the structural invariant cannot be proven
6. always bypass aging for `/v1/responses/compact`

If future live evidence proves a different native transport requirement, that becomes a separately reviewed compatibility change rather than an implicit copy from a provider router.

## P2 — Provider-reported token/cache telemetry

Status: **IMPLEMENTED — EXECUTION DEFERRED**

The response path now includes a bounded, read-only side-band usage collector.

It recognizes provider usage shapes including:

- `input_tokens` / `prompt_tokens`
- `output_tokens` / `completion_tokens`
- `input_tokens_details.cached_tokens`
- `prompt_tokens_details.cached_tokens`
- `prompt_cache_hit_tokens`

For SSE Responses streams, usage is read from response events without changing the relayed bytes. For non-streaming JSON, collection is bounded to 8 MiB. Oversized or malformed usage telemetry results in “usage unavailable”; it never fails or rewrites inference.

Authoritative separation remains:

- serialized bytes saved: directly measured by TokenSaver
- estimated tokens saved: `round(bytes_saved / 4)` and explicitly approximate
- provider input/cache/output tokens: provider-reported observation when available

## P3 — Explicit live A/B probes

Status: **IMPLEMENTED — NOT EXECUTED**

Two scripts were added. Neither runs implicitly or from ordinary CI. Both require `--yes` because they make live provider requests.

### Provider token reduction

```bash
python3 scripts/live-token-ab.py \
  --yes \
  --url 'http://127.0.0.1:<port>/<capability>/v1/responses' \
  --model '<native-model>' \
  --output validation/live-token-ab.json
```

The script:

- saves the current TokenSaver saving preference
- sends an identical deterministic tool-heavy request with saving OFF
- sends the same request with saving ON
- restores the original preference in `finally`
- requires provider-reported input tokens
- records the exact request-body SHA-256 and actual input-token delta

Credentials, when required, are accepted only through environment variables and are never printed.

### Omitted-middle quality

```bash
python3 scripts/live-aging-quality.py \
  --yes \
  --url 'http://127.0.0.1:<port>/<capability>/v1/responses' \
  --model '<native-model>' \
  --output validation/live-aging-quality.json
```

The OFF control must recover two random facts from the intact result. In the ON case, the middle fact is deliberately outside the receipt evidence. Passing behavior is either:

- request a safe tool replay for exact content, or
- explicitly acknowledge that exact omitted content is unavailable without inventing it

Inventing the omitted fact is a failure.

## P4 — Skip-reason observability

Status: **IMPLEMENTED — EXECUTION DEFERRED**

`tokensaver diagnostics` exposes aggregate skip reasons:

- protected frontier
- at/below threshold
- unconsumed
- unsupported output
- receipt not smaller

This makes “0 saved” an explainable state rather than an ambiguous one.

The menu-bar surface continues to show the compact high-level evaluated / eligible / compacted counts; the CLI is the detailed diagnostic surface so the tray remains small.

## P5 — Prompt-cache evidence

Status: **IMPLEMENTED — EXECUTION DEFERRED**

Provider usage is split into two comparable cache buckets:

- `aged_cache`: ordinary Responses turns that actually compacted at least one result
- `unaged_cache`: ordinary Responses turns that were not compacted

Compaction endpoint and unrelated native passthrough traffic are excluded from this A/B comparison.

Export persisted numeric evidence with:

```bash
python3 scripts/cache-evidence.py \
  --savings-file '<TokenSaver data dir>/savings.json' \
  --output validation/cache-evidence.json
```

The export contains only numeric aggregate telemetry.

## P6 — Conservative default and release gate

Status: **IMPLEMENTED — FINAL EXECUTION DEFERRED**

A fresh install now defaults `saving_enabled = false` because aging changes historical context. Existing persisted user choices are preserved; this release does not silently re-default an explicit preference.

The pure aging domain remains enabled when called directly by tests/fixtures. The OFF default is a product/runtime policy, not a weakening of the deterministic aging engine.

Release evidence gate:

```bash
python3 scripts/verify-aging-release.py \
  --token-ab validation/live-token-ab.json \
  --quality validation/live-aging-quality.json \
  --cache validation/cache-evidence.json \
  --tokensaver-bin tokensaver \
  --output validation/aging-release-gate.json
```

The gate requires:

- valid token A/B evidence schema
- provider-reported input-token reduction with aging ON
- passing omitted-middle quality probe
- same model used for token and quality A/B
- minimum aged/unaged cache sample sizes
- cache regression within the configured tolerance
- observed aged requests and provider-usage telemetry
- passing `tokensaver doctor`, unless explicitly skipped for an offline evidence-review environment

This evidence gate complements, rather than replaces, the repository's full Rust/format/clippy/architecture/packaging validation.

## Final execution order

Implementation was intentionally authored without running tests during P0–P6. The final execution sequence is:

1. run the full repository validation workflow once P6 implementation is complete
2. fix any compile/test/clippy/format/architecture failures and rerun until green
3. perform installed native Codex smoke validation
4. explicitly run the live token A/B and quality probes only with user-authorized provider traffic
5. collect sufficient aged/unaged cache telemetry
6. run the aging release evidence gate
7. only then make a release claim about native token reduction/quality/cache behavior

Until steps 3–6 have executed, the code path is implemented but live release evidence remains unproven.
