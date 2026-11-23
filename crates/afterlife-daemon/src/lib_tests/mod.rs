mod local_mcp_and_blobs;
mod oauth_and_schema;

use std::sync::Arc;

use afterlife_protocol::AfterlifeConfig;
use http_body_util::BodyExt;
use serde_json::Value;

use crate::AppDaemonState;

pub(super) fn test_state(name: &str) -> (Arc<AppDaemonState>, std::path::PathBuf) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("afterlifed_e2e_{name}_{nanos}"));
    let cfg = AfterlifeConfig::local_default(dir.to_str().unwrap());
    let state = AppDaemonState::from_config(cfg).unwrap();
    (state, dir)
}

pub(super) async fn read_json(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

pub(super) fn seed_active_and_gate_jobs(
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

