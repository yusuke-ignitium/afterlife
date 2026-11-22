use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use afterlife_core::{AfterlifeEngine, CommandId, EngineCommand, JobEvent, OwnerId, TimestampMs};
use afterlife_protocol::{
    CommandEnvelope, CommandOutcome, EventRecord, PortError, RegisteredClient, ScopeSet,
    StoredRefreshToken, WorkspaceSnapshot, WorkspaceStore,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

const SQLITE_MIGRATION_0001: &str = include_str!("../../../db/migrations/sqlite/0001_init.sql");

fn command_payload_hash(cmd: &EngineCommand) -> Result<String, PortError> {
    let mut val = serde_json::to_value(cmd).map_err(|e| PortError::Storage(e.to_string()))?;
    if let Some(obj) = val.as_object_mut() {
        obj.remove("at");
    }
    let canonical = serde_json::to_vec(&val).map_err(|e| PortError::Storage(e.to_string()))?;
    let mut h = Sha256::new();
    h.update(&canonical);
    Ok(format!("{:x}", h.finalize()))
}

fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn event_type_name(ev: &JobEvent) -> &'static str {
    match ev {
        JobEvent::JobQueued { .. } => "job_queued",
        JobEvent::JobUpdated { .. } => "job_updated",
        JobEvent::JobArchived { .. } => "job_archived",
        JobEvent::JobTransitioned { .. } => "job_transitioned",
        JobEvent::JobReworked { .. } => "job_reworked",
        JobEvent::JobClaimed { .. } => "job_claimed",
        JobEvent::JobLeaseRenewed { .. } => "job_lease_renewed",
        JobEvent::JobLeaseReleased { .. } => "job_lease_released",
        JobEvent::ArtifactAttached { .. } => "artifact_attached",
        JobEvent::GateVerificationRecorded { .. } => "gate_verification_recorded",
        JobEvent::StepRecorded { .. } => "step_recorded",
        JobEvent::FeedbackSubmitted { .. } => "feedback_submitted",
        JobEvent::PhilosophyProposed { .. } => "philosophy_proposed",
        JobEvent::PhilosophyUpdated { .. } => "philosophy_updated",
        JobEvent::SpecProposed { .. } => "spec_proposed",
        JobEvent::SpecUpdated { .. } => "spec_updated",
        JobEvent::StageSubTemplateChanged { .. } => "stage_sub_template_changed",
    }
}

