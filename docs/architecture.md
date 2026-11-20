# Afterlife Control Plane Architecture & Runtime Design

This document captures the **protocol, security, concurrency, and domain invariants** that cannot be expressed in Terraform HCL alone. Infrastructure topology is defined declaratively in `infra/aws/*.tf`, database schemas in `db/migrations/{sqlite,postgres}/0001_init.sql`, runtime config in `config/*.toml`, and the MCP contract in `schemas/mcp-tools.schema.json`.

---

## 1. Local-First ↔ AWS Production (Hexagonal Ports)

`afterlife-core` has zero I/O dependencies. `afterlife-protocol` defines four port traits so the exact same domain engine, REST/SSE API, and Streamable HTTP MCP server run locally and on AWS ECS without rewriting business logic:

| Port Trait (`afterlife-protocol`) | Local (`config/local.toml`) | AWS Production (`config/production.example.toml`) |
| --- | --- | --- |
| `WorkspaceStore` | `SqliteWorkspaceStore` (WAL + `BEGIN IMMEDIATE`) | `PostgresWorkspaceStore` (`SELECT ... FOR UPDATE` + PostgreSQL RLS) |
| `ArtifactBlobStore` | `LocalFsBlobStore` (`~/.afterlife/artifacts/<owner>/<sha256>`) | Amazon S3 (`s3://<artifacts-bucket>/blobs/<owner>/<sha256>`) |
| `TelemetrySink` | `JsonlTelemetrySink` (`dt=YYYY-MM-DD/owner=<id>/events.jsonl`) | Amazon S3 append-only JSONL (`s3:PutObject` only) |
| `EventBus` | `InMemoryEventBus` (`tokio::sync::broadcast`) | PostgreSQL `LISTEN/NOTIFY` (`afterlife_events` trigger) |

Target production footprint (30–100 engineers): **~$85–$115/month** on AWS (`ALB` + 2× `ECS Fargate ARM64 0.5 vCPU / 1 GB` in public subnets with strict ALB-only SG to avoid NAT Gateway + `RDS PostgreSQL 16 db.t4g.small` in private subnets + S3 + Secrets Manager).

---

## 2. OAuth 2.1 MCP Broker + Google Workspace OIDC Federation

> **Why Terraform cannot express this:** Terraform provisions the ALB, ACM TLS cert, and Secrets Manager entries for the Google Client Secret and JWT signing key, but the **federation state machine, RFC 9728/8414/7591 discovery & DCR endpoints, PKCE S256 verification, Refresh Token Rotation, and Agent scope stripping** live inside `afterlifed` (`crates/afterlife-daemon/src/auth.rs`).

### Why an OAuth 2.1 Broker sits in front of Google Workspace OIDC

1. **Native MCP client compatibility:** Claude Desktop/Code, Codex, Cursor Agent, and Grok bot expect an **MCP OAuth 2.1 Authorization Server** supporting:
   - `GET /.well-known/oauth-protected-resource` (RFC 9728)
   - `GET /.well-known/oauth-authorization-server` (RFC 8414)
   - `POST /oauth/register` (RFC 7591 Dynamic Client Registration)
   - `GET /oauth/authorize` + `POST /oauth/token` with mandatory **PKCE S256**
   - `POST /oauth/revoke` (RFC 7009)
   Google Workspace OIDC does **not** support RFC 7591 DCR or custom application scopes.
2. **Domain-scoped authority & instant kill-switch:** `afterlifed` mints short-lived (15 min) HS256/EdDSA access tokens carrying `jti`, `sub` (Google OIDC `sub`), `act` (RFC 8693 delegated agent identity), and `scope`. Revoking a token writes `jti` to `revoked_tokens` and immediately invalidates the agent before TTL expiry.
3. **Refresh Token Rotation with reuse detection:** Refresh tokens are stored as SHA-256 hashes in `oauth_refresh_tokens` and rotated on every `/oauth/token` refresh (`consumed_at`). Re-presenting a consumed token fails closed.

### End-to-End Authentication Sequence (`mode = "google_oidc"`)

