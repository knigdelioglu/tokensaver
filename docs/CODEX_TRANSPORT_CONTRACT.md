# Native Codex Transport Contract

## Purpose

TokenSaver transparently intercepts the built-in OpenAI provider used by Codex, applies safe historical tool-result aging only to ordinary Responses requests, and forwards native traffic to the same first-party upstream family Codex would otherwise use.

TokenSaver is not a provider/model router. It does not choose alternative models, translate third-party protocols, or own OpenAI credentials.

## Verified Codex baseline

Phase 3 was reviewed against OpenAI Codex commit:

`9ded177ce7c1c0bd2047f902936c177612ab3434`

Verified properties used by TokenSaver:

1. root `openai_base_url` overrides the built-in `openai` provider
2. the built-in provider retains Responses wire behavior, OpenAI auth, and WebSocket capability when this URL is overridden
3. `CODEX_HOME` is used when present; otherwise Codex home is `~/.codex`
4. Codex's own test suite treats HTTP `426 Upgrade Required` during Responses WebSocket connect as an immediate switch to HTTP Responses
5. native ChatGPT-backend request compression can use `Content-Encoding: zstd`
6. remote compaction uses `responses/compact`
7. first-party auth attaches `Authorization`; account-scoped ChatGPT/agent auth also attaches `ChatGPT-Account-ID`, while API-key auth has no account ID
8. the same provider base URL is used by native models, memory summarization, standalone search, and image clients
9. realtime/WebRTC request shape depends on provider/base-URL semantics, so realtime must not inherit TokenSaver's Responses loopback URL

The baseline is version-sensitive. Future Codex changes require compatibility review rather than speculative config rewrites.

## Core semantic invariant

For one logical ordinary Responses request:

```text
TokenSaver OFF
vs.
TokenSaver ON
```

the semantic payload may differ only in historical tool-result `output` fields explicitly approved by the aging policy.

TokenSaver must not rewrite:

- user/system/developer messages
- assistant messages
- reasoning items
- tool-call arguments
- model selection
- reasoning level
- MCP/skills/subagent settings
- unrelated request fields
- native models/search/images/memory payloads
- upstream model responses

## Configuration ownership

For the verified Codex baseline TokenSaver temporarily owns the built-in OpenAI provider base URL while connected:

```toml
openai_base_url = "http://127.0.0.1:<port>/<64-hex-capability>/v1"
```

It must **not** replace or create `model_providers.openai`.

Because current realtime/WebRTC behavior is base-URL-sensitive, TokenSaver also installs the following bypasses **only when the user has not already configured them**:

```toml
experimental_realtime_webrtc_call_base_url = "<native ChatGPT Codex URL>"
experimental_realtime_ws_base_url = "https://api.openai.com/v1"
```

Pre-existing user values for either realtime key are left unchanged and are not considered TokenSaver-owned.

### Connect transaction

```text
resolve or recover endpoint
  ↓
bind loopback successfully
  ↓
write owner-only restoration snapshot
  ↓
atomically install openai_base_url
  ↓
install only-missing realtime bypass values
```

Snapshot records:

- schema version
- whether `openai_base_url` existed
- previous `openai_base_url` value when present
- exact TokenSaver endpoint installed
- which realtime bypass keys TokenSaver itself installed and their exact values

### Disconnect transaction

```text
load snapshot
  ↓
verify every TokenSaver-owned current value still matches
  ↓
restore/remove openai_base_url
  ↓
remove only realtime keys TokenSaver installed
  ↓
atomically write config
  ↓
remove snapshot
```

A different current value for any TokenSaver-owned key is **drift**. TokenSaver refuses to overwrite it.

### Restart rule

If snapshot/config survive an unclean shutdown:

- load the snapshot before generating a new endpoint
- recover the exact old port and capability from the managed `/v1` URL
- bind that same endpoint
- leave matching Codex config untouched
- if it cannot be safely recovered/rebound, report failure rather than silently rotating the endpoint

## Local caller security

