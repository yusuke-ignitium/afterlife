pub mod engine;
pub mod error;
pub mod fixtures;
pub mod ids;
pub mod model;

pub use engine::{AfterlifeEngine, EngineCommand, validate_stages, validate_workspace};
pub use error::{TransitionError, ValidationError};
pub use ids::{
    ArtifactId, CommandId, DistrictId, FeedbackId, FencingToken, JobId, LeaseId, OwnerId,
    PhilosophyVersion, ProposalId, RunnerId, SpecVersion, StageId, SubMachineTemplateId,
    SubStageId, TimestampMs,
};
pub use model::{
    Actor, Artifact, ArtifactKind, District, ExecutionContext, Feedback, FeedbackKind, GateRubric,
    GateVerdict, GateVerification, InlineLink, Job, JobEvent, JobLease, JobLocation,
    ParsedInlineText, Philosophy, PhilosophyProposal, Principle, ProcedureSpec, ProposalStatus,
    RunnerKind, SpecProposal, StageDef, StageKind, SubMachineTemplate, SubStageDef,
    is_openable_url, parse_inline_links,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_actor(owner: &OwnerId, name: &str) -> Actor {
        Actor::Agent {
            owner_id: owner.clone(),
            client_id: format!("client-{name}"),
            runner_label: name.into(),
            token_jti: format!("jti-{name}-1"),
        }
    }

    fn seed_job_in_district(engine: &mut AfterlifeEngine, title: &str) -> JobId {
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

    fn seed_job_at_approval_gate(engine: &mut AfterlifeEngine, title: &str) -> JobId {
        let job_id = seed_job_in_district(engine, title);
        for _ in 0..32 {
            let job = engine.find_job(&job_id).unwrap();
            if engine.is_job_at_approval_gate(job) {
                return job_id;
            }
            engine.advance_job(&job_id).unwrap();
        }
        panic!("job {job_id} did not reach approval gate");
    }

    #[test]
    fn default_workspace_passes_graph_validation() {
        let engine = AfterlifeEngine::new();
        assert!(engine.validate().is_ok());
        assert!(engine.jobs.is_empty());
        assert!(engine.queued_jobs().is_empty());
        assert!(engine.approval_jobs().is_empty());
        assert_eq!(engine.districts.len(), 1);
        assert_eq!(engine.template_order.len(), 4);
        assert_eq!(
            engine.districts[0].active_philosophy().version,
            PhilosophyVersion::new(1)
        );
    }

    #[test]
    fn advance_steps_through_sub_machine_before_parent_stage() {
        let mut engine = AfterlifeEngine::new();
        let job_id = seed_job_in_district(&mut engine, "Sub-stage stepping test");

        assert_eq!(
            engine.find_job(&job_id).unwrap().location,
            JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("discover"),
                sub_stage_id: Some(SubStageId::from("trace_sources")),
            }
        );

        engine.advance_job(&job_id).unwrap();
        assert_eq!(
            engine.find_job(&job_id).unwrap().location,
            JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("discover"),
                sub_stage_id: Some(SubStageId::from("isolate_probe")),
            }
        );

        engine.advance_job(&job_id).unwrap();
        assert_eq!(
            engine.find_job(&job_id).unwrap().location,
            JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("discover"),
                sub_stage_id: Some(SubStageId::from("lock_scope")),
            }
        );

        engine.advance_job(&job_id).unwrap();
        assert_eq!(
            engine.find_job(&job_id).unwrap().location,
            JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("design"),
                sub_stage_id: None,
            }
        );

        engine.advance_job(&job_id).unwrap();
        assert_eq!(
            engine.find_job(&job_id).unwrap().location,
            JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("baseline_guard"),
                sub_stage_id: Some(SubStageId::from("capture_baseline")),
            }
        );

        engine.advance_job(&job_id).unwrap();
        assert_eq!(
            engine.find_job(&job_id).unwrap().location,
            JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("baseline_guard"),
                sub_stage_id: Some(SubStageId::from("lock_regression_guards")),
            }
        );

        for _ in 0..7 {
            engine.advance_job(&job_id).unwrap();
        }
        assert_eq!(
            engine.find_job(&job_id).unwrap().location,
            JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("approve"),
                sub_stage_id: None,
            }
        );

        engine.advance_job(&job_id).unwrap();
        assert!(matches!(
            engine.find_job(&job_id).unwrap().location,
            JobLocation::Completed {
                ref terminal_stage_id,
                ..
            } if terminal_stage_id == &StageId::from("shipped")
        ));

        assert!(matches!(
            engine.advance_job(&job_id),
            Err(TransitionError::InvalidLocation { .. })
        ));
    }

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
                    job_id,
                    expected_revision: None,
                    fencing_token: None,
                    at: 2_000_000_010_000,
                },
            )
            .unwrap_err();
        assert!(matches!(err, TransitionError::HumanApprovalRequired { .. }));
    }

    #[test]
    fn fencing_token_rejects_zombie_agent_after_lease_takeover() {
        let mut engine = AfterlifeEngine::new();
        let agent_a = agent_actor(&engine.owner_id, "claude");
        let agent_b = agent_actor(&engine.owner_id, "codex");
        let job_id = seed_job_in_district(&mut engine, "Lease fencing job");
        engine.advance_job(&job_id).unwrap();
        engine.advance_job(&job_id).unwrap();

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
                    uri: "s3://bucket/stale.rs".into(),
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

        let gate_err = engine
            .apply(
                &agent_b,
                EngineCommand::AdvanceJob {
                    job_id: job_id.clone(),
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
                    expected_revision: None,
                    fencing_token: Some(token_b),
                    at: t0 + 21_000,
                },
            )
            .unwrap();
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
    fn optimistic_cas_detects_revision_conflict() {
        let mut engine = AfterlifeEngine::new();
        let actor = engine.local_actor();
        let job_id = seed_job_at_approval_gate(&mut engine, "CAS conflict job");
        let stale_rev = engine.find_job(&job_id).unwrap().revision - 1;

        let err = engine
            .apply(
                &actor,
                EngineCommand::AdvanceJob {
                    job_id,
                    expected_revision: Some(stale_rev),
                    fencing_token: None,
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
    fn rework_moves_back_and_records_feedback() {
        let mut engine = AfterlifeEngine::new();
        let actor = engine.local_actor();
        let job_id = seed_job_at_approval_gate(&mut engine, "Rework target job");

        engine
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
                sub_stage_id: Some(SubStageId::from("implement_core")),
            }
        );
        assert_eq!(engine.feedbacks.len(), 1);
    }

    #[test]
    fn cannot_attach_sub_template_to_gate_or_terminal() {
        let mut engine = AfterlifeEngine::new();
        let actor = engine.local_actor();
        let err = engine
            .apply(
                &actor,
                EngineCommand::SetStageSubTemplate {
                    district_id: DistrictId::from("DIST-001"),
                    stage_id: StageId::from("approve"),
                    template_id: Some(SubMachineTemplateId::from("tmpl-build-loop")),
                    at: 1000,
                },
            )
            .unwrap_err();

        assert!(matches!(err, TransitionError::SubTemplateNotAllowed { .. }));
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
}
