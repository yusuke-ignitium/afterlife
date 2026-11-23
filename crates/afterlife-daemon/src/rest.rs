use std::convert::Infallible;
use std::sync::Arc;

use afterlife_core::{CommandId, EngineCommand, JobId};
use afterlife_protocol::{CommandEnvelope, CommandOutcome, EventRecord, PortError, Scope};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
};
use serde::Deserialize;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};
use uuid::Uuid;

use crate::{
    AppDaemonState,
    artifacts::{ArtifactBlobError, read_owned_blob_by_sha256, verify_blob_reference},
    auth::{AuthPrincipal, HumanWritePrincipal},
};

pub fn authorize_command(
    principal: &afterlife_protocol::Principal,
    cmd: &EngineCommand,
) -> Result<(), (StatusCode, &'static str)> {
    let required = match cmd {
        EngineCommand::QueueJob { .. } => Scope::JobsCreate,
        EngineCommand::ApproveJob { .. }
        | EngineCommand::StopJob { .. }
        | EngineCommand::ResumeJob { .. }
        | EngineCommand::ApprovePhilosophyProposal { .. }
        | EngineCommand::ApproveSpecProposal { .. }
        | EngineCommand::ApproveTemplateProposal { .. }
        | EngineCommand::ApproveDistrictProposal { .. } => Scope::ApprovalsDecide,
        EngineCommand::SetStageSubTemplate { .. }
        | EngineCommand::SyncBuiltinTemplates { .. } => Scope::DistrictsAdmin,
        _ => Scope::JobsWork,
    };
    if !principal.scopes.contains(&required) {
        return Err((StatusCode::FORBIDDEN, "insufficient_scope"));
    }

    if matches!(cmd, EngineCommand::ProposeDistrictCreate { .. })
        && !principal.scopes.allows_workspace_structure()
    {
        return Err((StatusCode::FORBIDDEN, "district_scope_denied"));
    }

    let target_district = match cmd {
        EngineCommand::PullNextJob { district_id, .. }
        | EngineCommand::AssignJobToDistrict { district_id, .. }
        | EngineCommand::SubmitFeedback { district_id, .. }
        | EngineCommand::ProposePhilosophyUpdate { district_id, .. }
        | EngineCommand::ProposeSpecUpdate { district_id, .. }
        | EngineCommand::ProposeDistrictArchive { district_id, .. }
        | EngineCommand::ProposeDistrictRestore { district_id, .. }
        | EngineCommand::SetStageSubTemplate { district_id, .. } => Some(district_id),
        EngineCommand::QueueJob {
            target_district_hint,
            ..
        }
        | EngineCommand::UpdateJob {
            target_district_hint,
            ..
        } => target_district_hint.as_ref(),
        _ => None,
    };
    if let Some(did) = target_district {
        if !principal.scopes.allows_district(did) {
            return Err((StatusCode::FORBIDDEN, "district_scope_denied"));
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct DispatchError {
    pub status: StatusCode,
    pub code: String,
    pub error: String,
    pub detail: Option<serde_json::Value>,
}

impl DispatchError {
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "status": self.status.as_u16(),
            "code": self.code,
            "error": self.error,
        });
        if let Some(d) = &self.detail {
            obj["detail"] = d.clone();
        }
        obj
    }

    pub fn into_response(self) -> Response {
        (self.status, Json(self.to_json())).into_response()
    }
}

