use std::collections::BTreeMap;

use crate::*;

mod artifact_uris_and_specs;
mod fade_px_guard;
mod module_size_guard;
mod governance_and_rework;
mod seed_invariants;
mod step_artifacts;
mod stop_resume;
mod workflow_and_concurrency;

pub(super) fn agent_actor(owner: &OwnerId, name: &str) -> Actor {
    Actor::Agent {
        owner_id: owner.clone(),
        client_id: format!("client-{name}"),
        runner_label: name.into(),
        token_jti: format!("jti-{name}-1"),
    }
}

pub(super) fn seed_job_in_district(engine: &mut AfterlifeEngine, title: &str) -> JobId {
    let dist_id = DistrictId::from("DIST-001");
    let job_id = engine.queue_preset_job(title, Some(&dist_id));
    let human = engine.local_actor();
    engine
        .apply(
            &human,
            EngineCommand::AssignJobToDistrict {
                job_id: job_id.clone(),
                district_id: dist_id,
                expected_revision: None,
                at: 1_763_600_010_000,
            },
        )
        .unwrap();
    job_id
}

pub(super) fn build_required_fields(ctx: &ExecutionContext) -> BTreeMap<String, String> {
    ctx.required_step_fields
        .iter()
        .map(|f| (f.key.clone(), format!("Recorded {} for {}", f.key, ctx.stage.id)))
        .collect()
}

pub(super) fn advance_one_step_as_agent(
    engine: &mut AfterlifeEngine,
    job_id: &JobId,
    agent: &Actor,
    at: TimestampMs,
) {
    let ctx = engine.compile_execution_context(job_id).unwrap();
    let claim_ev = engine
        .apply(
            agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("test-agent"),
                runner_label: "Test Agent".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at,
            },
        )
        .unwrap();
    let token = match &claim_ev[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    let mut attached_ids = Vec::new();
    if let Some(&kind) = ctx.required_artifact_kinds.first() {
        let art_ev = engine
            .apply(
                agent,
                EngineCommand::AttachArtifact {
                    job_id: job_id.clone(),
                    kind,
                    label: format!("{}-evidence.json", ctx.stage.id),
                    uri: format!("https://example.com/{}/evidence.json", ctx.stage.id),
                    sha256: None,
                    fencing_token: Some(token),
                    at: at + 1,
                },
            )
            .unwrap();
        if let JobEvent::ArtifactAttached { artifact_id, .. } = &art_ev[0] {
            attached_ids.push(artifact_id.clone());
        }
    }

    let fields = build_required_fields(&ctx);
    let res = engine.apply(
        agent,
        EngineCommand::AdvanceJob {
            job_id: job_id.clone(),
            fields: fields.clone(),
            artifact_ids: attached_ids.clone(),
            expected_revision: None,
            fencing_token: Some(token),
            at: at + 2,
        },
    );
    if let Err(TransitionError::GateVerificationRequired { .. }) = res {
        let checks = ctx
            .stage
            .gate_rubric
            .as_ref()
            .map(|r| r.checks.iter().map(|c| (c.clone(), true)).collect())
            .unwrap_or_default();
        engine
            .apply(
                agent,
                EngineCommand::RecordGateVerification {
                    job_id: job_id.clone(),
                    verdict: GateVerdict::Pass,
                    summary: format!("Verified {}", ctx.stage.id),
                    checklist_results: checks,
                    fencing_token: Some(token),
                    at: at + 3,
                },
            )
            .unwrap();
        engine
            .apply(
                agent,
                EngineCommand::AdvanceJob {
                    job_id: job_id.clone(),
                    fields,
                    artifact_ids: attached_ids,
                    expected_revision: None,
                    fencing_token: Some(token),
                    at: at + 4,
                },
            )
            .unwrap();
    } else {
        res.unwrap();
    }
}

pub(super) fn seed_job_at_approval_gate(engine: &mut AfterlifeEngine, title: &str) -> JobId {
    let job_id = seed_job_in_district(engine, title);
    let agent = agent_actor(&engine.owner_id, "seed-runner");
    for step in 0..32u64 {
        let job = engine.find_job(&job_id).unwrap();
        if engine.is_job_at_approval_gate(job) {
            return job_id;
        }
        advance_one_step_as_agent(engine, &job_id, &agent, 1_763_600_100_000 + step * 100);
    }
    panic!("job {job_id} did not reach approval gate");
}