The loopback endpoint uses a 256-bit random capability encoded as lowercase hex in the URL path:

```text
http://127.0.0.1:<port>/<64-hex-capability>/v1
```

Rules:

- bind only to loopback
- capability must match before upstream handling
- compare equal-length secrets without early byte mismatch exit
- require the managed `/v1` suffix
- reject browser-origin requests
- expose no permissive CORS surface
- use a finite native route/method allow-list
- use fixed first-party upstreams only
- never act as a general local forward proxy
- capability must not appear in routine logs/telemetry

## Native upstream preservation

Overriding `openai_base_url` hides the built-in provider's original target URL from the request itself. TokenSaver preserves Codex's first-party auth-mode distinction using the request headers Codex already emits.

### Account-scoped request

If request headers contain:

- `ChatGPT-Account-ID`, or
- `X-OpenAI-Fedramp`

TokenSaver forwards to:

```text
https://chatgpt.com/backend-api/codex
```

### API-key-style request

When there is no account-scoped routing header, TokenSaver forwards to:

```text
https://api.openai.com/v1
```

TokenSaver does not inspect/decode bearer credentials to make this decision and does not store them.

The local configured base already ends in `/v1`. Before joining either first-party upstream, TokenSaver strips exactly that one local prefix from the request path. This yields, for example:

```text
local:   /v1/responses
ChatGPT: https://chatgpt.com/backend-api/codex/responses
API:     https://api.openai.com/v1/responses
```

## Authentication/header contract

TokenSaver relays Codex-provided credentials rather than replacing them.

The upstream request header set is allow-listed. It may include first-party Codex/OpenAI fields such as:

- `authorization`
- `chatgpt-account-id`
- `x-openai-fedramp`
- `if-none-match`
- `openai-beta`
- `openai-organization`
- `openai-project`
- Codex request/session/attestation metadata required by native traffic

`Content-Type` is preserved when present. Upstream response compression is requested as `identity` so the local relay does not need to reinterpret response bodies.

Arbitrary browser, cookie, proxy, and unknown headers are excluded. Transport observations never contain auth values.

## Finite native route contract

TokenSaver accepts only the native paths verified for the pinned Codex baseline:

| Local path | Method | Behavior |
|---|---:|---|
| `/v1/responses` | POST | aging-eligible Responses path |
| `/v1/responses/compact` | POST | exact aging bypass |
| `/v1/models` | GET | exact native passthrough |
| `/v1/memories/trace_summarize` | POST | exact native passthrough |
| `/v1/alpha/search` | POST | exact native passthrough |
| `/v1/images/generations` | POST | exact native passthrough |
| `/v1/images/edits` | POST | exact native passthrough |

Unknown paths are rejected with no upstream request. The allow-list is intentionally not a generic authenticated proxy surface.

Native passthrough request bodies never enter TokenSaver's aging parser.

## Realtime and WebSocket behavior

### Responses WebSocket fallback

The built-in OpenAI provider currently advertises WebSocket support. TokenSaver implements HTTP Responses relay only.

For a WebSocket Upgrade attempt on the protected `/v1/responses` path TokenSaver returns:

`426 Upgrade Required`

The pinned Codex test suite explicitly recognizes this as immediate HTTP fallback.

### Voice/realtime bypass

Realtime/WebRTC traffic is deliberately not proxied by TokenSaver in Phase 3. When the user has not configured explicit realtime URLs, TokenSaver temporarily points:

- WebRTC call creation to the native ChatGPT Codex URL derived from `chatgpt_base_url` or its current default
- realtime WebSocket transport to `https://api.openai.com/v1`

This keeps unrelated voice/realtime protocol behavior out of the token-saving transport and avoids changing request shape merely because `openai_base_url` points at loopback.

## Compression contract

TokenSaver's aging adapter supports:

- identity
- zstd
- gzip
- x-gzip
- deflate
- Brotli

Zstandard is verified in the pinned Codex ChatGPT request-compression test. The other reversible formats are defensive compatibility inherited from the reference router behavior; they are not claimed as current Codex requirements.

