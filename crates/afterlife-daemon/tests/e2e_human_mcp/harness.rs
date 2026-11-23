use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use afterlife_daemon::{AppDaemonState, build_router};
use afterlife_protocol::AfterlifeConfig;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::{BodyExt, Full};
use hyper::{
    Method, Request, StatusCode,
    body::{Bytes, Incoming},
    header,
};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

static NEXT_DIR_SEQ: AtomicU64 = AtomicU64::new(1);

pub(crate) struct TempDataDir(PathBuf);

impl TempDataDir {
    pub(crate) fn new(tag: &str) -> Self {
        let seq = NEXT_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("afterlife_e2e_{tag}_{pid}_{seq}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create isolated e2e temp dir");
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDataDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) struct LiveServer {
    pub(crate) addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl LiveServer {
    pub(crate) async fn start(data_dir: &Path) -> Self {
        let cfg = AfterlifeConfig::local_default(data_dir.to_str().unwrap());
        Self::start_with_config(cfg).await
    }

    pub(crate) async fn start_desktop_embedded(data_dir: &Path) -> Self {
        let cfg = AfterlifeConfig::desktop_embedded(data_dir.to_str().unwrap());
        Self::start_with_config(cfg).await
    }

    pub(crate) async fn start_with_config(mut cfg: AfterlifeConfig) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");

        cfg.server.bind_addr = addr.to_string();
        cfg.server.public_base_url = format!("http://{addr}");
        if let afterlife_protocol::AuthConfig::LocalDev { ref mut issuer, .. } = cfg.auth {
            *issuer = format!("http://{addr}");
        }

        let state = AppDaemonState::from_config(cfg).expect("init AppDaemonState");
        let router = build_router(state);

        let (tx, rx) = oneshot::channel::<()>();
        let join = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });

        Self {
            addr,
            shutdown_tx: Some(tx),
            join: Some(join),
        }
    }

    pub(crate) async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.join.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }
    }
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

pub(crate) struct WireResponse {
    pub(crate) status: StatusCode,
    headers: hyper::HeaderMap,
    pub(crate) body: Bytes,
}

impl WireResponse {
    pub(crate) fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| {
            panic!(
                "expected JSON response (status {}), parse error: {e}\nraw body: {}",
                self.status,
                String::from_utf8_lossy(&self.body)
            )
        })
    }
}

pub(crate) async fn send_http(
    addr: SocketAddr,
    method: Method,
    path: &str,
    bearer: Option<&str>,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> WireResponse {
    let stream = TcpStream::connect(addr)
        .await
        .expect("connect to live e2e server");
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .expect("http1 handshake");
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, addr.to_string())
        .header(header::CONNECTION, "close");
    if let Some(tok) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    if let Some(ct) = content_type {
        req = req.header(header::CONTENT_TYPE, ct);
    }
    let req = req.body(Full::new(Bytes::from(body))).unwrap();

    let resp = sender.send_request(req).await.expect("send http request");
    let status = resp.status();
    let headers = resp.headers().clone();
    let collected = resp
        .into_body()
        .collect()
        .await
        .expect("read http body")
        .to_bytes();
    WireResponse {
        status,
        headers,
        body: collected,
    }
}

pub(crate) struct SseStream {
    body: Incoming,
    buf: String,
}

