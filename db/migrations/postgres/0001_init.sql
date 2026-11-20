-- Afterlife AWS Production Storage Schema (PostgreSQL 16+)
-- Enforces strict per-user operational data isolation via composite owner_id keys
-- AND PostgreSQL Row-Level Security (RLS) defense-in-depth.
-- Analytics/telemetry permissions are intentionally NOT managed here; events are
-- streamed asynchronously to S3 where AWS IAM / Lake Formation controls access.

BEGIN;

CREATE TABLE IF NOT EXISTS workspaces (
    owner_id        TEXT PRIMARY KEY,
    state_json      JSONB NOT NULL,
    latest_seq      BIGINT NOT NULL DEFAULT 0,
    updated_at_ms   BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS idempotency_keys (
    owner_id        TEXT NOT NULL,
    command_id      TEXT NOT NULL,
    request_hash    TEXT NOT NULL DEFAULT '',
    seq_start       BIGINT NOT NULL,
    seq_end         BIGINT NOT NULL,
    events_json     JSONB NOT NULL,
    created_at_ms   BIGINT NOT NULL,
    PRIMARY KEY (owner_id, command_id)
);

CREATE TABLE IF NOT EXISTS event_log (
    seq             BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    owner_id        TEXT NOT NULL,
    command_id      TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    event_json      JSONB NOT NULL,
    occurred_at_ms  BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_event_log_owner_seq
    ON event_log (owner_id, seq);

-- Row-Level Security (RLS): each transaction sets
--   SET LOCAL app.current_owner_id = '<google_oidc_sub>';
-- guaranteeing cross-user isolation even against application bugs.
ALTER TABLE workspaces ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspaces FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS workspaces_owner_isolation ON workspaces;
CREATE POLICY workspaces_owner_isolation ON workspaces
    USING (owner_id = current_setting('app.current_owner_id', true))
    WITH CHECK (owner_id = current_setting('app.current_owner_id', true));

ALTER TABLE idempotency_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE idempotency_keys FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS idempotency_owner_isolation ON idempotency_keys;
CREATE POLICY idempotency_owner_isolation ON idempotency_keys
    USING (owner_id = current_setting('app.current_owner_id', true))
    WITH CHECK (owner_id = current_setting('app.current_owner_id', true));

ALTER TABLE event_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE event_log FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS event_log_owner_isolation ON event_log;
CREATE POLICY event_log_owner_isolation ON event_log
    USING (owner_id = current_setting('app.current_owner_id', true))
    WITH CHECK (owner_id = current_setting('app.current_owner_id', true));

-- Cross-instance SSE fanout via native PostgreSQL LISTEN/NOTIFY (no Redis required).
CREATE OR REPLACE FUNCTION notify_afterlife_event() RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify(
        'afterlife_events',
        json_build_object(
            'seq', NEW.seq,
            'owner_id', NEW.owner_id,
            'command_id', NEW.command_id,
            'event_type', NEW.event_type
        )::text
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_notify_afterlife_event ON event_log;
CREATE TRIGGER trg_notify_afterlife_event
    AFTER INSERT ON event_log
    FOR EACH ROW EXECUTE FUNCTION notify_afterlife_event();

-- OAuth 2.1 Broker persistence (RFC 7591 DCR clients, refresh token rotation, revocation list)
CREATE TABLE IF NOT EXISTS oauth_clients (
    client_id       TEXT PRIMARY KEY,
    client_name     TEXT NOT NULL,
    redirect_uris   JSONB NOT NULL,
    grant_types     JSONB NOT NULL,
    is_agent_client BOOLEAN NOT NULL DEFAULT TRUE,
    created_at_ms   BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS oauth_refresh_tokens (
    token_hash      TEXT PRIMARY KEY,
    owner_id        TEXT NOT NULL,
    actor_json      JSONB NOT NULL,
    client_id       TEXT NOT NULL REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
    scopes_raw      TEXT NOT NULL,
    expires_at_ms   BIGINT NOT NULL,
    rotated         BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_oauth_refresh_owner
    ON oauth_refresh_tokens (owner_id);

CREATE TABLE IF NOT EXISTS revoked_tokens (
    jti             TEXT PRIMARY KEY,
    expires_at_ms   BIGINT NOT NULL
);

COMMIT;
