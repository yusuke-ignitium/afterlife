use afterlife_core::{
    Actor, CommandId, DistrictId, EngineCommand, GateRubric, JobEvent, OwnerId, Principle,
    RunnerKind, StageDef, StageId, StageKind,
};
use afterlife_protocol::{CommandEnvelope, WorkspaceStore};

use crate::SqliteWorkspaceStore;

fn temp_path(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("afterlife_dist_compat_{name}_{nanos}"))
}

fn minimal_stages() -> Vec<StageDef> {
    vec![
        StageDef {
            id: StageId::from("work"),
            label: "Work".into(),
            runner: "AI Agent".into(),
            runner_kind: RunnerKind::Ai,
            kind: StageKind::Work,
            procedure: "Do work.".into(),
            gate_rubric: None,
            sub_template_id: None,
            record_fields: vec![],
            required_artifact_kinds: vec![],
            next: vec![StageId::from("approve")],
            rework_to: None,
        },
        StageDef {
            id: StageId::from("approve"),
            label: "Approve".into(),
            runner: "Human".into(),
            runner_kind: RunnerKind::Human,
            kind: StageKind::ApprovalGate,
            procedure: "Approve.".into(),
            gate_rubric: Some(GateRubric {
                checks: vec!["OK".into()],
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
            procedure: "Done.".into(),
            gate_rubric: None,
            sub_template_id: None,
            record_fields: vec![],
            required_artifact_kinds: vec![],
            next: vec![],
            rework_to: None,
        },
    ]
}

#[test]
fn legacy_districts_without_archived_fields_load_and_persist_lifecycle_across_reopen() {
    let dir = temp_path("lifecycle_reopen");
    let db_path = dir.join("afterlife.db");
    let owner = OwnerId::from("dist-compat-owner");
    let human = Actor::local_human(owner.clone());

    let store = SqliteWorkspaceStore::open(&db_path).unwrap();
    let _ = store.load_workspace(&owner).unwrap();
    drop(store);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let raw: String = conn
        .query_row(
            "SELECT state_json FROM workspaces WHERE owner_id = ?1",
            [owner.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    let mut val: serde_json::Value = serde_json::from_str(&raw).unwrap();
    val.as_object_mut().unwrap().remove("district_proposals");
    for d in val["districts"].as_array_mut().unwrap() {
        let obj = d.as_object_mut().unwrap();
        obj.remove("archived");
        obj.remove("archived_at");
    }
    conn.execute(
        "UPDATE workspaces SET state_json = ?1 WHERE owner_id = ?2",
        rusqlite::params![serde_json::to_string(&val).unwrap(), owner.as_str()],
    )
    .unwrap();
    drop(conn);

    let reopened = SqliteWorkspaceStore::open(&db_path).unwrap();
    let snap = reopened.load_workspace(&owner).unwrap();
    assert!(snap.engine.district_proposals.is_empty());
    assert!(snap.engine.districts.iter().all(|d| !d.archived && d.archived_at.is_none()));

    let prop = reopened
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-dc-1"),
            actor: human.clone(),
            command: EngineCommand::ProposeDistrictCreate {
                district_id: None,
                name: "DISTRICT 06 - Persisted".into(),
                setup_by: None,
                summary: "Persist test".into(),
                principles: vec![Principle {
                    id: "P1".into(),
                    title: "T1".into(),
                    statement: "S1".into(),
                    rationale: "R1".into(),
                }],
                anti_patterns: vec![],
                decision_heuristics: vec![],
                stages: minimal_stages(),
                changelog: "v1".into(),
                rationale: "Need persisted lane".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_010_000,
            },
        })
        .unwrap();
    let pid = match &prop.events[0] {
        JobEvent::DistrictProposalCreated { proposal_id, .. } => proposal_id.clone(),
        other => panic!("unexpected: {other:?}"),
    };
    reopened
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-dc-2"),
            actor: human.clone(),
            command: EngineCommand::ApproveDistrictProposal {
                proposal_id: pid,
                at: 1_900_000_011_000,
            },
        })
        .unwrap();

    let arch_prop = reopened
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-dc-3"),
            actor: human.clone(),
            command: EngineCommand::ProposeDistrictArchive {
                district_id: DistrictId::from("DIST-006"),
                rationale: "Archive persisted lane".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_012_000,
            },
        })
        .unwrap();
    let arch_pid = match &arch_prop.events[0] {
        JobEvent::DistrictProposalCreated { proposal_id, .. } => proposal_id.clone(),
        other => panic!("unexpected: {other:?}"),
    };
    reopened
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-dc-4"),
            actor: human.clone(),
            command: EngineCommand::ApproveDistrictProposal {
                proposal_id: arch_pid,
                at: 1_900_000_013_000,
            },
        })
        .unwrap();
    drop(reopened);

    let after_restart = SqliteWorkspaceStore::open(&db_path).unwrap();
    let snap2 = after_restart.load_workspace(&owner).unwrap();
    let d6 = snap2.engine.find_district(&DistrictId::from("DIST-006")).unwrap();
    assert!(d6.archived);
    assert_eq!(d6.archived_at, Some(1_900_000_013_000));
    assert_eq!(d6.philosophy_history.len(), 1);
    assert_eq!(d6.spec_history.len(), 1);

    let res_prop = after_restart
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-dc-5"),
            actor: human.clone(),
            command: EngineCommand::ProposeDistrictRestore {
                district_id: DistrictId::from("DIST-006"),
                rationale: "Restore after restart".into(),
                source_feedback_ids: vec![],
                at: 1_900_000_014_000,
            },
        })
        .unwrap();
    let res_pid = match &res_prop.events[0] {
        JobEvent::DistrictProposalCreated { proposal_id, .. } => proposal_id.clone(),
        other => panic!("unexpected: {other:?}"),
    };
    after_restart
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-dc-6"),
            actor: human,
            command: EngineCommand::ApproveDistrictProposal {
                proposal_id: res_pid,
                at: 1_900_000_015_000,
            },
        })
        .unwrap();
    drop(after_restart);

    let after_restore = SqliteWorkspaceStore::open(&db_path).unwrap();
    let snap3 = after_restore.load_workspace(&owner).unwrap();
    let d6_restored = snap3.engine.find_district(&DistrictId::from("DIST-006")).unwrap();
    assert!(!d6_restored.archived);
    assert_eq!(d6_restored.archived_at, None);

    let _ = std::fs::remove_dir_all(&dir);
}