impl SseStream {
    pub(crate) async fn connect(addr: SocketAddr, since_seq: u64) -> Self {
        let stream = TcpStream::connect(addr).await.expect("connect SSE");
        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .expect("http1 SSE handshake");
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/events?since_seq={since_seq}"))
            .header(header::HOST, addr.to_string())
            .header(header::ACCEPT, "text/event-stream")
            .body(Full::new(Bytes::new()))
            .unwrap();

        let resp = sender.send_request(req).await.expect("open SSE stream");
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "SSE endpoint must return 200"
        );
        Self {
            body: resp.into_body(),
            buf: String::new(),
        }
    }

    pub(crate) async fn next_event(&mut self) -> Value {
        loop {
            if let Some(idx) = self.buf.find("\n\n") {
                let frame = self.buf[..idx].to_string();
                self.buf.drain(..idx + 2);
                let data_lines: Vec<&str> = frame
                    .lines()
                    .filter_map(|l| l.strip_prefix("data:"))
                    .map(str::trim)
                    .collect();
                if !data_lines.is_empty() {
                    let payload = data_lines.join("\n");
                    return serde_json::from_str(&payload)
                        .unwrap_or_else(|e| panic!("invalid SSE JSON `{payload}`: {e}"));
                }
            }
            let frame = tokio::time::timeout(Duration::from_secs(3), self.body.frame())
                .await
                .expect("timed out waiting for SSE event")
                .expect("SSE stream ended unexpectedly")
                .expect("SSE frame read error");
            if let Ok(chunk) = frame.into_data() {
                self.buf.push_str(&String::from_utf8_lossy(&chunk));
            }
        }
    }
}

