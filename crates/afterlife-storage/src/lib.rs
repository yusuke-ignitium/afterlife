pub mod factory;
pub mod local_adapters;
pub mod sqlite;

pub use factory::RuntimePorts;
pub use local_adapters::{InMemoryEventBus, JsonlTelemetrySink, LocalFsBlobStore, NoopTelemetrySink};
pub use sqlite::SqliteWorkspaceStore;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use afterlife_core::{
        Actor, CommandId, EngineCommand, JobId, OwnerId, RunnerId, TransitionError,
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
        let _ = init_store.load_workspace(&owner).unwrap();
        drop(init_store);

        let barrier = Arc::new(Barrier::new(16));
        let mut handles = Vec::new();
        for idx in 0..16 {
            let db = db_path.clone();
            let b = barrier.clone();
            let owner = owner.clone();
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
                        job_id: JobId::from("JOB-045"),
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
}
