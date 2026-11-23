use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use afterlife_core::{EngineCommand, ProposalId};
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

use crate::{AppDaemonState, build_router};

static NEXT_DIR_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let seq = NEXT_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "afterlife_dist_gov_{tag}_{}_{seq}",
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

struct TestServer {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    async fn start(data_dir: &Path) -> Self {
        let cfg = AfterlifeConfig::local_default(data_dir.to_str().unwrap());
        let state = AppDaemonState::from_config(cfg).unwrap();
        let app = build_router(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });
        Self {
            addr,
            shutdown: Some(tx),
            handle: Some(handle),
        }
    }

    async fn send_raw(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> (StatusCode, hyper::HeaderMap, Vec<u8>) {
        let stream = TcpStream::connect(self.addr).await.unwrap();
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, self.addr.to_string());
        if let Some(tok) = bearer {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {tok}"));
        }
        if let Some(ct) = content_type {
            builder = builder.header(header::CONTENT_TYPE, ct);
        }
        let req = builder.body(Full::new(Bytes::from(body))).unwrap();
        let resp = sender.send_request(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, headers, bytes)
    }

    async fn http_json(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (ct, payload) = match body {
            Some(b) => (Some("application/json"), serde_json::to_vec(&b).unwrap()),
            None => (None, Vec::new()),
        };
        let (status, _, bytes) = self.send_raw(method, path, bearer, ct, payload).await;
        let val = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
        (status, val)
    }

    async fn issue_agent_token(&self) -> String {
        self.issue_scoped_agent_token("jobs:read%20jobs:work%20jobs:create")
            .await
    }

    async fn issue_scoped_agent_token(&self, encoded_scope: &str) -> String {
        let (st, reg) = self
            .http_json(
                Method::POST,
                "/oauth/register",
                None,
                Some(json!({
                    "client_name": "District Agent",
                    "redirect_uris": ["http://127.0.0.1:8787/cb"],
                    "is_agent_client": true
                })),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED);
        let client_id = reg["client_id"].as_str().unwrap();
        let verifier = "district-pkce-verifier-9999999999999999999999999999999";
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let auth_path = format!(
            "/oauth/authorize?client_id={client_id}&redirect_uri=http://127.0.0.1:8787/cb&response_type=code&code_challenge={challenge}&code_challenge_method=S256&scope={encoded_scope}&state=e2e&auto_approve=1"
        );
        let (st, headers, _) = self
            .send_raw(Method::GET, &auth_path, None, None, Vec::new())
            .await;
        assert!(st == StatusCode::SEE_OTHER || st == StatusCode::TEMPORARY_REDIRECT);
        let loc = headers.get(header::LOCATION).unwrap().to_str().unwrap();
        let code = loc.split("code=").nth(1).unwrap().split('&').next().unwrap();
        let form = format!(
            "grant_type=authorization_code&code={code}&redirect_uri=http://127.0.0.1:8787/cb&client_id={client_id}&code_verifier={verifier}"
        );
        let (st, _, raw) = self
            .send_raw(
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

    async fn mcp_call(&self, bearer: &str, tool: &str, args: Value) -> (bool, Value) {
        let (st, rpc) = self
            .http_json(
                Method::POST,
                "/mcp",
                Some(bearer),
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": { "name": tool, "arguments": args }
                })),
            )
            .await;
        assert_eq!(st, StatusCode::OK);
        let is_error = rpc["result"]["isError"].as_bool().unwrap_or(false);
        let text = rpc["result"]["content"][0]["text"].as_str().unwrap_or("{}");
        let parsed: Value = serde_json::from_str(text).unwrap_or_else(|_| json!({ "raw": text }));
        (!is_error, parsed)
    }

    async fn human_cmd(&self, cmd_id: &str, command: EngineCommand) -> (StatusCode, Value) {
        self.http_json(
            Method::POST,
            "/v1/commands",
            None,
            Some(json!({
                "command_id": cmd_id,
                "command": command
            })),
        )
        .await
    }

    async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