pub(crate) async fn oauth_register_and_login(
    addr: SocketAddr,
    client_name: &str,
    is_agent_client: bool,
    requested_scope: &str,
) -> (String, String, String, String) {
    let reg = send_http(
        addr,
        Method::POST,
        "/oauth/register",
        None,
        Some("application/json"),
        json!({
            "client_name": client_name,
            "redirect_uris": ["http://127.0.0.1:9999/cb"],
            "is_agent_client": is_agent_client
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    assert_eq!(reg.status, StatusCode::CREATED);
    let client_id = reg.json()["client_id"].as_str().unwrap().to_string();

    let verifier = format!("pkce-verifier-{client_id}-deterministic-secret-0123456789");
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    let encoded_scope = requested_scope.replace(' ', "%20");
    let auth_path = format!(
        "/oauth/authorize?client_id={client_id}&redirect_uri=http://127.0.0.1:9999/cb&response_type=code&code_challenge={challenge}&code_challenge_method=S256&scope={encoded_scope}&state=e2e&auto_approve=1"
    );
    let auth = send_http(addr, Method::GET, &auth_path, None, None, Vec::new()).await;
    assert!(
        auth.status == StatusCode::SEE_OTHER || auth.status == StatusCode::TEMPORARY_REDIRECT,
        "expected redirect from /oauth/authorize, got {}",
        auth.status
    );
    let loc = auth
        .headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    let code = loc
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_string();

    let form = format!(
        "grant_type=authorization_code&code={code}&redirect_uri=http://127.0.0.1:9999/cb&client_id={client_id}&code_verifier={verifier}"
    );
    let tok = send_http(
        addr,
        Method::POST,
        "/oauth/token",
        None,
        Some("application/x-www-form-urlencoded"),
        form.into_bytes(),
    )
    .await;
    assert_eq!(tok.status, StatusCode::OK);
    let tok_json = tok.json();
    (
        client_id,
        tok_json["access_token"].as_str().unwrap().to_string(),
        tok_json["refresh_token"].as_str().unwrap().to_string(),
        tok_json["scope"].as_str().unwrap().to_string(),
    )
}

#[derive(Clone)]
pub(crate) struct McpClient {
    pub(crate) addr: SocketAddr,
    pub(crate) bearer: Option<String>,
    rpc_seq: Arc<AtomicU64>,
}

impl McpClient {
    pub(crate) fn new(addr: SocketAddr, bearer: Option<String>) -> Self {
        Self {
            addr,
            bearer,
            rpc_seq: Arc::new(AtomicU64::new(1)),
        }
    }

    pub(crate) async fn handshake(&self) {
        let init = self
            .raw_post(json!({
                "jsonrpc": "2.0",
                "id": self.next_id(),
                "method": "initialize",
                "params": { "protocolVersion": "2025-03-26", "clientInfo": { "name": "e2e-agent", "version": "1.0" } }
            }))
            .await;
        assert_eq!(init.status, StatusCode::OK);
        assert_eq!(init.json()["result"]["protocolVersion"], "2025-03-26");

        let notif = self
            .raw_post(json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .await;
        assert_eq!(notif.status, StatusCode::ACCEPTED);

        let ping = self
            .raw_post(json!({
                "jsonrpc": "2.0",
                "id": self.next_id(),
                "method": "ping"
            }))
            .await;
        assert_eq!(ping.status, StatusCode::OK);
        assert_eq!(ping.json()["result"], json!({}));
    }

    fn next_id(&self) -> u64 {
        self.rpc_seq.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) async fn raw_post(&self, payload: Value) -> WireResponse {
        send_http(
            self.addr,
            Method::POST,
            "/mcp",
            self.bearer.as_deref(),
            Some("application/json"),
            payload.to_string().into_bytes(),
        )
        .await
    }

    pub(crate) async fn call_tool(&self, name: &str, arguments: Value) -> (bool, Value) {
        let resp = self
            .raw_post(json!({
                "jsonrpc": "2.0",
                "id": self.next_id(),
                "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            }))
            .await;
        assert_eq!(
            resp.status,
            StatusCode::OK,
            "MCP HTTP transport for tool `{name}` failed: {}",
            String::from_utf8_lossy(&resp.body)
        );
        let body = resp.json();
        if let Some(err) = body.get("error") {
            return (true, err.clone());
        }
        let is_err = body["result"]["isError"].as_bool().unwrap_or(false);
        let text = body["result"]["content"][0]["text"]
            .as_str()
            .expect("MCP tool text content");
        let parsed: Value = serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("invalid JSON inside MCP tool text `{text}`: {e}"));
        (is_err, parsed)
    }

    pub(crate) async fn call_ok(&self, name: &str, arguments: Value) -> Value {
        let (is_err, val) = self.call_tool(name, arguments).await;
        assert!(
            !is_err,
            "expected MCP tool `{name}` to succeed, got error payload: {val}"
        );
        val
    }

    pub(crate) async fn call_err(
        &self,
        name: &str,
        arguments: Value,
        expected_status: u64,
        expected_code: &str,
    ) -> Value {
        let (is_err, val) = self.call_tool(name, arguments).await;
        assert!(
            is_err,
            "expected MCP tool `{name}` to fail ({expected_status} {expected_code}), but succeeded: {val}"
        );
        assert_eq!(
            val["status"].as_u64(),
            Some(expected_status),
            "unexpected status for `{name}`: {val}"
        );
        assert_eq!(
            val["code"].as_str(),
            Some(expected_code),
            "unexpected code for `{name}`: {val}"
        );
        val
    }
}

pub(crate) struct HumanClient {
    pub(crate) addr: SocketAddr,
    bearer: Option<String>,
}

impl HumanClient {
    pub(crate) fn new(addr: SocketAddr, bearer: Option<String>) -> Self {
        Self { addr, bearer }
    }

    pub(crate) async fn workspace(&self) -> Value {
        let r = send_http(
            self.addr,
            Method::GET,
            "/v1/workspace",
            self.bearer.as_deref(),
            None,
            Vec::new(),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK);
        r.json()
    }

    pub(crate) async fn post_command(&self, command_id: Option<&str>, command: Value) -> WireResponse {
        let payload = match command_id {
            Some(cid) => json!({ "command_id": cid, "command": command }),
            None => json!({ "command": command }),
        };
        send_http(
            self.addr,
            Method::POST,
            "/v1/commands",
            self.bearer.as_deref(),
            Some("application/json"),
            payload.to_string().into_bytes(),
        )
        .await
    }

    pub(crate) async fn metrics(&self) -> Value {
        let r = send_http(
            self.addr,
            Method::GET,
            "/v1/metrics",
            None,
            None,
            Vec::new(),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK);
        r.json()
    }
}

