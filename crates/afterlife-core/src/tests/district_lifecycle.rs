use crate::*;

use super::agent_actor;

fn minimal_valid_stages() -> Vec<StageDef> {
    vec![
        StageDef {
            id: StageId::from("work"),
            label: "Execute Work".into(),
            runner: "AI Agent".into(),
            runner_kind: RunnerKind::Ai,
            kind: StageKind::Work,
            procedure: "Execute task according to philosophy.".into(),
            gate_rubric: Some(GateRubric {
                checks: vec!["Output verified".into()],
                auto_verify_by_llm: true,
                requires_human_approval: false,
            }),
            sub_template_id: None,
            record_fields: vec![],
            required_artifact_kinds: vec![],
            next: vec![StageId::from("approve")],
            rework_to: None,
        },
        StageDef {
            id: StageId::from("approve"),
            label: "Human Approval".into(),
            runner: "Human".into(),
            runner_kind: RunnerKind::Human,
            kind: StageKind::ApprovalGate,
            procedure: "Human approves delivery.".into(),
            gate_rubric: Some(GateRubric {
                checks: vec!["Meets bar".into()],
                auto_verify_by_llm: false,
                requires_human_approval: true,
            }),
            sub_template_id: None,
            record_fields: vec![],
            required_artifact_kinds: vec![],
            next: vec![StageId::from("done")],
            rework_to: Some(StageId::from("work")),
        },
        StageDef {
            id: StageId::from("done"),
            label: "Done".into(),
            runner: "System".into(),
            runner_kind: RunnerKind::Human,
            kind: StageKind::Terminal,
            procedure: "Completed.".into(),
            gate_rubric: None,
            sub_template_id: None,
            record_fields: vec![],
            required_artifact_kinds: vec![],
            next: vec![],
            rework_to: None,
        },
    ]
}

fn minimal_principles() -> Vec<Principle> {
    vec![Principle {
        id: "P-1".into(),
        title: "Verify First".into(),
        statement: "Validate before shipping.".into(),
        rationale: "Prevents regressions.".into(),
    }]
}