```text
MCP Client (Claude / Codex / Cursor / Grok)     afterlifed (OAuth 2.1 Broker)         Google Workspace OIDC
  |                                                |                                      |
  |-- POST /mcp (no Bearer) ---------------------->|                                      |
  |<-- 401 + WWW-Authenticate: resource_metadata --|                                      |
  |                                                |                                      |
  |-- GET /.well-known/oauth-protected-resource -->|                                      |
  |-- GET /.well-known/oauth-authorization-server->|                                      |
  |-- POST /oauth/register (DCR) ----------------->|  persist client_id in DB             |
  |<-- { client_id } ------------------------------|                                      |
  |                                                |                                      |
  |-- Browser: GET /oauth/authorize?PKCE_S256 ---->|                                      |
  |                                                |-- 302 Redirect (state, nonce, hd) -->|
  |                                                |<-- /oauth/google/callback?code=... --|
  |                                                |-- Exchange code + verify id_token -->|
  |                                                |   * Verify Google JWKS signature     |
  |                                                |   * Verify `hd == hosted_domain`     |
  |                                                |   * Verify `email_verified == true`  |
  |                                                |   * Map `sub` -> OwnerId             |
  |                                                |   * Strip human-only scopes if Agent |
  |<-- 302 redirect_uri?code=<broker_code> --------|                                      |
  |                                                |                                      |
  |-- POST /oauth/token (code + code_verifier) --->|  Verify SHA256(verifier)==challenge  |
  |<-- { access_token (15m), refresh_token (30d) }-|                                      |
  |                                                |                                      |
  |-- POST /mcp (Authorization: Bearer <jwt>) ---->|  Validate JWT + jti + scope + owner  |
```

### Defense-in-Depth: Human vs Delegated Agent Boundary (RFC 8693)

Human approval gates (`StageKind::ApprovalGate`) and governance upgrades (`ApprovePhilosophyProposal`, `ApproveSpecProposal`) **must never be bypassed by an AI agent**, even if a prompt-injected agent requests elevated scopes:

1. **Layer 1 — OAuth Broker (`ScopeSet::without_human_only()`):** Whenever an OAuth client represents an AI agent (not the first-party human desktop/CLI), the broker **unconditionally strips** `approvals:decide` and `districts:admin` at authorization and token issuance time.
2. **Layer 2 — Domain Engine (`afterlife-core`):** Every `EngineCommand` executes with an explicit `Actor::Human { owner_id, email }` or `Actor::Agent { owner_id, agent_id, client_id, label }`. `AfterlifeEngine::apply` enforces `actor.is_human()` on:
   - advancing a Job past a `StageKind::ApprovalGate`,
   - approving a `PhilosophyProposal` (`vN -> vN+1`),
   - approving a `SpecProposal` (`vN -> vN+1`).
   Any agent attempt returns `TransitionError::HumanApprovalRequired` (HTTP `403`).

---

## 3. Data Isolation & Multi-Agent Concurrency Model

### Three-Layer Per-User Operational Isolation

Operational workspace data (Districts, Philosophies, ProcedureSpecs, Jobs, Leases, Feedback, Artifacts) is **strictly private per user (`owner_id` = Google OIDC `sub`)**:

1. **Auth Layer:** `Principal.owner_id` is derived exclusively from the verified JWT `sub` claim—never from request body or query params.
2. **Domain Layer:** `AfterlifeEngine::apply` verifies `actor.owner_id() == &self.owner_id`, rejecting cross-owner commands with `TransitionError::OwnerMismatch`.
3. **Database Layer (PostgreSQL RLS):** `db/migrations/postgres/0001_init.sql` enables `FORCE ROW LEVEL SECURITY` on all tenant tables (`workspace_state`, `event_log`, `idempotency_keys`, `oauth_refresh_tokens`) with policy:
   ```sql
   owner_id = current_setting('app.current_owner_id', true)
   ```
   Every PostgreSQL transaction opens with `SET LOCAL app.current_owner_id = $1` using a non-superuser application role (without `BYPASSRLS`), making cross-owner data leaks structurally impossible even under application bugs.

### Concurrency Invariants (Zero Double-Claim, Zero Zombie Writes, Exactly-Once)

1. **Serialized Per-Owner Critical Section:**
   - SQLite: `WAL` mode + `busy_timeout(5000)` + `BEGIN IMMEDIATE` transaction.
   - PostgreSQL: `SELECT ... FROM workspace_state WHERE owner_id = $1 FOR UPDATE`.
2. **Exclusive Lease + Monotonic `FencingToken`:**
   - `ClaimJob` succeeds only if no active lease exists (or `lease.expires_at <= now_ms`), issuing `fencing_token = next_fencing_token + 1`.
   - Every agent mutation (`HeartbeatLease`, `ReleaseLease`, `AdvanceJob`, `ReworkJob`, `AttachArtifact`, `RecordGateVerification`) must supply the active `fencing_token`. If an agent stalls past TTL and another runner takes over (`token = 2`), any delayed write from the zombie agent (`token = 1`) is rejected with `TransitionError::StaleFencingToken`.
3. **Optimistic CAS (`expected_revision`):** Clients can supply `expected_revision` on `AssignJobToDistrict`, `ClaimJob`, `AdvanceJob`, and `ReworkJob` to detect concurrent UI/CLI edits (`TransitionError::RevisionConflict`).
4. **Command Idempotency (`command_id`):** Every mutating REST/MCP call carries a UUIDv7 `command_id`. The `idempotency_keys(owner_id, command_id)` table is checked and inserted inside the same ACID transaction as the state update, returning the cached `CommandOutcome` with `idempotent_replay: true` on network retries.

