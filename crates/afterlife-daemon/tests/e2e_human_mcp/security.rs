use hyper::{Method, StatusCode};
use serde_json::{Value, json};

use crate::harness::{LiveServer, McpClient, TempDataDir, oauth_register_and_login, send_http};

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

    let mcp = McpClient::new(server.addr, None);
    mcp.handshake().await;
    let created = mcp
        .call_ok("jobs_create", json!({ "title": "Legit Agent Job" }))
        .await;
    let gate_job_id = created["events"][0]["job_id"].as_str().unwrap().to_string();
    mcp.call_ok(
        "jobs_assign",
        json!({ "job_id": gate_job_id, "district_id": "DIST-001" }),
    )
    .await;
    loop {
        let ctx = mcp
            .call_ok("job_get_context", json!({ "job_id": gate_job_id }))
            .await;
        let is_human_gate = ctx["stage"]["kind"] == "approval_gate"
            || ctx["stage"]["gate_rubric"]["requires_human_approval"] == true;
        if is_human_gate {
            break;
        }
        let claim = mcp
            .call_ok(
                "jobs_claim",
                json!({ "job_id": gate_job_id, "runner_label": "Desktop Test Agent", "ttl_ms": 60_000 }),
            )
            .await;
        let token = claim["events"][0]["fencing_token"].as_u64().unwrap();
        let mut artifact_ids: Vec<String> = Vec::new();
        if let Some(kind) = ctx["required_artifact_kinds"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
        {
            let attached = mcp
                .call_ok(
                    "artifacts_attach",
                    json!({
                        "job_id": gate_job_id,
                        "kind": kind,
                        "label": "desktop-evidence.json",
                        "uri": "https://example.com/desktop-evidence.json",
                        "fencing_token": token
                    }),
                )
                .await;
            artifact_ids.push(
                attached["events"][0]["artifact_id"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            );
        }
        let mut fields = serde_json::Map::new();
        for f in ctx["required_step_fields"].as_array().unwrap() {
            let key = f["key"].as_str().unwrap();
            fields.insert(key.to_string(), json!(format!("Recorded {key}")));
        }
        let (is_err, _) = mcp
            .call_tool(
                "jobs_advance",
                json!({
                    "job_id": gate_job_id,
                    "fields": fields,
                    "artifact_ids": artifact_ids,
                    "fencing_token": token
                }),
            )
            .await;
        if is_err {
            let checks_arr = ctx["stage"]["gate_rubric"]["checks"].as_array().unwrap();
            let checklist: Vec<Value> = checks_arr
                .iter()
                .map(|c| json!([c.as_str().unwrap(), true]))
                .collect();
            mcp.call_ok(
                "gate_verify_record",
                json!({
                    "job_id": gate_job_id,
                    "verdict": "pass",
                    "summary": "Stage exit verified",
                    "checklist_results": checklist,
                    "fencing_token": token
                }),
            )
            .await;
            mcp.call_ok(
                "jobs_advance",
                json!({
                    "job_id": gate_job_id,
                    "fields": fields,
                    "artifact_ids": artifact_ids,
                    "fencing_token": token
                }),
            )
            .await;
        }
    }

    let spoof_approve = send_http(
        server.addr,
        Method::POST,
        "/v1/commands",
        None,
        Some("application/json"),
        json!({
            "command": {
                "cmd": "approve_job",
                "job_id": gate_job_id,
                "note": null,
                "expected_revision": null,
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
                "job_id": gate_job_id,
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

    mcp.call_err(
        "jobs_advance",
        json!({ "job_id": gate_job_id }),
        403,
        "human_approval_required",
    )
    .await;

    server.shutdown().await;
}
