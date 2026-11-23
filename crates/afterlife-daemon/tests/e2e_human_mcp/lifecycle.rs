use hyper::{Method, StatusCode};
use serde_json::{Value, json};

use crate::harness::{
    HumanClient, LiveServer, McpClient, SseStream, TempDataDir, oauth_register_and_login,
    send_http,
};

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
    assert_eq!(ctx["stage"]["id"], "discover");

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

        let art_kind = ctx["required_artifact_kinds"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or("source_code");
        let payload_text = format!("{{\"job\":\"{job_id}\",\"stage\":\"{stage_id}\"}}");
        let attached = agent
            .call_ok(
                "artifacts_attach",
                json!({
                    "job_id": job_id,
                    "kind": art_kind,
                    "label": format!("{stage_id}-output.json"),
                    "content": payload_text,
                    "fencing_token": token
                }),
            )
            .await;
        let art_id = attached["events"][0]["artifact_id"]
            .as_str()
            .unwrap()
            .to_string();
        let art_uri = attached["events"][0]["uri"].as_str().unwrap();
        assert!(art_uri.starts_with("blob://"));
        let read_back = agent
            .call_ok(
                "artifacts_read",
                json!({ "job_id": job_id, "artifact_id": art_id }),
            )
            .await;
        assert_eq!(read_back["storage"], "blob");
        assert_eq!(read_back["encoding"], "utf-8");
        assert_eq!(read_back["content"], payload_text);
        let blob_sha = read_back["sha256"].as_str().unwrap();
        let rest_blob = send_http(
            server.addr,
            Method::GET,
            &format!("/v1/blobs/{blob_sha}"),
            agent.bearer.as_deref(),
            None,
            Vec::new(),
        )
        .await;
        assert_eq!(rest_blob.status, StatusCode::OK);
        assert_eq!(rest_blob.body, payload_text.as_bytes());

        let mut fields = serde_json::Map::new();
        for f in ctx["required_step_fields"].as_array().unwrap() {
            let key = f["key"].as_str().unwrap();
            fields.insert(
                key.to_string(),
                json!(format!("Recorded {key} at {stage_id}")),
            );
        }

        let rubric = &ctx["stage"]["gate_rubric"];
        let is_sub_stage = !ctx["sub_stage"].is_null();
        let (is_err, adv_res) = agent
            .call_tool(
                "jobs_advance",
                json!({
                    "job_id": job_id,
                    "fields": fields,
                    "artifact_ids": [art_id],
                    "fencing_token": token
                }),
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
                    json!({
                        "job_id": job_id,
                        "fields": fields,
                        "artifact_ids": [art_id],
                        "fencing_token": token
                    }),
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
            json!({ "job_id": job_id, "district_id": "DIST-001" }),
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
                "cmd": "approve_job",
                "job_id": job_id,
                "note": "Approved after reviewing step records",
                "expected_revision": null,
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
    assert_eq!(job_after["step_records"].as_array().unwrap().len(), 13);

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

