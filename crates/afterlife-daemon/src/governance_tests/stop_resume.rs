use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use afterlife_core::{EngineCommand, JobId};
use crate::{AppDaemonState, build_router};
use afterlife_protocol::AfterlifeConfig;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, StatusCode, body::Bytes, header};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

static NEXT_DIR_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let seq = NEXT_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "afterlife_stop_e2e_{tag}_{}_{seq}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn send_raw(
    addr: SocketAddr,
    method: Method,
    path: &str,
    bearer: Option<&str>,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> (StatusCode, hyper::HeaderMap, Bytes) {
    let stream = TcpStream::connect(addr).await.unwrap();
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
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
    let resp = sender
        .send_request(req.body(Full::new(Bytes::from(body))).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes)
}

async fn http_json(
    addr: SocketAddr,
    method: Method,
    path: &str,
    bearer: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (ct, bytes) = match body {
        Some(v) => (Some("application/json"), serde_json::to_vec(&v).unwrap()),
        None => (None, Vec::new()),
    };
    let (st, _, raw) = send_raw(addr, method, path, bearer, ct, bytes).await;
    let parsed = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
    (st, parsed)
}

async fn issue_agent_token(addr: SocketAddr) -> String {
    let (st, reg) = http_json(
        addr,
        Method::POST,
        "/oauth/register",
        None,
        Some(json!({
            "client_name": "AgentStopTest",
            "redirect_uris": ["http://127.0.0.1:9999/cb"],
            "is_agent_client": true
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let client_id = reg["client_id"].as_str().unwrap().to_string();
    let verifier = format!("pkce-verifier-{client_id}-deterministic-secret-0123456789");
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());
    let auth_path = format!(
        "/oauth/authorize?client_id={client_id}&redirect_uri=http://127.0.0.1:9999/cb&response_type=code&code_challenge={challenge}&code_challenge_method=S256&scope=jobs:read%20jobs:work%20jobs:create&state=e2e&auto_approve=1"
    );
    let (st, headers, _) = send_raw(addr, Method::GET, &auth_path, None, None, Vec::new()).await;
    assert!(st == StatusCode::SEE_OTHER || st == StatusCode::TEMPORARY_REDIRECT);
    let loc = headers.get(header::LOCATION).unwrap().to_str().unwrap();
    let code = loc
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();
    let form = format!(
        "grant_type=authorization_code&code={code}&redirect_uri=http://127.0.0.1:9999/cb&client_id={client_id}&code_verifier={verifier}"
    );
    let (st, _, raw) = send_raw(
        addr,
        Method::POST,
        "/oauth/token",
        None,
        Some("application/x-www-form-urlencoded"),
        form.into_bytes(),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let tok: Value = serde_json::from_slice(&raw).unwrap();
    tok["access_token"].as_str().unwrap().to_string()
}

async fn mcp_call(addr: SocketAddr, name: &str, args: Value) -> (bool, Value) {
    let (st, res) = http_json(
        addr,
        Method::POST,
        "/mcp",
        None,
        Some(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": name, "arguments": args }
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let is_err = res["result"]["isError"].as_bool().unwrap_or(false);
    let payload: Value =
        serde_json::from_str(res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    (is_err, payload)
}

#[tokio::test]
async fn stop_and_resume_enforce_auth_and_expose_state_over_mcp() {
    let dir = TempDir::new("e2e");
    let cfg = AfterlifeConfig::local_default(dir.path().to_str().unwrap());
    let state = AppDaemonState::from_config(cfg).unwrap();
    let app = build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });

    let (err, created) = mcp_call(addr, "jobs_create", json!({ "title": "Stop E2E" })).await;
    assert!(!err);
    let job_id = created["events"][0]["job_id"].as_str().unwrap().to_string();
    let (err, _) = mcp_call(
        addr,
        "jobs_assign",
        json!({ "job_id": job_id, "district_id": "DIST-001" }),
    )
    .await;
    assert!(!err);
    let (err, _) = mcp_call(addr, "jobs_claim", json!({ "job_id": job_id })).await;
    assert!(!err);

    let agent_token = issue_agent_token(addr).await;

    let (st_agent_stop, _) = http_json(
        addr,
        Method::POST,
        "/v1/commands",
        Some(&agent_token),
        Some(json!({
            "command": EngineCommand::StopJob {
                job_id: JobId::from(job_id.as_str()),
                reason: None,
                expected_revision: None,
                at: 1_900_000_000_000
            }
        })),
    )
    .await;
    assert_eq!(st_agent_stop, StatusCode::FORBIDDEN);

    let (st_human_stop, _) = http_json(
        addr,
        Method::POST,
        "/v1/commands",
        None,
        Some(json!({
            "command": EngineCommand::StopJob {
                job_id: JobId::from(job_id.as_str()),
                reason: Some("Operator hold".into()),
                expected_revision: None,
                at: 1_900_000_000_001
            }
        })),
    )
    .await;
    assert_eq!(st_human_stop, StatusCode::OK);

    let (err, ctx) = mcp_call(addr, "job_get_context", json!({ "job_id": job_id })).await;
    assert!(!err);
    assert_eq!(ctx["stopped"], true);

    let (err, list) = mcp_call(addr, "jobs_list", json!({})).await;
    assert!(!err);
    let listed_job = list["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"] == job_id)
        .unwrap();
    assert_eq!(listed_job["stopped"], true);

    let (err, blocked_claim) = mcp_call(addr, "jobs_claim", json!({ "job_id": job_id })).await;
    assert!(err);
    assert_eq!(blocked_claim["code"], "job_stopped");

    let (st_agent_resume, _) = http_json(
        addr,
        Method::POST,
        "/v1/commands",
        Some(&agent_token),
        Some(json!({
            "command": EngineCommand::ResumeJob {
                job_id: JobId::from(job_id.as_str()),
                expected_revision: None,
                at: 1_900_000_000_002
            }
        })),
    )
    .await;
    assert_eq!(st_agent_resume, StatusCode::FORBIDDEN);

    let (st_human_resume, _) = http_json(
        addr,
        Method::POST,
        "/v1/commands",
        None,
        Some(json!({
            "command": EngineCommand::ResumeJob {
                job_id: JobId::from(job_id.as_str()),
                expected_revision: None,
                at: 1_900_000_000_003
            }
        })),
    )
    .await;
    assert_eq!(st_human_resume, StatusCode::OK);

    let (err, reclaimed) = mcp_call(addr, "jobs_claim", json!({ "job_id": job_id })).await;
    assert!(!err, "claim after resume should succeed: {reclaimed}");

    let _ = tx.send(());
    let _ = join.await;
}
