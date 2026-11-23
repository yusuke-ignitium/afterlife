use afterlife_protocol::AfterlifeConfig;
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::build_router;

use super::{read_json, seed_active_and_gate_jobs, test_state};

#[tokio::test]
async fn full_oauth_pkce_dcr_to_mcp_workflow_and_security_guards() {
    let (state, dir) = test_state("oauth_mcp");
    let (active_job_id, gate_job_id) = seed_active_and_gate_jobs(&state);
    let app = build_router(state);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-protected-resource")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let reg_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/oauth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "client_name": "Cursor Agent",
                        "redirect_uris": ["http://127.0.0.1:9999/cb"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg_resp.status(), StatusCode::CREATED);
    let reg_body = read_json(reg_resp).await;
    let client_id = reg_body["client_id"].as_str().unwrap().to_string();

    let verifier = "super-secret-pkce-verifier-0123456789abcdef";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let auth_uri = format!(
        "/oauth/authorize?client_id={}&redirect_uri=http://127.0.0.1:9999/cb&response_type=code&code_challenge={}&code_challenge_method=S256&scope=jobs:read%20jobs:create%20jobs:work%20approvals:decide&auto_approve=1",
        client_id, challenge
    );
    let auth_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(auth_uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(auth_resp.status(), StatusCode::SEE_OTHER);
    let location = auth_resp
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    let code = location.split("code=").nth(1).unwrap().to_string();

    let tok_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/oauth/token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "grant_type": "authorization_code",
                        "client_id": client_id,
                        "code": code,
                        "code_verifier": verifier,
                        "redirect_uri": "http://127.0.0.1:9999/cb"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tok_resp.status(), StatusCode::OK);
    let tok_body = read_json(tok_resp).await;
    let access_token = tok_body["access_token"].as_str().unwrap().to_string();
    let granted_scope = tok_body["scope"].as_str().unwrap();
    assert!(!granted_scope.contains("approvals:decide"));
    assert!(granted_scope.contains("jobs:work"));

    let ctx_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "tools/call",
                        "params": {
                            "name": "job_get_context",
                            "arguments": { "job_id": active_job_id.as_str() }
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ctx_resp.status(), StatusCode::OK);
    let ctx_json = read_json(ctx_resp).await;
    let text = ctx_json["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Ship Every State as a Whole"));
    assert_eq!(ctx_json["result"]["isError"], false);

    for (req_id, tool_name, arguments) in [
        (
            2,
            "jobs_claim",
            json!({ "job_id": gate_job_id.as_str(), "ttl_ms": 60000 }),
        ),
        (
            3,
            "jobs_assign",
            json!({ "job_id": gate_job_id.as_str(), "district_id": "DIST-001" }),
        ),
        (4, "jobs_advance", json!({ "job_id": gate_job_id.as_str() })),
    ] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "jsonrpc": "2.0",
                            "id": req_id,
                            "method": "tools/call",
                            "params": { "name": tool_name, "arguments": arguments }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let j = read_json(resp).await;
        assert_eq!(
            j["result"]["isError"], true,
            "{tool_name} must fail at approval_gate"
        );
        let err_payload: Value =
            serde_json::from_str(j["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(err_payload["status"], 403);
        assert_eq!(err_payload["code"], "human_approval_required");
    }

    let rev_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/oauth/revoke")
                .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rev_resp.status(), StatusCode::OK);

    let after_rev = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/list" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after_rev.status(), StatusCode::UNAUTHORIZED);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_files_and_mcp_schema_stay_in_sync_with_code() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let local_cfg = AfterlifeConfig::from_toml_file(workspace_root.join("config/local.toml"))
        .expect("config/local.toml must parse cleanly");
    assert!(matches!(
        local_cfg.auth,
        afterlife_protocol::AuthConfig::LocalDev { .. }
    ));

    let prod_cfg =
        AfterlifeConfig::from_toml_file(workspace_root.join("config/production.example.toml"))
            .expect("config/production.example.toml must parse cleanly");
    assert!(matches!(
        prod_cfg.auth,
        afterlife_protocol::AuthConfig::GoogleOidc { .. }
    ));

    let schema_raw =
        std::fs::read_to_string(workspace_root.join("schemas/mcp-tools.schema.json"))
            .expect("schemas/mcp-tools.schema.json must exist");
    let schema_json: Value =
        serde_json::from_str(&schema_raw).expect("mcp-tools.schema.json must be valid JSON");
    assert_eq!(
        schema_json["tools"],
        crate::mcp::mcp_tool_definitions(),
        "schemas/mcp-tools.schema.json drifted from mcp::mcp_tool_definitions()"
    );
    let jobs_advance_desc = schema_json["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "jobs_advance")
        .and_then(|t| t["description"].as_str())
        .expect("jobs_advance tool description must exist");
    assert!(jobs_advance_desc.contains("job_get_context"));
    assert!(!jobs_advance_desc.contains("context_compile"));
}