#[tokio::test]
async fn mcp_agent_can_propose_district_lifecycle_but_only_human_can_approve_and_list_filters_archived() {
    let dir = TempDir::new("mcp_dist");
    let srv = TestServer::start(dir.path()).await;
    let agent_tok = srv.issue_agent_token().await;

    let (ok, prop_res) = srv
        .mcp_call(
            &agent_tok,
            "districts_propose_create",
            json!({
                "name": "DISTRICT 06 - MCP Lane",
                "summary": "MCP governed lane",
                "principles": [{"id": "P-M1", "title": "Safe", "statement": "Stay safe.", "rationale": "Safety."}],
                "stages": [
                    {
                        "id": "work",
                        "label": "Work",
                        "runner": "AI Agent",
                        "runner_kind": "Ai",
                        "kind": "Work",
                        "procedure": "Execute.",
                        "gate_rubric": null,
                        "sub_template_id": null,
                        "next": ["approve"],
                        "rework_to": null
                    },
                    {
                        "id": "approve",
                        "label": "Approve",
                        "runner": "Human",
                        "runner_kind": "Human",
                        "kind": "ApprovalGate",
                        "procedure": "Approve.",
                        "gate_rubric": {"checks": ["OK"], "auto_verify_by_llm": false, "requires_human_approval": true},
                        "sub_template_id": null,
                        "next": ["done"],
                        "rework_to": "work"
                    },
                    {
                        "id": "done",
                        "label": "Done",
                        "runner": "System",
                        "runner_kind": "Human",
                        "kind": "Terminal",
                        "procedure": "Done.",
                        "gate_rubric": null,
                        "sub_template_id": null,
                        "next": [],
                        "rework_to": null
                    }
                ],
                "rationale": "Create lane via MCP"
            }),
        )
        .await;
    assert!(ok, "propose_create failed: {prop_res}");
    let create_pid = prop_res["events"][0]["proposal_id"].as_str().unwrap().to_string();

    let (ok_approve, err_approve) = srv
        .mcp_call(
            &agent_tok,
            "districts_approve_proposal",
            json!({ "proposal_id": create_pid }),
        )
        .await;
    assert!(!ok_approve, "agent must NOT approve district proposal: {err_approve}");

    let (st, _) = srv
        .human_cmd(
            "h-dist-1",
            EngineCommand::ApproveDistrictProposal {
                proposal_id: ProposalId::from(create_pid.as_str()),
                at: 1_900_000_100_000,
            },
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    let (ok, list_active) = srv.mcp_call(&agent_tok, "districts_list", json!({})).await;
    assert!(ok);
    assert!(list_active["districts"].as_array().unwrap().iter().any(|d| d["id"] == "DIST-006"));

    let (ok, arch_prop) = srv
        .mcp_call(
            &agent_tok,
            "districts_propose_archive",
            json!({ "district_id": "DIST-006", "rationale": "Archive via MCP" }),
        )
        .await;
    assert!(ok, "propose_archive failed: {arch_prop}");
    let arch_pid = arch_prop["events"][0]["proposal_id"].as_str().unwrap().to_string();

    let (ok_arch_agent, _) = srv
        .mcp_call(
            &agent_tok,
            "districts_approve_proposal",
            json!({ "proposal_id": arch_pid }),
        )
        .await;
    assert!(!ok_arch_agent);

    let (st, _) = srv
        .human_cmd(
            "h-dist-2",
            EngineCommand::ApproveDistrictProposal {
                proposal_id: ProposalId::from(arch_pid.as_str()),
                at: 1_900_000_101_000,
            },
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    let (ok, list_default) = srv.mcp_call(&agent_tok, "districts_list", json!({})).await;
    assert!(ok);
    assert!(list_default["districts"].as_array().unwrap().iter().all(|d| d["id"] != "DIST-006"));

    let (ok, list_all) = srv
        .mcp_call(&agent_tok, "districts_list", json!({ "include_archived": true }))
        .await;
    assert!(ok);
    let d6 = list_all["districts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == "DIST-006")
        .expect("DIST-006 must appear when include_archived=true");
    assert_eq!(d6["archived"], true);

    let (ok, res_prop) = srv
        .mcp_call(
            &agent_tok,
            "districts_propose_restore",
            json!({ "district_id": "DIST-006", "rationale": "Restore via MCP" }),
        )
        .await;
    assert!(ok, "propose_restore failed: {res_prop}");
    let res_pid = res_prop["events"][0]["proposal_id"].as_str().unwrap().to_string();

    let (st, _) = srv
        .human_cmd(
            "h-dist-3",
            EngineCommand::ApproveDistrictProposal {
                proposal_id: ProposalId::from(res_pid.as_str()),
                at: 1_900_000_102_000,
            },
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    let (ok, list_restored) = srv.mcp_call(&agent_tok, "districts_list", json!({})).await;
    assert!(ok);
    assert!(list_restored["districts"].as_array().unwrap().iter().any(|d| d["id"] == "DIST-006" && d["archived"] == false));

    let scoped_tok = srv
        .issue_scoped_agent_token("jobs:read%20jobs:work%20jobs:create%20district:DIST-001")
        .await;
    let create_payload = json!({
        "district_id": "DIST-009",
        "name": "Forbidden Scoped Lane",
        "summary": "Should be denied",
        "principles": [{"id": "P-S", "title": "T", "statement": "S", "rationale": "R"}],
        "stages": [
            {"id": "work", "label": "Work", "runner": "AI", "runner_kind": "Ai", "kind": "Work", "procedure": "P", "gate_rubric": null, "sub_template_id": null, "next": ["approve"], "rework_to": null},
            {"id": "approve", "label": "Approve", "runner": "Human", "runner_kind": "Human", "kind": "ApprovalGate", "procedure": "P", "gate_rubric": {"checks": ["OK"], "auto_verify_by_llm": false, "requires_human_approval": true}, "sub_template_id": null, "next": ["done"], "rework_to": "work"},
            {"id": "done", "label": "Done", "runner": "System", "runner_kind": "Human", "kind": "Terminal", "procedure": "P", "gate_rubric": null, "sub_template_id": null, "next": [], "rework_to": null}
        ],
        "rationale": "Attempt create from scoped token"
    });
    let (ok_sc_create, err_sc_create) = srv
        .mcp_call(&scoped_tok, "districts_propose_create", create_payload.clone())
        .await;
    assert!(!ok_sc_create, "district-scoped token must not propose create: {err_sc_create}");
    assert_eq!(err_sc_create["code"], "district_scope_denied");

    let mut auto_create_payload = create_payload.clone();
    auto_create_payload.as_object_mut().unwrap().remove("district_id");
    let (ok_sc_auto, err_sc_auto) = srv
        .mcp_call(&scoped_tok, "districts_propose_create", auto_create_payload)
        .await;
    assert!(!ok_sc_auto, "district-scoped token must not propose auto-id create: {err_sc_auto}");
    assert_eq!(err_sc_auto["code"], "district_scope_denied");

    let (ok_unscoped_create, _) = srv
        .mcp_call(&agent_tok, "districts_propose_create", create_payload)
        .await;
    assert!(ok_unscoped_create);
    let (ok_unscoped_arch6, _) = srv
        .mcp_call(&agent_tok, "districts_propose_archive", json!({ "district_id": "DIST-006", "rationale": "Archive D6" }))
        .await;
    assert!(ok_unscoped_arch6);
    let (ok_scoped_arch1, _) = srv
        .mcp_call(&scoped_tok, "districts_propose_archive", json!({ "district_id": "DIST-001", "rationale": "Archive D1" }))
        .await;
    assert!(ok_scoped_arch1);

    let (ok_sc_list, sc_list) = srv.mcp_call(&scoped_tok, "districts_list", json!({})).await;
    assert!(ok_sc_list);
    let visible_districts = sc_list["districts"].as_array().unwrap();
    assert_eq!(visible_districts.len(), 1);
    assert_eq!(visible_districts[0]["id"], "DIST-001");
    let visible_props = sc_list["pending_district_proposals"].as_array().unwrap();
    assert_eq!(visible_props.len(), 1, "scoped token leaked out-of-scope proposals: {visible_props:?}");
    assert_eq!(visible_props[0]["action"]["action"], "archive");
    assert_eq!(visible_props[0]["action"]["district_id"], "DIST-001");

    srv.stop().await;
}
