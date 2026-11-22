use std::sync::Arc;

use afterlife_core::{
    ArtifactId, DistrictId, EngineCommand, FencingToken, JobId, JobLocation, RunnerId,
};
use afterlife_protocol::{Principal, Scope};
use serde_json::{Value, json};

use crate::AppDaemonState;

use super::{
    cmd_id_from, invalid_arg, parse_optional_fencing_token, parse_optional_u64, parse_required_u64,
    rpc_err, rpc_ok, run_mcp_cmd, tool_text,
};

pub(super) fn handle_districts_list(state: &Arc<AppDaemonState>, principal: &Principal, id: Value) -> Value {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return rpc_ok(
            id,
            tool_text(json!({ "error": "insufficient_scope" }), true),
        );
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

pub(super) fn handle_jobs_list(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
) -> Value {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return rpc_ok(
            id,
            tool_text(
                json!({ "status": 403, "code": "insufficient_scope", "error": "insufficient_scope" }),
                true,
            ),
        );
    }
    let limit = match parse_optional_u64(&args, "limit") {
        Ok(Some(v)) if (1..=200).contains(&v) => v as usize,
        Ok(Some(v)) => {
            return invalid_arg(id, "limit", &format!("must be between 1 and 200 (got {v})"));
        }
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
        Err(e) => rpc_ok(
            id,
            tool_text(
                json!({ "status": 500, "code": "storage_error", "error": e.to_string() }),
                true,
            ),
        ),
    }
}

pub(super) fn handle_job_get_context(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
) -> Value {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return rpc_ok(
            id,
            tool_text(
                json!({ "status": 403, "code": "insufficient_scope", "error": "insufficient_scope" }),
                true,
            ),
        );
    }
    let Some(jid) = args.get("job_id").and_then(|v| v.as_str()) else {
        return rpc_err(id, -32602, "missing job_id");
    };
    match state.ports.store.load_workspace(&principal.owner_id) {
        Ok(snap) => match snap.engine.compile_execution_context(&JobId::from(jid)) {
            Some(ctx) => rpc_ok(id, tool_text(json!(ctx), false)),
            None => rpc_ok(
                id,
                tool_text(
                    json!({ "status": 404, "code": "job_context_not_available", "error": "job not active in a district" }),
                    true,
                ),
            ),
        },
        Err(e) => rpc_ok(
            id,
            tool_text(
                json!({ "status": 500, "code": "storage_error", "error": e.to_string() }),
                true,
            ),
        ),
    }
}

pub(super) fn handle_jobs_create(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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

pub(super) fn handle_jobs_update(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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
        EngineCommand::UpdateJob {
            job_id: JobId::from(jid),
            title: args.get("title").and_then(|v| v.as_str()).map(str::to_string),
            raw_input: args
                .get("raw_input")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            spec_summary: args
                .get("spec_summary")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            target_district_hint: args
                .get("target_district_hint")
                .and_then(|v| v.as_str())
                .map(DistrictId::from),
            expected_revision,
            fencing_token,
            at: now,
        },
    )
}

pub(super) fn handle_jobs_archive(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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
        EngineCommand::ArchiveJob {
            job_id: JobId::from(jid),
            reason: args
                .get("reason")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            expected_revision,
            fencing_token,
            at: now,
        },
    )
}

pub(super) fn handle_jobs_assign(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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

pub(super) fn handle_jobs_claim(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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

pub(super) fn handle_jobs_heartbeat(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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

pub(super) fn handle_jobs_advance(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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
    let mut fields = std::collections::BTreeMap::new();
    if let Some(fields_v) = args.get("fields") {
        let Some(obj) = fields_v.as_object() else {
            return invalid_arg(
                id,
                "fields",
                "must be an object mapping field keys to strings",
            );
        };
        for (k, v) in obj {
            let Some(s) = v.as_str() else {
                return invalid_arg(id, "fields", &format!("field '{k}' must be a string"));
            };
            fields.insert(k.clone(), s.to_string());
        }
    }
    let mut artifact_ids = Vec::new();
    if let Some(arr_v) = args.get("artifact_ids") {
        let Some(arr) = arr_v.as_array() else {
            return invalid_arg(id, "artifact_ids", "must be an array of artifact IDs");
        };
        for item in arr {
            let Some(s) = item.as_str() else {
                return invalid_arg(id, "artifact_ids", "each artifact id must be a string");
            };
            artifact_ids.push(ArtifactId::from(s));
        }
    }
    run_mcp_cmd(
        state,
        principal,
        id,
        cmd_id,
        EngineCommand::AdvanceJob {
            job_id: JobId::from(jid),
            fields,
            artifact_ids,
            expected_revision,
            fencing_token,
            at: now,
        },
    )
}

pub(super) fn handle_jobs_rework(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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
                .map(str::to_string),
            at: now,
        },
    )
}

