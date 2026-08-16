use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::http::header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, UPGRADE};
use axum::http::{HeaderMap, Method, Request, Response, StatusCode};
use axum::Router;
use futures_util::StreamExt;
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
const MAX_ENCODED_BODY_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct TransportSettings {
    pub(crate) bind_port: u16,
    pub(crate) capability: CallerCapability,
    pub(crate) aging_policy: AgingPolicy,
    pub(crate) observer: Option<mpsc::UnboundedSender<TransportObservation>>,
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
        observer: mpsc::UnboundedSender<TransportObservation>,
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
        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(TransportError::ClientBuild)?;
        let state = Arc::new(ServerState {
            capability: settings.capability.clone(),
            aging_policy: aging_policy.clone(),
            observer: settings.observer,
            client,
        });
        let control = TransportControl {
            local_addr,
            capability: settings.capability,
            aging_policy,
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
    observer: Option<mpsc::UnboundedSender<TransportObservation>>,
    client: reqwest::Client,
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

    let Some(upstream_path) = state.capability.authenticate_path(request.uri().path()) else {
        return empty_response(StatusCode::NOT_FOUND);
    };
    if !is_supported_path(upstream_path) {
        return empty_response(StatusCode::NOT_FOUND);
    }

    // Current Codex advertises WebSocket support for the built-in OpenAI
    // provider even when openai_base_url is overridden. TokenSaver serves HTTP
    // only and uses the client's explicit 426 -> HTTP fallback behavior.
    if request.headers().get(UPGRADE).is_some() {
        return empty_response(StatusCode::UPGRADE_REQUIRED);
    }
    if request.method() != Method::POST {
        return empty_response(StatusCode::METHOD_NOT_ALLOWED);
    }
    if !is_json_request(request.headers()) {
        return empty_response(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

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
        upstream_path,
        policy,
    );

    if let Some(observer) = &state.observer {
        let _ = observer.send(TransportObservation {
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
        .post(upstream_url)
        .headers(headers)
        .body(prepared.bytes)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return empty_response(StatusCode::BAD_GATEWAY),
    };

    relay_response(upstream)
}

/// The built-in Codex provider chooses ChatGPT backend for first-party account
/// auth modes and api.openai.com for API-key auth before TokenSaver overrides
/// its base URL. After interception, the account-ID routing header is the
/// transport-level signal available to preserve that distinction without
/// reading or owning Codex credentials.
fn native_upstream_base_url(headers: &HeaderMap) -> &'static str {
    if headers.contains_key("chatgpt-account-id") || headers.contains_key("x-openai-fedramp") {
        CHATGPT_CODEX_BASE_URL
    } else {
        OPENAI_API_BASE_URL
    }
}

fn relay_response(upstream: reqwest::Response) -> Response<Body> {
    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let stream = upstream
        .bytes_stream()
        .map(|chunk| chunk.map_err(io::Error::other));
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;

    for (name, value) in &upstream_headers {
        if is_hop_by_hop_response_header(name.as_str()) {
            continue;
        }
        response.headers_mut().append(name.clone(), value.clone());
    }
    response
}

fn is_supported_path(path: &str) -> bool {
    matches!(
        path,
        "/responses" | "/v1/responses" | "/responses/compact" | "/v1/responses/compact"
    )
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
    use super::{native_upstream_base_url, CHATGPT_CODEX_BASE_URL, OPENAI_API_BASE_URL};
    use axum::http::{HeaderMap, HeaderValue};

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
}
