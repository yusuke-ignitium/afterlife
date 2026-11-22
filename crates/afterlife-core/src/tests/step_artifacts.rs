use crate::*;

use super::{advance_one_step_as_agent, agent_actor, build_required_fields, seed_job_in_district};

#[test]
fn required_artifact_kinds_preserves_any_of_or_semantics() {
    let mut engine = AfterlifeEngine::new();
    let agent = agent_actor(&engine.owner_id, "or-guard-runner");
    let job_id = seed_job_in_district(&mut engine, "OR artifact guard");
    let mut ts = 1_950_000_000_000u64;

    for _ in 0..4 {
        advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
        ts += 100;
    }
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("baseline_guard"),
            sub_stage_id: Some(SubStageId::from("capture_baseline")),
        }
    );

    let claim = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("or-guard-runner"),
                runner_label: "OR Guard Runner".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: ts,
            },
        )
        .unwrap();
    let tok = match &claim[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    let attach = engine
        .apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::VerificationReport,
                label: "baseline-report.json".into(),
                uri: "https://example.com/baseline-report.json".into(),
                sha256: None,
                fencing_token: Some(tok),
                at: ts + 10,
            },
        )
        .unwrap();
    let art_id = match &attach[0] {
        JobEvent::ArtifactAttached { artifact_id, .. } => artifact_id.clone(),
        _ => unreachable!(),
    };

    let ctx = engine.compile_execution_context(&job_id).unwrap();
    assert_eq!(
        ctx.required_artifact_kinds,
        vec![ArtifactKind::Snapshot, ArtifactKind::VerificationReport]
    );
    engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: build_required_fields(&ctx),
                artifact_ids: vec![art_id.clone()],
                expected_revision: None,
                fencing_token: Some(tok),
                at: ts + 20,
            },
        )
        .unwrap();

    let last_rec = engine.find_job(&job_id).unwrap().step_records.last().unwrap();
    assert_eq!(last_rec.artifact_ids, vec![art_id]);
}

