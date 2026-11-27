use std::{
    path::Path,
    sync::Mutex,
    time::Duration,
};

use afterlife_core::{JobId, OwnerId};
use afterlife_protocol::{
    LinearIssueLink, LinearOAuthToken, LinearSyncCursor, LinearSyncStore, PortError,
};
use rusqlite::{Connection, OptionalExtension, params};

const LINEAR_SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS linear_oauth_tokens (
    owner_id            TEXT PRIMARY KEY NOT NULL,
    client_id           TEXT NOT NULL,
    client_secret       TEXT,
    access_token        TEXT NOT NULL,
    refresh_token       TEXT,
    expires_at_ms       INTEGER,
    scope               TEXT NOT NULL,
    viewer_id           TEXT,
    viewer_name         TEXT,
    viewer_email        TEXT,
    default_district_id TEXT,
    updated_at_ms       INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS linear_issue_links (
    owner_id            TEXT NOT NULL,
    linear_issue_id     TEXT NOT NULL,
    linear_identifier   TEXT NOT NULL,
    linear_url          TEXT NOT NULL,
    job_id              TEXT NOT NULL,
    last_synced_at_ms   INTEGER NOT NULL,
    PRIMARY KEY (owner_id, linear_issue_id),
    UNIQUE (owner_id, job_id)
);

CREATE INDEX IF NOT EXISTS idx_linear_links_owner_job
    ON linear_issue_links (owner_id, job_id);

CREATE TABLE IF NOT EXISTS linear_processed_comments (
    owner_id            TEXT NOT NULL,
    comment_id          TEXT NOT NULL,
    outcome             TEXT NOT NULL,
    processed_at_ms     INTEGER NOT NULL,
    PRIMARY KEY (owner_id, comment_id)
);

CREATE TABLE IF NOT EXISTS linear_sync_state (
    owner_id            TEXT PRIMARY KEY NOT NULL,
    last_outbound_seq   INTEGER NOT NULL DEFAULT 0,
    last_sync_at_ms     INTEGER,
    last_error          TEXT
);
";

pub struct SqliteLinearStore {
    conn: Mutex<Connection>,
}

impl SqliteLinearStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PortError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| PortError::Storage(format!("mkdir {}: {e}", parent.display())))?;
        }
        let conn = Connection::open(path)
            .map_err(|e| PortError::Storage(format!("sqlite linear open {}: {e}", path.display())))?;
        Self::configure_and_migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, PortError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| PortError::Storage(format!("sqlite linear in-memory: {e}")))?;
        Self::configure_and_migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
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
        conn.execute_batch(LINEAR_SCHEMA_SQL)
            .map_err(|e| PortError::Storage(format!("linear sqlite migration failed: {e}")))?;
        Ok(())
    }
}