pub fn transition_error_to_dispatch(te: afterlife_core::TransitionError) -> DispatchError {
    let status = match &te {
        afterlife_core::TransitionError::OwnerMismatch { .. }
        | afterlife_core::TransitionError::HumanApprovalRequired { .. }
        | afterlife_core::TransitionError::AgentExecutionRequired { .. } => StatusCode::FORBIDDEN,
        afterlife_core::TransitionError::JobNotFound(_)
        | afterlife_core::TransitionError::DistrictNotFound(_)
        | afterlife_core::TransitionError::StageNotFound { .. }
        | afterlife_core::TransitionError::TemplateNotFound(_)
        | afterlife_core::TransitionError::ProposalNotFound(_) => StatusCode::NOT_FOUND,
        afterlife_core::TransitionError::RevisionConflict { .. }
        | afterlife_core::TransitionError::AlreadyLeased { .. }
        | afterlife_core::TransitionError::StaleFencingToken { .. }
        | afterlife_core::TransitionError::LeaseExpired { .. }
        | afterlife_core::TransitionError::LeaseMissing { .. }
        | afterlife_core::TransitionError::JobAlreadyInDistrict { .. }
        | afterlife_core::TransitionError::JobStopped { .. }
        | afterlife_core::TransitionError::JobNotStopped { .. }
        | afterlife_core::TransitionError::PhilosophyVersionConflict { .. }
        | afterlife_core::TransitionError::SpecVersionConflict { .. }
        | afterlife_core::TransitionError::DistrictArchived(_)
        | afterlife_core::TransitionError::DistrictNotArchived(_)
        | afterlife_core::TransitionError::DistrictAlreadyExists(_)
        | afterlife_core::TransitionError::DistrictHasActiveJobs { .. } => StatusCode::CONFLICT,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    DispatchError {
        status,
        code: te.code().to_string(),
        error: te.to_string(),
        detail: serde_json::to_value(&te).ok(),
    }
}

pub fn dispatch_command(
    state: &Arc<AppDaemonState>,
    principal: &afterlife_protocol::Principal,
    command_id: CommandId,
    command: EngineCommand,
) -> Result<CommandOutcome, DispatchError> {
    if let Err((status, code)) = authorize_command(principal, &command) {
        return Err(DispatchError {
            status,
            code: code.to_string(),
            error: code.to_string(),
            detail: None,
        });
    }

    if let EngineCommand::AttachArtifact {
        ref uri,
        ref sha256,
        ..
    } = command
    {
        verify_blob_reference(state, &principal.owner_id, uri, sha256.as_deref())?;
    }

    let env = CommandEnvelope {
        command_id: command_id.clone(),
        actor: principal.actor.clone(),
        command,
    };
    let outcome = state
        .ports
        .store
        .execute_command(env)
        .map_err(|err| match err {
            PortError::Domain(te) => transition_error_to_dispatch(te),
            PortError::InvalidInput(msg) => DispatchError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_input".into(),
                error: msg,
                detail: None,
            },
            PortError::IdempotencyConflict { command_id } => DispatchError {
                status: StatusCode::CONFLICT,
                code: "idempotency_conflict".into(),
                error: format!(
                    "command_id '{command_id}' was already used with a different command payload"
                ),
                detail: Some(serde_json::json!({ "command_id": command_id })),
            },
            other => DispatchError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error".into(),
                error: other.to_string(),
                detail: None,
            },
        })?;

    if !outcome.idempotent_replay {
        for (offset, ev) in outcome.events.iter().enumerate() {
            let record = EventRecord {
                seq: outcome.seq_start + offset as u64,
                command_id: command_id.clone(),
                owner_id: principal.owner_id.clone(),
                event: ev.clone(),
            };
            let _ = state.ports.telemetry.emit(&record);
            state.ports.event_bus.publish(record);
        }
    }

    Ok(outcome)
}

pub async fn health(State(state): State<Arc<AppDaemonState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "afterlifed",
        "issuer": state.auth.issuer(),
        "human_http_policy": state.auth.human_http_policy_label(),
        "policy_limits": {
            "min_lease_ttl_ms": afterlife_core::engine::policy::MIN_LEASE_TTL_MS,
            "max_lease_ttl_ms": afterlife_core::engine::policy::MAX_LEASE_TTL_MS,
            "jobs_list_default_limit": 50,
            "jobs_list_max_limit": 200,
            "max_artifact_blob_bytes": afterlife_core::engine::policy::MAX_ARTIFACT_BLOB_BYTES,
        },
        "log_path": state.metrics.log_path().map(|p| p.display().to_string()),
    }))
}

pub async fn get_metrics(State(state): State<Arc<AppDaemonState>>) -> impl IntoResponse {
    let telemetry_dir = match &state.config.telemetry {
        afterlife_protocol::TelemetryConfig::JsonlFile { dir } => Some(dir.clone()),
        _ => None,
    };
    Json(state.metrics.snapshot(telemetry_dir))
}

