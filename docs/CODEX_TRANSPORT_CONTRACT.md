# Native Codex Transport Contract

## Purpose

This document defines the integration boundary TokenSaver must satisfy before it may transparently optimize real Codex traffic.

The contract is intentionally narrower than Codex Router. TokenSaver does not select or translate models. It intercepts the user's existing native Codex request path only to apply safe tool-result aging and then forwards the request to the same intended native upstream.

The original token-aging behavior was derived from `duolahypercho/codex-router` around `v0.4.0-beta.4`. Version-sensitive Codex transport/configuration behavior is verified against OpenAI Codex itself before implementation.

## Phase 3 verified Codex baseline

Phase 3 implementation was reviewed against OpenAI Codex commit:

`9ded177ce7c1c0bd2047f902936c177612ab3434`

Verified properties used by TokenSaver:

1. `ConfigToml` exposes a root `openai_base_url` specifically as the base URL override for the built-in `openai` model provider.
2. The built-in OpenAI provider receives that override while retaining its normal Responses wire API, OpenAI-auth requirement, WebSocket support, and native provider identity.
3. TokenSaver therefore owns only root `openai_base_url`; it does **not** create or replace `model_providers.openai`.
4. Codex resolves its home from a non-empty `CODEX_HOME` when provided and otherwise uses `~/.codex`; TokenSaver follows the same rule for `config.toml`.
5. Codex's own WebSocket fallback test verifies that HTTP `426 Upgrade Required` on the Responses WebSocket connection immediately switches the session to HTTP Responses transport.
6. Codex's own ChatGPT-backend compression test verifies current request bodies may use `Content-Encoding: zstd`.
7. The native compact endpoint path is `responses/compact`; TokenSaver bypasses aging for both `/responses/compact` and `/v1/responses/compact` forms produced by base-URL composition.

TokenSaver additionally accepts gzip/x-gzip, deflate, and Brotli request encodings defensively because these are safely reversible transport encodings and have been used by the reference router. Their presence is not claimed as a requirement of the pinned Codex commit.

This baseline is not a promise about every future Codex release. Later releases must be compatibility-checked before TokenSaver silently changes config or transport assumptions.

## Non-negotiable semantic invariant

For one logical Codex request, compare:

- TokenSaver connected with aging disabled
- TokenSaver connected with aging enabled

The semantic payloads may differ only where the aging policy explicitly permits an eligible historical tool-result body to be replaced with a deterministic receipt.

TokenSaver must not silently rewrite:

- user messages
- system/developer instructions
- assistant messages
- reasoning items
- tool-call arguments
- model selection
- reasoning level
- MCP configuration
- skills
- subagent configuration
- unrelated request parameters

## Connection model

Target flow:

```text
Codex
  │
  │ native Responses traffic
  ▼
TokenSaver loopback endpoint
  │
  ├─ authenticate capability path
  ├─ inspect/decode request
  ├─ bypass explicit conversation compaction
  ├─ optionally age eligible historical tool results
  └─ preserve unrelated semantics
  │
  ▼
Same native Codex/OpenAI upstream
  │
  ▼
TokenSaver relays stream
  │
  ▼
Codex
```

TokenSaver preserves the normal Codex account and model-selection experience.

## Codex configuration ownership

For the verified baseline, TokenSaver may own exactly one Codex configuration key:

```toml
openai_base_url = "http://127.0.0.1:<port>/<capability>"
```

It must not create a substitute OpenAI provider table.

Before changing the Codex config, TokenSaver must durably snapshot:

- whether `openai_base_url` previously existed
- its exact previous string value when present
- the exact TokenSaver endpoint installed

Connect ordering is:

```text
resolve/recover endpoint
  ↓
bind loopback listener successfully
  ↓
write owner-only restoration snapshot
  ↓
atomically update Codex config
```

Disconnect ordering is:

```text
load snapshot
  ↓
verify current openai_base_url is still TokenSaver-owned value
  ↓
restore/remove only that key
  ↓
atomically write Codex config
  ↓
remove snapshot
```

If the current value differs from the value TokenSaver installed, that is configuration drift. TokenSaver must not overwrite it.

Connect must not change:

- selected model
- model reasoning settings
- MCP configuration
- skills
- project trust
- permissions
- subagent settings
- unrelated provider entries
- unrelated root configuration

## Restart ownership

The capability URL is part of TokenSaver-owned local state. After an unclean shutdown, the persisted snapshot may still point Codex at the previous loopback endpoint.

On restart TokenSaver must:

1. load the existing snapshot before creating a fresh endpoint
2. recover the exact prior loopback port and capability from the installed URL
3. bind that exact endpoint successfully
4. leave Codex config unchanged when it already matches

If that endpoint cannot be safely recovered or rebound, TokenSaver must report an error rather than silently rotate the capability while Codex still points to the old endpoint.

## Request paths

### Ordinary Responses traffic

Supported native inference paths:

- `/responses`
- `/v1/responses`

Expected processing order:

```text
receive request
  ↓
validate loopback caller capability
  ↓
reject browser-origin traffic
  ↓
decode body compression when optimization needs inspection
  ↓
parse only what is required
  ↓
normalize Responses history
  ↓
run aging policy
  ↓
validate index + tool-result kind + call_id before each replacement
  ↓
replace only eligible output fields
  ↓
serialize/recompress as required
  ↓
forward to fixed native upstream
```

