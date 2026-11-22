pub mod factory;
pub mod local_adapters;
pub mod sqlite;

pub use factory::RuntimePorts;
pub use local_adapters::{
    InMemoryEventBus, JsonlTelemetrySink, LocalFsBlobStore, NoopTelemetrySink,
};
pub use sqlite::SqliteWorkspaceStore;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use afterlife_core::{
        Actor, CommandId, EngineCommand, OwnerId, RunnerId, TransitionError,
    };
    use afterlife_protocol::{
        ArtifactBlobStore, CommandEnvelope, PortError, TelemetrySink, WorkspaceStore,
    };

    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("afterlife_test_{name}_{nanos}"))
    }

    #[test]
    fn concurrent_16_thread_claim_allows_strictly_one_winner() {
        let dir = temp_path("concurrent_claim");
        let db_path = dir.join("afterlife.db");
        let owner = OwnerId::from("alice@company.com");

        let init_store = SqliteWorkspaceStore::open(&db_path).unwrap();
        let queued = init_store
            .execute_command(CommandEnvelope {
                command_id: CommandId::from("cmd-seed-concurrent"),
                actor: Actor::local_human(owner.clone()),
                command: EngineCommand::QueueJob {
                    title: "Concurrent claim target".into(),
                    raw_input: "raw".into(),
                    target_district_hint: None,
                    at: 1_999_999_000_000,
                },
            })
            .unwrap();
        let target_job_id = match &queued.events[0] {
            afterlife_core::JobEvent::JobQueued { job_id, .. } => job_id.clone(),
            _ => unreachable!(),
        };
        drop(init_store);

        let barrier = Arc::new(Barrier::new(16));
        let mut handles = Vec::new();
        for idx in 0..16 {
            let db = db_path.clone();
            let b = barrier.clone();
            let owner = owner.clone();
            let target_job_id = target_job_id.clone();
            handles.push(thread::spawn(move || {
                let store = SqliteWorkspaceStore::open(&db).unwrap();
                let actor = Actor::Agent {
                    owner_id: owner,
                    client_id: format!("agent-{idx}"),
                    runner_label: format!("Runner-{idx}"),
                    token_jti: format!("jti-{idx}"),
                };
                b.wait();
                store.execute_command(CommandEnvelope {
                    command_id: CommandId::new(format!("cmd-claim-{idx}")),
                    actor,
                    command: EngineCommand::ClaimJob {
                        job_id: target_job_id,
                        runner_id: RunnerId::new(format!("runner-{idx}")),
                        runner_label: format!("Runner-{idx}"),
                        ttl_ms: 60_000,
                        expected_revision: None,
                        at: 2_000_000_000_000,
                    },
                })
            }));
        }

        let mut winners = 0usize;
        let mut already_leased = 0usize;
        for h in handles {
            match h.join().unwrap() {
                Ok(out) => {
                    assert!(!out.idempotent_replay);
                    winners += 1;
                }
                Err(PortError::Domain(TransitionError::AlreadyLeased { .. })) => {
                    already_leased += 1;
                }
                Err(other) => panic!("unexpected error: {other}"),
            }
        }
        assert_eq!(winners, 1, "exactly one runner must win concurrent claim");
        assert_eq!(already_leased, 15);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn idempotency_key_guarantees_exactly_once_and_persists_across_reopen() {
        let dir = temp_path("idempotency");
        let db_path = dir.join("afterlife.db");
        let owner = OwnerId::from("bob@company.com");
        let actor = Actor::local_human(owner.clone());

        let store = SqliteWorkspaceStore::open(&db_path).unwrap();
        let env = CommandEnvelope {
            command_id: CommandId::from("cmd-uuid-001"),
            actor: actor.clone(),
            command: EngineCommand::QueueJob {
                title: "Persist across restart".into(),
                raw_input: "raw".into(),
                target_district_hint: None,
                at: 1_763_600_100_000,
            },
        };

        let first = store.execute_command(env.clone()).unwrap();
        assert!(!first.idempotent_replay);
        assert_eq!(first.events.len(), 1);

        let replay = store.execute_command(env).unwrap();
        assert!(replay.idempotent_replay);
        assert_eq!(replay.seq_start, first.seq_start);
        assert_eq!(replay.events, first.events);
        drop(store);

        let reopened = SqliteWorkspaceStore::open(&db_path).unwrap();
        let snap = reopened.load_workspace(&owner).unwrap();
        let matches: Vec<_> = snap
            .engine
            .jobs
            .iter()
            .filter(|j| j.title == "Persist across restart")
            .collect();
        assert_eq!(matches.len(), 1);

        let events = reopened.list_events_since(&owner, 0, 50).unwrap();
        assert_eq!(events.len(), 1);

        let other = OwnerId::from("carol@company.com");
        let other_events = reopened.list_events_since(&other, 0, 50).unwrap();
        assert!(other_events.is_empty());
        let other_snap = reopened.load_workspace(&other).unwrap();
        assert!(
            other_snap
                .engine
                .jobs
                .iter()
                .all(|j| j.title != "Persist across restart")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn blob_store_and_jsonl_telemetry_partition_by_owner_and_date() {
        let dir = temp_path("blobs_telemetry");
        let owner = OwnerId::from("alice@company.com");
        let blobs = LocalFsBlobStore::new(dir.join("blobs")).unwrap();
        let blob_ref = blobs.put_blob(&owner, "diff.patch", b"hello diff").unwrap();
        assert_eq!(
            blobs.get_blob(&owner, &blob_ref.sha256).unwrap().unwrap(),
            b"hello diff"
        );
        let other = OwnerId::from("mallory@company.com");
        assert!(blobs.get_blob(&other, &blob_ref.sha256).unwrap().is_none());

        let sink = JsonlTelemetrySink::new(dir.join("telemetry")).unwrap();
        let store = SqliteWorkspaceStore::open_in_memory().unwrap();
        let actor = Actor::local_human(owner.clone());
        store
            .execute_command(CommandEnvelope {
                command_id: CommandId::from("cmd-tel-1"),
                actor,
                command: EngineCommand::QueueJob {
                    title: "Telemetry job".into(),
                    raw_input: "raw".into(),
                    target_district_hint: None,
                    at: 1_763_600_000_000,
                },
            })
            .unwrap();
        let records = store.list_events_since(&owner, 0, 10).unwrap();
        sink.emit(&records[0]).unwrap();

        let expected_file = dir
            .join("telemetry")
            .join("dt=2025-11-20")
            .join("owner=alice_company_com")
            .join("events.jsonl");
        assert!(expected_file.exists());
        let content = std::fs::read_to_string(expected_file).unwrap();
        assert!(content.contains("Telemetry job"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_workspace_json_without_artifact_sub_stage_loads_cleanly() {
        let dir = temp_path("legacy_artifact_compat");
        let db_path = dir.join("afterlife.db");
        let owner = OwnerId::from("legacy-owner");

        let store = SqliteWorkspaceStore::open(&db_path).unwrap();
        store
            .execute_command(CommandEnvelope {
                command_id: CommandId::from("cmd-legacy-1"),
                actor: Actor::local_human(owner.clone()),
                command: EngineCommand::QueueJob {
                    title: "Legacy compat job".into(),
                    raw_input: "raw".into(),
                    target_district_hint: None,
                    at: 1_763_600_000_000,
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
                if let Some(arts) = job["artifacts"].as_array_mut() {
                    for art in arts {
                        if let Some(obj) = art.as_object_mut() {
                            obj.remove("produced_at_sub_stage");
                        }
                    }
                }
            }
        }
        let stripped = serde_json::to_string(&parsed).unwrap();
        conn.execute(
            "UPDATE workspaces SET state_json = ?1 WHERE owner_id = ?2",
            rusqlite::params![stripped, owner.as_str()],
        )
        .unwrap();
        drop(conn);

        let reopened = SqliteWorkspaceStore::open(&db_path).unwrap();
        let snap = reopened.load_workspace(&owner).unwrap();
        assert_eq!(snap.engine.jobs.len(), 1);
        assert_eq!(snap.engine.jobs[0].artifacts.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_workspace_with_historical_artifact_uris_loads_cleanly() {
        let dir = temp_path("legacy_artifact_uris");
        let db_path = dir.join("afterlife.db");
        let owner = OwnerId::from("historical-owner");

        let store = SqliteWorkspaceStore::open(&db_path).unwrap();
        store
            .execute_command(CommandEnvelope {
                command_id: CommandId::from("cmd-hist-1"),
                actor: Actor::local_human(owner.clone()),
                command: EngineCommand::QueueJob {
                    title: "Historical URI job".into(),
                    raw_input: "raw".into(),
                    target_district_hint: None,
                    at: 1_763_600_000_000,
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
        parsed["jobs"][0]["artifacts"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "art-legacy-01",
                "kind": "snapshot",
                "label": "legacy-baseline.json",
                "uri": "evidence://job-001/before-baseline.json",
                "sha256": null,
                "produced_at_stage": "baseline_guard",
                "revision_at_capture": 2
            }));
        conn.execute(
            "UPDATE workspaces SET state_json = ?1 WHERE owner_id = ?2",
            rusqlite::params![serde_json::to_string(&parsed).unwrap(), owner.as_str()],
        )
        .unwrap();
        drop(conn);

        let reopened = SqliteWorkspaceStore::open(&db_path).unwrap();
        let snap = reopened.load_workspace(&owner).unwrap();
        assert_eq!(snap.engine.jobs[0].artifacts.len(), 2);
        assert_eq!(snap.engine.jobs[0].artifacts[0].uri, "queue-drop.json");
        assert_eq!(
            snap.engine.jobs[0].artifacts[1].uri,
            "evidence://job-001/before-baseline.json"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn blob_store_enforces_size_limit_sanitizes_filenames_and_writes_atomically() {
        let dir = temp_path("blob_atomic_guards");
        let blobs = LocalFsBlobStore::new(dir.join("blobs")).unwrap();
        let owner = OwnerId::from("alice@example.com");

        let oversized = vec![b'x'; afterlife_core::MAX_ARTIFACT_BLOB_BYTES + 1];
        let err = blobs
            .put_blob(&owner, "huge.bin", &oversized)
            .unwrap_err();
        assert!(matches!(err, PortError::InvalidInput(_)));

        let first = blobs
            .put_blob(&owner, "../../evil/report (1).json", b"{\"ok\":true}")
            .unwrap();
        assert!(first.uri.starts_with("blob://alice_example_com/"));
        assert!(first.uri.ends_with("/report__1_.json"));
        let second = blobs
            .put_blob(&owner, "../../evil/report (1).json", b"{\"ok\":true}")
            .unwrap();
        assert_eq!(first, second);

        assert!(blobs.get_blob(&owner, "not-64-hex").unwrap().is_none());
        assert!(
            blobs
                .get_blob(&owner, &first.sha256.to_ascii_uppercase())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            blobs.get_blob(&owner, &first.sha256).unwrap().unwrap(),
            b"{\"ok\":true}"
        );

        let owner_dir = dir.join("blobs").join("alice_example_com");
        let tmp_leftovers: Vec<_> = std::fs::read_dir(&owner_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
            .collect();
        assert!(tmp_leftovers.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
