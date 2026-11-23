use afterlife_core::{Actor, CommandId, DistrictId, EngineCommand, JobEvent, OwnerId};
use afterlife_protocol::{CommandEnvelope, WorkspaceStore};
use crate::SqliteWorkspaceStore;

fn temp_path(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("afterlife_stop_compat_{name}_{nanos}"))
}

#[test]
fn legacy_jobs_without_stopped_fields_load_cleanly_across_all_locations() {
    let dir = temp_path("all_locations");
    let db_path = dir.join("afterlife.db");
    let owner = OwnerId::from("stop-compat-owner");
    let human = Actor::local_human(owner.clone());

    let store = SqliteWorkspaceStore::open(&db_path).unwrap();
    let q1 = store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-sc-1"),
            actor: human.clone(),
            command: EngineCommand::QueueJob {
                title: "Queued legacy job".into(),
                raw_input: "raw".into(),
                target_district_hint: None,
                at: 1_763_600_000_000,
            },
        })
        .unwrap();
    let q1_id = match &q1.events[0] {
        JobEvent::JobQueued { job_id, .. } => job_id.clone(),
        _ => unreachable!(),
    };
    let q2 = store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-sc-2"),
            actor: human.clone(),
            command: EngineCommand::QueueJob {
                title: "In-district legacy job".into(),
                raw_input: "raw".into(),
                target_district_hint: Some(DistrictId::from("DIST-001")),
                at: 1_763_600_001_000,
            },
        })
        .unwrap();
    let q2_id = match &q2.events[0] {
        JobEvent::JobQueued { job_id, .. } => job_id.clone(),
        _ => unreachable!(),
    };
    store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-sc-assign"),
            actor: human.clone(),
            command: EngineCommand::AssignJobToDistrict {
                job_id: q2_id.clone(),
                district_id: DistrictId::from("DIST-001"),
                expected_revision: None,
                at: 1_763_600_002_000,
            },
        })
        .unwrap();
    let q3 = store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-sc-3"),
            actor: human.clone(),
            command: EngineCommand::QueueJob {
                title: "Archived legacy job".into(),
                raw_input: "raw".into(),
                target_district_hint: None,
                at: 1_763_600_003_000,
            },
        })
        .unwrap();
    let q3_id = match &q3.events[0] {
        JobEvent::JobQueued { job_id, .. } => job_id.clone(),
        _ => unreachable!(),
    };
    store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("cmd-sc-archive"),
            actor: human,
            command: EngineCommand::ArchiveJob {
                job_id: q3_id.clone(),
                reason: Some("Legacy archive".into()),
                expected_revision: None,
                fencing_token: None,
                at: 1_763_600_004_000,
            },
        })
        .unwrap();
    drop(store);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let state_json: String = conn
        .query_row(
            "SELECT state_json FROM workspaces WHERE owner_id = ?1",
            rusqlite::params![owner.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    let mut parsed: serde_json::Value = serde_json::from_str(&state_json).unwrap();
    if let Some(jobs) = parsed["jobs"].as_array_mut() {
        for job in jobs {
            if let Some(obj) = job.as_object_mut() {
                obj.remove("stopped");
                obj.remove("stopped_at");
                obj.remove("stopped_reason");
            }
        }
    }
    conn.execute(
        "UPDATE workspaces SET state_json = ?1 WHERE owner_id = ?2",
        rusqlite::params![serde_json::to_string(&parsed).unwrap(), owner.as_str()],
    )
    .unwrap();
    drop(conn);

    let reopened = SqliteWorkspaceStore::open(&db_path).unwrap();
    let snap = reopened.load_workspace(&owner).unwrap();
    assert!(snap.engine.find_job(&q1_id).is_some());
    assert!(snap.engine.find_job(&q2_id).is_some());
    assert!(snap.engine.find_job(&q3_id).is_some());

    let _ = std::fs::remove_dir_all(&dir);
}