#[test]
fn district_create_archive_restore_requires_human_approval_and_enforces_guards() {
    let mut engine = AfterlifeEngine::new();
    let human = engine.local_actor();
    let agent = agent_actor(&engine.owner_id, "district-agent");

    let bad_name = engine.apply(
        &agent,
        EngineCommand::ProposeDistrictCreate {
            district_id: None,
            name: "   ".into(),
            setup_by: None,
            summary: "Valid summary".into(),
            principles: minimal_principles(),
            anti_patterns: vec![],
            decision_heuristics: vec![],
            stages: minimal_valid_stages(),
            changelog: "v1".into(),
            rationale: "Create new district".into(),
            source_feedback_ids: vec![],
            at: 1_900_000_000_000,
        },
    );
    assert!(matches!(bad_name, Err(TransitionError::InvalidInput { ref field, .. }) if field == "name"));

    let missing_gate_stages: Vec<StageDef> = minimal_valid_stages()
        .into_iter()
        .filter(|s| s.kind != StageKind::ApprovalGate)
        .map(|mut s| {
            if s.id.as_str() == "work" {
                s.next = vec![StageId::from("done")];
            }
            s
        })
        .collect();
    let no_gate = engine.apply(
        &agent,
        EngineCommand::ProposeDistrictCreate {
            district_id: None,
            name: "No Gate District".into(),
            setup_by: None,
            summary: "Summary".into(),
            principles: minimal_principles(),
            anti_patterns: vec![],
            decision_heuristics: vec![],
            stages: missing_gate_stages,
            changelog: "v1".into(),
            rationale: "Test gate validation".into(),
            source_feedback_ids: vec![],
            at: 1_900_000_000_100,
        },
    );
    assert!(matches!(no_gate, Err(TransitionError::InvalidProposedSpec { .. })));

    let dup_id = engine.apply(
        &agent,
        EngineCommand::ProposeDistrictCreate {
            district_id: Some(DistrictId::from("DIST-001")),
            name: "Duplicate ID".into(),
            setup_by: None,
            summary: "Summary".into(),
            principles: minimal_principles(),
            anti_patterns: vec![],
            decision_heuristics: vec![],
            stages: minimal_valid_stages(),
            changelog: "v1".into(),
            rationale: "Test duplicate id".into(),
            source_feedback_ids: vec![],
            at: 1_900_000_000_200,
        },
    );
    assert!(matches!(dup_id, Err(TransitionError::DistrictAlreadyExists(_))));

    let prop_ev = engine
        .apply(
            &agent,
            EngineCommand::ProposeDistrictCreate {
                district_id: None,
                name: "DISTRICT 06 - Release Ops".into(),
                setup_by: Some("Release Lead".into()),
                summary: "Safe release automation with human gates.".into(),
                principles: minimal_principles(),
                anti_patterns: vec!["Skip verification".into()],
                decision_heuristics: vec!["Check signatures first".into()],
                stages: minimal_valid_stages(),
                changelog: "Initial release workflow".into(),
                rationale: "Need dedicated release governance lane".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_001_000,
            },
        )
        .unwrap();
    let create_pid = match &prop_ev[0] {
        JobEvent::DistrictProposalCreated {
            proposal_id,
            action,
            ..
        } => {
            assert_eq!(action, "create");
            proposal_id.clone()
        }
        other => panic!("unexpected event: {other:?}"),
    };

    let agent_approve = engine.apply(
        &agent,
        EngineCommand::ApproveDistrictProposal {
            proposal_id: create_pid.clone(),
            at: 1_900_000_001_100,
        },
    );
    assert!(matches!(agent_approve, Err(TransitionError::HumanApprovalRequired { .. })));

    let created_ev = engine
        .apply(
            &human,
            EngineCommand::ApproveDistrictProposal {
                proposal_id: create_pid.clone(),
                at: 1_900_000_001_200,
            },
        )
        .unwrap();
    let new_dist_id = match &created_ev[0] {
        JobEvent::DistrictCreated {
            district_id, name, ..
        } => {
            assert_eq!(district_id.as_str(), "DIST-006");
            assert_eq!(name, "DISTRICT 06 - Release Ops");
            district_id.clone()
        }
        other => panic!("unexpected event: {other:?}"),
    };
    assert!(!engine.find_district(&new_dist_id).unwrap().archived);

    let jid = engine.queue_preset_job("Active Release Job", Some(&new_dist_id));
    engine
        .apply(
            &human,
            EngineCommand::AssignJobToDistrict {
                job_id: jid.clone(),
                district_id: new_dist_id.clone(),
                expected_revision: None,
                at: 1_900_000_002_000,
            },
        )
        .unwrap();

    let blocked_archive = engine.apply(
        &agent,
        EngineCommand::ProposeDistrictArchive {
            district_id: new_dist_id.clone(),
            rationale: "Try archiving with active job".into(),
            source_feedback_ids: vec![],
            at: 1_900_000_002_100,
        },
    );
    assert!(matches!(
        blocked_archive,
        Err(TransitionError::DistrictHasActiveJobs { ref district_id, ref active_job_ids })
        if district_id == &new_dist_id && active_job_ids == &vec![jid.clone()]
    ));

    engine
        .apply(
            &human,
            EngineCommand::ArchiveJob {
                job_id: jid.clone(),
                reason: Some("Done for test".into()),
                expected_revision: None,
                fencing_token: None,
                at: 1_900_000_002_200,
            },
        )
        .unwrap();

    let arch_prop_ev = engine
        .apply(
            &agent,
            EngineCommand::ProposeDistrictArchive {
                district_id: new_dist_id.clone(),
                rationale: "Retiring temporary release lane".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_003_000,
            },
        )
        .unwrap();
    let arch_pid = match &arch_prop_ev[0] {
        JobEvent::DistrictProposalCreated {
            proposal_id,
            action,
            ..
        } => {
            assert_eq!(action, "archive");
            proposal_id.clone()
        }
        other => panic!("unexpected event: {other:?}"),
    };

    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ApproveDistrictProposal {
                proposal_id: arch_pid.clone(),
                at: 1_900_000_003_100,
            }
        ),
        Err(TransitionError::HumanApprovalRequired { .. })
    ));

    let arch_done = engine
        .apply(
            &human,
            EngineCommand::ApproveDistrictProposal {
                proposal_id: arch_pid,
                at: 1_900_000_003_200,
            },
        )
        .unwrap();
    assert!(matches!(
        &arch_done[0],
        JobEvent::DistrictArchived { district_id, .. } if district_id == &new_dist_id
    ));
    let archived_dist = engine.find_district(&new_dist_id).unwrap();
    assert!(archived_dist.archived);
    assert_eq!(archived_dist.archived_at, Some(1_900_000_003_200));
    assert_eq!(archived_dist.philosophy_history.len(), 1);
    assert_eq!(archived_dist.spec_history.len(), 1);
    assert!(engine.active_districts().iter().all(|d| d.id != new_dist_id));

    let q_job = engine.queue_preset_job("Blocked Job", Some(&new_dist_id));
    assert!(matches!(
        engine.apply(
            &human,
            EngineCommand::AssignJobToDistrict {
                job_id: q_job,
                district_id: new_dist_id.clone(),
                expected_revision: None,
                at: 1_900_000_003_300,
            }
        ),
        Err(TransitionError::DistrictArchived(_))
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::PullNextJob {
                district_id: new_dist_id.clone(),
                at: 1_900_000_003_310,
            }
        ),
        Err(TransitionError::DistrictArchived(_))
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ProposePhilosophyUpdate {
                district_id: new_dist_id.clone(),
                base_version: PhilosophyVersion::new(1),
                proposed_district_name: None,
                summary: "Blocked".into(),
                principles: minimal_principles(),
                anti_patterns: vec![],
                decision_heuristics: vec![],
                rationale: "Blocked".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_003_320,
            }
        ),
        Err(TransitionError::DistrictArchived(_))
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ProposeSpecUpdate {
                district_id: new_dist_id.clone(),
                base_version: SpecVersion::new(1),
                proposed_district_name: None,
                stages: minimal_valid_stages(),
                changelog: "Blocked".into(),
                rationale: "Blocked".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_003_330,
            }
        ),
        Err(TransitionError::DistrictArchived(_))
    ));
    assert!(matches!(
        engine.apply(
            &human,
            EngineCommand::SetStageSubTemplate {
                district_id: new_dist_id.clone(),
                stage_id: StageId::from("work"),
                template_id: None,
                at: 1_900_000_003_340,
            }
        ),
        Err(TransitionError::DistrictArchived(_))
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ProposeDistrictArchive {
                district_id: new_dist_id.clone(),
                rationale: "Double archive".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_003_350,
            }
        ),
        Err(TransitionError::DistrictArchived(_))
    ));
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ProposeDistrictRestore {
                district_id: DistrictId::from("DIST-001"),
                rationale: "Restore active".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_003_360,
            }
        ),
        Err(TransitionError::DistrictNotArchived(_))
    ));

    let res_prop_ev = engine
        .apply(
            &agent,
            EngineCommand::ProposeDistrictRestore {
                district_id: new_dist_id.clone(),
                rationale: "Re-enabling release lane for next launch".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_004_000,
            },
        )
        .unwrap();
    let res_pid = match &res_prop_ev[0] {
        JobEvent::DistrictProposalCreated {
            proposal_id,
            action,
            ..
        } => {
            assert_eq!(action, "restore");
            proposal_id.clone()
        }
        other => panic!("unexpected event: {other:?}"),
    };
    assert!(matches!(
        engine.apply(
            &agent,
            EngineCommand::ApproveDistrictProposal {
                proposal_id: res_pid.clone(),
                at: 1_900_000_004_100,
            }
        ),
        Err(TransitionError::HumanApprovalRequired { .. })
    ));

    let res_done = engine
        .apply(
            &human,
            EngineCommand::ApproveDistrictProposal {
                proposal_id: res_pid,
                at: 1_900_000_004_200,
            },
        )
        .unwrap();
    assert!(matches!(
        &res_done[0],
        JobEvent::DistrictRestored { district_id, .. } if district_id == &new_dist_id
    ));
    let restored = engine.find_district(&new_dist_id).unwrap();
    assert!(!restored.archived);
    assert_eq!(restored.archived_at, None);
    assert!(engine.active_districts().iter().any(|d| d.id == new_dist_id));
}
