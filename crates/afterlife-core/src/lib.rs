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
    MAX_ARTIFACT_BLOB_BYTES, ParsedArtifactUri, ParsedBlobUri, ParsedInlineText, Philosophy,
    PhilosophyProposal, Principle, ProcedureSpec, ProposalStatus, RunnerKind, SpecProposal,
    StageDef, StageKind, STEP_FIELD_DECISION_LABEL, STEP_FIELD_EXECUTION_LABEL,
    STEP_FIELD_NOTE_LABEL, STEP_FIELD_REASON_LABEL, StepFieldDef, StepRecord, StepRecordOutcome,
    SubMachineTemplate, SubStageDef, builtin_step_field_label, default_step_fields,
    format_blob_uri, is_openable_url, is_valid_lower_sha256, parse_artifact_uri, parse_blob_uri,
    parse_inline_links, sanitize_blob_filename, sanitize_owner_segment,
};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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

    fn build_required_fields(ctx: &ExecutionContext) -> BTreeMap<String, String> {
        ctx.required_step_fields
            .iter()
            .map(|f| (f.key.clone(), format!("Recorded {} for {}", f.key, ctx.stage.id)))
            .collect()
    }

    fn advance_one_step_as_agent(
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

    fn seed_job_at_approval_gate(engine: &mut AfterlifeEngine, title: &str) -> JobId {
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

    #[test]
    fn default_workspace_passes_graph_validation() {
        let engine = AfterlifeEngine::new();
        assert!(engine.validate().is_ok());
        assert!(engine.jobs.is_empty());
        assert!(engine.queued_jobs().is_empty());
        assert!(engine.approval_jobs().is_empty());
        assert_eq!(engine.districts.len(), 5);
        assert_eq!(engine.template_order.len(), 7);
        for district in &engine.districts {
            assert_eq!(
                district.active_philosophy().version,
                PhilosophyVersion::new(1)
            );
            let stages = district.stages();
            assert!(
                stages.iter().any(|s| s.is_gate()),
                "district {} must include a human approval gate",
                district.id
            );
            assert!(
                stages.iter().any(|s| s.is_terminal()),
                "district {} must include a terminal stage",
                district.id
            );
        }
        let serialized = format!("{:?}{:?}", engine.districts, engine.templates);
        for forbidden in [
            "gpu-perf-optimize",
            "AGENTS.md",
            ".cursor",
            ".agents",
            "Antigravity",
            "Cursor",
            "cargo",
            "Metal",
            "1 分で",
            "1分で",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "generic workspace seed must not contain environment-specific or time-boxed token `{forbidden}`"
            );
        }
    }

    fn contains_cjk_or_fullwidth(text: &str) -> bool {
        text.chars().any(|c| {
            matches!(
                c,
                '\u{3000}'..='\u{303F}'
                    | '\u{3040}'..='\u{30FF}'
                    | '\u{3400}'..='\u{4DBF}'
                    | '\u{4E00}'..='\u{9FFF}'
                    | '\u{FF00}'..='\u{FFEF}'
            )
        })
    }

    #[test]
    fn builtin_workspace_defaults_and_step_fields_are_english_only() {
        assert!(contains_cjk_or_fullwidth("日本語"));
        assert!(!contains_cjk_or_fullwidth("English-only default text (v1)"));

        let engine = AfterlifeEngine::new();
        let districts_debug = format!("{:?}", engine.districts);
        let templates_debug = format!("{:?}", engine.templates);
        assert!(
            !contains_cjk_or_fullwidth(&districts_debug),
            "default districts must not contain Japanese or full-width characters"
        );
        assert!(
            !contains_cjk_or_fullwidth(&templates_debug),
            "default sub-machine templates must not contain Japanese or full-width characters"
        );

        let defaults = default_step_fields();
        assert_eq!(defaults.len(), 2);
        for field in &defaults {
            assert!(!field.key.trim().is_empty());
            assert!(!field.label.trim().is_empty());
            assert!(!field.prompt.trim().is_empty());
            assert!(
                !contains_cjk_or_fullwidth(&field.label)
                    && !contains_cjk_or_fullwidth(&field.prompt),
                "default_step_fields entry `{}` must be English-only",
                field.key
            );
            assert_eq!(
                builtin_step_field_label(&field.key),
                Some(field.label.as_str())
            );
        }

        for key in ["decision", "execution", "note", "reason"] {
            let label = builtin_step_field_label(key)
                .unwrap_or_else(|| panic!("missing builtin label for `{key}`"));
            assert!(!label.trim().is_empty());
            assert!(
                !contains_cjk_or_fullwidth(label),
                "builtin fallback label for `{key}` must be English-only"
            );
        }
        assert_eq!(builtin_step_field_label("custom_user_key"), None);
    }

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

    #[test]
    fn find_stage_for_job_prefers_pinned_spec_version_and_falls_back_to_active() {
        let mut engine = AfterlifeEngine::new();
        let human = engine.local_actor();
        let agent = agent_actor(&engine.owner_id, "spec-runner");
        let dist_id = DistrictId::from("DIST-001");
        let v1_job_id = seed_job_in_district(&mut engine, "V1 pinned job");

        let mut v2_stages = engine
            .find_district(&dist_id)
            .unwrap()
            .active_spec()
            .stages
            .clone();
        let design = v2_stages
            .iter_mut()
            .find(|s| s.id == StageId::from("design"))
            .unwrap();
        design.label = "Design V2".into();

        let prop = engine
            .apply(
                &agent,
                EngineCommand::ProposeSpecUpdate {
                    district_id: dist_id.clone(),
                    base_version: SpecVersion::new(1),
                    stages: v2_stages,
                    changelog: "Rename design stage in v2".into(),
                    rationale: "Test pinned spec resolution".into(),
                    source_feedback_ids: Vec::new(),
                    at: 2_100_000_000_000,
                },
            )
            .unwrap();
        let prop_id = match &prop[0] {
            JobEvent::SpecProposed { proposal_id, .. } => proposal_id.clone(),
            _ => unreachable!(),
        };
        engine
            .apply(
                &human,
                EngineCommand::ApproveSpecProposal {
                    proposal_id: prop_id,
                    at: 2_100_000_001_000,
                },
            )
            .unwrap();

        let v2_job_id = seed_job_in_district(&mut engine, "V2 pinned job");
        let unpinned_job_id = engine.queue_preset_job("Unpinned queued job", Some(&dist_id));

        let v1_job = engine.find_job(&v1_job_id).unwrap();
        let v2_job = engine.find_job(&v2_job_id).unwrap();
        let unpinned_job = engine.find_job(&unpinned_job_id).unwrap();

        assert_eq!(
            engine
                .find_stage_for_job(v1_job, &dist_id, &StageId::from("design"))
                .unwrap()
                .label,
            "Design Whole"
        );
        assert_eq!(
            engine
                .find_stage_for_job(v2_job, &dist_id, &StageId::from("design"))
                .unwrap()
                .label,
            "Design V2"
        );
        assert_eq!(
            engine
                .find_stage_for_job(unpinned_job, &dist_id, &StageId::from("design"))
                .unwrap()
                .label,
            "Design V2"
        );
    }

    #[test]
    fn external_openable_artifact_uris_accepted_on_attach() {
        let mut engine = AfterlifeEngine::new();
        let agent = agent_actor(&engine.owner_id, "uri-guard-runner");
        let job_id = seed_job_in_district(&mut engine, "External URI guard");
        let claim = engine
            .apply(
                &agent,
                EngineCommand::ClaimJob {
                    job_id: job_id.clone(),
                    runner_id: RunnerId::from("uri-guard-runner"),
                    runner_label: "URI Guard Runner".into(),
                    ttl_ms: 60_000,
                    expected_revision: None,
                    at: 2_200_000_000_000,
                },
            )
            .unwrap();
        let tok = match &claim[0] {
            JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
            _ => unreachable!(),
        };

        for (idx, uri) in [
            "https://github.com/org/repo/pull/1",
            "http://127.0.0.1:4319/v1/health",
            "linear://team/issue/ENG-42",
            "slack://channel?team=T1&id=C1",
        ]
        .iter()
        .enumerate()
        {
            engine
                .apply(
                    &agent,
                    EngineCommand::AttachArtifact {
                        job_id: job_id.clone(),
                        kind: ArtifactKind::Issue,
                        label: format!("ext-{idx}"),
                        uri: (*uri).to_string(),
                        sha256: None,
                        fencing_token: Some(tok),
                        at: 2_200_000_000_100 + idx as u64,
                    },
                )
                .unwrap();
        }

        for bad in ["javascript:alert(1)", "data:text/plain,hi", "vbscript:x"] {
            let err = engine
                .apply(
                    &agent,
                    EngineCommand::AttachArtifact {
                        job_id: job_id.clone(),
                        kind: ArtifactKind::Spec,
                        label: "bad".into(),
                        uri: bad.into(),
                        sha256: None,
                        fencing_token: Some(tok),
                        at: 2_200_000_000_200,
                    },
                )
                .unwrap_err();
            assert!(matches!(err, TransitionError::InvalidInput { .. }));
        }
    }

    #[test]
    fn attach_artifact_rejects_fictional_uris_and_mismatched_blob_sha256() {
        let mut engine = AfterlifeEngine::new();
        let agent = agent_actor(&engine.owner_id, "uri-strict-runner");
        let job_id = seed_job_in_district(&mut engine, "Strict URI contract");
        let claim = engine
            .apply(
                &agent,
                EngineCommand::ClaimJob {
                    job_id: job_id.clone(),
                    runner_id: RunnerId::from("uri-strict-runner"),
                    runner_label: "Strict URI Runner".into(),
                    ttl_ms: 60_000,
                    expected_revision: None,
                    at: 2_300_000_000_000,
                },
            )
            .unwrap();
        let tok = match &claim[0] {
            JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
            _ => unreachable!(),
        };

        for fictional in [
            "evidence://job-001/before-baseline.json",
            "file:///etc/passwd",
            "s3://bucket/key.json",
            "queue-drop.json",
            "blob://local-owner/short-sha/report.json",
            "blob://local-owner/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/../evil.json",
        ] {
            let err = engine
                .apply(
                    &agent,
                    EngineCommand::AttachArtifact {
                        job_id: job_id.clone(),
                        kind: ArtifactKind::Snapshot,
                        label: "fictional".into(),
                        uri: fictional.into(),
                        sha256: None,
                        fencing_token: Some(tok),
                        at: 2_300_000_000_100,
                    },
                )
                .unwrap_err();
            assert!(
                matches!(err, TransitionError::InvalidInput { .. }),
                "expected InvalidInput for `{fictional}`, got {err:?}"
            );
        }

        let sha_a = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let sha_b = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
        let blob_uri = format_blob_uri(engine.owner_id.as_str(), sha_a, "report.json");

        let mismatch_err = engine
            .apply(
                &agent,
                EngineCommand::AttachArtifact {
                    job_id: job_id.clone(),
                    kind: ArtifactKind::VerificationReport,
                    label: "report.json".into(),
                    uri: blob_uri.clone(),
                    sha256: Some(sha_b.into()),
                    fencing_token: Some(tok),
                    at: 2_300_000_000_200,
                },
            )
            .unwrap_err();
        assert!(matches!(mismatch_err, TransitionError::InvalidInput { .. }));

        engine
            .apply(
                &agent,
                EngineCommand::AttachArtifact {
                    job_id: job_id.clone(),
                    kind: ArtifactKind::VerificationReport,
                    label: "report.json".into(),
                    uri: format!("  {blob_uri}  "),
                    sha256: Some(sha_a.into()),
                    fencing_token: Some(tok),
                    at: 2_300_000_000_300,
                },
            )
            .unwrap();
        let saved = engine
            .find_job(&job_id)
            .unwrap()
            .artifacts
            .last()
            .unwrap();
        assert_eq!(saved.uri, blob_uri);
    }
}
