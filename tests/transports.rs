use medium_scraper_mcp::config::Config;
use medium_scraper_mcp::http_server::build_router;
use rmcp::model::{ClientCapabilities, ClientConfig, Implementation, ProtocolVersion};
use rmcp::service::ClientLifecycleMode;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ClientServiceExt;
use std::sync::Arc;
use std::time::Duration;

async fn wait_for_port(addr: std::net::SocketAddr) {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not start listening on {addr}");
}

fn auto_lifecycle() -> ClientLifecycleMode {
    ClientLifecycleMode::Auto {
        preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        legacy_version: Some(ProtocolVersion::V_2025_11_25),
    }
}

async fn spawn_server(token: Option<String>) -> std::net::SocketAddr {
    let router = build_router(Arc::new(Config::from_env()), token, vec![]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    wait_for_port(addr).await;
    addr
}

#[tokio::test]
async fn streamable_http_lists_tools() {
    let addr = spawn_server(None).await;
    let transport = StreamableHttpClientTransport::from_uri(format!("http://{addr}/mcp"));
    let client = ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new("test-client", "0.0.1"),
    )
    .serve_with_lifecycle(transport, auto_lifecycle())
    .await
    .expect("client connect");

    let tools = client
        .list_tools(Default::default())
        .await
        .expect("list tools");
    let names: Vec<String> = tools.tools.iter().map(|t| t.name.to_string()).collect();
    assert!(
        names.contains(&"read_medium_post".to_string()),
        "expected read_medium_post among tools, got: {names:?}"
    );
    assert!(
        names.len() >= 8,
        "expected at least 8 tools, got: {names:?}"
    );

    let _ = client.cancel().await;
}

#[tokio::test]
async fn streamable_http_rejects_bad_token() {
    let addr = spawn_server(Some("sekrit".into())).await;

    // /healthz is behind the same bearer middleware when a token is set.
    let status = reqwest::get(format!("http://{addr}/healthz"))
        .await
        .expect("healthz request")
        .status();
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);

    // Without the bearer token the MCP client cannot complete the handshake
    // or list tools.
    let transport = StreamableHttpClientTransport::from_uri(format!("http://{addr}/mcp"));
    let connect = ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new("test-client", "0.0.1"),
    )
    .serve_with_lifecycle(transport, auto_lifecycle())
    .await;
    match connect {
        Err(_) => {} // handshake rejected, as expected
        Ok(client) => {
            let tools = client
                .list_tools(Default::default())
                .await
                .expect("unauthenticated list_tools must fail");
            panic!(
                "unauthenticated client listed tools: {:?}",
                tools.tools.len()
            );
        }
    }
}
