use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::http::header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, UPGRADE};
use axum::http::{HeaderMap, Method, Request, Response, StatusCode};
use axum::Router;
use futures_util::{Stream, StreamExt};
use reqwest::redirect::Policy;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};

use crate::modules::aging::AgingPolicy;

use super::capability::CallerCapability;
use super::headers::{has_browser_origin, native_upstream_headers};
use super::observation::TransportObservation;
use super::request::prepare_responses_body;

const CHATGPT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const OPENAI_API_BASE_URL: &str = "https://api.openai.com/v1";
const MAX_ENCODED_BODY_BYTES: usize = 64 * 1024 * 1024;
const MAX_CONCURRENT_REQUESTS: usize = 16;
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug)]
pub(crate) struct TransportSettings {
    pub(crate) bind_port: u16,
    pub(crate) capability: CallerCapability,
    pub(crate) aging_policy: AgingPolicy,
    pub(crate) observer: Option<mpsc::Sender<TransportObservation>>,
}

impl TransportSettings {
    pub(crate) fn native_codex(bind_port: u16, aging_policy: AgingPolicy) -> Self {
        Self::native_codex_with_capability(
            bind_port,
            CallerCapability::generate(),
            aging_policy,
        )
    }

    pub(crate) fn native_codex_with_capability(
        bind_port: u16,
        capability: CallerCapability,
        aging_policy: AgingPolicy,
    ) -> Self {
        Self {
            bind_port,
            capability,
            aging_policy,
            observer: None,
        }
    }

    pub(crate) fn with_observer(
        mut self,
        observer: mpsc::Sender<TransportObservation>,
    ) -> Self {
        self.observer = Some(observer);
        self
    }
}

#[derive(Clone)]
pub(crate) struct TransportControl {
    local_addr: SocketAddr,
    capability: CallerCapability,
    aging_policy: Arc<RwLock<AgingPolicy>>,
    active_requests: Arc<AtomicUsize>,
    draining: Arc<AtomicBool>,
}

impl TransportControl {
    pub(crate) fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub(crate) fn codex_base_url(&self) -> String {
        self.capability.loopback_base_url(self.local_addr.port())
    }

    pub(crate) async fn set_aging_enabled(&self, enabled: bool) {
        self.aging_policy.write().await.enabled = enabled;
    }

    pub(crate) async fn aging_policy(&self) -> AgingPolicy {
        *self.aging_policy.read().await
    }

    pub(crate) fn active_requests(&self) -> usize {
        self.active_requests.load(Ordering::Acquire)
    }

    /// Stop admitting new native requests and return the number of requests
    /// already in flight. A caller may disconnect only when this returns zero.
    pub(crate) fn begin_drain(&self) -> usize {
        self.draining.store(true, Ordering::Release);
        self.active_requests.load(Ordering::Acquire)
    }

    /// Resume normal request admission after a drain attempt was refused.
    pub(crate) fn resume_accepting(&self) {
        self.draining.store(false, Ordering::Release);
    }
}

pub(crate) struct BoundTransport {
    listener: TcpListener,
    state: Arc<ServerState>,
    control: TransportControl,
}

impl BoundTransport {
    pub(crate) async fn bind(settings: TransportSettings) -> Result<Self, TransportError> {
        let listener = TcpListener::bind(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            settings.bind_port,
        ))
        .await?;
        let local_addr = listener.local_addr()?;
        let aging_policy = Arc::new(RwLock::new(settings.aging_policy));
        let active_requests = Arc::new(AtomicUsize::new(0));
        let draining = Arc::new(AtomicBool::new(false));
        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .map_err(TransportError::ClientBuild)?;
        let state = Arc::new(ServerState {
            capability: settings.capability.clone(),
            aging_policy: aging_policy.clone(),
            active_requests: active_requests.clone(),
            draining: draining.clone(),
            observer: settings.observer,
            client,
        });
        let control = TransportControl {
            local_addr,
            capability: settings.capability,
            aging_policy,
            active_requests,
            draining,
        };

        Ok(Self {
            listener,
            state,
            control,
        })
    }

    pub(crate) fn control(&self) -> TransportControl {
        self.control.clone()
    }

    pub(crate) async fn serve(self) -> Result<(), TransportError> {
        let app = Router::new()
            .fallback(handle_request)
            .with_state(self.state);
        axum::serve(self.listener, app)
            .await
            .map_err(TransportError::Serve)
    }
}

#[derive(Clone)]
struct ServerState {
    capability: CallerCapability,
    aging_policy: Arc<RwLock<AgingPolicy>>,
    active_requests: Arc<AtomicUsize>,
    draining: Arc<AtomicBool>,
    observer: Option<mpsc::Sender<TransportObservation>>,
    client: reqwest::Client,
}

struct ActiveRequestGuard {
    counter: Arc<AtomicUsize>,
}