impl LinearSyncStore for SqliteLinearStore {
    fn load_token(&self, owner_id: &OwnerId) -> Result<Option<LinearOAuthToken>, PortError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT client_id, client_secret, access_token, refresh_token, expires_at_ms, scope,
                    viewer_id, viewer_name, viewer_email, default_district_id, updated_at_ms
             FROM linear_oauth_tokens
             WHERE owner_id = ?1",
            params![owner_id.as_str()],
            |r| {
                let expires_raw: Option<i64> = r.get(4)?;
                let updated_raw: i64 = r.get(10)?;
                Ok(LinearOAuthToken {
                    owner_id: owner_id.clone(),
                    client_id: r.get(0)?,
                    client_secret: r.get(1)?,
                    access_token: r.get(2)?,
                    refresh_token: r.get(3)?,
                    expires_at_ms: expires_raw.map(|v| v.max(0) as u64),
                    scope: r.get(5)?,
                    viewer_id: r.get(6)?,
                    viewer_name: r.get(7)?,
                    viewer_email: r.get(8)?,
                    default_district_id: r.get(9)?,
                    updated_at_ms: updated_raw.max(0) as u64,
                })
            },
        )
        .optional()
        .map_err(|e| PortError::Storage(e.to_string()))
    }

    fn save_token(&self, token: &LinearOAuthToken) -> Result<(), PortError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO linear_oauth_tokens (
                owner_id, client_id, client_secret, access_token, refresh_token, expires_at_ms,
                scope, viewer_id, viewer_name, viewer_email, default_district_id, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(owner_id) DO UPDATE SET
                client_id = excluded.client_id,
                client_secret = excluded.client_secret,
                access_token = excluded.access_token,
                refresh_token = excluded.refresh_token,
                expires_at_ms = excluded.expires_at_ms,
                scope = excluded.scope,
                viewer_id = excluded.viewer_id,
                viewer_name = excluded.viewer_name,
                viewer_email = excluded.viewer_email,
                default_district_id = excluded.default_district_id,
                updated_at_ms = excluded.updated_at_ms",
            params![
                token.owner_id.as_str(),
                &token.client_id,
                &token.client_secret,
                &token.access_token,
                &token.refresh_token,
                token.expires_at_ms.map(|v| v as i64),
                &token.scope,
                &token.viewer_id,
                &token.viewer_name,
                &token.viewer_email,
                &token.default_district_id,
                token.updated_at_ms as i64,
            ],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(())
    }

    fn delete_token(&self, owner_id: &OwnerId) -> Result<(), PortError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM linear_oauth_tokens WHERE owner_id = ?1",
            params![owner_id.as_str()],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(())
    }

    fn list_links(&self, owner_id: &OwnerId) -> Result<Vec<LinearIssueLink>, PortError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT linear_issue_id, linear_identifier, linear_url, job_id, last_synced_at_ms
                 FROM linear_issue_links
                 WHERE owner_id = ?1
                 ORDER BY last_synced_at_ms DESC, linear_identifier ASC",
            )
            .map_err(|e| PortError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map(params![owner_id.as_str()], |r| {
                let synced: i64 = r.get(4)?;
                Ok(LinearIssueLink {
                    owner_id: owner_id.clone(),
                    linear_issue_id: r.get(0)?,
                    linear_identifier: r.get(1)?,
                    linear_url: r.get(2)?,
                    job_id: JobId::from(r.get::<_, String>(3)?),
                    last_synced_at_ms: synced.max(0) as u64,
                })
            })
            .map_err(|e| PortError::Storage(e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| PortError::Storage(e.to_string()))
    }

    fn find_link_by_issue(
        &self,
        owner_id: &OwnerId,
        linear_issue_id: &str,
    ) -> Result<Option<LinearIssueLink>, PortError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT linear_issue_id, linear_identifier, linear_url, job_id, last_synced_at_ms
             FROM linear_issue_links
             WHERE owner_id = ?1 AND linear_issue_id = ?2",
            params![owner_id.as_str(), linear_issue_id],
            |r| {
                let synced: i64 = r.get(4)?;
                Ok(LinearIssueLink {
                    owner_id: owner_id.clone(),
                    linear_issue_id: r.get(0)?,
                    linear_identifier: r.get(1)?,
                    linear_url: r.get(2)?,
                    job_id: JobId::from(r.get::<_, String>(3)?),
                    last_synced_at_ms: synced.max(0) as u64,
                })
            },
        )
        .optional()
        .map_err(|e| PortError::Storage(e.to_string()))
    }

    fn find_link_by_job(
        &self,
        owner_id: &OwnerId,
        job_id: &JobId,
    ) -> Result<Option<LinearIssueLink>, PortError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT linear_issue_id, linear_identifier, linear_url, job_id, last_synced_at_ms
             FROM linear_issue_links
             WHERE owner_id = ?1 AND job_id = ?2",
            params![owner_id.as_str(), job_id.as_str()],
            |r| {
                let synced: i64 = r.get(4)?;
                Ok(LinearIssueLink {
                    owner_id: owner_id.clone(),
                    linear_issue_id: r.get(0)?,
                    linear_identifier: r.get(1)?,
                    linear_url: r.get(2)?,
                    job_id: JobId::from(r.get::<_, String>(3)?),
                    last_synced_at_ms: synced.max(0) as u64,
                })
            },
        )
        .optional()
        .map_err(|e| PortError::Storage(e.to_string()))
    }

    fn upsert_link(&self, link: &LinearIssueLink) -> Result<(), PortError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO linear_issue_links (
                owner_id, linear_issue_id, linear_identifier, linear_url, job_id, last_synced_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(owner_id, linear_issue_id) DO UPDATE SET
                linear_identifier = excluded.linear_identifier,
                linear_url = excluded.linear_url,
                job_id = excluded.job_id,
                last_synced_at_ms = excluded.last_synced_at_ms",
            params![
                link.owner_id.as_str(),
                &link.linear_issue_id,
                &link.linear_identifier,
                &link.linear_url,
                link.job_id.as_str(),
                link.last_synced_at_ms as i64,
            ],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(())
    }

    fn is_comment_processed(&self, owner_id: &OwnerId, comment_id: &str) -> Result<bool, PortError> {
        let conn = self.conn.lock().unwrap();
        let found: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM linear_processed_comments WHERE owner_id = ?1 AND comment_id = ?2",
                params![owner_id.as_str(), comment_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(found.is_some())
    }

    fn mark_comment_processed(
        &self,
        owner_id: &OwnerId,
        comment_id: &str,
        outcome: &str,
        processed_at_ms: u64,
    ) -> Result<(), PortError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO linear_processed_comments (owner_id, comment_id, outcome, processed_at_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(owner_id, comment_id) DO UPDATE SET
                outcome = excluded.outcome,
                processed_at_ms = excluded.processed_at_ms",
            params![owner_id.as_str(), comment_id, outcome, processed_at_ms as i64],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(())
    }

    fn load_cursor(&self, owner_id: &OwnerId) -> Result<LinearSyncCursor, PortError> {
        let conn = self.conn.lock().unwrap();
        let found = conn
            .query_row(
                "SELECT last_outbound_seq, last_sync_at_ms, last_error
                 FROM linear_sync_state
                 WHERE owner_id = ?1",
                params![owner_id.as_str()],
                |r| {
                    let seq_raw: i64 = r.get(0)?;
                    let sync_raw: Option<i64> = r.get(1)?;
                    Ok(LinearSyncCursor {
                        owner_id: owner_id.clone(),
                        last_outbound_seq: seq_raw.max(0) as u64,
                        last_sync_at_ms: sync_raw.map(|v| v.max(0) as u64),
                        last_error: r.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(found.unwrap_or_else(|| LinearSyncCursor::initial(owner_id.clone())))
    }

    fn save_cursor(&self, cursor: &LinearSyncCursor) -> Result<(), PortError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO linear_sync_state (owner_id, last_outbound_seq, last_sync_at_ms, last_error)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(owner_id) DO UPDATE SET
                last_outbound_seq = excluded.last_outbound_seq,
                last_sync_at_ms = excluded.last_sync_at_ms,
                last_error = excluded.last_error",
            params![
                cursor.owner_id.as_str(),
                cursor.last_outbound_seq as i64,
                cursor.last_sync_at_ms.map(|v| v as i64),
                &cursor.last_error,
            ],
        )
        .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_store_persists_tokens_links_comments_and_isolates_owners() {
        let store = SqliteLinearStore::open_in_memory().unwrap();
        let owner_a = OwnerId::from("owner-a");
        let owner_b = OwnerId::from("owner-b");

        let token = LinearOAuthToken {
            owner_id: owner_a.clone(),
            client_id: "lin-client-1".into(),
            client_secret: None,
            access_token: "access-1".into(),
            refresh_token: Some("refresh-1".into()),
            expires_at_ms: Some(1_900_000_000_000),
            scope: "read,comments:create".into(),
            viewer_id: Some("usr-1".into()),
            viewer_name: Some("Alice".into()),
            viewer_email: Some("alice@example.com".into()),
            default_district_id: Some("DIST-001".into()),
            updated_at_ms: 1_800_000_000_000,
        };
        store.save_token(&token).unwrap();
        assert_eq!(store.load_token(&owner_a).unwrap(), Some(token.clone()));
        assert_eq!(store.load_token(&owner_b).unwrap(), None);

        let link = LinearIssueLink {
            owner_id: owner_a.clone(),
            linear_issue_id: "issue-uuid-1".into(),
            linear_identifier: "ENG-101".into(),
            linear_url: "https://linear.app/acme/issue/ENG-101".into(),
            job_id: JobId::from("JOB-001"),
            last_synced_at_ms: 1_800_000_001_000,
        };
        store.upsert_link(&link).unwrap();
        assert_eq!(
            store.find_link_by_issue(&owner_a, "issue-uuid-1").unwrap(),
            Some(link.clone())
        );
        assert_eq!(
            store.find_link_by_job(&owner_a, &JobId::from("JOB-001")).unwrap(),
            Some(link.clone())
        );
        assert_eq!(
            store.find_link_by_issue(&owner_b, "issue-uuid-1").unwrap(),
            None
        );

        assert!(!store.is_comment_processed(&owner_a, "cmt-1").unwrap());
        store
            .mark_comment_processed(&owner_a, "cmt-1", "applied", 1_800_000_002_000)
            .unwrap();
        assert!(store.is_comment_processed(&owner_a, "cmt-1").unwrap());
        assert!(!store.is_comment_processed(&owner_b, "cmt-1").unwrap());

        let cursor = LinearSyncCursor {
            owner_id: owner_a.clone(),
            last_outbound_seq: 42,
            last_sync_at_ms: Some(1_800_000_003_000),
            last_error: None,
        };
        store.save_cursor(&cursor).unwrap();
        assert_eq!(store.load_cursor(&owner_a).unwrap(), cursor);
        assert_eq!(store.load_cursor(&owner_b).unwrap().last_outbound_seq, 0);

        store.delete_token(&owner_a).unwrap();
        assert_eq!(store.load_token(&owner_a).unwrap(), None);
        assert_eq!(store.list_links(&owner_a).unwrap().len(), 1);
    }
}
