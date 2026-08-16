# Native Codex Transport Contract

## Purpose

This document defines the integration boundary TokenSaver must satisfy before it may transparently optimize real Codex traffic.

The contract is intentionally narrower than Codex Router. TokenSaver does not select or translate models. It intercepts the user's existing native Codex request path only to apply safe tool-result aging and then forwards the request to the same intended native upstream.

The concrete transport behavior observed in `duolahypercho/codex-router` around `v0.4.0-beta.4` is a reference, not a permanent assumption about every future Codex release. Phase 3 must verify the current supported Codex build before relying on any version-sensitive detail.

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

TokenSaver should preserve the normal Codex account and model-selection experience.

## Codex configuration ownership

TokenSaver may change only the minimum configuration required to route the native Codex request path through its loopback endpoint.

Before the first change it must snapshot every value it owns.

Connect must not change:

- selected model
- model reasoning settings
- MCP configuration
- skills
- project trust
- permissions
- subagent settings
- unrelated provider entries

Disconnect/uninstall must restore the exact TokenSaver-owned previous values unless drift proves the user or another tool changed them afterwards. In a drift case, TokenSaver must fail safely and report the conflict instead of overwriting newer user state.

## Request paths

### Ordinary Responses traffic

Ordinary supported native Responses requests are eligible for inspection and aging.

Expected processing order:

```text
receive request
  ↓
validate local caller/transport
  ↓
decode body compression
  ↓
parse only what is required
  ↓
detect explicit conversation-compaction request
  ↓
if ordinary request: run aging policy
  ↓
serialize/recompress as required
  ↓
forward to native upstream
```

### Conversation compaction

`/responses/compact` is an explicit bypass in the initial reference behavior.

A compaction summarizer must see the original history rather than aging receipts. Any future equivalent compaction trigger must be added only after it is verified against the supported Codex version.

Tool-result aging and conversation compaction are complementary; TokenSaver must not optimize one by degrading the other.

## Authentication contract

TokenSaver must not require a separate OpenAI API key merely to optimize the user's native Codex session.

Rules:

- use the authentication already supplied by Codex on the native request path
- forward only an explicit allow-list of headers required by the native upstream
- do not forward arbitrary local/browser headers
- never log tokens, account identifiers, capability secrets, or equivalent credentials
- never expose them in tray/CLI/status output
- never replace a credential explicitly supplied by the caller with another credential

The allow-list is version-sensitive and must be verified during Phase 3.

## Loopback security

The local service must:

- bind to loopback only by default
- avoid public LAN/WAN exposure
- reject browser-origin traffic where appropriate
- not grant permissive CORS access
- use a local caller capability/authentication mechanism if the final integration requires one
- redact local capability values from diagnostics

No local endpoint should become a generic unauthenticated proxy to the native upstream.

## Transport compatibility

The reference Codex Router implementation shows that supported Codex builds may use:

- an initial Responses WebSocket attempt followed by HTTP fallback
- gzip
- deflate
- Brotli
- Zstandard-compressed request bodies
- streamed Responses events
- cancellation/abort behavior

TokenSaver Phase 3 must test which of these are required by the current supported Codex build.

If the current build still requires the same WebSocket fallback behavior, TokenSaver may deliberately reject/upgrade the unsupported WebSocket path in the same compatible manner and serve the HTTP fallback. This must be verified live rather than copied blindly.

## Request-body transformation rule

Aging operates only after the request body is safely decoded.

The transformation must preserve:

- item ordering
- call/result pairing
- all non-aged item fields
- all non-eligible result bodies
- request fields unrelated to aging

If parsing or validation is ambiguous, preserve/pass through the original request rather than constructing a partial substitute.

## Response contract

TokenSaver is not a response enhancer.

Responses from the native upstream must be relayed without semantic transformation.

The transport may perform only mechanics required for correct relay, such as:

- streaming bytes/events
- required header handling
- cancellation propagation
- connection lifecycle

TokenSaver must not rewrite model text, tool calls, or completion content.

## Hard OFF mode

When aging is disabled, the loopback transport may remain connected, but it must perform no aging rewrite.

OFF mode is a first-class diagnostic baseline and must be covered by integration tests.

The intended test is:

```text
request through TokenSaver OFF
vs.
request through TokenSaver ON
```

After normalizing unavoidable transport framing, the only semantic JSON differences in the ON version may be explicitly eligible aged tool-result bodies.

## Observability contract

Transport may emit metadata needed to prove operation, including:

- request observed
- optimizer evaluated
- optimizer skipped/bypassed
- results evaluated
- results aged
- measured bytes saved
- last optimization time

It must not emit original tool-result bodies or secrets into routine logs/telemetry.

## Compatibility rule

TokenSaver must maintain an explicit supported Codex version/configuration matrix once real integration begins.

For an unsupported or unknown Codex configuration:

- do not guess config keys
- do not rewrite unknown config structures
- do not claim Connected
- report the compatibility problem
- preserve the user's existing configuration

## Phase 3 acceptance tests derived from this contract

1. Native Codex request succeeds through TokenSaver.
2. Streamed tool-call turn succeeds.
3. Cancellation reaches the upstream and client correctly.
4. Required compressed body formats round-trip correctly.
5. Required auth reaches the native upstream and never appears in logs/state.
6. Explicit conversation compaction bypasses aging.
7. Aging OFF is semantically transparent.
8. Aging ON changes only eligible historical tool-result bodies.
9. Connect/disconnect restores TokenSaver-owned Codex config exactly.
10. Config drift fails safely.
11. Unsupported request/config shapes preserve original state or fail closed without speculative rewriting.
