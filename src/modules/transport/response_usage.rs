use serde_json::Value;

const MAX_JSON_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SSE_LINE_BYTES: usize = 1024 * 1024;

/// Provider-reported usage observed at the native transport boundary.
///
/// This is deliberately transport-local rather than a telemetry-domain type so
/// the transport module remains independent of persistence/aggregation. The
/// application layer maps it into telemetry after the response stream ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProviderUsageObservation {
    pub(crate) input_tokens: u64,
    pub(crate) cached_input_tokens: u64,
    pub(crate) output_tokens: u64,
}

/// Read-only streaming observer for Responses usage metadata.
///
/// Every upstream byte is forwarded independently by the transport. This
/// collector receives copies of chunks and can only update numeric state. A
/// malformed/oversized response simply produces no usage observation; it can
/// never delay, fail, truncate, or rewrite the model response.
pub(crate) struct ResponseUsageCollector {
    event_stream: bool,
    json_capture: Vec<u8>,
    json_overflowed: bool,
    sse_line: Vec<u8>,
    sse_line_overflowed: bool,
    usage: Option<ProviderUsageObservation>,
}

impl ResponseUsageCollector {
    pub(crate) fn new(event_stream: bool) -> Self {
        Self {
            event_stream,
            json_capture: Vec::new(),
            json_overflowed: false,
            sse_line: Vec::new(),
            sse_line_overflowed: false,
            usage: None,
        }
    }

    pub(crate) fn observe(&mut self, chunk: &[u8]) {
        if self.event_stream {
            self.observe_sse(chunk);
        } else {
            self.observe_json(chunk);
        }
    }

    pub(crate) fn finish(mut self) -> Option<ProviderUsageObservation> {
        if self.event_stream {
            if !self.sse_line.is_empty() && !self.sse_line_overflowed {
                let line = std::mem::take(&mut self.sse_line);
                self.observe_sse_line(&line);
            }
        } else if !self.json_overflowed
            && let Ok(payload) = serde_json::from_slice::<Value>(&self.json_capture)
            && let Some(usage) = usage_from_payload(&payload)
        {
            self.usage = Some(usage);
        }
        self.usage
    }

    fn observe_json(&mut self, chunk: &[u8]) {
        if self.json_overflowed {
            return;
        }
        let Some(next_len) = self.json_capture.len().checked_add(chunk.len()) else {
            self.json_overflowed = true;
            self.json_capture.clear();
            return;
        };
        if next_len > MAX_JSON_CAPTURE_BYTES {
            self.json_overflowed = true;
            self.json_capture.clear();
            return;
        }
        self.json_capture.extend_from_slice(chunk);
    }

    fn observe_sse(&mut self, chunk: &[u8]) {
        for byte in chunk {
            if *byte == b'\n' {
                if !self.sse_line_overflowed {
                    let line = std::mem::take(&mut self.sse_line);
                    self.observe_sse_line(&line);
                } else {
                    self.sse_line.clear();
                }
                self.sse_line_overflowed = false;
                continue;
            }

            if self.sse_line_overflowed {
                continue;
            }
            if self.sse_line.len() >= MAX_SSE_LINE_BYTES {
                self.sse_line.clear();
                self.sse_line_overflowed = true;
                continue;
            }
            self.sse_line.push(*byte);
        }
    }

    fn observe_sse_line(&mut self, line: &[u8]) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(data) = line.strip_prefix(b"data:") else {
            return;
        };
        let data = trim_ascii_start(data);
        if data == b"[DONE]" || data.is_empty() {
            return;
        }
        let Ok(payload) = serde_json::from_slice::<Value>(data) else {
            return;
        };
        if let Some(usage) = usage_from_payload(&payload) {
            self.usage = Some(usage);
        }
    }
}

fn trim_ascii_start(mut value: &[u8]) -> &[u8] {
    while let Some((first, rest)) = value.split_first() {
        if first.is_ascii_whitespace() {
            value = rest;
        } else {
            break;
        }
    }
    value
}

fn usage_from_payload(payload: &Value) -> Option<ProviderUsageObservation> {
    for candidate in [payload.get("usage"), payload.pointer("/response/usage")]
        .into_iter()
        .flatten()
    {
        if let Some(usage) = usage_from_value(candidate) {
            return Some(usage);
        }
    }
    None
}

fn usage_from_value(usage: &Value) -> Option<ProviderUsageObservation> {
    let input_tokens = token_count(usage.get("input_tokens"))
        .or_else(|| token_count(usage.get("prompt_tokens")));
    let output_tokens = token_count(usage.get("output_tokens"))
        .or_else(|| token_count(usage.get("completion_tokens")));

    if input_tokens.is_none() && output_tokens.is_none() {
        return None;
    }

    let input_tokens = input_tokens.unwrap_or(0);
    let cached_input_tokens = usage
        .pointer("/input_tokens_details/cached_tokens")
        .and_then(token_count_value)
        .or_else(|| {
            usage
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(token_count_value)
        })
        .or_else(|| token_count(usage.get("prompt_cache_hit_tokens")))
        .unwrap_or(0)
        .min(input_tokens);

    Some(ProviderUsageObservation {
        input_tokens,
        cached_input_tokens,
        output_tokens: output_tokens.unwrap_or(0),
    })
}

fn token_count(value: Option<&Value>) -> Option<u64> {
    value.and_then(token_count_value)
}

fn token_count_value(value: &Value) -> Option<u64> {
    value.as_u64()
}

#[cfg(test)]
mod tests {
    use super::{ProviderUsageObservation, ResponseUsageCollector};

    #[test]
    fn non_streaming_openai_usage_is_observed() {
        let payload = serde_json::json!({
            "usage": {
                "input_tokens": 1200,
                "input_tokens_details": { "cached_tokens": 900 },
                "output_tokens": 77
            }
        });
        let encoded = serde_json::to_vec(&payload).expect("encode usage fixture");
        let mut collector = ResponseUsageCollector::new(false);
        collector.observe(&encoded);
        assert_eq!(
            collector.finish(),
            Some(ProviderUsageObservation {
                input_tokens: 1200,
                cached_input_tokens: 900,
                output_tokens: 77,
            })
        );
    }

    #[test]
    fn streamed_nested_usage_survives_chunk_boundaries() {
        let mut collector = ResponseUsageCollector::new(true);
        collector.observe(b"event: response.completed\ndata: {\"type\":\"response.completed\",\"res");
        collector.observe(b"ponse\":{\"usage\":{\"input_tokens\":40,\"output_tokens\":3,\"input_tokens_details\":{\"cached_tokens\":30}}}}\n\n");
        assert_eq!(
            collector.finish(),
            Some(ProviderUsageObservation {
                input_tokens: 40,
                cached_input_tokens: 30,
                output_tokens: 3,
            })
        );
    }
}
