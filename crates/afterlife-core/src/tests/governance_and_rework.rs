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
                proposed_district_name: None,
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

#[test]
fn proposals_can_rename_district_on_human_approval_and_reject_blank_names() {
    let mut engine = AfterlifeEngine::new();
    let human = engine.local_actor();
    let agent = agent_actor(&engine.owner_id, "evolve-bot");
    let dist_id = DistrictId::from("DIST-002");

    let blank_err = engine
        .apply(
            &agent,
            EngineCommand::ProposePhilosophyUpdate {
                district_id: dist_id.clone(),
                base_version: PhilosophyVersion::new(1),
                proposed_district_name: Some("   ".into()),
                summary: "Updated summary".into(),
                principles: vec![Principle {
                    id: "P-1".into(),
                    title: "Title".into(),
                    statement: "Statement".into(),
                    rationale: "Rationale".into(),
                }],
                anti_patterns: vec![],
                decision_heuristics: vec![],
                rationale: "Check blank rejection".into(),
                source_feedback_ids: vec![],
                at: 1_800_000_000_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        blank_err,
        TransitionError::InvalidInput { ref field, .. } if field == "proposed_district_name"
    ));

    let phil_ev = engine
        .apply(
            &agent,
            EngineCommand::ProposePhilosophyUpdate {
                district_id: dist_id.clone(),
                base_version: PhilosophyVersion::new(1),
                proposed_district_name: Some("  DISTRICT 02 - Discovery & Shaping  ".into()),
                summary: "Updated philosophy".into(),
                principles: vec![Principle {
                    id: "P-1".into(),
                    title: "Title".into(),
                    statement: "Statement".into(),
                    rationale: "Rationale".into(),
                }],
                anti_patterns: vec![],
                decision_heuristics: vec![],
                rationale: "Evolve role".into(),
                source_feedback_ids: vec![],
                at: 1_800_000_001_000,
            },
        )
        .unwrap();
    let phil_pid = match &phil_ev[0] {
        JobEvent::PhilosophyProposed { proposal_id, .. } => proposal_id.clone(),
        _ => unreachable!(),
    };
    assert_eq!(engine.pending_philosophy_proposals().len(), 1);
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ApprovePhilosophyProposal {
                proposal_id: phil_pid.clone(),
                at: 1_800_000_002_000,
            },
        ),
        Err(TransitionError::HumanApprovalRequired { .. })
    ));

    engine
        .apply(
            &human,
            EngineCommand::ApprovePhilosophyProposal {
                proposal_id: phil_pid,
                at: 1_800_000_003_000,
            },
        )
        .unwrap();
    assert_eq!(
        engine.find_district(&dist_id).unwrap().name,
        "DISTRICT 02 - Discovery & Shaping"
    );
    assert!(engine.pending_philosophy_proposals().is_empty());

    let stages = engine.find_district(&dist_id).unwrap().stages().to_vec();
    let spec_ev = engine
        .apply(
            &agent,
            EngineCommand::ProposeSpecUpdate {
                district_id: dist_id.clone(),
                base_version: SpecVersion::new(1),
                proposed_district_name: Some("DISTRICT 02 - Shaped Delivery".into()),
                stages,
                changelog: "Rename via spec".into(),
                rationale: "Align district name with spec".into(),
                source_feedback_ids: vec![],
                at: 1_800_000_004_000,
            },
        )
        .unwrap();
    let spec_pid = match &spec_ev[0] {
        JobEvent::SpecProposed { proposal_id, .. } => proposal_id.clone(),
        _ => unreachable!(),
    };
    assert_eq!(engine.pending_spec_proposals().len(), 1);
    engine
        .apply(
            &human,
            EngineCommand::ApproveSpecProposal {
                proposal_id: spec_pid,
                at: 1_800_000_005_000,
            },
        )
        .unwrap();
    assert_eq!(
        engine.find_district(&dist_id).unwrap().name,
        "DISTRICT 02 - Shaped Delivery"
    );
    assert!(engine.pending_spec_proposals().is_empty());
}

