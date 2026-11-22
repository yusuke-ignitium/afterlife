use std::collections::BTreeMap;

use crate::*;

use super::{advance_one_step_as_agent, agent_actor, build_required_fields, seed_job_at_approval_gate, seed_job_in_district};

#[test]
fn advance_steps_through_sub_machine_before_parent_stage() {
    let mut engine = AfterlifeEngine::new();
    let agent = agent_actor(&engine.owner_id, "step-runner");
    let job_id = seed_job_in_district(&mut engine, "Sub-stage stepping test");
    let mut ts = 1_763_600_200_000u64;

    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("discover"),
            sub_stage_id: Some(SubStageId::from("trace_sources")),
        }
    );

    advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
    ts += 100;
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("discover"),
            sub_stage_id: Some(SubStageId::from("isolate_probe")),
        }
    );

    advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
    ts += 100;
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("discover"),
            sub_stage_id: Some(SubStageId::from("lock_criteria")),
        }
    );

    advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
    ts += 100;
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("design"),
            sub_stage_id: None,
        }
    );

    advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
    ts += 100;
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("baseline_guard"),
            sub_stage_id: Some(SubStageId::from("capture_baseline")),
        }
    );

    advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
    ts += 100;
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("baseline_guard"),
            sub_stage_id: Some(SubStageId::from("lock_regression_guards")),
        }
    );

    for _ in 0..7 {
        advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
        ts += 100;
    }
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("approve"),
            sub_stage_id: None,
        }
    );
    assert_eq!(engine.find_job(&job_id).unwrap().step_records.len(), 12);

    engine
        .approve_job(&job_id, Some("Verified all evidence and diffs".into()))
        .unwrap();
    assert!(matches!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::Completed {
            ref terminal_stage_id,
            ..
        } if terminal_stage_id == &StageId::from("shipped")
    ));
    assert_eq!(engine.find_job(&job_id).unwrap().step_records.len(), 13);
    assert_eq!(
        engine.find_job(&job_id).unwrap().step_records.last().unwrap().outcome,
        StepRecordOutcome::Approved
    );

    assert!(matches!(
        engine.approve_job(&job_id, None),
        Err(TransitionError::InvalidLocation { .. })
    ));
}

#[test]
fn human_cannot_advance_work_stage_and_step_contract_is_enforced() {
    let mut engine = AfterlifeEngine::new();
    let human = engine.local_actor();
    let agent = agent_actor(&engine.owner_id, "contract-runner");
    let job_id = seed_job_in_district(&mut engine, "Step contract test");

    let human_adv = engine
        .apply(
            &human,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: BTreeMap::new(),
                artifact_ids: Vec::new(),
                expected_revision: None,
                fencing_token: None,
                at: 1_900_000_000_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        human_adv,
        TransitionError::AgentExecutionRequired { .. }
    ));

    let human_approve_work = engine.approve_job(&job_id, None).unwrap_err();
    assert!(matches!(
        human_approve_work,
        TransitionError::InvalidLocation { .. }
    ));

    let claim = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("contract-runner"),
                runner_label: "Contract Runner".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: 1_900_000_001_000,
            },
        )
        .unwrap();
    let tok = match &claim[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    let missing_err = engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: BTreeMap::from([("sources_read".into(), "   ".into())]),
                artifact_ids: Vec::new(),
                expected_revision: None,
                fencing_token: Some(tok),
                at: 1_900_000_002_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        missing_err,
        TransitionError::StepRecordIncomplete {
            ref missing_fields,
            ref missing_artifact_kinds,
            ..
        } if missing_fields.contains(&"sources_read".to_string())
            && missing_fields.contains(&"findings".to_string())
            && missing_artifact_kinds.is_empty()
    ));

    engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: BTreeMap::from([
                    ("sources_read".into(), "crates/afterlife-core/src/engine/mod.rs".into()),
                    ("findings".into(), "Sub-stages require deterministic records".into()),
                ]),
                artifact_ids: Vec::new(),
                expected_revision: None,
                fencing_token: Some(tok),
                at: 1_900_000_003_000,
            },
        )
        .unwrap();

    let next_ctx = engine.compile_execution_context(&job_id).unwrap();
    assert_eq!(next_ctx.step_records.len(), 1);
    assert_eq!(
        next_ctx.step_records[0].sub_stage_id,
        Some(SubStageId::from("trace_sources"))
    );
    assert_eq!(
        next_ctx.step_records[0].fields.get("findings").map(String::as_str),
        Some("Sub-stages require deterministic records")
    );

    for step in 0..3u64 {
        advance_one_step_as_agent(&mut engine, &job_id, &agent, 1_900_000_010_000 + step * 100);
    }
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("baseline_guard"),
            sub_stage_id: Some(SubStageId::from("capture_baseline")),
        }
    );

    let claim2 = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("contract-runner"),
                runner_label: "Contract Runner".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: 1_900_000_020_000,
            },
        )
        .unwrap();
    let tok2 = match &claim2[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    let no_art_err = engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: BTreeMap::from([
                    ("baseline_metrics".into(), "p95=12ms".into()),
                    ("conditions".into(), "cold run, 100 iterations".into()),
                ]),
                artifact_ids: Vec::new(),
                expected_revision: None,
                fencing_token: Some(tok2),
                at: 1_900_000_021_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        no_art_err,
        TransitionError::StepRecordIncomplete {
            ref missing_fields,
            ref missing_artifact_kinds,
            ..
        } if missing_fields.is_empty()
            && missing_artifact_kinds.contains(&ArtifactKind::Snapshot)
    ));
}

