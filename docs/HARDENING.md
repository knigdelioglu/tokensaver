# TokenSaver Hardening and Release-Gate Contract

Phase 8 turns TokenSaver's safety assumptions into explicit runtime limits, compatibility warnings, and fail-closed release gates. The purpose is not to make every failure impossible; it is to ensure unsupported, overloaded, malformed, or insufficiently validated states are bounded and visible rather than silently guessed through.

## Runtime resource bounds

The native Codex transport has explicit process-level bounds:

- maximum encoded request body: **64 MiB**
- maximum decoded body used for inspection: **256 MiB**
- maximum concurrent native requests: **16**
- upstream connect timeout: **15 seconds**
- TCP keepalive enabled
- no total response-stream timeout, because legitimate long Codex streams must remain possible

Oversized `Content-Length` is rejected before body collection. Bodies without a trustworthy length are still bounded by Axum's body collector.

Compression bombs are bounded by the decoded-body limit. Unsupported/unsafe compression or parse/rewrite failures remain fail-original for aging rather than producing a guessed partial rewrite.

## Concurrency and request lifetime

The transport request counter covers the full downstream response-stream lifetime. A request slot remains occupied until the relayed body reaches EOF or the downstream drops it.

When all 16 request slots are occupied, new authenticated native requests receive `429 Too Many Requests` rather than causing unbounded process growth.

The disconnect drain gate remains independent of this limit: new admission stops before config restoration, and an already-active stream blocks detach.

## Bounded telemetry

Transport observations are content-free but are still bounded:

- observation channel capacity: **1024**
- transport uses non-blocking `try_send`
- a saturated/dead observation consumer never delays inference
- every failed observation enqueue increments a content-free dropped-observation counter

Dropped observations mean savings statistics may be incomplete. They are therefore exposed through the runtime/control health snapshot and `tokensaver doctor` reports a warning when the counter is non-zero.

A request is not recorded as an optimization observation until the upstream request has progressed far enough to return response headers. A connection failure before that point therefore cannot inflate the savings counters.

## Owner-local control-channel bounds

The Unix control channel is also bounded:

- maximum message/response size: **64 KiB**
- maximum simultaneous control clients: **16**
- connect/read/write timeout: **5 seconds**
- parent directory owner-only (`0700`)
- socket owner-only (`0600`)
- finite JSON command protocol only

A client flood cannot create an unbounded number of runtime tasks. Excess clients are dropped rather than queued indefinitely.

## Source-level secret redaction

Security does not rely only on tray/CLI wrapper redaction.

`CodexConfigError::Display` itself does not print:

- capability-bearing loopback URLs
- active installed/requested TokenSaver endpoints
- drift expected/actual values
- parser context from malformed Codex TOML
- parser context from malformed restoration snapshots

The underlying error variants may retain values internally because config comparison/restoration needs them, but outward display strings are evidence-bounded.

Test sources explicitly assert that capability and drift values do not appear in formatted errors.

## Codex compatibility drift

TokenSaver's protocol implementation is pinned to:

```text
openai/codex@9ded177ce7c1c0bd2047f902936c177612ab3434
```

That commit's Rust workspace version is `0.0.0`, so TokenSaver does not invent a semantic-version range from the source commit.

Instead, `tokensaver doctor` treats exact `codex --version` output as a release-validation identity:

- exact value present in the source allow-list → PASS
- detected but not explicitly release-validated → WARN
- version identity unavailable → WARN

The allow-list is intentionally empty until the final executed validation pass. Adding a version to it must happen before the final validation manifest is generated, because changing the allow-list changes the TokenSaver source commit.

## Release packaging is fail-closed

Development packaging remains available through:

```bash
bash scripts/package-macos.sh
```

A distributable release must use:

```bash
bash scripts/release-macos.sh
```

The release script first invokes `scripts/verify-release-gates.py`. Packaging is refused unless local `validation/release-manifest.json` proves all required release gates for:

- the exact current Git commit
- the exact current TokenSaver Cargo version
- the pinned Codex protocol baseline
- the exact currently installed `codex --version` identity used during validation

The local completed manifest is gitignored. The repository contains only `validation/release-manifest.example.json`, with every gate false.

Required gates are:

1. deterministic aging suite
2. architecture-contract suite
3. telemetry/benchmark suite
4. recovery/quality structural suite
5. transport integration suite
6. config restoration/drift suite
7. desktop runtime/tray suite
8. CLI/doctor suite
9. real Codex smoke test
10. compaction-bypass test
11. ON/OFF payload-diff invariant
12. tray/backend state consistency
13. privacy/log/UI/CLI redaction review
14. install/uninstall round trip
15. realistic long-session savings + quality benchmark

The verifier does **not** execute these gates or manufacture evidence. It only verifies a manifest produced by the final validation process. A false/missing/stale manifest blocks release packaging.

## Existing recovery guarantees retained

Phase 8 does not weaken earlier guarantees:

- listener remains IPv4 loopback only
- strict 256-bit capability remains required
- browser-origin requests are rejected
- route set remains finite; TokenSaver is not an arbitrary proxy
- redirects remain disabled upstream
- conversation compaction bypasses aging
- mixed/unsupported tool output remains exact
- configuration drift blocks restoration overwrite
- crash-surviving restoration snapshot remains authoritative
- uninstall purge remains non-recursive and snapshot-blocking
- routine telemetry never contains tool-result or receipt bodies

## Deferred executed validation

Implementation is authored, but Phase 8 is not release-validated until the user's final execution pass runs the required suites and creates the release manifest.

Still to execute:

- compile/test/lint/format
- malformed/oversized/compressed-body integration cases
- concurrency saturation and recovery
- interrupted response-stream lifecycle
- control-client saturation/timeouts
- telemetry queue saturation and dropped-counter visibility
- upstream connection failure not counted as savings
- crash/restart and power-loss recovery
- Codex-version warning/allow-list behavior
- real ON/OFF Codex payload and quality comparison
- performance/memory/latency benchmark
- release-manifest negative and positive cases
- signed/notarized packaging when real credentials are available

No test, build, lint, formatter, benchmark, CI, live Codex, packaging, or release-verifier command was executed while implementing Phase 8.
