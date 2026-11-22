use std::collections::BTreeMap;

use crate::*;

use super::{agent_actor, seed_job_at_approval_gate};

#[test]
fn agent_cannot_bypass_human_approval_gate_or_philosophy_approval() {
    let mut engine = AfterlifeEngine::new();
    let agent = agent_actor(&engine.owner_id, "cursor-agent");
    let job_id = seed_job_at_approval_gate(&mut engine, "Gate guard job");

    let claim_err = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("cursor-agent"),
                runner_label: "Cursor Agent".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: 2_000_000_000_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        claim_err,
        TransitionError::HumanApprovalRequired { .. }
    ));

    let assign_err = engine
        .apply(
            &agent,
            EngineCommand::AssignJobToDistrict {
                job_id: job_id.clone(),
                district_id: DistrictId::from("DIST-001"),
                expected_revision: None,
                at: 2_000_000_005_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        assign_err,
        TransitionError::HumanApprovalRequired { .. }
    ));

    let err = engine
        .apply(
            &agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields: BTreeMap::new(),
                artifact_ids: Vec::new(),
                expected_revision: None,
                fencing_token: None,
                at: 2_000_000_010_000,
            },
        )
        .unwrap_err();
    assert!(matches!(err, TransitionError::HumanApprovalRequired { .. }));

    let approve_err = engine
        .apply(
            &agent,
            EngineCommand::ApproveJob {
                job_id,
                note: None,
                expected_revision: None,
                at: 2_000_000_011_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        approve_err,
        TransitionError::HumanApprovalRequired { .. }
    ));
}

#[test]
fn owner_isolation_rejects_foreign_actor() {
    let mut engine = AfterlifeEngine::new();
    let intruder = Actor::local_human(OwnerId::from("other-user@company.com"));
    let err = engine
        .apply(
            &intruder,
            EngineCommand::QueueJob {
                title: "unauthorized".into(),
                raw_input: "nope".into(),
                target_district_hint: None,
                at: 1000,
            },
        )
        .unwrap_err();
    assert!(matches!(err, TransitionError::OwnerMismatch { .. }));
}

#[test]
fn feedback_and_philosophy_evolution_workflow() {
    let mut engine = AfterlifeEngine::new();
    let human = engine.local_actor();
    let frontier_model = agent_actor(&engine.owner_id, "claude-opus");
    let dist_id = DistrictId::from("DIST-001");
    let job_id = seed_job_at_approval_gate(&mut engine, "Evolution job");

    let fb_ev = engine
        .apply(
            &human,
            EngineCommand::SubmitFeedback {
                district_id: dist_id.clone(),
                job_id: Some(job_id.clone()),
                stage_id: Some(StageId::from("approve")),
                kind: FeedbackKind::PhilosophyInsight,
                body: "DBスキーマ変更時はダウンタイムゼロ互換マイグレーションを必須にしたい"
                    .into(),
                at: 1000,
            },
        )
        .unwrap();
    let fb_id = match &fb_ev[0] {
        JobEvent::FeedbackSubmitted { feedback_id, .. } => feedback_id.clone(),
        _ => unreachable!(),
    };

    let prop_ev = engine
        .apply(
            &frontier_model,
            EngineCommand::ProposePhilosophyUpdate {
                district_id: dist_id.clone(),
                base_version: PhilosophyVersion::new(1),
                summary: "計測駆動・型安全・無停止マイグレーション原則".into(),
                principles: vec![Principle {
                    id: "P-DEV-6".into(),
                    title: "Zero-Downtime Migrations".into(),
                    statement: "DBスキーマは必ず expand-and-contract で後方互換にする。".into(),
                    rationale: "ローリングデプロイ中の旧インスタンスと衝突させないため。"
                        .into(),
                }],
                anti_patterns: vec!["既存カラムの即時DROP".into()],
                decision_heuristics: vec![],
                rationale: "FBを反映しDB無停止原則を追加".into(),
                source_feedback_ids: vec![fb_id],
                at: 2000,
            },
        )
        .unwrap();
    let prop_id = match &prop_ev[0] {
        JobEvent::PhilosophyProposed { proposal_id, .. } => proposal_id.clone(),
        _ => unreachable!(),
    };

    let err = engine
        .apply(
            &frontier_model,
            EngineCommand::ApprovePhilosophyProposal {
                proposal_id: prop_id.clone(),
                at: 3000,
            },
        )
        .unwrap_err();
    assert!(matches!(err, TransitionError::HumanApprovalRequired { .. }));

    engine
        .apply(
            &human,
            EngineCommand::ApprovePhilosophyProposal {
                proposal_id: prop_id,
                at: 4000,
            },
        )
        .unwrap();
    assert_eq!(
        engine
            .find_district(&dist_id)
            .unwrap()
            .active_philosophy_version,
        PhilosophyVersion::new(2)
    );
    let ctx = engine.compile_execution_context(&job_id).unwrap();
    assert_eq!(ctx.philosophy.version, PhilosophyVersion::new(1));
    assert_eq!(ctx.recent_feedbacks.len(), 1);
}

#[test]
fn rework_moves_back_and_records_feedback() {
    let mut engine = AfterlifeEngine::new();
    let actor = engine.local_actor();
    let job_id = seed_job_at_approval_gate(&mut engine, "Rework target job");
    let records_before = engine.find_job(&job_id).unwrap().step_records.len();

    let events = engine
        .apply(
            &actor,
            EngineCommand::ReworkJob {
                job_id: job_id.clone(),
                expected_revision: None,
                fencing_token: None,
                reason: Some("テストケースが境界値不足".into()),
                at: 5000,
            },
        )
        .unwrap();
    assert_eq!(
        engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from("build"),
            sub_stage_id: Some(SubStageId::from("apply_change")),
        }
    );
    assert_eq!(engine.feedbacks.len(), 1);
    assert!(matches!(
        &events[0],
        JobEvent::StepRecorded {
            outcome: StepRecordOutcome::Reworked,
            stage_id,
            ..
        } if stage_id == &StageId::from("approve")
    ));
    assert!(events.iter().any(|e| matches!(e, JobEvent::JobReworked { .. })));

    let job = engine.find_job(&job_id).unwrap();
    assert_eq!(job.step_records.len(), records_before + 1);
    let rework_rec = job.step_records.last().unwrap();
    assert_eq!(rework_rec.outcome, StepRecordOutcome::Reworked);
    assert_eq!(rework_rec.stage_id, StageId::from("approve"));
    assert_eq!(rework_rec.sub_stage_id, None);
    assert_eq!(
        rework_rec.fields.get("reason").map(String::as_str),
        Some("テストケースが境界値不足")
    );

    let ctx = engine.compile_execution_context(&job_id).unwrap();
    assert_eq!(ctx.step_records.last(), Some(rework_rec));
}
