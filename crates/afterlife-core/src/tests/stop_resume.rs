use std::collections::BTreeMap;

use crate::*;

use super::agent_actor;

#[test]
fn stop_and_resume_enforce_human_only_revoke_lease_block_all_agent_mutations_and_restore_position() {
    let mut engine = AfterlifeEngine::new();
    let human = engine.local_actor();
    let agent = agent_actor(&engine.owner_id, "worker-agent");
    let dist_id = DistrictId::from("DIST-001");

    let job_id = engine.queue_preset_job("Stop/Resume target", Some(&dist_id));

    assert!(matches!(
        engine.apply(
            &human,
            EngineCommand::StopJob {
                job_id: job_id.clone(),
                reason: None,
                expected_revision: None,
                at: 1_800_000_000_000,
            },
        ),
        Err(TransitionError::InvalidLocation { .. })
    ));

    engine.assign_job_to_district(&job_id, &dist_id).unwrap();

    assert!(matches!(
        engine.apply(
            &human,
            EngineCommand::ResumeJob {
                job_id: job_id.clone(),
                expected_revision: None,
                at: 1_800_000_001_000,
            },
        ),
        Err(TransitionError::JobNotStopped { .. })
    ));

    let claim_ev = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("worker-agent"),
                runner_label: "Worker".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: 1_800_000_002_000,
            },
        )
        .unwrap();
    let ft1 = match &claim_ev[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    engine
        .apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::Spec,
                label: "notes".into(),
                uri: "https://example.com/spec".into(),
                sha256: None,
                fencing_token: Some(ft1),
                at: 1_800_000_003_000,
            },
        )
        .unwrap();

    let mut fields = BTreeMap::new();
    fields.insert("sources_read".into(), "src/lib.rs".into());
    fields.insert("findings".into(), "verified".into());
    engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields,
                artifact_ids: vec![],
                expected_revision: None,
                fencing_token: Some(ft1),
                at: 1_800_000_004_000,
            },
        )
        .unwrap();

    let claim_ev2 = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("worker-agent"),
                runner_label: "Worker".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: 1_800_000_005_000,
            },
        )
        .unwrap();
    let ft2 = match &claim_ev2[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    let pre_stop_job = engine.find_job(&job_id).unwrap().clone();
    let pre_stop_loc = pre_stop_job.location.clone();
    let pre_stop_steps = pre_stop_job.step_records.len();
    let pre_stop_arts = pre_stop_job.artifacts.len();

    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::StopJob {
                job_id: job_id.clone(),
                reason: Some("Agent must not stop".into()),
                expected_revision: None,
                at: 1_800_000_006_000,
            },
        ),
        Err(TransitionError::HumanApprovalRequired { .. })
    ));

    let stop_ev = engine
        .apply(
            &human,
            EngineCommand::StopJob {
                job_id: job_id.clone(),
                reason: Some("Hold for operator review".into()),
                expected_revision: None,
                at: 1_800_000_007_000,
            },
        )
        .unwrap();
    assert!(matches!(
        &stop_ev[0],
        JobEvent::JobStopped {
            revoked_fencing_token: Some(tok),
            ..
        } if *tok == ft2
    ));

    let stopped_job = engine.find_job(&job_id).unwrap();
    assert!(stopped_job.stopped);
    assert!(stopped_job.lease.is_none());
    assert_eq!(stopped_job.location, pre_stop_loc);
    let ctx = engine.compile_execution_context(&job_id).unwrap();
    assert!(ctx.stopped);
    assert_eq!(
        ctx.stopped_reason.as_deref(),
        Some("Hold for operator review")
    );

    assert!(matches!(
        engine.apply(
            &human,
            EngineCommand::StopJob {
                job_id: job_id.clone(),
                reason: None,
                expected_revision: None,
                at: 1_800_000_008_000,
            },
        ),
        Err(TransitionError::JobStopped { .. })
    ));

    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("worker-agent"),
                runner_label: "Worker".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: 1_800_000_009_000,
            },
        ),
        Err(TransitionError::JobStopped { .. })
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::HeartbeatLease {
                job_id: job_id.clone(),
                fencing_token: ft2,
                ttl_ms: 60_000,
                at: 1_800_000_010_000,
            },
        ),
        Err(TransitionError::JobStopped { .. })
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: BTreeMap::new(),
                artifact_ids: vec![],
                expected_revision: None,
                fencing_token: Some(ft2),
                at: 1_800_000_011_000,
            },
        ),
        Err(TransitionError::JobStopped { .. })
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ReworkJob {
                job_id: job_id.clone(),
                expected_revision: None,
                fencing_token: Some(ft2),
                reason: Some("Attempt rework while stopped".into()),
                at: 1_800_000_012_000,
            },
        ),
        Err(TransitionError::JobStopped { .. })
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::RawNote,
                label: "blocked".into(),
                uri: "https://example.com/blocked".into(),
                sha256: None,
                fencing_token: Some(ft2),
                at: 1_800_000_013_000,
            },
        ),
        Err(TransitionError::JobStopped { .. })
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::RecordGateVerification {
                job_id: job_id.clone(),
                verdict: GateVerdict::Pass,
                summary: "blocked".into(),
                checklist_results: vec![],
                fencing_token: Some(ft2),
                at: 1_800_000_014_000,
            },
        ),
        Err(TransitionError::JobStopped { .. })
    ));

    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ResumeJob {
                job_id: job_id.clone(),
                expected_revision: None,
                at: 1_800_000_015_000,
            },
        ),
        Err(TransitionError::HumanApprovalRequired { .. })
    ));

    let resume_ev = engine
        .apply(
            &human,
            EngineCommand::ResumeJob {
                job_id: job_id.clone(),
                expected_revision: None,
                at: 1_800_000_016_000,
            },
        )
        .unwrap();
    assert!(matches!(&resume_ev[0], JobEvent::JobResumed { .. }));

    let resumed = engine.find_job(&job_id).unwrap();
    assert!(!resumed.stopped);
    assert_eq!(resumed.location, pre_stop_loc);
    assert_eq!(resumed.step_records.len(), pre_stop_steps);
    assert_eq!(resumed.artifacts.len(), pre_stop_arts);
    assert_eq!(
        resumed.pinned_philosophy_version,
        pre_stop_job.pinned_philosophy_version
    );
    assert_eq!(resumed.pinned_spec_version, pre_stop_job.pinned_spec_version);

    let reclaimed = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id,
                runner_id: RunnerId::from("worker-agent"),
                runner_label: "Worker".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: 1_800_000_017_000,
            },
        )
        .unwrap();
    assert!(matches!(&reclaimed[0], JobEvent::JobClaimed { .. }));
}