impl ActiveRequestGuard {
    fn try_enter(counter: Arc<AtomicUsize>) -> Option<Self> {
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < MAX_CONCURRENT_REQUESTS).then_some(current + 1)
            })
            .ok()?;
        Some(Self { counter })
    }
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeRouteKind {
    Responses,
    Compaction,
    Passthrough,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeRoute {
    method: Method,
    kind: NativeRouteKind,
}

#[derive(Debug)]
pub(crate) enum TransportError {
    Io(io::Error),
    ClientBuild(reqwest::Error),
    Serve(io::Error),
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "transport I/O failed: {error}"),
            Self::ClientBuild(error) => write!(formatter, "failed to build upstream client: {error}"),
            Self::Serve(error) => write!(formatter, "loopback server failed: {error}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::Serve(error) => Some(error),
            Self::ClientBuild(error) => Some(error),
        }
    }
}

impl From<io::Error> for TransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

async fn handle_request(
    State(state): State<Arc<ServerState>>,
    request: Request<Body>,
) -> Response<Body> {
    if has_browser_origin(request.headers()) {
        return empty_response(StatusCode::FORBIDDEN);
    }

    let Some(local_path) = state.capability.authenticate_path(request.uri().path()) else {
        return empty_response(StatusCode::NOT_FOUND);
    };
    let Some(route) = native_route(local_path) else {
        return empty_response(StatusCode::NOT_FOUND);
    };

    if route.kind == NativeRouteKind::Responses && request.headers().get(UPGRADE).is_some() {
        return empty_response(StatusCode::UPGRADE_REQUIRED);
    }
    if request.method() != route.method {
        return empty_response(StatusCode::METHOD_NOT_ALLOWED);
    }
    if route.method == Method::POST && !is_json_request(request.headers()) {
        return empty_response(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    if encoded_content_length_exceeds_limit(request.headers()) {
        return empty_response(StatusCode::PAYLOAD_TOO_LARGE);
    }
    if state.draining.load(Ordering::Acquire) {
        return empty_response(StatusCode::SERVICE_UNAVAILABLE);
    }

    let Some(active_request) = ActiveRequestGuard::try_enter(state.active_requests.clone()) else {
        return empty_response(StatusCode::TOO_MANY_REQUESTS);
    };
    // Close the check/increment race with begin_drain(): a request that entered
    // while draining flipped to true is rejected before it can reach upstream.
    if state.draining.load(Ordering::Acquire) {
        return empty_response(StatusCode::SERVICE_UNAVAILABLE);
    }

    let Some(upstream_path) = upstream_path(local_path) else {
        return empty_response(StatusCode::NOT_FOUND);
    };
    let query = request.uri().query().map(str::to_owned);
    let inbound_headers = request.headers().clone();
    let upstream_base_url = native_upstream_base_url(&inbound_headers);
    let content_encoding = match inbound_headers.get(CONTENT_ENCODING) {
        None => None,
        Some(value) => match value.to_str() {
            Ok(value) => Some(value.to_owned()),
            Err(_) => return empty_response(StatusCode::BAD_REQUEST),
        },
    };
    let body = match to_bytes(request.into_body(), MAX_ENCODED_BODY_BYTES).await {
        Ok(body) => body,
        Err(_) => return empty_response(StatusCode::PAYLOAD_TOO_LARGE),
    };

    let policy = *state.aging_policy.read().await;
    let prepared = prepare_responses_body(
        &body,
        content_encoding.as_deref(),
        local_path,
        policy,
    );

    if let Some(observer) = &state.observer {
        // Telemetry is best-effort and content-free. A stuck consumer must not
        // create an unbounded memory queue or delay native Codex traffic.
        let _ = observer.try_send(TransportObservation {
            outcome: prepared.outcome,
            aging_stats: prepared.aging.stats.clone(),
        });
    }

    let mut headers = native_upstream_headers(&inbound_headers);
    if let Some(content_encoding) = content_encoding {
        if let Ok(value) = content_encoding.parse() {
            headers.insert(CONTENT_ENCODING, value);
        } else {
            return empty_response(StatusCode::BAD_REQUEST);
        }
    }

    let mut upstream_url = format!("{upstream_base_url}{upstream_path}");
    if let Some(query) = query {
        upstream_url.push('?');
        upstream_url.push_str(&query);
    }

    let upstream = match state
        .client
        .request(route.method, upstream_url)
        .headers(headers)
        .body(prepared.bytes)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return empty_response(StatusCode::BAD_GATEWAY),
    };

    relay_response(upstream, active_request)
}

fn encoded_content_length_exceeds_limit(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|value| value > MAX_ENCODED_BODY_BYTES as u64)
}

fn native_upstream_base_url(headers: &HeaderMap) -> &'static str {
    if headers.contains_key("chatgpt-account-id") || headers.contains_key("x-openai-fedramp") {
        CHATGPT_CODEX_BASE_URL
    } else {
        OPENAI_API_BASE_URL
    }
}