#[test]
fn required_artifact_kinds_rejects_unlinked_cross_substage_and_pre_rework_artifacts() {
    let mut engine = AfterlifeEngine::new();
    let human = engine.local_actor();
    let agent = agent_actor(&engine.owner_id, "strict-artifact-runner");
    let job_id = seed_job_in_district(&mut engine, "Strict artifact contract");
    let mut ts = 1_960_000_000_000u64;

    for _ in 0..4 {
        advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
        ts += 100;
    }
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("baseline_guard"),
            sub_stage_id: Some(SubStageId::from("capture_baseline")),
        }
    );

    let claim = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("strict-artifact-runner"),
                runner_label: "Strict Artifact Runner".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: ts,
            },
        )
        .unwrap();
    let tok = match &claim[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    let attach = engine
        .apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::Snapshot,
                label: "before.json".into(),
                uri: "https://example.com/before.json".into(),
                sha256: None,
                fencing_token: Some(tok),
                at: ts + 10,
            },
        )
        .unwrap();
    let snap_art_id = match &attach[0] {
        JobEvent::ArtifactAttached { artifact_id, .. } => artifact_id.clone(),
        _ => unreachable!(),
    };
    let attached_art = engine
        .find_job(&job_id)
        .unwrap()
        .artifacts
        .iter()
        .find(|a| a.id == snap_art_id)
        .unwrap();
    assert_eq!(
        attached_art.produced_at_sub_stage,
        Some(SubStageId::from("capture_baseline"))
    );

    let ctx = engine.compile_execution_context(&job_id).unwrap();
    let unlinked_err = engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: build_required_fields(&ctx),
                artifact_ids: Vec::new(),
                expected_revision: None,
                fencing_token: Some(tok),
                at: ts + 20,
            },
        )
        .unwrap_err();
    assert!(matches!(
        unlinked_err,
        TransitionError::StepRecordIncomplete {
            ref missing_fields,
            ref missing_artifact_kinds,
            ..
        } if missing_fields.is_empty()
            && missing_artifact_kinds.contains(&ArtifactKind::Snapshot)
    ));

    let wrong_kind_attach = engine
        .apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::RawNote,
                label: "note-only.txt".into(),
                uri: "https://example.com/note-only.txt".into(),
                sha256: None,
                fencing_token: Some(tok),
                at: ts + 22,
            },
        )
        .unwrap();
    let wrong_kind_id = match &wrong_kind_attach[0] {
        JobEvent::ArtifactAttached { artifact_id, .. } => artifact_id.clone(),
        _ => unreachable!(),
    };
    let wrong_kind_err = engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: build_required_fields(&ctx),
                artifact_ids: vec![wrong_kind_id.clone()],
                expected_revision: None,
                fencing_token: Some(tok),
                at: ts + 25,
            },
        )
        .unwrap_err();
    assert!(matches!(
        wrong_kind_err,
        TransitionError::StepRecordIncomplete {
            ref missing_artifact_kinds,
            ..
        } if missing_artifact_kinds.contains(&ArtifactKind::Snapshot)
    ));

    engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: build_required_fields(&ctx),
                artifact_ids: vec![wrong_kind_id, snap_art_id],
                expected_revision: None,
                fencing_token: Some(tok),
                at: ts + 30,
            },
        )
        .unwrap();
    ts += 100;

    for _ in 0..4 {
        advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
        ts += 100;
    }
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("verify"),
            sub_stage_id: Some(SubStageId::from("live_verify")),
        }
    );

    let claim_va = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("strict-artifact-runner"),
                runner_label: "Strict Artifact Runner".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: ts,
            },
        )
        .unwrap();
    let tok_va = match &claim_va[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };
    let cross_attach = engine
        .apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::VerificationReport,
                label: "wrong-substage-report.json".into(),
                uri: "https://example.com/wrong-substage.json".into(),
                sha256: None,
                fencing_token: Some(tok_va),
                at: ts + 10,
            },
        )
        .unwrap();
    let cross_sub_art_id = match &cross_attach[0] {
        JobEvent::ArtifactAttached { artifact_id, .. } => artifact_id.clone(),
        _ => unreachable!(),
    };
    let ctx_va = engine.compile_execution_context(&job_id).unwrap();
    engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: build_required_fields(&ctx_va),
                artifact_ids: Vec::new(),
                expected_revision: None,
                fencing_token: Some(tok_va),
                at: ts + 20,
            },
        )
        .unwrap();
    ts += 100;

    advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
    ts += 100;
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("verify"),
            sub_stage_id: Some(SubStageId::from("bundle_evidence")),
        }
    );

    let claim_be1 = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("strict-artifact-runner"),
                runner_label: "Strict Artifact Runner".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: ts,
            },
        )
        .unwrap();
    let tok_be1 = match &claim_be1[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };
    let ctx_be1 = engine.compile_execution_context(&job_id).unwrap();
    let cross_err = engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: build_required_fields(&ctx_be1),
                artifact_ids: vec![cross_sub_art_id],
                expected_revision: None,
                fencing_token: Some(tok_be1),
                at: ts + 10,
            },
        )
        .unwrap_err();
    assert!(matches!(
        cross_err,
        TransitionError::StepRecordIncomplete {
            ref missing_artifact_kinds,
            ..
        } if missing_artifact_kinds.contains(&ArtifactKind::VerificationReport)
    ));

    let be1_attach = engine
        .apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::VerificationReport,
                label: "round1-report.json".into(),
                uri: "https://example.com/round1.json".into(),
                sha256: None,
                fencing_token: Some(tok_be1),
                at: ts + 20,
            },
        )
        .unwrap();
    let round1_art_id = match &be1_attach[0] {
        JobEvent::ArtifactAttached { artifact_id, .. } => artifact_id.clone(),
        _ => unreachable!(),
    };
    let checks = ctx_be1
        .stage
        .gate_rubric
        .as_ref()
        .unwrap()
        .checks
        .iter()
        .map(|c| (c.clone(), true))
        .collect();
    engine
        .apply(
            &agent,
            EngineCommand::RecordGateVerification {
                job_id: job_id.clone(),
                verdict: GateVerdict::Pass,
                summary: "Round 1 verified".into(),
                checklist_results: checks,
                fencing_token: Some(tok_be1),
                at: ts + 30,
            },
        )
        .unwrap();
    engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: build_required_fields(&ctx_be1),
                artifact_ids: vec![round1_art_id.clone()],
                expected_revision: None,
                fencing_token: Some(tok_be1),
                at: ts + 40,
            },
        )
        .unwrap();
    ts += 100;

    engine
        .apply(
            &human,
            EngineCommand::ReworkJob {
                job_id: job_id.clone(),
                expected_revision: None,
                fencing_token: None,
                reason: None,
                at: ts,
            },
        )
        .unwrap();
    assert_eq!(
        engine
            .find_job(&job_id)
            .unwrap()
            .step_records
            .last()
            .unwrap()
            .outcome,
        StepRecordOutcome::Reworked
    );
    assert!(
        engine
            .find_job(&job_id)
            .unwrap()
            .step_records
            .last()
            .unwrap()
            .fields
            .is_empty()
    );
    ts += 100;

    for _ in 0..5 {
        advance_one_step_as_agent(&mut engine, &job_id, &agent, ts);
        ts += 100;
    }
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("verify"),
            sub_stage_id: Some(SubStageId::from("bundle_evidence")),
        }
    );

    let claim_be2 = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("strict-artifact-runner"),
                runner_label: "Strict Artifact Runner".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: ts,
            },
        )
        .unwrap();
    let tok_be2 = match &claim_be2[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };
    let ctx_be2 = engine.compile_execution_context(&job_id).unwrap();
    let stale_rework_err = engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: build_required_fields(&ctx_be2),
                artifact_ids: vec![round1_art_id],
                expected_revision: None,
                fencing_token: Some(tok_be2),
                at: ts + 10,
            },
        )
        .unwrap_err();
    assert!(matches!(
        stale_rework_err,
        TransitionError::StepRecordIncomplete {
            ref missing_artifact_kinds,
            ..
        } if missing_artifact_kinds.contains(&ArtifactKind::VerificationReport)
    ));

    let be2_attach = engine
        .apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::VerificationReport,
                label: "round2-report.json".into(),
                uri: "https://example.com/round2.json".into(),
                sha256: None,
                fencing_token: Some(tok_be2),
                at: ts + 20,
            },
        )
        .unwrap();
    let round2_art_id = match &be2_attach[0] {
        JobEvent::ArtifactAttached { artifact_id, .. } => artifact_id.clone(),
        _ => unreachable!(),
    };
    engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: build_required_fields(&ctx_be2),
                artifact_ids: vec![round2_art_id.clone()],
                expected_revision: None,
                fencing_token: Some(tok_be2),
                at: ts + 30,
            },
        )
        .unwrap();
    assert_eq!(
        engine
            .find_job(&job_id)
            .unwrap()
            .step_records
            .last()
            .unwrap()
            .artifact_ids,
        vec![round2_art_id]
    );
}
