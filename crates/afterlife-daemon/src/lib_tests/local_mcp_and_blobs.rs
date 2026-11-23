use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::build_router;

use super::{read_json, seed_active_and_gate_jobs, test_state};

#[tokio::test]
async fn local_mcp_protocol_compat_agent_guard_and_metrics_logging() {
    let (state, dir) = test_state("local_mcp_compat");
    let (active_job_id, gate_job_id) = seed_active_and_gate_jobs(&state);
    let app = build_router(state);

    let notif = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(notif.status(), StatusCode::ACCEPTED);

    let ping_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ping_resp.status(), StatusCode::OK);
    let ping_json = read_json(ping_resp).await;
    assert_eq!(ping_json["result"], json!({}));

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 99,
                        "method": "tools/call",
                        "params": {
                            "name": "jobs_claim",
                            "arguments": { "job_id": "JOB-999" }
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let missing_json = read_json(missing).await;
    assert_eq!(missing_json["result"]["isError"], true);
    let missing_err: Value = serde_json::from_str(
        missing_json["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(missing_err["status"], 404);

    let claim_gate = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "tools/call",
                        "params": {
                            "name": "jobs_claim",
                            "arguments": { "job_id": gate_job_id.as_str() }
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let claim_gate_json = read_json(claim_gate).await;
    assert_eq!(claim_gate_json["result"]["isError"], true);
    let claim_gate_err: Value = serde_json::from_str(
        claim_gate_json["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claim_gate_err["status"], 403);
    assert_eq!(claim_gate_err["code"], "human_approval_required");

    let bad_ttl = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 3,
                        "method": "tools/call",
                        "params": {
                            "name": "jobs_claim",
                            "arguments": { "job_id": active_job_id.as_str(), "ttl_ms": -100 }
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let bad_ttl_json = read_json(bad_ttl).await;
    let bad_ttl_err: Value = serde_json::from_str(
        bad_ttl_json["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(bad_ttl_err["status"], 422);
    assert_eq!(bad_ttl_err["code"], "invalid_input");

    for (req_id, title, expected_err) in [
        (4, "First Job Title", false),
        (5, "Different Job Title", true),
    ] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "jsonrpc": "2.0",
                            "id": req_id,
                            "method": "tools/call",
                            "params": {
                                "name": "jobs_create",
                                "arguments": { "command_id": "idem-payload-test", "title": title }
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let j = read_json(resp).await;
        assert_eq!(j["result"]["isError"], expected_err);
        if expected_err {
            let err_v: Value =
                serde_json::from_str(j["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(err_v["status"], 409);
            assert_eq!(err_v["code"], "idempotency_conflict");
        }
    }

    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 6,
                        "method": "tools/call",
                        "params": {
                            "name": "jobs_list",
                            "arguments": { "limit": 2, "offset": 0 }
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let list_json = read_json(list_resp).await;
    let list_v: Value =
        serde_json::from_str(list_json["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap();
    assert_eq!(list_v["jobs"].as_array().unwrap().len(), 2);
    assert_eq!(list_v["has_more"], true);

    let m_resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(m_resp.status(), StatusCode::OK);
    let m_json = read_json(m_resp).await;
    assert_eq!(m_json["mcp_notifications"], 1);
    assert_eq!(m_json["mcp_tools"]["jobs_claim"]["forbidden"], 1);
    assert_eq!(m_json["mcp_tools"]["jobs_create"]["conflicts"], 1);

    let log_path = m_json["log_path"].as_str().unwrap();
    let log_content = std::fs::read_to_string(log_path).expect("requests.jsonl must exist");
    assert!(log_content.contains("\"tool\":\"jobs_claim\""));
    assert!(log_content.contains("\"outcome\":\"forbidden\""));
    assert!(log_content.contains("\"outcome\":\"conflict\""));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn artifact_blob_attach_checks_authority_before_persisting_and_serves_owner_only() {
    let (state, dir) = test_state("blob_fail_closed");
    let (active_job_id, gate_job_id) = seed_active_and_gate_jobs(&state);
    let owner = afterlife_core::OwnerId::from(afterlife_core::fixtures::LOCAL_OWNER_ID);
    let app = build_router(state.clone());

    let gate_payload = "secret-gate-attempt";
    let mut h = Sha256::new();
    h.update(gate_payload.as_bytes());
    let gate_sha = format!("{:x}", h.finalize());

    let gate_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "tools/call",
                        "params": {
                            "name": "artifacts_attach",
                            "arguments": {
                                "job_id": gate_job_id,
                                "kind": "verification_report",
                                "label": "gate.txt",
                                "content": gate_payload
                            }
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let gate_json = read_json(gate_resp).await;
    assert_eq!(gate_json["result"]["isError"], true);
    let gate_err: Value =
        serde_json::from_str(gate_json["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap();
    assert_eq!(gate_err["status"], 403);
    assert!(
        state
            .ports
            .blobs
            .get_blob(&owner, &gate_sha)
            .unwrap()
            .is_none(),
        "rejected approval-gate attach must not persist orphan blob"
    );

    let unleased_payload = "unleased-attempt";
    let mut h2 = Sha256::new();
    h2.update(unleased_payload.as_bytes());
    let unleased_sha = format!("{:x}", h2.finalize());

    let unleased_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "tools/call",
                        "params": {
                            "name": "artifacts_attach",
                            "arguments": {
                                "job_id": active_job_id,
                                "kind": "snapshot",
                                "label": "unleased.txt",
                                "content": unleased_payload
                            }
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let unleased_json = read_json(unleased_resp).await;
    assert_eq!(unleased_json["result"]["isError"], true);
    assert!(
        state
            .ports
            .blobs
            .get_blob(&owner, &unleased_sha)
            .unwrap()
            .is_none(),
        "rejected unleased attach must not persist orphan blob"
    );

    let missing_blob_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/blobs/{unleased_sha}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_blob_resp.status(), StatusCode::NOT_FOUND);

    let bad_sha_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/v1/blobs/not-a-sha")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_sha_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let _ = std::fs::remove_dir_all(&dir);
}