pub async fn reset_metrics(
    State(state): State<Arc<AppDaemonState>>,
    HumanWritePrincipal(principal): HumanWritePrincipal,
) -> Response {
    if !state.auth.allows_local_human_http() || !principal.actor.is_human() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "metrics_reset_local_dev_only" })),
        )
            .into_response();
    }
    state.metrics.reset();
    Json(serde_json::json!({ "reset": true })).into_response()
}

pub async fn get_workspace(
    State(state): State<Arc<AppDaemonState>>,
    AuthPrincipal(principal): AuthPrincipal,
) -> Response {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "insufficient_scope" })),
        )
            .into_response();
    }
    match state.ports.store.load_workspace(&principal.owner_id) {
        Ok(snap) => Json(snap).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_blob(
    State(state): State<Arc<AppDaemonState>>,
    AuthPrincipal(principal): AuthPrincipal,
    Path(sha256): Path<String>,
) -> Response {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "insufficient_scope" })),
        )
            .into_response();
    }
    match read_owned_blob_by_sha256(state.ports.blobs.as_ref(), &principal.owner_id, &sha256) {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (header::CACHE_CONTROL, "private, immutable"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            bytes,
        )
            .into_response(),
        Err(ArtifactBlobError::InvalidSha256) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "invalid_sha256" })),
        )
            .into_response(),
        Err(ArtifactBlobError::NotFound(_)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "blob_not_found" })),
        )
            .into_response(),
        Err(
            ArtifactBlobError::Storage(e)
            | ArtifactBlobError::OwnerMismatch { actual: e, .. },
        ) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
        Err(ArtifactBlobError::NotBlobUri | ArtifactBlobError::DigestMismatch) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "invalid_sha256" })),
        )
            .into_response(),
    }
}

pub async fn get_job_context(
    State(state): State<Arc<AppDaemonState>>,
    AuthPrincipal(principal): AuthPrincipal,
    Path(job_id): Path<String>,
) -> Response {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "insufficient_scope" })),
        )
            .into_response();
    }
    let snap = match state.ports.store.load_workspace(&principal.owner_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    match snap
        .engine
        .compile_execution_context(&JobId::from(job_id.as_str()))
    {
        Some(ctx) => Json(ctx).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "job_context_not_available" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct CommandHttpRequest {
    pub command_id: Option<CommandId>,
    pub command: EngineCommand,
}

pub async fn post_command(
    State(state): State<Arc<AppDaemonState>>,
    HumanWritePrincipal(principal): HumanWritePrincipal,
    Json(req): Json<CommandHttpRequest>,
) -> Response {
    let command_id = req
        .command_id
        .unwrap_or_else(|| CommandId::new(Uuid::now_v7().to_string()));
    match dispatch_command(&state, &principal, command_id, req.command) {
        Ok(outcome) => Json(outcome).into_response(),
        Err(err) => err.into_response(),
    }
}

#[derive(Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    pub since_seq: u64,
}

pub async fn sse_events(
    State(state): State<Arc<AppDaemonState>>,
    AuthPrincipal(principal): AuthPrincipal,
    Query(q): Query<EventsQuery>,
) -> Response {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "insufficient_scope" })),
        )
            .into_response();
    }
    let owner_id = principal.owner_id.clone();
    let rx = state.ports.event_bus.subscribe();
    let backlog = state
        .ports
        .store
        .list_events_since(&owner_id, q.since_seq, 500)
        .unwrap_or_default();
    let last_backlog_seq = backlog.last().map(|r| r.seq).unwrap_or(q.since_seq);

    let backlog_stream = tokio_stream::iter(backlog.into_iter().map(|rec| {
        let data = serde_json::to_string(&rec).unwrap_or_default();
        Ok::<SseEvent, Infallible>(
            SseEvent::default()
                .id(rec.seq.to_string())
                .event("afterlife_event")
                .data(data),
        )
    }));

    let live_stream = BroadcastStream::new(rx).filter_map(move |item| match item {
        Ok(rec) if rec.owner_id == owner_id && rec.seq > last_backlog_seq => {
            let data = serde_json::to_string(&rec).unwrap_or_default();
            Some(Ok::<SseEvent, Infallible>(
                SseEvent::default()
                    .id(rec.seq.to_string())
                    .event("afterlife_event")
                    .data(data),
            ))
        }
        _ => None,
    });

    Sse::new(backlog_stream.chain(live_stream))
        .keep_alive(KeepAlive::default())
        .into_response()
}
