use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::Barrier;

use crate::harness::{HumanClient, LiveServer, McpClient, TempDataDir};

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
            json!({ "job_id": job_id, "district_id": "DIST-001" }),
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
    for bad_uri in [
        "javascript:alert(1)",
        "evidence://job-001/before-baseline.json",
        "file:///etc/passwd",
        "s3://afterlife/fabricated/report.json",
        "blob://local-owner/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/missing.json",
        "blob://other-owner/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/stolen.json",
    ] {
        agent
            .call_err(
                "artifacts_attach",
                json!({
                    "job_id": job_id,
                    "kind": "spec",
                    "label": "bad-uri",
                    "uri": bad_uri,
                    "fencing_token": token
                }),
                422,
                "invalid_input",
            )
            .await;
    }
    agent
        .call_err(
            "artifacts_attach",
            json!({
                "job_id": job_id,
                "kind": "spec",
                "label": "both-sources",
                "uri": "https://example.com/a",
                "content": "duplicate source",
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
                "label": "no-source",
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

    for idx in 1..=3 {
        agent
            .call_ok("jobs_create", json!({ "title": format!("Pagination Job {idx}") }))
            .await;
    }

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

