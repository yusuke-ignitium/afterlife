-- Afterlife Local Storage Schema (SQLite WAL)
-- Mirrors db/migrations/postgres/0001_init.sql with strict per-owner isolation.

CREATE TABLE IF NOT EXISTS workspaces (
    owner_id        TEXT PRIMARY KEY NOT NULL,
    state_json      TEXT NOT NULL,
    latest_seq      INTEGER NOT NULL DEFAULT 0,
    updated_at_ms   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS idempotency_keys (
    owner_id        TEXT NOT NULL,
    command_id      TEXT NOT NULL,
    request_hash    TEXT NOT NULL DEFAULT '',
    seq_start       INTEGER NOT NULL,
    seq_end         INTEGER NOT NULL,
    events_json     TEXT NOT NULL,
    created_at_ms   INTEGER NOT NULL,
    PRIMARY KEY (owner_id, command_id)
);

CREATE TABLE IF NOT EXISTS event_log (
    seq             INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id        TEXT NOT NULL,
    command_id      TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    event_json      TEXT NOT NULL,
    occurred_at_ms  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_event_log_owner_seq
    ON event_log (owner_id, seq);

CREATE TABLE IF NOT EXISTS oauth_clients (
    client_id       TEXT PRIMARY KEY NOT NULL,
    client_name     TEXT NOT NULL,
    redirect_uris   TEXT NOT NULL,
    grant_types     TEXT NOT NULL,
    is_agent_client INTEGER NOT NULL,
    created_at_ms   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS oauth_refresh_tokens (
    token_hash      TEXT PRIMARY KEY NOT NULL,
    owner_id        TEXT NOT NULL,
    actor_json      TEXT NOT NULL,
    client_id       TEXT NOT NULL,
    scopes_raw      TEXT NOT NULL,
    expires_at_ms   INTEGER NOT NULL,
    rotated         INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_oauth_refresh_owner
    ON oauth_refresh_tokens (owner_id);

CREATE TABLE IF NOT EXISTS revoked_tokens (
    jti             TEXT PRIMARY KEY NOT NULL,
    expires_at_ms   INTEGER NOT NULL
);
