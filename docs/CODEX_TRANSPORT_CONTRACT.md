# Native Codex Transport Contract

## Purpose

TokenSaver transparently intercepts the built-in OpenAI provider used by Codex, applies safe historical tool-result aging, and forwards the request to the same first-party upstream family Codex would otherwise use.

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

The baseline is version-sensitive. Future Codex changes require compatibility review rather than speculative config rewrites.

## Core semantic invariant

For one logical Codex request:

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
- upstream model responses

## Configuration ownership

For the verified Codex baseline TokenSaver owns exactly one Codex key:

```toml
openai_base_url = "http://127.0.0.1:<port>/<capability>"
```

It must **not** replace or create `model_providers.openai`.

### Connect transaction

```text
resolve or recover endpoint
  ↓
bind loopback successfully
  ↓
write owner-only restoration snapshot
  ↓
atomically write openai_base_url
```

Snapshot records:

- schema version
- whether `openai_base_url` existed
- previous value when present
- exact TokenSaver endpoint installed

### Disconnect transaction

```text
load snapshot
  ↓
verify current value still equals TokenSaver-installed value
  ↓
restore/remove only openai_base_url
  ↓
atomically write config
  ↓
remove snapshot
```

A different current value is **drift**. TokenSaver refuses to overwrite it.

### Restart rule

If snapshot/config survive an unclean shutdown:

- load the snapshot before generating a new endpoint
- recover the exact old port and capability
- bind that same endpoint
- leave matching Codex config untouched
- if it cannot be safely recovered/rebound, report failure rather than silently rotating the endpoint

## Local caller security

The loopback endpoint uses a 256-bit random capability encoded as lowercase hex in the URL path:

```text
http://127.0.0.1:<port>/<capability>
```

Rules:

- bind only to loopback
- capability must match before upstream handling
- compare equal-length secrets without early byte mismatch exit
- reject browser-origin requests
- expose no permissive CORS surface
- accept only explicitly supported Responses paths
- use fixed first-party upstreams only
- never act as a general local forward proxy
- capability must not appear in routine logs/telemetry

## Native upstream preservation

Overriding `openai_base_url` hides the built-in provider's original target URL from the request itself. TokenSaver preserves Codex's first-party auth-mode distinction using the request headers Codex already emits:

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

## Authentication/header contract

TokenSaver relays Codex-provided credentials rather than replacing them.

The upstream request header set is allow-listed. It may include first-party Codex/OpenAI fields such as:

- `authorization`
- `chatgpt-account-id`
- `x-openai-fedramp`
- `openai-beta`
- `openai-organization`
- `openai-project`
- Codex request/session/attestation metadata required by native traffic

It excludes arbitrary browser, cookie, proxy, and unknown headers.

Transport observations never contain auth values.

## Supported paths

Ordinary Responses:

- `/responses`
- `/v1/responses`

Compaction bypass:

- `/responses/compact`
- `/v1/responses/compact`

Unsupported paths are not proxied.

## WebSocket fallback

The built-in OpenAI provider currently advertises WebSocket support. TokenSaver implements HTTP Responses relay only.

For a WebSocket Upgrade attempt on a protected Responses path TokenSaver returns:

`426 Upgrade Required`

The pinned Codex test suite explicitly recognizes this as immediate HTTP fallback.

## Compression contract

TokenSaver supports:

- identity
- zstd
- gzip
- x-gzip
- deflate
- Brotli

Zstandard is verified in the pinned Codex ChatGPT request-compression test. The other reversible formats are defensive compatibility inherited from the reference router behavior; they are not claimed as current Codex requirements.

Rules:

- decode declared chains in reverse application order
- bound decoded body size
- preserve original encoded bytes when no aging rewrite is required
- after a successful rewrite, encode with the same declared chain
- unsupported/malformed compression causes fail-original

## Responses aging adapter

Processing order:

```text
receive
  ↓
authenticate capability
  ↓
validate path / method / content type
  ↓
detect compaction bypass
  ↓
decode if aging inspection is needed
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

`responses/compact` is never aged before forwarding. The native compaction service must see the original tool-result history rather than TokenSaver receipts.

## Hard OFF

When saving is disabled:

- the loopback transport may remain connected
- aging is not run
- the original encoded body is forwarded directly

OFF is the semantic baseline for final ON/OFF comparison.

## Response relay

Upstream response status, allowed headers, and streaming body are relayed without rewriting model content or tool calls. Hop-by-hop framing headers are removed where required for the local HTTP relay.

Cancellation is expected to propagate through normal client/body-stream connection lifecycle and must be verified in the final live test pass.

## Observability

Transport emits content-free evidence only:

- disabled
- compaction bypass
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
- do not claim Connected
- preserve user state and report the compatibility issue

## Deferred validation suite

Test sources have been or will be authored for:

1. Codex-home resolution
2. root `openai_base_url` connect/disconnect
3. unrelated config preservation
4. pre-existing base URL restoration
5. drift refusal
6. crash/restart endpoint reuse
7. capability authentication
8. browser-origin rejection
9. header allow-listing
10. account-scoped vs API-key upstream selection
11. OFF byte preservation
12. compact bypass byte preservation
13. ON semantic diff limited to eligible output
14. mixed/image output preservation
15. gzip/x-gzip/deflate/Brotli/zstd adapter round-trip
16. unsupported compression fail-original
17. real installed Codex smoke test
18. real streamed tool-call turn
19. live cancellation behavior
20. authoritative auth/header behavior

Per project instruction, no test/build/lint/formatter/CI command is executed during implementation. Final validation runs only when the user explicitly requests it.