pub struct SqliteWorkspaceStore {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl SqliteWorkspaceStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PortError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| PortError::Storage(format!("mkdir {}: {e}", parent.display())))?;
            }
        }
        let conn = Connection::open(&path)
            .map_err(|e| PortError::Storage(format!("sqlite open {}: {e}", path.display())))?;
        Self::configure_and_migrate(&conn)?;
        Ok(Self {
            path,
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, PortError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| PortError::Storage(format!("sqlite in-memory: {e}")))?;
        Self::configure_and_migrate(&conn)?;
        Ok(Self {
            path: PathBuf::from(":memory:"),
            conn: Mutex::new(conn),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn configure_and_migrate(conn: &Connection) -> Result<(), PortError> {
        conn.busy_timeout(Duration::from_millis(5_000))
            .map_err(|e| PortError::Storage(e.to_string()))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        conn.execute_batch(SQLITE_MIGRATION_0001)
            .map_err(|e| PortError::Storage(format!("sqlite migration failed: {e}")))?;
        let mut stmt = conn
            .prepare("PRAGMA table_info(idempotency_keys)")
            .map_err(|e| PortError::Storage(e.to_string()))?;
        let has_req_hash = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| PortError::Storage(e.to_string()))?
            .filter_map(Result::ok)
            .any(|col| col == "request_hash");
        drop(stmt);
        if !has_req_hash {
            conn.execute(
                "ALTER TABLE idempotency_keys ADD COLUMN request_hash TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|e| {
                PortError::Storage(format!("sqlite alter idempotency_keys failed: {e}"))
            })?;
        }
        Ok(())
    }

    fn load_or_seed_in_tx(
        tx: &rusqlite::Transaction<'_>,
        owner_id: &OwnerId,
    ) -> Result<(AfterlifeEngine, u64), PortError> {
        let row: Option<(String, i64)> = tx
            .query_row(
                "SELECT state_json, latest_seq FROM workspaces WHERE owner_id = ?1",
                params![owner_id.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| PortError::Storage(e.to_string()))?;

        if let Some((state_json, latest_seq)) = row {
            let engine: AfterlifeEngine = serde_json::from_str(&state_json)
                .map_err(|e| PortError::Storage(format!("corrupt workspace state: {e}")))?;
            return Ok((engine, latest_seq.max(0) as u64));
        }

        let engine = AfterlifeEngine::new_for_owner(owner_id.clone());
        let state_json =
            serde_json::to_string(&engine).map_err(|e| PortError::Storage(e.to_string()))?;
        tx.execute(
            "INSERT INTO workspaces (owner_id, state_json, latest_seq, updated_at_ms)
             VALUES (?1, ?2, 0, ?3)",
            params![owner_id.as_str(), state_json, now_ms() as i64],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;

        Ok((engine, 0))
    }
}

impl WorkspaceStore for SqliteWorkspaceStore {
    fn load_workspace(&self, owner_id: &OwnerId) -> Result<WorkspaceSnapshot, PortError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PortError::Storage(e.to_string()))?;
        let (engine, latest_seq) = Self::load_or_seed_in_tx(&tx, owner_id)?;
        tx.commit().map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(WorkspaceSnapshot {
            owner_id: owner_id.clone(),
            latest_seq,
            engine,
        })
    }

    fn execute_command(&self, env: CommandEnvelope) -> Result<CommandOutcome, PortError> {
        if env.command_id.as_str().trim().is_empty() {
            return Err(PortError::InvalidInput(
                "command_id must not be empty".into(),
            ));
        }
        let req_hash = command_payload_hash(&env.command)?;
        let owner_id = env.actor.owner_id().clone();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PortError::Storage(e.to_string()))?;

        let existing: Option<(String, i64, i64, String)> = tx
            .query_row(
                "SELECT request_hash, seq_start, seq_end, events_json
                 FROM idempotency_keys
                 WHERE owner_id = ?1 AND command_id = ?2",
                params![owner_id.as_str(), env.command_id.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(|e| PortError::Storage(e.to_string()))?;

        if let Some((stored_hash, seq_start, seq_end, events_json)) = existing {
            if !stored_hash.is_empty() && stored_hash != req_hash {
                return Err(PortError::IdempotencyConflict {
                    command_id: env.command_id.as_str().to_string(),
                });
            }
            let events: Vec<JobEvent> = serde_json::from_str(&events_json)
                .map_err(|e| PortError::Storage(e.to_string()))?;
            tx.commit().map_err(|e| PortError::Storage(e.to_string()))?;
            return Ok(CommandOutcome {
                command_id: env.command_id,
                seq_start: seq_start as u64,
                seq_end: seq_end as u64,
                events,
                idempotent_replay: true,
            });
        }

        let (mut engine, mut latest_seq) = Self::load_or_seed_in_tx(&tx, &owner_id)?;
        let events = engine.apply(&env.actor, env.command)?;

        if engine.event_log.len() > 256 {
            let drain = engine.event_log.len() - 256;
            engine.event_log.drain(0..drain);
        }

        let mut seq_start = latest_seq;
        let mut seq_end = latest_seq;
        for (idx, ev) in events.iter().enumerate() {
            let ev_json =
                serde_json::to_string(ev).map_err(|e| PortError::Storage(e.to_string()))?;
            tx.execute(
                "INSERT INTO event_log (owner_id, command_id, event_type, event_json, occurred_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    owner_id.as_str(),
                    env.command_id.as_str(),
                    event_type_name(ev),
                    ev_json,
                    ev.timestamp_ms() as i64
                ],
            )
            .map_err(|e| PortError::Storage(e.to_string()))?;
            let seq = tx.last_insert_rowid() as u64;
            if idx == 0 {
                seq_start = seq;
            }
            seq_end = seq;
            latest_seq = seq;
        }

        let state_json =
            serde_json::to_string(&engine).map_err(|e| PortError::Storage(e.to_string()))?;
        let events_json =
            serde_json::to_string(&events).map_err(|e| PortError::Storage(e.to_string()))?;
        let ts = now_ms() as i64;

        tx.execute(
            "UPDATE workspaces
             SET state_json = ?1, latest_seq = ?2, updated_at_ms = ?3
             WHERE owner_id = ?4",
            params![state_json, latest_seq as i64, ts, owner_id.as_str()],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;

        tx.execute(
            "INSERT INTO idempotency_keys (owner_id, command_id, request_hash, seq_start, seq_end, events_json, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                owner_id.as_str(),
                env.command_id.as_str(),
                req_hash,
                seq_start as i64,
                seq_end as i64,
                events_json,
                ts
            ],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;

        tx.commit().map_err(|e| PortError::Storage(e.to_string()))?;

        Ok(CommandOutcome {
            command_id: env.command_id,
            seq_start,
            seq_end,
            events,
            idempotent_replay: false,
        })
    }

    fn list_events_since(
        &self,
        owner_id: &OwnerId,
        since_seq: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>, PortError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT seq, command_id, owner_id, event_json
                 FROM event_log
                 WHERE owner_id = ?1 AND seq > ?2
                 ORDER BY seq ASC
                 LIMIT ?3",
            )
            .map_err(|e| PortError::Storage(e.to_string()))?;

        let rows = stmt
            .query_map(
                params![owner_id.as_str(), since_seq as i64, limit.max(1) as i64],
                |r| {
                    let seq: i64 = r.get(0)?;
                    let command_id: String = r.get(1)?;
                    let owner: String = r.get(2)?;
                    let event_json: String = r.get(3)?;
                    Ok((seq, command_id, owner, event_json))
                },
            )
            .map_err(|e| PortError::Storage(e.to_string()))?;

        let mut out = Vec::new();
        for row in rows {
            let (seq, command_id, owner, event_json) =
                row.map_err(|e| PortError::Storage(e.to_string()))?;
            let event: JobEvent =
                serde_json::from_str(&event_json).map_err(|e| PortError::Storage(e.to_string()))?;
            out.push(EventRecord {
                seq: seq as u64,
                command_id: CommandId::new(command_id),
                owner_id: OwnerId::new(owner),
                event,
            });
        }
        Ok(out)
    }

    fn register_oauth_client(&self, client: RegisteredClient) -> Result<(), PortError> {
        let conn = self.conn.lock().unwrap();
        let redirect_uris = serde_json::to_string(&client.redirect_uris)
            .map_err(|e| PortError::Storage(e.to_string()))?;
        let grant_types = serde_json::to_string(&client.grant_types)
            .map_err(|e| PortError::Storage(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO oauth_clients
             (client_id, client_name, redirect_uris, grant_types, is_agent_client, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                client.client_id,
                client.client_name,
                redirect_uris,
                grant_types,
                if client.is_agent_client { 1i64 } else { 0i64 },
                client.created_at as i64
            ],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(())
    }

    fn get_oauth_client(&self, client_id: &str) -> Result<Option<RegisteredClient>, PortError> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(String, String, String, String, i64, i64)> = conn
            .query_row(
                "SELECT client_id, client_name, redirect_uris, grant_types, is_agent_client, created_at_ms
                 FROM oauth_clients WHERE client_id = ?1",
                params![client_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()
            .map_err(|e| PortError::Storage(e.to_string()))?;

        let Some((cid, cname, uris_json, grants_json, is_agent, created_at)) = row else {
            return Ok(None);
        };
        Ok(Some(RegisteredClient {
            client_id: cid,
            client_name: cname,
            redirect_uris: serde_json::from_str(&uris_json)
                .map_err(|e| PortError::Storage(e.to_string()))?,
            grant_types: serde_json::from_str(&grants_json)
                .map_err(|e| PortError::Storage(e.to_string()))?,
            is_agent_client: is_agent != 0,
            created_at: created_at as u64,
        }))
    }

    fn store_refresh_token(&self, token: StoredRefreshToken) -> Result<(), PortError> {
        let conn = self.conn.lock().unwrap();
        let actor_json =
            serde_json::to_string(&token.actor).map_err(|e| PortError::Storage(e.to_string()))?;
        conn.execute(
            "INSERT INTO oauth_refresh_tokens
             (token_hash, owner_id, actor_json, client_id, scopes_raw, expires_at_ms, rotated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                token.token_hash,
                token.owner_id.as_str(),
                actor_json,
                token.client_id,
                token.scopes.to_space_separated(),
                token.expires_at as i64,
                if token.rotated { 1i64 } else { 0i64 }
            ],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(())
    }

    fn consume_refresh_token(
        &self,
        token_hash: &str,
        now: TimestampMs,
    ) -> Result<Option<StoredRefreshToken>, PortError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PortError::Storage(e.to_string()))?;
        let row: Option<(String, String, String, String, i64, i64)> = tx
            .query_row(
                "SELECT owner_id, actor_json, client_id, scopes_raw, expires_at_ms, rotated
                 FROM oauth_refresh_tokens WHERE token_hash = ?1",
                params![token_hash],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| PortError::Storage(e.to_string()))?;

        let Some((owner_id, actor_json, client_id, scopes_raw, expires_at, rotated)) = row else {
            return Ok(None);
        };
        if rotated != 0 || (expires_at as u64) <= now {
            return Ok(None);
        }
        tx.execute(
            "UPDATE oauth_refresh_tokens SET rotated = 1 WHERE token_hash = ?1",
            params![token_hash],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        tx.commit().map_err(|e| PortError::Storage(e.to_string()))?;

        Ok(Some(StoredRefreshToken {
            token_hash: token_hash.to_string(),
            owner_id: OwnerId::new(owner_id),
            actor: serde_json::from_str(&actor_json)
                .map_err(|e| PortError::Storage(e.to_string()))?,
            client_id,
            scopes: ScopeSet::parse_space_separated(&scopes_raw),
            expires_at: expires_at as u64,
            rotated: true,
        }))
    }

    fn revoke_token_jti(&self, jti: &str, expires_at: TimestampMs) -> Result<(), PortError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO revoked_tokens (jti, expires_at_ms) VALUES (?1, ?2)",
            params![jti, expires_at as i64],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(())
    }

    fn is_token_jti_revoked(&self, jti: &str) -> Result<bool, PortError> {
        let conn = self.conn.lock().unwrap();
        let found: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM revoked_tokens WHERE jti = ?1",
                params![jti],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(found.is_some())
    }
}
