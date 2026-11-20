use std::{sync::Arc, time::Instant};

use afterlife_core::{
    ArtifactKind, CommandId, DistrictId, EngineCommand, FeedbackId, FeedbackKind, FencingToken,
    GateVerdict, JobId, JobLocation, PhilosophyVersion, Principle, RunnerId, SpecVersion,
    StageDef, StageId,
};
use afterlife_protocol::Scope;
use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppDaemonState,
    auth::{McpPrincipal, now_ms},
    rest::dispatch_command,
};

#[derive(Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

fn rpc_ok(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn rpc_err(id: Value, code: i32, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

fn tool_text(val: Value, is_error: bool) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&val).unwrap_or_default()
        }],
        "isError": is_error
    })
}

pub fn mcp_tool_definitions() -> Value {
    json!([
        {
            "name": "districts_list",
            "description": "List the authenticated user's Districts along with their active Philosophy and ProcedureSpec versions.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "jobs_list",
            "description": "List Jobs in the authenticated user's workspace with pagination (optionally filtered by district_id; archived jobs excluded unless include_archived is true).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "district_id": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                    "offset": { "type": "integer", "minimum": 0 },
                    "include_archived": { "type": "boolean" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "job_get_context",
            "description": "Compile the full hierarchical ExecutionContext (District Philosophy + Stage Procedure + Sub-Stage Procedure + GateRubric + recent Feedbacks) for a Job.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id"],
                "properties": { "job_id": { "type": "string" } },
                "additionalProperties": false
            }
        },
        {
            "name": "jobs_create",
            "description": "Queue a new Job idempotently into the user's workspace. Inline Markdown links like [LIN-123](https://linear.app/...) in title or raw_input render as clickable links in the UI.",
            "inputSchema": {
                "type": "object",
                "required": ["title"],
                "properties": {
                    "command_id": { "type": "string" },
                    "title": { "type": "string", "description": "Job title; supports inline [LABEL](https://...) Markdown links (e.g. Linear ticket IDs)." },
                    "raw_input": { "type": "string", "description": "Job details/context; supports inline [LABEL](https://...) Markdown links." },
                    "target_district_hint": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "jobs_update",
            "description": "Update a Job's title, raw_input, spec_summary, or target_district_hint. Supports inline [LABEL](https://...) Markdown links. In-district updates require an active fencing_token and cannot target Human ApprovalGates.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id"],
                "properties": {
                    "command_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "title": { "type": "string", "description": "Updated title; supports inline [LABEL](https://...) Markdown links." },
                    "raw_input": { "type": "string", "description": "Updated raw input; supports inline [LABEL](https://...) Markdown links." },
                    "spec_summary": { "type": "string", "description": "Updated spec summary; supports inline [LABEL](https://...) Markdown links." },
                    "target_district_hint": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "fencing_token": { "type": "integer" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "jobs_archive",
            "description": "Soft-delete (archive) a Job out of active views while preserving audit history. Active in-district jobs require fencing_token; Human ApprovalGates cannot be archived by agents.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id"],
                "properties": {
                    "command_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "reason": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "fencing_token": { "type": "integer" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "jobs_assign",
            "description": "Assign a queued Job into a District's entry stage.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id", "district_id"],
                "properties": {
                    "command_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "district_id": { "type": "string" },
                    "expected_revision": { "type": "integer" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "jobs_claim",
            "description": "Atomically acquire an exclusive lease and monotonic fencing_token on a Job.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id"],
                "properties": {
                    "command_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "runner_label": { "type": "string" },
                    "ttl_ms": { "type": "integer" },
                    "expected_revision": { "type": "integer" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "jobs_heartbeat",
            "description": "Extend an active Job lease using the held fencing_token.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id", "fencing_token"],
                "properties": {
                    "command_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "fencing_token": { "type": "integer" },
                    "ttl_ms": { "type": "integer" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "jobs_advance",
            "description": "Advance a Job to its next sub-stage or parent stage. Requires active fencing_token and passing GateVerification (when GateRubric is configured); Human ApprovalGates cannot be bypassed by agents. Advancing releases the current stage lease so the next stage requires a fresh jobs_claim.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id"],
                "properties": {
                    "command_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "fencing_token": { "type": "integer" },
                    "expected_revision": { "type": "integer" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "jobs_rework",
            "description": "Send a Job back to the stage's rework target and record the reason as Feedback.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id"],
                "properties": {
                    "command_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "fencing_token": { "type": "integer" },
                    "reason": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "artifacts_attach",
            "description": "Attach an artifact reference (issue, spec, source_code, pull_request, ci_report, snapshot, verification_report, raw_note) to a leased Job. http(s)://, linear://, and slack:// URIs render as clickable links.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id", "kind", "label", "uri"],
                "properties": {
                    "command_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "kind": { "type": "string", "enum": ["raw_note", "issue", "spec", "source_code", "pull_request", "ci_report", "snapshot", "verification_report"] },
                    "label": { "type": "string" },
                    "uri": { "type": "string", "description": "Artifact URI; http(s)://, linear://, and slack:// URIs render as clickable links in the UI." },
                    "sha256": { "type": "string" },
                    "fencing_token": { "type": "integer" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "gate_verify_record",
            "description": "Record an LLM gate verification result against the current stage's GateRubric.",
            "inputSchema": {
                "type": "object",
                "required": ["job_id", "verdict", "summary"],
                "properties": {
                    "command_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "verdict": { "type": "string", "enum": ["pass", "needs_work", "fail"] },
                    "summary": { "type": "string" },
                    "checklist_results": { "type": "array" },
                    "fencing_token": { "type": "integer" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "feedback_submit",
            "description": "Submit structured Feedback on a District/Job/Stage to accumulate learnings for Philosophy and ProcedureSpec evolution.",
            "inputSchema": {
                "type": "object",
                "required": ["district_id", "kind", "body"],
                "properties": {
                    "command_id": { "type": "string" },
                    "district_id": { "type": "string" },
                    "job_id": { "type": "string" },
                    "stage_id": { "type": "string" },
                    "kind": { "type": "string", "enum": ["approval_note", "rework_reason", "philosophy_insight", "procedure_fix"] },
                    "body": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "philosophy_propose",
            "description": "Propose a new version of a District's explicit Philosophy based on accumulated feedback (requires human sign-off to activate).",
            "inputSchema": {
                "type": "object",
                "required": ["district_id", "base_version", "summary", "principles", "rationale"],
                "properties": {
                    "command_id": { "type": "string" },
                    "district_id": { "type": "string" },
                    "base_version": { "type": "integer" },
                    "summary": { "type": "string" },
                    "principles": { "type": "array" },
                    "anti_patterns": { "type": "array", "items": { "type": "string" } },
                    "decision_heuristics": { "type": "array", "items": { "type": "string" } },
                    "rationale": { "type": "string" },
                    "source_feedback_ids": { "type": "array", "items": { "type": "string" } }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "spec_propose",
            "description": "Propose a new version of a District's nested state-machine ProcedureSpec (requires human sign-off to activate).",
            "inputSchema": {
                "type": "object",
                "required": ["district_id", "base_version", "stages", "changelog", "rationale"],
                "properties": {
                    "command_id": { "type": "string" },
                    "district_id": { "type": "string" },
                    "base_version": { "type": "integer" },
                    "stages": { "type": "array" },
                    "changelog": { "type": "string" },
                    "rationale": { "type": "string" },
                    "source_feedback_ids": { "type": "array", "items": { "type": "string" } }
                },
                "additionalProperties": false
            }
        }
    ])
}

pub async fn mcp_post(
    State(state): State<Arc<AppDaemonState>>,
    McpPrincipal(principal): McpPrincipal,
    Json(req): Json<McpRequest>,
) -> Response {
    let started = Instant::now();
    let actor_label = principal.actor.display_label();

    if req.jsonrpc != "2.0" {
        let id = req.id.clone().unwrap_or(Value::Null);
        state.metrics.record_mcp_call(
            &actor_label,
            &req.method,
            None,
            "error",
            started.elapsed().as_micros() as u64,
            Some("invalid jsonrpc version"),
        );
        return Json(rpc_err(id, -32600, "invalid jsonrpc version")).into_response();
    }

    if req.method.starts_with("notifications/") || req.id.is_none() {
        state.metrics.record_mcp_call(
            &actor_label,
            &req.method,
            None,
            "ok",
            started.elapsed().as_micros() as u64,
            None,
        );
        return StatusCode::ACCEPTED.into_response();
    }

    let id = req.id.clone().unwrap_or(Value::Null);
    match req.method.as_str() {
        "initialize" => {
            state.metrics.record_mcp_call(
                &actor_label,
                "initialize",
                None,
                "ok",
                started.elapsed().as_micros() as u64,
                None,
            );
            Json(rpc_ok(
                id,
                json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "afterlife-control-plane", "version": env!("CARGO_PKG_VERSION") }
                }),
            ))
            .into_response()
        }
        "ping" => {
            state.metrics.record_mcp_call(
                &actor_label,
                "ping",
                None,
                "ok",
                started.elapsed().as_micros() as u64,
                None,
            );
            Json(rpc_ok(id, json!({}))).into_response()
        }
        "tools/list" => {
            state.metrics.record_mcp_call(
                &actor_label,
                "tools/list",
                None,
                "ok",
                started.elapsed().as_micros() as u64,
                None,
            );
            Json(rpc_ok(id, json!({ "tools": mcp_tool_definitions() }))).into_response()
        }
        "tools/call" => {
            let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
                state.metrics.record_mcp_call(
                    &actor_label,
                    "tools/call",
                    None,
                    "error",
                    started.elapsed().as_micros() as u64,
                    Some("missing tool name"),
                );
                return Json(rpc_err(id, -32602, "missing tool name")).into_response();
            };
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let res = handle_tool_call(&state, &principal, id, name, args);
            let (outcome, detail) = classify_mcp_outcome(&res);
            state.metrics.record_mcp_call(
                &actor_label,
                "tools/call",
                Some(name),
                outcome,
                started.elapsed().as_micros() as u64,
                detail.as_deref(),
            );
            Json(res).into_response()
        }
        other => {
            state.metrics.record_mcp_call(
                &actor_label,
                other,
                None,
                "error",
                started.elapsed().as_micros() as u64,
                Some("method not found"),
            );
            Json(rpc_err(id, -32601, format!("method not found: {other}"))).into_response()
        }
    }
}

pub async fn mcp_get() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "POST, DELETE")],
        "SSE notifications are served on GET /v1/events",
    )
        .into_response()
}

pub async fn mcp_delete() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

fn classify_mcp_outcome(resp: &Value) -> (&'static str, Option<String>) {
    if let Some(err) = resp.get("error") {
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned);
        return ("error", msg);
    }
    let is_err = resp
        .get("result")
        .and_then(|r| r.get("isError"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !is_err {
        return ("ok", None);
    }
    let text = resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.get(0))
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str());
    if let Some(raw) = text {
        if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
            let status = parsed.get("status").and_then(|v| v.as_u64());
            let detail = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
            return match status {
                Some(409) => ("conflict", detail),
                Some(403) => ("forbidden", detail),
                _ => ("error", detail),
            };
        }
    }
    ("error", None)
}

fn invalid_arg(id: Value, field: &str, reason: &str) -> Value {
    rpc_ok(
        id,
        tool_text(
            json!({
                "status": 422,
                "code": "invalid_input",
                "error": format!("invalid argument '{field}': {reason}")
            }),
            true,
        ),
    )
}

fn parse_optional_u64(args: &Value, field: &str) -> Result<Option<u64>, String> {
    match args.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("expected non-negative integer for '{field}', got {v}")),
    }
}

fn parse_required_u64(args: &Value, field: &str) -> Result<u64, String> {
    match args.get(field) {
        None | Some(Value::Null) => Err(format!("missing required integer field '{field}'")),
        Some(v) => v
            .as_u64()
            .ok_or_else(|| format!("expected non-negative integer for '{field}', got {v}")),
    }
}

fn parse_optional_fencing_token(args: &Value) -> Result<Option<FencingToken>, String> {
    parse_optional_u64(args, "fencing_token").map(|opt| opt.map(FencingToken::new))
}

fn cmd_id_from(args: &Value) -> Result<CommandId, String> {
    match args.get("command_id") {
        None | Some(Value::Null) => Ok(CommandId::new(Uuid::now_v7().to_string())),
        Some(v) => {
            let Some(s) = v.as_str() else {
                return Err("command_id must be a string".into());
            };
            if s.trim().is_empty() {
                return Err("command_id must not be empty".into());
            }
            Ok(CommandId::from(s))
        }
    }
}

fn handle_tool_call(
    state: &Arc<AppDaemonState>,
    principal: &afterlife_protocol::Principal,
    id: Value,
    name: &str,
    args: Value,
) -> Value {
    let now = now_ms();
    match name {
        "districts_list" => {
            if !principal.scopes.contains(&Scope::JobsRead) {
                return rpc_ok(id, tool_text(json!({ "error": "insufficient_scope" }), true));
            }
            match state.ports.store.load_workspace(&principal.owner_id) {
                Ok(snap) => {
                    let list: Vec<Value> = snap
                        .engine
                        .districts
                        .iter()
                        .filter(|d| principal.scopes.allows_district(&d.id))
                        .map(|d| {
                            json!({
                                "id": d.id,
                                "name": d.name,
                                "active_philosophy_version": d.active_philosophy_version,
                                "philosophy_summary": d.active_philosophy().summary,
                                "active_spec_version": d.active_spec_version,
                                "stage_count": d.stages().len(),
                            })
                        })
                        .collect();
                    rpc_ok(id, tool_text(json!({ "districts": list }), false))
                }
                Err(e) => rpc_ok(id, tool_text(json!({ "error": e.to_string() }), true)),
            }
        }
        "jobs_list" => {
            if !principal.scopes.contains(&Scope::JobsRead) {
                return rpc_ok(id, tool_text(json!({ "status": 403, "code": "insufficient_scope", "error": "insufficient_scope" }), true));
            }
            let limit = match parse_optional_u64(&args, "limit") {
                Ok(Some(v)) if (1..=200).contains(&v) => v as usize,
                Ok(Some(v)) => return invalid_arg(id, "limit", &format!("must be between 1 and 200 (got {v})")),
                Ok(None) => 50,
                Err(msg) => return invalid_arg(id, "limit", &msg),
            };
            let offset = match parse_optional_u64(&args, "offset") {
                Ok(Some(v)) => v as usize,
                Ok(None) => 0,
                Err(msg) => return invalid_arg(id, "offset", &msg),
            };
            let filter_dist = args
                .get("district_id")
                .and_then(|v| v.as_str())
                .map(DistrictId::from);
            let include_archived = match args.get("include_archived") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                Some(_) => return invalid_arg(id, "include_archived", "must be a boolean"),
            };
            match state.ports.store.load_workspace(&principal.owner_id) {
                Ok(snap) => {
                    let filtered: Vec<_> = snap
                        .engine
                        .jobs
                        .iter()
                        .filter(|j| include_archived || !matches!(j.location, JobLocation::Archived))
                        .filter(|j| match &filter_dist {
                            Some(did) => j.is_in_district(did),
                            None => true,
                        })
                        .collect();
                    let total_count = filtered.len();
                    let jobs: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();
                    let has_more = offset.saturating_add(jobs.len()) < total_count;
                    rpc_ok(
                        id,
                        tool_text(
                            json!({
                                "latest_seq": snap.latest_seq,
                                "total_count": total_count,
                                "limit": limit,
                                "offset": offset,
                                "has_more": has_more,
                                "jobs": jobs
                            }),
                            false,
                        ),
                    )
                }
                Err(e) => rpc_ok(id, tool_text(json!({ "status": 500, "code": "storage_error", "error": e.to_string() }), true)),
            }
        }
        "job_get_context" => {
            if !principal.scopes.contains(&Scope::JobsRead) {
                return rpc_ok(id, tool_text(json!({ "status": 403, "code": "insufficient_scope", "error": "insufficient_scope" }), true));
            }
            let Some(jid) = args.get("job_id").and_then(|v| v.as_str()) else {
                return rpc_err(id, -32602, "missing job_id");
            };
            match state.ports.store.load_workspace(&principal.owner_id) {
                Ok(snap) => match snap.engine.compile_execution_context(&JobId::from(jid)) {
                    Some(ctx) => rpc_ok(id, tool_text(json!(ctx), false)),
                    None => rpc_ok(
                        id,
                        tool_text(json!({ "status": 404, "code": "job_context_not_available", "error": "job not active in a district" }), true),
                    ),
                },
                Err(e) => rpc_ok(id, tool_text(json!({ "status": 500, "code": "storage_error", "error": e.to_string() }), true)),
            }
        }
        "jobs_create" => {
            let Some(title) = args.get("title").and_then(|v| v.as_str()) else {
                return rpc_err(id, -32602, "missing title");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let raw_input = args
                .get("raw_input")
                .and_then(|v| v.as_str())
                .unwrap_or(title)
                .to_string();
            let hint = args
                .get("target_district_hint")
                .and_then(|v| v.as_str())
                .map(DistrictId::from);
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::QueueJob {
                    title: title.to_string(),
                    raw_input,
                    target_district_hint: hint,
                    at: now,
                },
            )
        }
        "jobs_update" => {
            let Some(jid) = args.get("job_id").and_then(|v| v.as_str()) else {
                return rpc_err(id, -32602, "missing job_id");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let expected_revision = match parse_optional_u64(&args, "expected_revision") {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "expected_revision", &msg),
            };
            let fencing_token = match parse_optional_fencing_token(&args) {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "fencing_token", &msg),
            };
            let title = args.get("title").and_then(|v| v.as_str()).map(str::to_string);
            let raw_input = args
                .get("raw_input")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let spec_summary = args
                .get("spec_summary")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let target_district_hint = args
                .get("target_district_hint")
                .and_then(|v| v.as_str())
                .map(DistrictId::from);
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::UpdateJob {
                    job_id: JobId::from(jid),
                    title,
                    raw_input,
                    spec_summary,
                    target_district_hint,
                    expected_revision,
                    fencing_token,
                    at: now,
                },
            )
        }
        "jobs_archive" => {
            let Some(jid) = args.get("job_id").and_then(|v| v.as_str()) else {
                return rpc_err(id, -32602, "missing job_id");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let expected_revision = match parse_optional_u64(&args, "expected_revision") {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "expected_revision", &msg),
            };
            let fencing_token = match parse_optional_fencing_token(&args) {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "fencing_token", &msg),
            };
            let reason = args
                .get("reason")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::ArchiveJob {
                    job_id: JobId::from(jid),
                    reason,
                    expected_revision,
                    fencing_token,
                    at: now,
                },
            )
        }
        "jobs_assign" => {
            let (Some(jid), Some(did)) = (
                args.get("job_id").and_then(|v| v.as_str()),
                args.get("district_id").and_then(|v| v.as_str()),
            ) else {
                return rpc_err(id, -32602, "missing job_id or district_id");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let expected_revision = match parse_optional_u64(&args, "expected_revision") {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "expected_revision", &msg),
            };
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::AssignJobToDistrict {
                    job_id: JobId::from(jid),
                    district_id: DistrictId::from(did),
                    expected_revision,
                    at: now,
                },
            )
        }
        "jobs_claim" => {
            let Some(jid) = args.get("job_id").and_then(|v| v.as_str()) else {
                return rpc_err(id, -32602, "missing job_id");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let ttl_ms = match parse_optional_u64(&args, "ttl_ms") {
                Ok(Some(v)) => v,
                Ok(None) => 300_000,
                Err(msg) => return invalid_arg(id, "ttl_ms", &msg),
            };
            let expected_revision = match parse_optional_u64(&args, "expected_revision") {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "expected_revision", &msg),
            };
            let runner_label = args
                .get("runner_label")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| principal.actor.display_label())
                .to_string();
            let runner_id = RunnerId::new(
                runner_label
                    .to_ascii_lowercase()
                    .replace(|c: char| !c.is_ascii_alphanumeric(), "-"),
            );
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::ClaimJob {
                    job_id: JobId::from(jid),
                    runner_id,
                    runner_label,
                    ttl_ms,
                    expected_revision,
                    at: now,
                },
            )
        }
        "jobs_heartbeat" => {
            let Some(jid) = args.get("job_id").and_then(|v| v.as_str()) else {
                return rpc_err(id, -32602, "missing job_id");
            };
            let tok = match parse_required_u64(&args, "fencing_token") {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "fencing_token", &msg),
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let ttl_ms = match parse_optional_u64(&args, "ttl_ms") {
                Ok(Some(v)) => v,
                Ok(None) => 300_000,
                Err(msg) => return invalid_arg(id, "ttl_ms", &msg),
            };
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::HeartbeatLease {
                    job_id: JobId::from(jid),
                    fencing_token: FencingToken::new(tok),
                    ttl_ms,
                    at: now,
                },
            )
        }
        "jobs_advance" => {
            let Some(jid) = args.get("job_id").and_then(|v| v.as_str()) else {
                return rpc_err(id, -32602, "missing job_id");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let expected_revision = match parse_optional_u64(&args, "expected_revision") {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "expected_revision", &msg),
            };
            let fencing_token = match parse_optional_fencing_token(&args) {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "fencing_token", &msg),
            };
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::AdvanceJob {
                    job_id: JobId::from(jid),
                    expected_revision,
                    fencing_token,
                    at: now,
                },
            )
        }
        "jobs_rework" => {
            let Some(jid) = args.get("job_id").and_then(|v| v.as_str()) else {
                return rpc_err(id, -32602, "missing job_id");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let expected_revision = match parse_optional_u64(&args, "expected_revision") {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "expected_revision", &msg),
            };
            let fencing_token = match parse_optional_fencing_token(&args) {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "fencing_token", &msg),
            };
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::ReworkJob {
                    job_id: JobId::from(jid),
                    expected_revision,
                    fencing_token,
                    reason: args
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    at: now,
                },
            )
        }
        "artifacts_attach" => {
            let (Some(jid), Some(kind_v), Some(label), Some(uri)) = (
                args.get("job_id").and_then(|v| v.as_str()),
                args.get("kind"),
                args.get("label").and_then(|v| v.as_str()),
                args.get("uri").and_then(|v| v.as_str()),
            ) else {
                return rpc_err(id, -32602, "missing required artifact fields");
            };
            let Ok(kind) = serde_json::from_value::<ArtifactKind>(kind_v.clone()) else {
                return invalid_arg(id, "kind", "invalid artifact kind");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let fencing_token = match parse_optional_fencing_token(&args) {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "fencing_token", &msg),
            };
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::AttachArtifact {
                    job_id: JobId::from(jid),
                    kind,
                    label: label.to_string(),
                    uri: uri.to_string(),
                    sha256: args
                        .get("sha256")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    fencing_token,
                    at: now,
                },
            )
        }
        "gate_verify_record" => {
            let (Some(jid), Some(verdict_v), Some(summary)) = (
                args.get("job_id").and_then(|v| v.as_str()),
                args.get("verdict"),
                args.get("summary").and_then(|v| v.as_str()),
            ) else {
                return rpc_err(id, -32602, "missing gate_verify_record fields");
            };
            let Ok(verdict) = serde_json::from_value::<GateVerdict>(verdict_v.clone()) else {
                return invalid_arg(id, "verdict", "invalid gate verdict");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let fencing_token = match parse_optional_fencing_token(&args) {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "fencing_token", &msg),
            };
            let checklist_results: Vec<(String, bool)> = match args.get("checklist_results") {
                None | Some(Value::Null) => Vec::new(),
                Some(v) => match serde_json::from_value(v.clone()) {
                    Ok(list) => list,
                    Err(e) => return invalid_arg(id, "checklist_results", &e.to_string()),
                },
            };
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::RecordGateVerification {
                    job_id: JobId::from(jid),
                    verdict,
                    summary: summary.to_string(),
                    checklist_results,
                    fencing_token,
                    at: now,
                },
            )
        }
        "feedback_submit" => {
            let (Some(did), Some(kind_v), Some(body)) = (
                args.get("district_id").and_then(|v| v.as_str()),
                args.get("kind"),
                args.get("body").and_then(|v| v.as_str()),
            ) else {
                return rpc_err(id, -32602, "missing feedback fields");
            };
            let Ok(kind) = serde_json::from_value::<FeedbackKind>(kind_v.clone()) else {
                return invalid_arg(id, "kind", "invalid feedback kind");
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::SubmitFeedback {
                    district_id: DistrictId::from(did),
                    job_id: args.get("job_id").and_then(|v| v.as_str()).map(JobId::from),
                    stage_id: args
                        .get("stage_id")
                        .and_then(|v| v.as_str())
                        .map(StageId::from),
                    kind,
                    body: body.to_string(),
                    at: now,
                },
            )
        }
        "philosophy_propose" => {
            let (Some(did), Some(summary), Some(rationale), Some(principles_v)) = (
                args.get("district_id").and_then(|v| v.as_str()),
                args.get("summary").and_then(|v| v.as_str()),
                args.get("rationale").and_then(|v| v.as_str()),
                args.get("principles"),
            ) else {
                return rpc_err(id, -32602, "missing philosophy_propose fields");
            };
            let base = match parse_required_u64(&args, "base_version") {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "base_version", &msg),
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let principles: Vec<Principle> = match serde_json::from_value(principles_v.clone()) {
                Ok(p) => p,
                Err(e) => return invalid_arg(id, "principles", &e.to_string()),
            };
            let anti_patterns: Vec<String> = match args.get("anti_patterns") {
                None | Some(Value::Null) => Vec::new(),
                Some(v) => match serde_json::from_value(v.clone()) {
                    Ok(a) => a,
                    Err(e) => return invalid_arg(id, "anti_patterns", &e.to_string()),
                },
            };
            let decision_heuristics: Vec<String> = match args.get("decision_heuristics") {
                None | Some(Value::Null) => Vec::new(),
                Some(v) => match serde_json::from_value(v.clone()) {
                    Ok(h) => h,
                    Err(e) => return invalid_arg(id, "decision_heuristics", &e.to_string()),
                },
            };
            let source_feedback_ids: Vec<FeedbackId> = match args.get("source_feedback_ids") {
                None | Some(Value::Null) => Vec::new(),
                Some(v) => match serde_json::from_value(v.clone()) {
                    Ok(f) => f,
                    Err(e) => return invalid_arg(id, "source_feedback_ids", &e.to_string()),
                },
            };
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::ProposePhilosophyUpdate {
                    district_id: DistrictId::from(did),
                    base_version: PhilosophyVersion::new(base),
                    summary: summary.to_string(),
                    principles,
                    anti_patterns,
                    decision_heuristics,
                    rationale: rationale.to_string(),
                    source_feedback_ids,
                    at: now,
                },
            )
        }
        "spec_propose" => {
            let (Some(did), Some(changelog), Some(rationale), Some(stages_v)) = (
                args.get("district_id").and_then(|v| v.as_str()),
                args.get("changelog").and_then(|v| v.as_str()),
                args.get("rationale").and_then(|v| v.as_str()),
                args.get("stages"),
            ) else {
                return rpc_err(id, -32602, "missing spec_propose fields");
            };
            let base = match parse_required_u64(&args, "base_version") {
                Ok(v) => v,
                Err(msg) => return invalid_arg(id, "base_version", &msg),
            };
            let cmd_id = match cmd_id_from(&args) {
                Ok(cid) => cid,
                Err(msg) => return invalid_arg(id, "command_id", &msg),
            };
            let stages: Vec<StageDef> = match serde_json::from_value(stages_v.clone()) {
                Ok(s) => s,
                Err(e) => return invalid_arg(id, "stages", &e.to_string()),
            };
            let source_feedback_ids: Vec<FeedbackId> = match args.get("source_feedback_ids") {
                None | Some(Value::Null) => Vec::new(),
                Some(v) => match serde_json::from_value(v.clone()) {
                    Ok(f) => f,
                    Err(e) => return invalid_arg(id, "source_feedback_ids", &e.to_string()),
                },
            };
            run_mcp_cmd(
                state,
                principal,
                id,
                cmd_id,
                EngineCommand::ProposeSpecUpdate {
                    district_id: DistrictId::from(did),
                    base_version: SpecVersion::new(base),
                    stages,
                    changelog: changelog.to_string(),
                    rationale: rationale.to_string(),
                    source_feedback_ids,
                    at: now,
                },
            )
        }
        other => rpc_err(id, -32601, format!("unknown tool: {other}")),
    }
}

fn run_mcp_cmd(
    state: &Arc<AppDaemonState>,
    principal: &afterlife_protocol::Principal,
    id: Value,
    command_id: CommandId,
    command: EngineCommand,
) -> Value {
    match dispatch_command(state, principal, command_id, command) {
        Ok(outcome) => rpc_ok(id, tool_text(json!(outcome), false)),
        Err(err) => rpc_ok(id, tool_text(err.to_json(), true)),
    }
}
