use std::{
    collections::HashSet,
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
    sync::{Barrier, oneshot},
};

static NEXT_DIR_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDataDir(PathBuf);

impl TempDataDir {
    fn new(tag: &str) -> Self {
        let seq = NEXT_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("afterlife_e2e_{tag}_{pid}_{seq}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create isolated e2e temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDataDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct LiveServer {
    addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl LiveServer {
    async fn start(data_dir: &Path) -> Self {
        let cfg = AfterlifeConfig::local_default(data_dir.to_str().unwrap());
        Self::start_with_config(cfg).await
    }

    async fn start_desktop_embedded(data_dir: &Path) -> Self {
        let cfg = AfterlifeConfig::desktop_embedded(data_dir.to_str().unwrap());
        Self::start_with_config(cfg).await
    }

    async fn start_with_config(mut cfg: AfterlifeConfig) -> Self {
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

    async fn shutdown(mut self) {
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

struct WireResponse {
    status: StatusCode,
    headers: hyper::HeaderMap,
    body: Bytes,
}

impl WireResponse {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| {
            panic!(
                "expected JSON response (status {}), parse error: {e}\nraw body: {}",
                self.status,
                String::from_utf8_lossy(&self.body)
            )
        })
    }
}

async fn send_http(
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

struct SseStream {
    body: Incoming,
    buf: String,
}

impl SseStream {
    async fn connect(addr: SocketAddr, since_seq: u64) -> Self {
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

    async fn next_event(&mut self) -> Value {
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

async fn oauth_register_and_login(
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
struct McpClient {
    addr: SocketAddr,
    bearer: Option<String>,
    rpc_seq: Arc<AtomicU64>,
}

impl McpClient {
    fn new(addr: SocketAddr, bearer: Option<String>) -> Self {
        Self {
            addr,
            bearer,
            rpc_seq: Arc::new(AtomicU64::new(1)),
        }
    }

    async fn handshake(&self) {
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

    async fn raw_post(&self, payload: Value) -> WireResponse {
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

    async fn call_tool(&self, name: &str, arguments: Value) -> (bool, Value) {
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

    async fn call_ok(&self, name: &str, arguments: Value) -> Value {
        let (is_err, val) = self.call_tool(name, arguments).await;
        assert!(
            !is_err,
            "expected MCP tool `{name}` to succeed, got error payload: {val}"
        );
        val
    }

    async fn call_err(
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

struct HumanClient {
    addr: SocketAddr,
    bearer: Option<String>,
}

impl HumanClient {
    fn new(addr: SocketAddr, bearer: Option<String>) -> Self {
        Self { addr, bearer }
    }

    async fn workspace(&self) -> Value {
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

    async fn post_command(&self, command_id: Option<&str>, command: Value) -> WireResponse {
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

    async fn metrics(&self) -> Value {
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

#[tokio::test]
async fn e2e_full_lifecycle_human_and_agent_collaboration_with_live_sse() {
    let tmp = TempDataDir::new("lifecycle");
    let server = LiveServer::start(tmp.path()).await;

    let (_, agent_token, _, granted_scope) = oauth_register_and_login(
        server.addr,
        "Claude Code Runner",
        true,
        "jobs:read jobs:create jobs:work approvals:decide districts:admin",
    )
    .await;
    assert!(!granted_scope.contains("approvals:decide"));
    assert!(!granted_scope.contains("districts:admin"));

    let agent = McpClient::new(server.addr, Some(agent_token));
    agent.handshake().await;

    let human = HumanClient::new(server.addr, None);
    let initial_seq = human.workspace().await["latest_seq"].as_u64().unwrap();
    let mut sse = SseStream::connect(server.addr, initial_seq).await;

    let created = agent
        .call_ok(
            "jobs_create",
            json!({
                "title": "Implement deterministic E2E harness",
                "raw_input": "Build zero-flake black-box Human x MCP E2E",
                "target_district_hint": "DIST-001"
            }),
        )
        .await;
    let job_id = created["events"][0]["job_id"].as_str().unwrap().to_string();

    let assigned = agent
        .call_ok(
            "jobs_assign",
            json!({ "job_id": job_id, "district_id": "DIST-001" }),
        )
        .await;
    assert!(assigned["events"][0]["fencing_token"].is_null());

    let ctx = agent
        .call_ok("job_get_context", json!({ "job_id": job_id }))
        .await;
    assert_eq!(ctx["district_id"], "DIST-001");
    assert_eq!(ctx["philosophy"]["version"], 1);
    assert_eq!(ctx["stage"]["id"], "design");

    loop {
        let ctx = agent
            .call_ok("job_get_context", json!({ "job_id": job_id }))
            .await;
        let stage_id = ctx["stage"]["id"].as_str().unwrap().to_string();
        let is_human_gate = ctx["stage"]["kind"] == "approval_gate"
            || ctx["stage"]["gate_rubric"]["requires_human_approval"] == true;
        if is_human_gate {
            break;
        }

        let claim = agent
            .call_ok(
                "jobs_claim",
                json!({ "job_id": job_id, "runner_label": "Claude Code", "ttl_ms": 60_000 }),
            )
            .await;
        let token = claim["events"][0]["fencing_token"].as_u64().unwrap();

        agent
            .call_ok(
                "artifacts_attach",
                json!({
                    "job_id": job_id,
                    "kind": "source_code",
                    "label": format!("{stage_id}-output.rs"),
                    "uri": format!("s3://afterlife/artifacts/{job_id}/{stage_id}.rs"),
                    "fencing_token": token
                }),
            )
            .await;

        let rubric = &ctx["stage"]["gate_rubric"];
        let is_sub_stage = !ctx["sub_stage"].is_null();
        let (is_err, adv_res) = agent
            .call_tool(
                "jobs_advance",
                json!({ "job_id": job_id, "fencing_token": token }),
            )
            .await;
        if is_err {
            assert_eq!(adv_res["status"], 422);
            assert_eq!(adv_res["code"], "gate_verification_required");
            assert!(!is_sub_stage || !rubric.is_null());

            let checks: Vec<Value> = rubric["checks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| json!([c.as_str().unwrap(), true]))
                .collect();
            agent
                .call_ok(
                    "gate_verify_record",
                    json!({
                        "job_id": job_id,
                        "verdict": "pass",
                        "summary": format!("Verified rubric for {stage_id}"),
                        "checklist_results": checks,
                        "fencing_token": token
                    }),
                )
                .await;
            agent
                .call_ok(
                    "jobs_advance",
                    json!({ "job_id": job_id, "fencing_token": token }),
                )
                .await;
        }
    }

    agent
        .call_err(
            "jobs_claim",
            json!({ "job_id": job_id }),
            403,
            "human_approval_required",
        )
        .await;
    agent
        .call_err(
            "jobs_assign",
            json!({ "job_id": job_id, "district_id": "DIST-002" }),
            403,
            "human_approval_required",
        )
        .await;
    agent
        .call_err(
            "jobs_advance",
            json!({ "job_id": job_id }),
            403,
            "human_approval_required",
        )
        .await;
    agent
        .call_err(
            "jobs_rework",
            json!({ "job_id": job_id }),
            403,
            "human_approval_required",
        )
        .await;
    agent
        .call_err(
            "gate_verify_record",
            json!({ "job_id": job_id, "verdict": "pass", "summary": "self-approved" }),
            403,
            "human_approval_required",
        )
        .await;
    agent
        .call_err(
            "feedback_submit",
            json!({
                "district_id": "DIST-001",
                "job_id": job_id,
                "kind": "approval_note",
                "body": "spoofed human approval"
            }),
            403,
            "human_approval_required",
        )
        .await;

    let approve = human
        .post_command(
            None,
            json!({
                "cmd": "advance_job",
                "job_id": job_id,
                "expected_revision": null,
                "fencing_token": null,
                "at": 2_000_000_000_000u64
            }),
        )
        .await;
    assert_eq!(approve.status, StatusCode::OK);
    let final_seq = approve.json()["seq_end"].as_u64().unwrap();

    let ws = human.workspace().await;
    let job_after = ws["engine"]["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"] == job_id)
        .unwrap();
    assert_eq!(job_after["location"]["state"], "completed");

    let mut last_seq = initial_seq;
    let mut saw_completed = false;
    while last_seq < final_seq {
        let ev = sse.next_event().await;
        let seq = ev["seq"].as_u64().unwrap();
        assert_eq!(
            seq,
            last_seq + 1,
            "SSE event seq must be strictly contiguous"
        );
        last_seq = seq;
        if ev["event"]["type"] == "job_transitioned"
            && ev["event"]["job_id"] == job_id
            && ev["event"]["to"]["state"] == "completed"
        {
            saw_completed = true;
        }
    }
    assert!(saw_completed, "SSE must observe final Completed transition");

    drop(sse);
    server.shutdown().await;
}

#[tokio::test]
async fn e2e_philosophy_and_spec_self_evolution_loop() {
    let tmp = TempDataDir::new("evolution");
    let server = LiveServer::start(tmp.path()).await;

    let (_, agent_token, _, _) = oauth_register_and_login(
        server.addr,
        "Frontier Reviewer",
        true,
        "jobs:read jobs:work",
    )
    .await;
    let agent = McpClient::new(server.addr, Some(agent_token.clone()));
    let human = HumanClient::new(server.addr, None);

    let fb = agent
        .call_ok(
            "feedback_submit",
            json!({
                "district_id": "DIST-001",
                "kind": "philosophy_insight",
                "body": "Always enforce Lease TTL upper bounds at domain policy layer."
            }),
        )
        .await;
    let fb_id = fb["events"][0]["feedback_id"].as_str().unwrap().to_string();

    let prop = agent
        .call_ok(
            "philosophy_propose",
            json!({
                "district_id": "DIST-001",
                "base_version": 1,
                "summary": "v2: Unified Policy + Bounded Leases",
                "principles": [{
                    "id": "P-01",
                    "title": "Single Policy Checkpoint",
                    "statement": "All invariants live in one policy layer.",
                    "rationale": "Prevents per-command bypasses like F1."
                }],
                "anti_patterns": ["Per-handler ad-hoc if guards"],
                "decision_heuristics": ["Fail closed on invalid TTL"],
                "rationale": "Learned from chaos report",
                "source_feedback_ids": [fb_id]
            }),
        )
        .await;
    let proposal_id = prop["events"][0]["proposal_id"]
        .as_str()
        .unwrap()
        .to_string();

    let dists_before = agent.call_ok("districts_list", json!({})).await;
    assert_eq!(dists_before["districts"][0]["active_philosophy_version"], 1);

    let agent_approve = send_http(
        server.addr,
        Method::POST,
        "/v1/commands",
        Some(&agent_token),
        Some("application/json"),
        json!({
            "command": {
                "cmd": "approve_philosophy_proposal",
                "proposal_id": proposal_id,
                "at": 2_000_000_000_000u64
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    assert_eq!(agent_approve.status, StatusCode::FORBIDDEN);

    let human_approve = human
        .post_command(
            None,
            json!({
                "cmd": "approve_philosophy_proposal",
                "proposal_id": proposal_id,
                "at": 2_000_000_001_000u64
            }),
        )
        .await;
    assert_eq!(human_approve.status, StatusCode::OK);

    let dists_after = agent.call_ok("districts_list", json!({})).await;
    assert_eq!(dists_after["districts"][0]["active_philosophy_version"], 2);

    agent
        .call_err(
            "philosophy_propose",
            json!({
                "district_id": "DIST-001",
                "base_version": 1,
                "summary": "Stale v2",
                "principles": [{ "id": "P-9", "title": "t", "statement": "s", "rationale": "r" }],
                "rationale": "stale"
            }),
            409,
            "philosophy_version_conflict",
        )
        .await;

    server.shutdown().await;
}

#[tokio::test]
async fn e2e_adversarial_chaos_invariants_f1_to_f9() {
    let tmp = TempDataDir::new("chaos");
    let server = LiveServer::start(tmp.path()).await;

    let agent = McpClient::new(server.addr, None);
    agent.handshake().await;

    let c = agent
        .call_ok("jobs_create", json!({ "title": "Chaos Target" }))
        .await;
    let job_id = c["events"][0]["job_id"].as_str().unwrap().to_string();
    agent
        .call_ok(
            "jobs_assign",
            json!({ "job_id": job_id, "district_id": "DIST-001" }),
        )
        .await;

    agent
        .call_err(
            "jobs_assign",
            json!({ "job_id": job_id, "district_id": "DIST-002" }),
            409,
            "job_already_in_district",
        )
        .await;

    for bad_ttl in [
        json!(9_007_199_254_740_991u64),
        json!(0),
        json!(-1),
        json!(-100_000),
    ] {
        agent
            .call_err(
                "jobs_claim",
                json!({ "job_id": job_id, "ttl_ms": bad_ttl }),
                422,
                "invalid_input",
            )
            .await;
    }

    let claim = agent
        .call_ok("jobs_claim", json!({ "job_id": job_id, "ttl_ms": 60_000 }))
        .await;
    let token = claim["events"][0]["fencing_token"].as_u64().unwrap();

    let hb_err = agent
        .call_err(
            "jobs_heartbeat",
            json!({ "job_id": job_id, "fencing_token": -5 }),
            422,
            "invalid_input",
        )
        .await;
    assert!(
        hb_err["error"].as_str().unwrap().contains("non-negative"),
        "expected clear negative-integer message, got {hb_err}"
    );

    agent
        .call_err(
            "jobs_create",
            json!({ "command_id": "", "title": "Empty Key" }),
            422,
            "invalid_input",
        )
        .await;
    agent
        .call_ok(
            "jobs_create",
            json!({ "command_id": "idem-f5", "title": "Original Payload" }),
        )
        .await;
    let replay = agent
        .call_ok(
            "jobs_create",
            json!({ "command_id": "idem-f5", "title": "Original Payload" }),
        )
        .await;
    assert_eq!(replay["idempotent_replay"], true);
    agent
        .call_err(
            "jobs_create",
            json!({ "command_id": "idem-f5", "title": "Tampered Payload" }),
            409,
            "idempotency_conflict",
        )
        .await;

    agent
        .call_err(
            "jobs_create",
            json!({ "title": "   " }),
            422,
            "invalid_input",
        )
        .await;
    agent
        .call_err(
            "jobs_create",
            json!({ "title": "Bad hint", "target_district_hint": "DIST-999" }),
            404,
            "district_not_found",
        )
        .await;
    agent
        .call_err(
            "artifacts_attach",
            json!({
                "job_id": job_id,
                "kind": "spec",
                "label": "xss",
                "uri": "javascript:alert(1)",
                "fencing_token": token
            }),
            422,
            "invalid_input",
        )
        .await;
    agent
        .call_err(
            "artifacts_attach",
            json!({
                "job_id": job_id,
                "kind": "spec",
                "label": "bad-hash",
                "uri": "https://example.com/a",
                "sha256": "not-a-sha256",
                "fencing_token": token
            }),
            422,
            "invalid_input",
        )
        .await;
    agent
        .call_err(
            "feedback_submit",
            json!({ "district_id": "DIST-001", "kind": "procedure_fix", "body": "   " }),
            422,
            "invalid_input",
        )
        .await;
    agent
        .call_err(
            "philosophy_propose",
            json!({
                "district_id": "DIST-001",
                "base_version": 1,
                "summary": "Bad principles",
                "principles": [null, 123],
                "rationale": "chaos"
            }),
            422,
            "invalid_input",
        )
        .await;
    agent
        .call_err(
            "philosophy_propose",
            json!({
                "district_id": "DIST-001",
                "base_version": 1,
                "summary": "Dangling feedback",
                "principles": [{ "id": "P-1", "title": "t", "statement": "s", "rationale": "r" }],
                "rationale": "chaos",
                "source_feedback_ids": ["FB-9999"]
            }),
            422,
            "invalid_input",
        )
        .await;

    agent
        .call_err("jobs_list", json!({ "limit": 999 }), 422, "invalid_input")
        .await;
    let page1 = agent
        .call_ok("jobs_list", json!({ "limit": 3, "offset": 0 }))
        .await;
    assert_eq!(page1["jobs"].as_array().unwrap().len(), 3);
    assert!(page1["total_count"].as_u64().unwrap() > 3);
    assert_eq!(page1["has_more"], true);

    server.shutdown().await;
}

#[tokio::test]
async fn e2e_high_concurrency_barrier_storm_zero_corruption() {
    let tmp = TempDataDir::new("storm");
    let server = LiveServer::start(tmp.path()).await;
    let agent = McpClient::new(server.addr, None);
    let human = HumanClient::new(server.addr, None);

    let created = agent
        .call_ok("jobs_create", json!({ "title": "Contended Job" }))
        .await;
    let job_id = created["events"][0]["job_id"].as_str().unwrap().to_string();

    let claim_n = 24usize;
    let barrier = Arc::new(Barrier::new(claim_n));
    let mut handles = Vec::with_capacity(claim_n);
    for idx in 0..claim_n {
        let cli = agent.clone();
        let b = barrier.clone();
        let jid = job_id.clone();
        handles.push(tokio::spawn(async move {
            b.wait().await;
            cli.call_tool(
                "jobs_claim",
                json!({ "job_id": jid, "runner_label": format!("Runner-{idx}"), "ttl_ms": 60_000 }),
            )
            .await
        }));
    }
    let mut winners = 0usize;
    let mut conflicts = 0usize;
    for h in handles {
        let (is_err, val) = h.await.unwrap();
        if !is_err {
            winners += 1;
        } else {
            assert_eq!(val["status"], 409);
            assert_eq!(val["code"], "already_leased");
            conflicts += 1;
        }
    }
    assert_eq!(
        winners, 1,
        "strictly 1 runner may win the 24-way claim race"
    );
    assert_eq!(conflicts, claim_n - 1);

    let idem_n = 24usize;
    let barrier = Arc::new(Barrier::new(idem_n));
    let mut handles = Vec::with_capacity(idem_n);
    for _ in 0..idem_n {
        let cli = agent.clone();
        let b = barrier.clone();
        handles.push(tokio::spawn(async move {
            b.wait().await;
            cli.call_ok(
                "jobs_create",
                json!({ "command_id": "storm-idem-key", "title": "Exactly Once Job" }),
            )
            .await
        }));
    }
    let mut created_ids = HashSet::new();
    let mut replay_count = 0usize;
    for h in handles {
        let val = h.await.unwrap();
        created_ids.insert(val["events"][0]["job_id"].as_str().unwrap().to_string());
        if val["idempotent_replay"] == true {
            replay_count += 1;
        }
    }
    assert_eq!(created_ids.len(), 1);
    assert_eq!(replay_count, idem_n - 1);

    let create_n = 32usize;
    let barrier = Arc::new(Barrier::new(create_n));
    let mut handles = Vec::with_capacity(create_n);
    for idx in 0..create_n {
        let cli = agent.clone();
        let b = barrier.clone();
        handles.push(tokio::spawn(async move {
            b.wait().await;
            cli.call_ok(
                "jobs_create",
                json!({ "title": format!("Parallel Storm Job #{idx}") }),
            )
            .await
        }));
    }
    let mut unique_ids = HashSet::new();
    for h in handles {
        let val = h.await.unwrap();
        unique_ids.insert(val["events"][0]["job_id"].as_str().unwrap().to_string());
    }
    assert_eq!(unique_ids.len(), create_n);

    let metrics = human.metrics().await;
    assert_eq!(metrics["status_counts"]["5xx"], 0);
    assert_eq!(
        metrics["mcp_tools"]["jobs_claim"]["conflicts"],
        (claim_n - 1) as u64
    );

    server.shutdown().await;
}

#[tokio::test]
async fn e2e_persistence_across_server_restart_and_oauth_revocation() {
    let tmp = TempDataDir::new("restart");

    let server1 = LiveServer::start(tmp.path()).await;
    let (client_id, access1, refresh1, _) = oauth_register_and_login(
        server1.addr,
        "Codex Runner",
        true,
        "jobs:read jobs:create jobs:work",
    )
    .await;
    let mcp1 = McpClient::new(server1.addr, Some(access1));
    let created = mcp1
        .call_ok(
            "jobs_create",
            json!({ "command_id": "survive-restart-1", "title": "Durable Job Across Restart" }),
        )
        .await;
    let durable_job_id = created["events"][0]["job_id"].as_str().unwrap().to_string();

    server1.shutdown().await;

    let server2 = LiveServer::start(tmp.path()).await;

    let refresh_resp = send_http(
        server2.addr,
        Method::POST,
        "/oauth/token",
        None,
        Some("application/x-www-form-urlencoded"),
        format!("grant_type=refresh_token&client_id={client_id}&refresh_token={refresh1}")
            .into_bytes(),
    )
    .await;
    assert_eq!(refresh_resp.status, StatusCode::OK);
    let access2 = refresh_resp.json()["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    let reuse_resp = send_http(
        server2.addr,
        Method::POST,
        "/oauth/token",
        None,
        Some("application/x-www-form-urlencoded"),
        format!("grant_type=refresh_token&client_id={client_id}&refresh_token={refresh1}")
            .into_bytes(),
    )
    .await;
    assert_eq!(reuse_resp.status, StatusCode::BAD_REQUEST);

    let mcp2 = McpClient::new(server2.addr, Some(access2.clone()));
    let replay = mcp2
        .call_ok(
            "jobs_create",
            json!({ "command_id": "survive-restart-1", "title": "Durable Job Across Restart" }),
        )
        .await;
    assert_eq!(replay["idempotent_replay"], true);
    assert_eq!(replay["events"][0]["job_id"], durable_job_id);

    let rev = send_http(
        server2.addr,
        Method::POST,
        "/oauth/revoke",
        Some(&access2),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(rev.status, StatusCode::OK);

    let after_rev = mcp2
        .raw_post(json!({ "jsonrpc": "2.0", "id": 99, "method": "tools/list" }))
        .await;
    assert_eq!(after_rev.status, StatusCode::UNAUTHORIZED);

    server2.shutdown().await;
}

#[tokio::test]
async fn e2e_desktop_embedded_fail_closed_blocks_http_human_impersonation() {
    let tmp = TempDataDir::new("desktop_fail_closed");
    let server = LiveServer::start_desktop_embedded(tmp.path()).await;

    let health = send_http(
        server.addr,
        Method::GET,
        "/v1/health",
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(health.status, StatusCode::OK);
    assert_eq!(health.json()["human_http_policy"], "desktop_only");

    let spoof_approve = send_http(
        server.addr,
        Method::POST,
        "/v1/commands",
        None,
        Some("application/json"),
        json!({
            "command": {
                "cmd": "advance_job",
                "job_id": "JOB-041",
                "expected_revision": null,
                "fencing_token": null,
                "at": 2_000_000_000_000u64
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    assert_eq!(spoof_approve.status, StatusCode::UNAUTHORIZED);
    assert_eq!(spoof_approve.json()["error"], "human_http_disabled");

    let spoof_reset = send_http(
        server.addr,
        Method::POST,
        "/v1/metrics/reset",
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(spoof_reset.status, StatusCode::UNAUTHORIZED);

    let spoof_dcr = send_http(
        server.addr,
        Method::POST,
        "/oauth/register",
        None,
        Some("application/json"),
        json!({
            "client_name": "Rogue Human Pretender",
            "redirect_uris": ["http://127.0.0.1:9999/cb"],
            "is_agent_client": false
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    assert_eq!(spoof_dcr.status, StatusCode::FORBIDDEN);
    assert_eq!(
        spoof_dcr.json()["error"],
        "human_client_registration_disabled"
    );

    let (_, agent_token, _, granted_scope) = oauth_register_and_login(
        server.addr,
        "Grok Bot Local",
        true,
        "jobs:read jobs:create jobs:work approvals:decide districts:admin",
    )
    .await;
    assert!(!granted_scope.contains("approvals:decide"));
    assert!(!granted_scope.contains("districts:admin"));

    let rest_gate_attempt = send_http(
        server.addr,
        Method::POST,
        "/v1/commands",
        Some(&agent_token),
        Some("application/json"),
        json!({
            "command": {
                "cmd": "advance_job",
                "job_id": "JOB-041",
                "expected_revision": null,
                "fencing_token": null,
                "at": 2_000_000_000_000u64
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    assert_eq!(rest_gate_attempt.status, StatusCode::FORBIDDEN);
    assert_eq!(rest_gate_attempt.json()["code"], "human_approval_required");

    let ws = send_http(
        server.addr,
        Method::GET,
        "/v1/workspace",
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(ws.status, StatusCode::OK);

    let mcp = McpClient::new(server.addr, None);
    mcp.handshake().await;
    let created = mcp
        .call_ok("jobs_create", json!({ "title": "Legit Agent Job" }))
        .await;
    assert!(created["events"][0]["job_id"].as_str().is_some());
    mcp.call_err(
        "jobs_advance",
        json!({ "job_id": "JOB-041" }),
        403,
        "human_approval_required",
    )
    .await;

    server.shutdown().await;
}
