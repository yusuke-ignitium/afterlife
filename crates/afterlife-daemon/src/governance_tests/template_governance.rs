use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use afterlife_core::{EngineCommand, ProposalId, SubMachineTemplateId};
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
            "afterlife_tmpl_gov_{tag}_{}_{seq}",
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
            "client_name": "AgentTemplateTest",
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

#[tokio::test]
async fn mcp_template_proposal_and_human_only_approval_and_sync() {
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

    let (st, list_res) = http_json(
        addr,
        Method::POST,
        "/mcp",
        None,
        Some(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "templates_list", "arguments": {} }
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let listed: Value =
        serde_json::from_str(list_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(listed["template_order"].as_array().unwrap().len(), 7);
    let first_stages = listed["templates"][0]["stages"].clone();

    let (st, prop_res) = http_json(
        addr,
        Method::POST,
        "/mcp",
        None,
        Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "template_propose",
                "arguments": {
                    "template_id": "tmpl-evidence",
                    "name": "Evidence & Probe",
                    "description": "Evolved via MCP proposal",
                    "philosophy_notes": "Verify before assuming.",
                    "stages": first_stages,
                    "rationale": "Refine wording from execution feedback"
                }
            }
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let prop_payload: Value =
        serde_json::from_str(prop_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let prop_id = prop_payload["events"][0]["proposal_id"]
        .as_str()
        .unwrap()
        .to_string();

    let agent_token = issue_agent_token(addr).await;

    let (st_agent_approve, _) = http_json(
        addr,
        Method::POST,
        "/v1/commands",
        Some(&agent_token),
        Some(json!({
            "command": EngineCommand::ApproveTemplateProposal {
                proposal_id: ProposalId::from(prop_id.as_str()),
                at: 1_900_000_000_000
            }
        })),
    )
    .await;
    assert_eq!(st_agent_approve, StatusCode::FORBIDDEN);

    let (st_agent_sync, _) = http_json(
        addr,
        Method::POST,
        "/v1/commands",
        Some(&agent_token),
        Some(json!({
            "command": EngineCommand::SyncBuiltinTemplates {
                template_ids: None,
                at: 1_900_000_000_001
            }
        })),
    )
    .await;
    assert_eq!(st_agent_sync, StatusCode::FORBIDDEN);

    let (st_human_approve, _) = http_json(
        addr,
        Method::POST,
        "/v1/commands",
        None,
        Some(json!({
            "command": EngineCommand::ApproveTemplateProposal {
                proposal_id: ProposalId::from(prop_id.as_str()),
                at: 1_900_000_000_002
            }
        })),
    )
    .await;
    assert_eq!(st_human_approve, StatusCode::OK);

    let (st_human_sync, _) = http_json(
        addr,
        Method::POST,
        "/v1/commands",
        None,
        Some(json!({
            "command": EngineCommand::SyncBuiltinTemplates {
                template_ids: Some(vec![SubMachineTemplateId::from("tmpl-evidence")]),
                at: 1_900_000_000_003
            }
        })),
    )
    .await;
    assert_eq!(st_human_sync, StatusCode::OK);

    let _ = tx.send(());
    let _ = join.await;
}