#[test]
fn template_proposal_requires_human_approval_and_validates_sub_stages() {
    let mut engine = AfterlifeEngine::new();
    let human = engine.local_actor();
    let agent = agent_actor(&engine.owner_id, "template-agent");
    let tid = SubMachineTemplateId::from("tmpl-evidence");

    let empty_err = engine
        .apply(
            &agent,
            EngineCommand::ProposeTemplateUpdate {
                template_id: tid.clone(),
                name: "Evidence".into(),
                description: "Desc".into(),
                philosophy_notes: "Notes".into(),
                stages: vec![],
                rationale: "Empty stages must fail".into(),
                source_feedback_ids: vec![],
                at: 1_800_000_010_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        empty_err,
        TransitionError::InvalidProposedTemplate { .. }
    ));

    let mut updated_stages = engine.templates[&tid].stages.clone();
    updated_stages[0].procedure = "Trace primary sources thoroughly.".into();
    let dup_stage = updated_stages[0].clone();
    let dup_err = engine
        .apply(
            &agent,
            EngineCommand::ProposeTemplateUpdate {
                template_id: tid.clone(),
                name: "Evidence & Probe".into(),
                description: "Updated evidence template".into(),
                philosophy_notes: "Verify first.".into(),
                stages: vec![updated_stages[0].clone(), dup_stage],
                rationale: "Duplicate sub-stage IDs must fail".into(),
                source_feedback_ids: vec![],
                at: 1_800_000_011_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        dup_err,
        TransitionError::InvalidProposedTemplate { .. }
    ));

    let prop_ev = engine
        .apply(
            &agent,
            EngineCommand::ProposeTemplateUpdate {
                template_id: tid.clone(),
                name: "Evidence & Probe".into(),
                description: "Updated evidence template".into(),
                philosophy_notes: "Verify first.".into(),
                stages: updated_stages.clone(),
                rationale: "Clarify trace_sources procedure".into(),
                source_feedback_ids: vec![],
                at: 1_800_000_012_000,
            },
        )
        .unwrap();
    let prop_id = match &prop_ev[0] {
        JobEvent::TemplateProposed { proposal_id, .. } => proposal_id.clone(),
        _ => unreachable!(),
    };
    assert_eq!(engine.pending_template_proposals().len(), 1);

    let agent_approve = engine
        .apply(
            &agent,
            EngineCommand::ApproveTemplateProposal {
                proposal_id: prop_id.clone(),
                at: 1_800_000_013_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        agent_approve,
        TransitionError::HumanApprovalRequired { .. }
    ));

    engine
        .apply(
            &human,
            EngineCommand::ApproveTemplateProposal {
                proposal_id: prop_id.clone(),
                at: 1_800_000_014_000,
            },
        )
        .unwrap();
    assert_eq!(
        engine.templates[&tid].description,
        "Updated evidence template"
    );
    assert!(engine.pending_template_proposals().is_empty());

    let double_approve = engine
        .apply(
            &human,
            EngineCommand::ApproveTemplateProposal {
                proposal_id: prop_id,
                at: 1_800_000_015_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        double_approve,
        TransitionError::ProposalAlreadyResolved(_)
    ));
}

#[test]
fn sync_builtin_templates_requires_human_and_preserves_custom_edits() {
    let mut engine = AfterlifeEngine::new();
    let human = engine.local_actor();
    let agent = agent_actor(&engine.owner_id, "sync-agent");
    let ev_id = SubMachineTemplateId::from("tmpl-evidence");
    let obs_id = SubMachineTemplateId::from("tmpl-observe");
    let custom_id = SubMachineTemplateId::from("tmpl-custom-jp");

    engine.templates.get_mut(&ev_id).unwrap().description =
        "一次仕様・既存実装トレースから隔離環境での実機検証、判断基準と前提の確定まで".into();
    engine.templates.get_mut(&obs_id).unwrap().description =
        "ユーザーが意図的にカスタマイズした観測テンプレート".into();
    engine.templates.insert(
        custom_id.clone(),
        SubMachineTemplate {
            id: custom_id.clone(),
            name: "Custom JP".into(),
            description: "カスタムSM".into(),
            philosophy_notes: "保持必須".into(),
            stages: vec![SubStageDef {
                id: SubStageId::from("custom_step"),
                label: "Custom Step".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure: "Run custom check".into(),
                gate_checks: vec!["Check passed".into()],
                record_fields: vec![],
                required_artifact_kinds: vec![],
            }],
        },
    );
    engine.template_order.push(custom_id.clone());

    assert_eq!(engine.outdated_builtin_template_ids(), vec![ev_id.clone()]);

    let agent_err = engine
        .apply(
            &agent,
            EngineCommand::SyncBuiltinTemplates {
                template_ids: None,
                at: 1_800_000_020_000,
            },
        )
        .unwrap_err();
    assert!(matches!(
        agent_err,
        TransitionError::HumanApprovalRequired { .. }
    ));

    let events = engine
        .apply(
            &human,
            EngineCommand::SyncBuiltinTemplates {
                template_ids: None,
                at: 1_800_000_021_000,
            },
        )
        .unwrap();
    match &events[0] {
        JobEvent::BuiltinTemplatesSynced {
            synced_template_ids,
            skipped_template_ids,
            ..
        } => {
            assert_eq!(synced_template_ids, &vec![ev_id.clone()]);
            assert!(skipped_template_ids.contains(&obs_id));
        }
        other => panic!("expected BuiltinTemplatesSynced, got {other:?}"),
    }
    assert!(engine.outdated_builtin_template_ids().is_empty());
    assert!(
        engine.templates[&ev_id]
            .description
            .is_ascii()
    );
    assert_eq!(
        engine.templates[&obs_id].description,
        "ユーザーが意図的にカスタマイズした観測テンプレート"
    );
    assert_eq!(engine.templates[&custom_id].description, "カスタムSM");

    let unknown_err = engine
        .apply(
            &human,
            EngineCommand::SyncBuiltinTemplates {
                template_ids: Some(vec![SubMachineTemplateId::from("tmpl-does-not-exist")]),
                at: 1_800_000_022_000,
            },
        )
        .unwrap_err();
    assert!(matches!(unknown_err, TransitionError::TemplateNotFound(_)));
}
