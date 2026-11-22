pub mod artifacts;
pub mod auth;
pub mod mcp;
pub mod metrics;
pub mod rest;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

use afterlife_protocol::{AfterlifeConfig, StorageConfig};
use afterlife_storage::RuntimePorts;
use axum::{
    Router,
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};

use crate::{auth::AuthBroker, metrics::DaemonMetrics};

pub struct AppDaemonState {
    pub config: AfterlifeConfig,
    pub ports: RuntimePorts,
    pub auth: AuthBroker,
    pub metrics: Arc<DaemonMetrics>,
}

fn resolve_log_dir(config: &AfterlifeConfig) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("AFTERLIFE_LOG_DIR") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    match &config.storage {
        StorageConfig::Sqlite { path } => Path::new(path).parent().map(|p| p.join("logs")),
        StorageConfig::Postgres { .. } => None,
    }
}

impl AppDaemonState {
    pub fn from_config(config: AfterlifeConfig) -> Result<Arc<Self>, String> {
        let ports = RuntimePorts::from_config(&config).map_err(|e| e.to_string())?;
        let auth = AuthBroker::new(config.auth.clone());
        let log_dir = resolve_log_dir(&config);
        let metrics = DaemonMetrics::new(log_dir.as_deref());
        Ok(Arc::new(Self {
            config,
            ports,
            auth,
            metrics,
        }))
    }
}

async fn metrics_middleware(
    State(state): State<Arc<AppDaemonState>>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();
    let skip_record = path == "/v1/metrics" || path == "/v1/metrics/reset";

    if !skip_record {
        state.metrics.in_flight.fetch_add(1, Ordering::Relaxed);
    }
    let started = Instant::now();
    let resp = next.run(req).await;
    let latency_us = started.elapsed().as_micros() as u64;
    if !skip_record {
        state.metrics.in_flight.fetch_sub(1, Ordering::Relaxed);
        state
            .metrics
            .record_http(&method, &path, resp.status().as_u16(), latency_us);
    }
    resp
}

