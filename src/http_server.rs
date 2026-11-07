//! Streamable HTTP transport wiring shared by the CLI and integration tests.

use crate::config::Config;
use crate::server::MediumScraperServer;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};

/// Build the axum router: MCP streamable HTTP at `/mcp`, health at `/healthz`.
///
/// `token` enables bearer auth on all routes. `extra_hosts` extends the
/// loopback-only Host allowlist (DNS-rebinding protection) for named-host
/// deployments. Streamable HTTP serves SSE-formatted event streams; the
/// legacy HTTP+SSE transport (GET /sse + POST /message) was deprecated by
/// the MCP spec and removed from rmcp 3.x.
pub fn build_router(cfg: Arc<Config>, token: Option<String>, extra_hosts: Vec<String>) -> Router {
    let allowed_hosts = ["127.0.0.1", "localhost", "::1"]
        .iter()
        .map(|s| s.to_string())
        .chain(extra_hosts)
        .collect::<Vec<_>>();

    let mut http_cfg = StreamableHttpServerConfig::default();
    http_cfg.allowed_hosts = allowed_hosts;

    let service = StreamableHttpService::new(
        {
            let cfg = cfg.clone();
            move || {
                MediumScraperServer::new((*cfg).clone())
                    .map_err(|e| std::io::Error::other(e.to_string()))
            }
        },
        LocalSessionManager::default().into(),
        http_cfg,
    );

    let state: Option<Arc<String>> = token.map(Arc::new);
    Router::new()
        .nest_service("/mcp", service)
        .route("/healthz", get(health))
        .layer(middleware::from_fn_with_state(
            state,
            bearer_auth_middleware,
        ))
}

async fn health() -> &'static str {
    "ok"
}

async fn bearer_auth_middleware(
    State(token): State<Option<Arc<String>>>,
    req: Request,
    next: Next,
) -> Response {
    let Some(token) = token else {
        return next.run(req).await;
    };
    let ok = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == format!("Bearer {token}"))
        .unwrap_or(false);
    if ok {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response()
    }
}