When no rewrite is required, TokenSaver should preserve the original encoded request body rather than decode/re-serialize it merely for transport convenience.

### Conversation compaction

Supported bypass paths:

- `/responses/compact`
- `/v1/responses/compact`

The compaction summarizer must receive the original history. TokenSaver does not age tool-result bodies on this path.

Tool-result aging and conversation compaction are complementary; TokenSaver must not optimize one by degrading the other.

## Authentication contract

TokenSaver does not require a separate OpenAI API key merely to optimize the user's existing native Codex request path.

Rules:

- relay authentication Codex already supplies
- use an explicit upstream-header allow-list
- do not forward arbitrary browser, cookie, proxy, or local headers
- never log tokens, account identifiers, capability secrets, or equivalent credentials
- never expose them in tray/CLI/status output
- never replace a credential explicitly supplied by the caller with a different credential

The allow-list is version-sensitive and remains subject to compatibility review.

## Loopback security

The local service must:

- bind to loopback only
- avoid public LAN/WAN exposure
- require a cryptographically random capability carried in the configured URL path
- compare the capability before considering any upstream request
- reject browser-origin traffic
- expose no permissive CORS surface
- proxy only explicitly supported Responses paths to a fixed upstream
- redact the capability from diagnostics/user-visible output unless a dedicated recovery workflow requires it

The service is not a generic local forward proxy.

## WebSocket fallback

The built-in OpenAI provider currently advertises WebSocket support. TokenSaver Phase 3 intentionally implements HTTP Responses relay only.

When Codex attempts a WebSocket upgrade on the protected Responses path, TokenSaver returns:

`426 Upgrade Required`

The pinned Codex test suite explicitly treats this status as a signal to switch immediately to the HTTP Responses path. TokenSaver does not emulate a partial WebSocket session.

## Request-body compression

Current native ChatGPT traffic may use Zstandard compression. TokenSaver supports:

- `zstd`
- `gzip`
- `x-gzip`
- `deflate`
- `br`
- identity/no encoding

Rules:

- multiple declared encodings are decoded in reverse application order
- decoded body size is bounded
- unsupported or malformed encodings never produce a speculative rewritten body
- if aging does not change the request, the original encoded bytes are retained
- if aging changes the request, the rewritten body is encoded using the same declared encoding chain

Compression failure during optimization causes fail-original behavior rather than partial context rewriting.

## Request-body transformation rule

Aging operates only after the request body is safely decoded.

The transformation preserves:

- history item ordering
- call/result pairing
- all non-aged item fields
- all non-eligible result bodies
- request fields unrelated to aging

The JSON implementation uses insertion-order-preserving maps so rewriting an eligible result does not deliberately reorder unrelated object fields.

Before applying each domain decision, the transport validates the original protocol item using:

- history index
- tool-result family (`function_call_output` or `custom_tool_call_output`)
- `call_id`

If any validation, decode, parse, serialization, or re-encoding step is uncertain, the entire transformation fails original and forwards the original encoded body.

## Response contract

TokenSaver is not a response enhancer.

Responses from the native upstream are streamed back without semantic transformation.

The relay may perform only transport mechanics such as:

- status/header relay
- hop-by-hop header filtering
- streaming body relay
- connection/cancellation lifecycle

TokenSaver must not rewrite model text, tool calls, or completion content.

## Hard OFF mode

When aging is disabled, the loopback transport may remain connected, but it performs no aging rewrite.

The original encoded body is used directly. OFF mode is the semantic baseline for later validation.

Intended final validation:

```text
request through TokenSaver OFF
vs.
request through TokenSaver ON
```

After normalizing unavoidable transport framing, the only semantic JSON differences in the ON version may be explicitly eligible aged tool-result bodies.

## Observability contract

Transport emits only content-free optimization evidence:

- optimizer outcome
- result counts
- largest evaluated result size
- bytes before/after/saved

Outcome distinguishes at least:

- disabled
- compaction bypass
- evaluated/no eligible result
- evaluated/no savings
- aged
- fail-original

Transport observations must never contain original tool-result bodies or compact receipts. Application code maps these observations into telemetry.

## Compatibility rule

TokenSaver must maintain an explicit supported Codex version/configuration baseline once real integration begins.

For an unsupported or unknown Codex configuration:

- do not guess config keys
- do not rewrite unknown config structures
- do not claim Connected
- report the compatibility problem
- preserve the user's existing configuration

## Phase 3 deferred validation suite

The following test sources are authored or required, but execution is intentionally deferred until the user requests the final validation pass:

1. root `openai_base_url` connect/disconnect round-trip
2. unrelated Codex config remains intact
3. restoration of a pre-existing `openai_base_url`
4. config drift refuses overwrite
5. capability path rejects wrong callers
6. browser-origin requests are rejected
7. native upstream header allow-list excludes arbitrary headers
8. aging OFF preserves original encoded request body
9. conversation compaction preserves original encoded body
10. aging ON changes only eligible historical tool-result output fields
11. mixed/image output remains unchanged
12. gzip/x-gzip/deflate/Brotli/Zstandard adapter round-trip
13. unsupported compression fails original
14. restart reuses the persisted port and capability
15. final real-Codex smoke test and stream/cancellation behavior

No Phase 3 test/build/lint/formatter/CI command is to be executed during implementation unless the user explicitly requests the final validation pass.