pub fn build_router(state: Arc<AppDaemonState>) -> Router {
    Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(auth::well_known_protected_resource),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(auth::well_known_authorization_server),
        )
        .route("/oauth/register", post(auth::register_client))
        .route("/oauth/authorize", get(auth::authorize_get))
        .route("/oauth/token", post(auth::token_post))
        .route("/oauth/revoke", post(auth::revoke_post))
        .route("/v1/health", get(rest::health))
        .route("/v1/metrics", get(rest::get_metrics))
        .route("/v1/metrics/reset", post(rest::reset_metrics))
        .route("/v1/workspace", get(rest::get_workspace))
        .route("/v1/blobs/{sha256}", get(rest::get_blob))
        .route("/v1/jobs/{job_id}/context", get(rest::get_job_context))
        .route("/v1/commands", post(rest::post_command))
        .route("/v1/events", get(rest::sse_events))
        .route(
            "/mcp",
            post(mcp::mcp_post)
                .get(mcp::mcp_get)
                .delete(mcp::mcp_delete),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics_middleware,
        ))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode, header},
    };
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    fn test_state(name: &str) -> (Arc<AppDaemonState>, std::path::PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("afterlifed_e2e_{name}_{nanos}"));
        let cfg = AfterlifeConfig::local_default(dir.to_str().unwrap());
        let state = AppDaemonState::from_config(cfg).unwrap();
        (state, dir)
    }

    async fn read_json(resp: axum::response::Response) -> Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn seed_active_and_gate_jobs(
        state: &Arc<AppDaemonState>,
    ) -> (afterlife_core::JobId, afterlife_core::JobId) {
        let owner = afterlife_core::OwnerId::from(afterlife_core::fixtures::LOCAL_OWNER_ID);
        let human = afterlife_core::Actor::local_human(owner.clone());
        let dist_id = afterlife_core::DistrictId::from("DIST-001");
        let mut seq = 1u64;
        let mut run = |cmd: afterlife_core::EngineCommand| {
            let id = afterlife_core::CommandId::new(format!("seed-cmd-{seq}"));
            seq += 1;
            state
                .ports
                .store
                .execute_command(afterlife_protocol::CommandEnvelope {
                    command_id: id,
                    actor: human.clone(),
                    command: cmd,
                })
                .unwrap()
        };

        let out_active = run(afterlife_core::EngineCommand::QueueJob {
            title: "Active Work Job".into(),
            raw_input: "seed active".into(),
            target_district_hint: Some(dist_id.clone()),
            at: 1_900_000_000_000,
        });
        let active_id = match &out_active.events[0] {
            afterlife_core::JobEvent::JobQueued { job_id, .. } => job_id.clone(),
            _ => unreachable!(),
        };
        run(afterlife_core::EngineCommand::AssignJobToDistrict {
            job_id: active_id.clone(),
            district_id: dist_id.clone(),
            expected_revision: None,
            at: 1_900_000_001_000,
        });

        let out_gate = run(afterlife_core::EngineCommand::QueueJob {
            title: "Approval Gate Job".into(),
            raw_input: "seed gate".into(),
            target_district_hint: Some(dist_id.clone()),
            at: 1_900_000_002_000,
        });
        let gate_id = match &out_gate.events[0] {
            afterlife_core::JobEvent::JobQueued { job_id, .. } => job_id.clone(),
            _ => unreachable!(),
        };
        run(afterlife_core::EngineCommand::AssignJobToDistrict {
            job_id: gate_id.clone(),
            district_id: dist_id,
            expected_revision: None,
            at: 1_900_000_003_000,
        });
        let seed_agent = afterlife_core::Actor::Agent {
            owner_id: owner.clone(),
            client_id: "seed-agent".into(),
            runner_label: "Seed Agent".into(),
            token_jti: "seed-jti".into(),
        };
        let run_agent = |cmd: afterlife_core::EngineCommand| {
            state
                .ports
                .store
                .execute_command(afterlife_protocol::CommandEnvelope {
                    command_id: afterlife_core::CommandId::new(uuid::Uuid::now_v7().to_string()),
                    actor: seed_agent.clone(),
                    command: cmd,
                })
                .unwrap()
        };
        for step in 0..32u64 {
            let snap = state.ports.store.load_workspace(&owner).unwrap();
            let job = snap.engine.find_job(&gate_id).unwrap();
            if snap.engine.is_job_at_approval_gate(job) {
                break;
            }
            let ctx = snap.engine.compile_execution_context(&gate_id).unwrap();
            let base_ts = 1_900_000_004_000 + step * 1_000;
            let claim_out = run_agent(afterlife_core::EngineCommand::ClaimJob {
                job_id: gate_id.clone(),
                runner_id: afterlife_core::RunnerId::from("seed-agent"),
                runner_label: "Seed Agent".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: base_ts,
            });
            let tok = match &claim_out.events[0] {
                afterlife_core::JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
                _ => unreachable!(),
            };
            let mut art_ids = Vec::new();
            if let Some(&kind) = ctx.required_artifact_kinds.first() {
                let art_out = run_agent(afterlife_core::EngineCommand::AttachArtifact {
                    job_id: gate_id.clone(),
                    kind,
                    label: format!("{}-evidence.json", ctx.stage.id),
                    uri: format!("https://example.com/{}/evidence.json", ctx.stage.id),
                    sha256: None,
                    fencing_token: Some(tok),
                    at: base_ts + 10,
                });
                if let afterlife_core::JobEvent::ArtifactAttached { artifact_id, .. } =
                    &art_out.events[0]
                {
                    art_ids.push(artifact_id.clone());
                }
            }
            let fields: std::collections::BTreeMap<String, String> = ctx
                .required_step_fields
                .iter()
                .map(|f| (f.key.clone(), format!("Recorded {}", f.key)))
                .collect();
            let adv_res = state
                .ports
                .store
                .execute_command(afterlife_protocol::CommandEnvelope {
                    command_id: afterlife_core::CommandId::new(uuid::Uuid::now_v7().to_string()),
                    actor: seed_agent.clone(),
                    command: afterlife_core::EngineCommand::AdvanceJob {
                        job_id: gate_id.clone(),
                        fields: fields.clone(),
                        artifact_ids: art_ids.clone(),
                        expected_revision: None,
                        fencing_token: Some(tok),
                        at: base_ts + 20,
                    },
                });
            if let Err(afterlife_protocol::PortError::Domain(
                afterlife_core::TransitionError::GateVerificationRequired { .. },
            )) = adv_res
            {
                let checks = ctx
                    .stage
                    .gate_rubric
                    .as_ref()
                    .map(|r| r.checks.iter().map(|c| (c.clone(), true)).collect())
                    .unwrap_or_default();
                run_agent(afterlife_core::EngineCommand::RecordGateVerification {
                    job_id: gate_id.clone(),
                    verdict: afterlife_core::GateVerdict::Pass,
                    summary: "Seed gate verified".into(),
                    checklist_results: checks,
                    fencing_token: Some(tok),
                    at: base_ts + 30,
                });
                run_agent(afterlife_core::EngineCommand::AdvanceJob {
                    job_id: gate_id.clone(),
                    fields,
                    artifact_ids: art_ids,
                    expected_revision: None,
                    fencing_token: Some(tok),
                    at: base_ts + 40,
                });
            } else {
                adv_res.unwrap();
            }
        }

        (active_id, gate_id)
    }

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
}