#[test]
fn fencing_token_rejects_zombie_agent_after_lease_takeover() {
    let mut engine = AfterlifeEngine::new();
    let agent_a = agent_actor(&engine.owner_id, "claude");
    let agent_b = agent_actor(&engine.owner_id, "codex");
    let job_id = seed_job_in_district(&mut engine, "Lease fencing job");
    advance_one_step_as_agent(&mut engine, &job_id, &agent_a, 1_999_000_000_000);
    advance_one_step_as_agent(&mut engine, &job_id, &agent_a, 1_999_000_001_000);

    let t0 = 2_000_000_000_000;
    let ev_a = engine
        .apply(
            &agent_a,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("claude"),
                runner_label: "Claude".into(),
                ttl_ms: 10_000,
                expected_revision: None,
                at: t0,
            },
        )
        .unwrap();
    let token_a = match &ev_a[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    let conflict = engine
        .apply(
            &agent_b,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("codex"),
                runner_label: "Codex".into(),
                ttl_ms: 10_000,
                expected_revision: None,
                at: t0 + 5_000,
            },
        )
        .unwrap_err();
    assert!(matches!(conflict, TransitionError::AlreadyLeased { .. }));

    let ev_b = engine
        .apply(
            &agent_b,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("codex"),
                runner_label: "Codex".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: t0 + 15_000,
            },
        )
        .unwrap();
    let token_b = match &ev_b[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };
    assert!(token_b > token_a);

    let stale_err = engine
        .apply(
            &agent_a,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::SourceCode,
                label: "stale.rs".into(),
                uri: "https://example.com/stale.rs".into(),
                sha256: None,
                fencing_token: Some(token_a),
                at: t0 + 20_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        stale_err,
        TransitionError::StaleFencingToken { provided, current, .. }
        if provided == token_a && current == token_b
    ));

    let exit_fields = build_required_fields(&engine.compile_execution_context(&job_id).unwrap());

    let gate_err = engine
        .apply(
            &agent_b,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: exit_fields.clone(),
                artifact_ids: Vec::new(),
                expected_revision: None,
                fencing_token: Some(token_b),
                at: t0 + 20_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        gate_err,
        TransitionError::GateVerificationRequired { .. }
    ));

    let required_checks = engine
        .compile_execution_context(&job_id)
        .unwrap()
        .stage
        .gate_rubric
        .unwrap()
        .checks;
    let checklist_results = required_checks.into_iter().map(|c| (c, true)).collect();

    engine
        .apply(
            &agent_b,
            EngineCommand::RecordGateVerification {
                job_id: job_id.clone(),
                verdict: GateVerdict::Pass,
                summary: "All discovery checks passed".into(),
                checklist_results,
                fencing_token: Some(token_b),
                at: t0 + 20_000,
            },
        )
        .unwrap();

    engine
        .apply(
            &agent_b,
            EngineCommand::AdvanceJob {
                job_id,
                fields: exit_fields,
                artifact_ids: Vec::new(),
                expected_revision: None,
                fencing_token: Some(token_b),
                at: t0 + 21_000,
            },
        )
        .unwrap();
}