Rules for ordinary Responses aging:

- decode declared chains in reverse application order
- bound decoded body size
- preserve original encoded bytes when no aging rewrite is required
- after a successful rewrite, encode with the same declared chain
- unsupported/malformed compression causes fail-original

Rules for compact/native passthrough:

- do not decode merely for TokenSaver
- forward original request bytes and declared encoding unchanged

## Responses aging adapter

Processing order:

```text
receive
  ↓
authenticate capability
  ↓
validate finite path / method / content type
  ↓
classify responses / compact / native passthrough
  ↓
responses only: decode if aging inspection is enabled
  ↓
parse Responses JSON
  ↓
normalize only recognized history shapes
  ↓
run pure aging domain
  ↓
validate index + result kind + call_id
  ↓
replace only approved output fields
  ↓
serialize / re-encode
  ↓
forward to preserved first-party upstream
```

Recognized history shapes are deliberately narrow. Unknown or mixed-media forms are not guessed into eligibility.

The rewritten JSON representation preserves object insertion order. If decode, parse, normalization, replacement validation, serialization, or re-encoding cannot complete safely, the entire optimization fails original and uses the original encoded request bytes.

## Conversation compaction

`/v1/responses/compact` is never aged before forwarding. The native compaction service must see the original tool-result history rather than TokenSaver receipts.

The aging adapter also recognizes `/responses/compact` as a defensive direct-call bypass, although the managed capability base currently exposes the `/v1` path form.

## Hard OFF

When saving is disabled:

- the loopback transport may remain connected
- ordinary Responses aging is not run
- the original encoded Responses body is forwarded directly
- compact remains classified as compaction bypass
- models/search/images/memory remain classified as native passthrough

OFF is the semantic baseline for final ON/OFF comparison.

## Response relay

Upstream response status, allowed headers, and streaming body are relayed without rewriting model content or tool calls. Hop-by-hop framing headers are removed where required for the local HTTP relay.

Dropping the downstream streaming body drops the upstream `reqwest` stream; end-to-end cancellation behavior still requires the final live validation pass.

## Observability

Transport emits content-free evidence only:

- disabled Responses
- explicit compaction bypass
- native passthrough
- fail-original
- evaluated/no eligible result
- evaluated/no savings
- aged
- numeric aging statistics

No original tool-result body, receipt body, bearer credential, account ID value, or capability secret is emitted as telemetry.

## Compatibility rule

On an unknown/unsupported Codex configuration:

- do not guess new config keys
- do not overwrite unknown config structures
- do not expose unknown native paths
- do not claim Connected
- preserve user state and report the compatibility issue

## Deferred validation suite

Test sources have been authored for:

1. Codex-home resolution
2. managed `/<64-hex>/v1` URL validation
3. root `openai_base_url` connect/disconnect
4. unrelated config preservation
5. pre-existing base URL restoration
6. managed realtime bypass installation/removal
7. pre-existing realtime override preservation
8. OpenAI/realtime drift refusal
9. crash/restart endpoint reuse
10. capability authentication
11. browser-origin rejection
12. header allow-listing
13. account-scoped vs API-key upstream selection helper behavior
14. local `/v1` upstream-path normalization
15. finite native route/method allow-list
16. native passthrough classification
17. OFF Responses byte preservation
18. compact bypass byte preservation
19. ON semantic diff limited to eligible output
20. mixed/image output preservation
21. gzip/x-gzip/deflate/Brotli/zstd adapter round-trip
22. unsupported compression fail-original
23. native passthrough telemetry aggregation

Still deferred for executed/live validation:

24. compile/test/lint/format pass
25. real installed Codex smoke test
26. real streamed tool-call turn
27. live cancellation behavior
28. authoritative ChatGPT/API-key auth-header behavior
29. native models/search/images/memory passthrough smoke cases
30. full captured ON/OFF semantic comparison

Per project instruction, no test/build/lint/formatter/CI command is executed during implementation. Final validation runs only when the user explicitly requests it.