fn upstream_path(local_path: &str) -> Option<&str> {
    let path = local_path.strip_prefix("/v1")?;
    if path.starts_with('/') {
        Some(path)
    } else {
        None
    }
}

fn native_route(path: &str) -> Option<NativeRoute> {
    let route = match path {
        "/v1/responses" => NativeRoute {
            method: Method::POST,
            kind: NativeRouteKind::Responses,
        },
        "/v1/responses/compact" => NativeRoute {
            method: Method::POST,
            kind: NativeRouteKind::Compaction,
        },
        "/v1/models" => NativeRoute {
            method: Method::GET,
            kind: NativeRouteKind::Passthrough,
        },
        "/v1/memories/trace_summarize"
        | "/v1/alpha/search"
        | "/v1/images/generations"
        | "/v1/images/edits" => NativeRoute {
            method: Method::POST,
            kind: NativeRouteKind::Passthrough,
        },
        _ => return None,
    };
    Some(route)
}

fn relay_response(upstream: reqwest::Response, active_request: ActiveRequestGuard) -> Response<Body> {
    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let mut upstream_stream = Box::pin(
        upstream
            .bytes_stream()
            .map(|chunk| chunk.map_err(io::Error::other)),
    );
    // The guard is captured by the stream itself, so the request remains active
    // until the response body is exhausted or the downstream client drops it.
    let guarded_stream = futures_util::stream::poll_fn(move |context| {
        let _keep_guard_alive = &active_request;
        upstream_stream.as_mut().poll_next(context)
    });
    let mut response = Response::new(Body::from_stream(guarded_stream));
    *response.status_mut() = status;

    for (name, value) in &upstream_headers {
        if is_hop_by_hop_response_header(name.as_str()) {
            continue;
        }
        response.headers_mut().append(name.clone(), value.clone());
    }
    response
}

fn is_json_request(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
        })
}

fn is_hop_by_hop_response_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
    ) || name.eq_ignore_ascii_case(CONTENT_LENGTH.as_str())
}

fn empty_response(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod routing_tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    use super::{
        encoded_content_length_exceeds_limit, native_route, native_upstream_base_url,
        upstream_path, ActiveRequestGuard, NativeRouteKind, CHATGPT_CODEX_BASE_URL,
        MAX_CONCURRENT_REQUESTS, MAX_ENCODED_BODY_BYTES, OPENAI_API_BASE_URL,
    };
    use axum::http::{HeaderMap, HeaderValue, Method};

    #[test]
    fn account_scoped_auth_routes_to_chatgpt_backend() {
        let mut headers = HeaderMap::new();
        headers.insert("chatgpt-account-id", HeaderValue::from_static("account-1"));
        assert_eq!(native_upstream_base_url(&headers), CHATGPT_CODEX_BASE_URL);
    }

    #[test]
    fn api_key_style_auth_without_account_id_routes_to_openai_api() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer sk-test"));
        assert_eq!(native_upstream_base_url(&headers), OPENAI_API_BASE_URL);
    }

    #[test]
    fn local_v1_prefix_is_removed_once_for_native_upstream() {
        assert_eq!(upstream_path("/v1/responses"), Some("/responses"));
        assert_eq!(upstream_path("/responses"), None);
    }

    #[test]
    fn finite_native_route_allowlist_has_expected_methods() {
        let models = native_route("/v1/models").expect("models route");
        assert_eq!(models.method, Method::GET);
        assert_eq!(models.kind, NativeRouteKind::Passthrough);

        let responses = native_route("/v1/responses").expect("responses route");
        assert_eq!(responses.method, Method::POST);
        assert_eq!(responses.kind, NativeRouteKind::Responses);

        let compact = native_route("/v1/responses/compact").expect("compact route");
        assert_eq!(compact.method, Method::POST);
        assert_eq!(compact.kind, NativeRouteKind::Compaction);

        assert!(native_route("/v1/realtime/calls").is_none());
        assert!(native_route("/v1/arbitrary-proxy-target").is_none());
    }

    #[test]
    fn encoded_content_length_is_rejected_before_body_collection() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-length",
            HeaderValue::from_str(&(MAX_ENCODED_BODY_BYTES as u64 + 1).to_string())
                .expect("content length"),
        );
        assert!(encoded_content_length_exceeds_limit(&headers));
    }

    #[test]
    fn concurrent_request_guard_is_strictly_bounded() {
        let counter = Arc::new(AtomicUsize::new(0));
        let guards = (0..MAX_CONCURRENT_REQUESTS)
            .map(|_| ActiveRequestGuard::try_enter(counter.clone()).expect("slot"))
            .collect::<Vec<_>>();
        assert!(ActiveRequestGuard::try_enter(counter.clone()).is_none());
        drop(guards);
        assert_eq!(counter.load(std::sync::atomic::Ordering::Acquire), 0);
    }
}
