//! Browser-origin and DNS-rebinding boundary for the local service, not client
//! authentication. Other local processes can still use the intentional CLI API.
use crate::config::AppConfig;
use anyhow::{bail, Context};
use axum::{
    extract::{Request, State},
    http::{
        header::{HOST, ORIGIN},
        HeaderMap, HeaderName, StatusCode, Uri,
    },
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct LocalAdmission {
    hosts: Arc<Vec<String>>,
    origins: Arc<Vec<String>>,
}
impl LocalAdmission {
    pub(super) fn from_config(config: &AppConfig) -> anyhow::Result<Self> {
        let mut hosts = vec![
            format!("127.0.0.1:{}", config.port),
            format!("localhost:{}", config.port),
            format!("[::1]:{}", config.port),
        ];
        if config.bind_address.is_loopback() {
            hosts.push(format!(
                "{}",
                std::net::SocketAddr::new(config.bind_address, config.port)
            ));
        }
        if config.port == 80 {
            hosts.extend(["127.0.0.1".into(), "localhost".into(), "[::1]".into()]);
        }
        hosts.sort();
        hosts.dedup();
        for origin in &config.allowed_frontend_origins {
            let url = url::Url::parse(origin).context("invalid configured frontend origin")?;
            let local = match url.host() {
                Some(url::Host::Domain("localhost")) => true,
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                _ => false,
            };
            if !local
                || !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.origin().ascii_serialization() != *origin
            {
                bail!("allowed_frontend_origins must contain canonical HTTP(S) loopback origins without credentials, paths, queries or fragments");
            }
        }
        Ok(Self {
            hosts: Arc::new(hosts),
            origins: Arc::new(config.allowed_frontend_origins.clone()),
        })
    }
    fn check(&self, headers: &HeaderMap, uri: &Uri) -> Result<(), &'static str> {
        let host = one_header(headers, HOST)?.ok_or("A single local API Host is required")?;
        if !self
            .hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
        {
            return Err("Request Host is not an approved loopback API address and port");
        }
        if uri
            .authority()
            .is_some_and(|authority| !authority.as_str().eq_ignore_ascii_case(host))
        {
            return Err("Request authority conflicts with its local Host");
        }
        let origin = one_header(headers, ORIGIN)?;
        let fetch_site = one_header(headers, HeaderName::from_static("sec-fetch-site"))?;
        if let Some(origin) = origin {
            if !self.origins.iter().any(|allowed| allowed == origin) {
                return Err("Request Origin is not an approved local frontend origin");
            }
        } else if fetch_site
            .is_some_and(|site| !matches!(site, "none" | "same-origin" | "same-site"))
        {
            return Err("Cross-site browser requests without a trusted Origin are not allowed");
        }
        Ok(())
    }
}
fn one_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, &'static str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err("Duplicate request authority or browser-origin headers are not allowed");
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| "Malformed request authority or browser-origin header")
        })
        .transpose()
}
pub(super) async fn admit(
    State(policy): State<LocalAdmission>,
    request: Request,
    next: Next,
) -> Response {
    if let Err(message) = policy.check(request.headers(), request.uri()) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":{"message":message}})),
        )
            .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::ws::WebSocketUpgrade,
        routing::{get, post},
        Router,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower_http::cors::{AllowOrigin, CorsLayer};
    struct Server {
        url: String,
        hits: Arc<AtomicUsize>,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn server() -> Server {
        let mut config = AppConfig {
            port: 0,
            allowed_frontend_origins: vec![
                "http://127.0.0.1:7332".into(),
                "http://localhost:3000".into(),
                "http://127.0.0.1:7456".into(),
            ],
            ..AppConfig::default()
        };
        let listener = crate::app::bind_listener(&mut config).await.unwrap();
        let port = config.port;
        let policy = LocalAdmission::from_config(&config).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let mutation = {
            let hits = hits.clone();
            move || {
                let hits = hits.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    StatusCode::NO_CONTENT
                }
            }
        };
        let ws = {
            let hits = hits.clone();
            move |upgrade: WebSocketUpgrade| {
                let hits = hits.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    upgrade.on_upgrade(|_| async {})
                }
            }
        };
        let origins = config
            .allowed_frontend_origins
            .iter()
            .map(|value| value.parse().unwrap())
            .collect::<Vec<_>>();
        let app = Router::new()
            .merge(super::super::desktop_ready::Readiness::from_secret(Some(&"00".repeat(32))).unwrap().router())
            .route("/api/chat/cancel-all", post(mutation.clone()))
            .route("/api/providers/open_ai/test", post(mutation))
            .route("/api/events/ws", get(ws))
            .layer(
                CorsLayer::new()
                    .allow_origin(AllowOrigin::list(origins))
                    .allow_methods([axum::http::Method::POST]),
            )
            .layer(axum::middleware::from_fn_with_state(policy, admit));
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Server {
            url: format!("http://127.0.0.1:{port}"),
            hits,
            task,
        }
    }
    #[tokio::test]
    async fn os_assigned_port_uses_the_bound_host_for_nonce_proof_and_rejects_port_zero() {
        let s = server().await;
        let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build().unwrap();
        let endpoint = format!("{}/api/desktop/ready?nonce={}", s.url, "1".repeat(64));
        let response = client.get(&endpoint).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: serde_json::Value = response.json().await.unwrap();
        assert_eq!(value["proof"], "08a55ef15743f3ba349804ebaebdf8f923dbfcf66d49ae2e50851b268cec94e2");
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert!(value.get("secret").is_none());
        for host in ["127.0.0.1:0", "127.0.0.1:1", "evil.example:7331"] {
            assert_eq!(client.get(&endpoint).header(HOST, host).send().await.unwrap().status(), StatusCode::FORBIDDEN);
        }
        assert_eq!(client.get(&endpoint).header(ORIGIN,"http://127.0.0.1:7332").send().await.unwrap().status(), StatusCode::OK);
        assert_eq!(client.get(&endpoint).header(ORIGIN,"https://evil.example").send().await.unwrap().status(), StatusCode::FORBIDDEN);
    }
    #[tokio::test]
    async fn local_cli_desktop_and_explicit_qa_origins_reach_handlers() {
        let s = server().await;
        let client = reqwest::Client::new();
        for origin in [
            None,
            Some("http://127.0.0.1:7332"),
            Some("http://localhost:3000"),
            Some("http://127.0.0.1:7456"),
        ] {
            let mut request = client.post(format!("{}/api/chat/cancel-all", s.url));
            if let Some(origin) = origin {
                request = request.header(ORIGIN, origin);
            }
            assert_eq!(
                request.send().await.unwrap().status(),
                StatusCode::NO_CONTENT
            );
        }
        assert_eq!(s.hits.load(Ordering::SeqCst), 4);
        let response = client
            .request(
                reqwest::Method::OPTIONS,
                format!("{}/api/chat/cancel-all", s.url),
            )
            .header(ORIGIN, "http://127.0.0.1:7332")
            .header("access-control-request-method", "POST")
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        assert_eq!(
            response.headers()["access-control-allow-origin"],
            "http://127.0.0.1:7332"
        );
        assert_eq!(s.hits.load(Ordering::SeqCst), 4);
    }
    #[tokio::test]
    async fn foreign_bodyless_mutations_preflight_and_websocket_never_reach_handlers() {
        let s = server().await;
        let client = reqwest::Client::new();
        for path in ["/api/chat/cancel-all", "/api/providers/open_ai/test"] {
            for origin in [
                "https://evil.example",
                "null",
                "http://127.0.0.1:7332/",
                "http://user@127.0.0.1:7332",
                "http://127.0.0.1:7332.evil.example",
                "http://127.0.0.1:7333",
                "http://127.0.0.1:7332 http://evil.example",
            ] {
                assert_eq!(
                    client
                        .post(format!("{}{path}", s.url))
                        .header(ORIGIN, origin)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::FORBIDDEN
                );
            }
        }
        for method in [reqwest::Method::GET, reqwest::Method::OPTIONS] {
            assert_eq!(
                client
                    .request(method, format!("{}/api/events/ws", s.url))
                    .header(ORIGIN, "https://evil.example")
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            client
                .post(format!("{}/api/chat/cancel-all", s.url))
                .header("sec-fetch-site", "cross-site")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(s.hits.load(Ordering::SeqCst), 0);
        let response = client
            .get(format!("{}/api/events/ws", s.url))
            .header(ORIGIN, "http://127.0.0.1:7332")
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        assert_eq!(s.hits.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn rebinding_and_wrong_ports_are_rejected_on_actual_router() {
        let s = server().await;
        let client = reqwest::Client::new();
        for host in [
            "evil.example:7331",
            "127.0.0.1.evil.example:7331",
            "127.0.0.1:1",
            "localhost",
            "localhost.:7331",
            "127.0.0.1:7331@evil.example",
        ] {
            assert_eq!(
                client
                    .post(format!("{}/api/chat/cancel-all", s.url))
                    .header(HOST, host)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(s.hits.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn duplicate_malformed_headers_and_noncanonical_config_fail_closed() {
        let config = AppConfig::default();
        let policy = LocalAdmission::from_config(&config).unwrap();
        let uri = "/api/events/ws".parse().unwrap();
        let mut headers = HeaderMap::new();
        assert!(policy.check(&headers, &uri).is_err());
        headers.insert(HOST, "127.0.0.1:7331".parse().unwrap());
        assert!(policy.check(&headers, &uri).is_ok());
        headers.append(HOST, "evil.example:7331".parse().unwrap());
        assert!(policy.check(&headers, &uri).is_err());
        headers.remove(HOST);
        headers.insert(HOST, "127.0.0.1:7331".parse().unwrap());
        headers.append(ORIGIN, "http://localhost:3000".parse().unwrap());
        headers.append(ORIGIN, "http://localhost:3000".parse().unwrap());
        assert!(policy.check(&headers, &uri).is_err());
        headers.remove(ORIGIN);
        headers.insert(
            ORIGIN,
            axum::http::HeaderValue::from_bytes(b"\xff").unwrap(),
        );
        assert!(policy.check(&headers, &uri).is_err());
        headers.remove(ORIGIN);
        assert!(policy
            .check(
                &headers,
                &"http://evil.example/api/events/ws".parse().unwrap()
            )
            .is_err());
        for origin in [
            "null",
            "https://evil.example",
            "http://127.0.0.1:3000/",
            "http://127.1:3000",
            "http://localhost:3000?q=x",
            "http://user@localhost:3000",
        ] {
            assert!(LocalAdmission::from_config(&AppConfig {
                allowed_frontend_origins: vec![origin.into()],
                ..config.clone()
            })
            .is_err());
        }
        assert!(LocalAdmission::from_config(&AppConfig {
            allowed_frontend_origins: vec!["https://[::1]:7456".into()],
            ..config
        })
        .is_ok());
    }
}