#[test]
fn optimistic_cas_detects_revision_conflict() {
    let mut engine = AfterlifeEngine::new();
    let actor = engine.local_actor();
    let job_id = seed_job_at_approval_gate(&mut engine, "CAS conflict job");
    let stale_rev = engine.find_job(&job_id).unwrap().revision - 1;

    let err = engine
        .apply(
            &actor,
            EngineCommand::ApproveJob {
                job_id,
                note: None,
                expected_revision: Some(stale_rev),
                at: 1000,
            },
        )
        .unwrap_err();

    assert!(matches!(
        err,
        TransitionError::RevisionConflict { expected, actual, .. }
        if expected == stale_rev && actual == stale_rev + 1
    ));
}

#[test]
fn queue_and_pull_emits_audit_events() {
    let mut engine = AfterlifeEngine::new();
    let new_id = engine.queue_preset_job("Test job", Some(&DistrictId::from("DIST-001")));
    assert_eq!(new_id.as_str(), "JOB-001");
    let pulled = engine
        .pull_next_job_into_district(&DistrictId::from("DIST-001"))
        .unwrap();
    assert_eq!(pulled, new_id);
    assert_eq!(engine.event_log.len(), 2);
}

#[test]
fn update_and_archive_enforce_cas_fencing_and_gate_guards() {
    let mut engine = AfterlifeEngine::new();
    let owner = engine.owner_id.clone();
    let agent = agent_actor(&owner, "worker-1");
    let human = Actor::Human {
        owner_id: owner.clone(),
        email: "owner@local".into(),
    };
    let gate_job_id = seed_job_at_approval_gate(&mut engine, "Approval gate guard target");

    let created = engine
        .apply(
            &agent,
            EngineCommand::QueueJob {
                title: "Initial".into(),
                raw_input: "raw".into(),
                target_district_hint: None,
                at: 1_000,
            },
        )
        .unwrap();
    let job_id = match &created[0] {
        JobEvent::JobQueued { job_id, .. } => job_id.clone(),
        _ => unreachable!(),
    };

    let err = engine
        .apply(
            &agent,
            EngineCommand::UpdateJob {
                job_id: job_id.clone(),
                title: None,
                raw_input: None,
                spec_summary: None,
                target_district_hint: None,
                expected_revision: None,
                fencing_token: None,
                at: 1_100,
            },
        )
        .unwrap_err();
    assert!(matches!(err, TransitionError::InvalidInput { .. }));

    engine
        .apply(
            &agent,
            EngineCommand::UpdateJob {
                job_id: job_id.clone(),
                title: Some("[LIN-9](https://linear.app/example/issue/LIN-9) Refined".into()),
                raw_input: None,
                spec_summary: Some("Refined spec".into()),
                target_district_hint: Some(DistrictId::from("DIST-001")),
                expected_revision: Some(1),
                fencing_token: None,
                at: 1_200,
            },
        )
        .unwrap();
    assert_eq!(engine.find_job(&job_id).unwrap().revision, 2);

    let err = engine
        .apply(
            &agent,
            EngineCommand::UpdateJob {
                job_id: gate_job_id.clone(),
                title: Some("Tamper approval gate".into()),
                raw_input: None,
                spec_summary: None,
                target_district_hint: None,
                expected_revision: None,
                fencing_token: None,
                at: 1_300,
            },
        )
        .unwrap_err();
    assert!(matches!(err, TransitionError::HumanApprovalRequired { .. }));

    let err = engine
        .apply(
            &agent,
            EngineCommand::ArchiveJob {
                job_id: gate_job_id,
                reason: Some("Suppress gate".into()),
                expected_revision: None,
                fencing_token: None,
                at: 1_400,
            },
        )
        .unwrap_err();
    assert!(matches!(err, TransitionError::HumanApprovalRequired { .. }));

    engine
        .apply(
            &agent,
            EngineCommand::ArchiveJob {
                job_id: job_id.clone(),
                reason: Some("Duplicate".into()),
                expected_revision: Some(2),
                fencing_token: None,
                at: 1_500,
            },
        )
        .unwrap();
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::Archived
    );

    let err = engine
        .apply(
            &human,
            EngineCommand::UpdateJob {
                job_id: job_id.clone(),
                title: Some("Edit after archive".into()),
                raw_input: None,
                spec_summary: None,
                target_district_hint: None,
                expected_revision: None,
                fencing_token: None,
                at: 1_600,
            },
        )
        .unwrap_err();
    assert!(matches!(err, TransitionError::InvalidLocation { .. }));
}