---

## 4. Explicit `Philosophy` + `ProcedureSpec` + Self-Evolving Feedback Loop

To keep agent quality consistent across heterogeneous models (Claude, Codex, Cursor, Grok) while allowing workflows to improve over time:

- **District (`DIST-001`, `DIST-002`, ...):** A bounded operational domain with its own **versioned `Philosophy` (`vN`)** and **versioned `ProcedureSpec` (`vM`)**.
- **`Philosophy` (explicitly separate from procedures):** Captures durable engineering/research *values*—`summary`, `principles` (with rationale), `anti_patterns`, and `decision_heuristics`.
- **`ProcedureSpec`:** Captures concrete nested state-machine execution (`StageDef` + optional `SubMachineTemplateId` + `GateRubric` checklists + `rework_target`).
- **`ExecutionContext` Compilation (`GET /v1/jobs/{id}/context` & MCP `job_get_context`):** Compiles the exact hierarchical prompt/context payload for a Job (Active District Philosophy + Current Stage Procedure + Current Sub-Stage Procedure + GateRubric + recent Feedbacks) so any connected agent receives the authoritative rules before touching code or artifacts.
- **Self-Evolution Loop:**
  1. Rework actions and human reviews automatically record structured `Feedback` (`ApprovalNote`, `ReworkReason`, `PhilosophyInsight`, `ProcedureFix`).
  2. A frontier reasoning model inspects accumulated feedback and calls `philosophy_propose` or `spec_propose` (referencing `base_version` and `source_feedback_ids`).
  3. Proposals stay `Pending` until a **human** executes `ApprovePhilosophyProposal` or `ApproveSpecProposal`, atomically activating `vN+1` while preserving full version history for audit/rollback.

---

## 5. Telemetry & Analytics Governance Boundary

- Every committed domain event is emitted to `TelemetrySink` as append-only Hive-partitioned JSONL:
  ```text
  events/dt=YYYY-MM-DD/owner=<owner_id>/events.jsonl
  ```
- **Strict Separation of Responsibilities:**
  - `afterlifed`'s ECS Task Role (`infra/aws/ecs.tf`) is granted **`s3:PutObject` ONLY** on the telemetry bucket—it cannot list or read back other users' telemetry.
  - Downstream analytics (Amazon Athena, QuickSight, Lake Formation, org-wide productivity metrics) are governed exclusively through AWS IAM (`analytics_reader_principal_arns` in `infra/aws/storage.tf`), completely decoupled from Afterlife's operational authorization model.

---

## 6. Production Cutover Checklist (What Is Wired Now vs Cloud Adapter Implementation)

| Capability | Current Status | Notes for AWS Production Cutover |
| --- | --- | --- |
| Domain engine (`District`, `Philosophy`, `ProcedureSpec`, `Feedback`, `FencingToken`, `Actor` guards) | Implemented & tested (`afterlife-core`) | Production-ready (0 I/O dependencies) |
| SQLite WAL store, Local FS blobs, Hive-partitioned JSONL telemetry, In-memory event bus | Implemented & tested (`afterlife-storage`) | Active in local `afterlifed` and `Afterlife.app` |
| OAuth 2.1 Broker (RFC 9728 / 8414 / 7591 DCR / PKCE S256 / Refresh rotation / `jti` revoke / scope stripping) | Implemented & E2E tested (`afterlife-daemon`) | Wire `/oauth/google/callback` HTTP token exchange + Google JWKS signature/claim verification (`hd`, `email_verified`, `sub`) using `reqwest` + RS256 verifier before enabling `mode = "google_oidc"` |
| PostgreSQL 16 schema + `FORCE ROW LEVEL SECURITY` + `LISTEN/NOTIFY` trigger | SQL migration ready (`db/migrations/postgres/0001_init.sql`) | Implement `PostgresWorkspaceStore` & `PostgresListenNotifyBus` (`sqlx`) behind `WorkspaceStore` / `EventBus` traits |
| S3 Artifacts & S3 Telemetry sinks | Port traits + Terraform buckets/IAM ready | Implement `S3BlobStore` & buffered `S3TelemetrySink` (`aws-sdk-s3`) behind `ArtifactBlobStore` / `TelemetrySink` traits |
| AWS Infrastructure (VPC, ALB+ACM, ECS Fargate ARM64, RDS PG16, S3, Secrets Manager) | Terraform HCL ready (`infra/aws/*.tf`) + `infra/Dockerfile` | Populate the 3 Secrets Manager values out-of-band, push ARM64 image to ECR, run `db/migrations/postgres/0001_init.sql` |
