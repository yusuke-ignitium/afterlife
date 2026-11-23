use std::sync::Arc;

use afterlife_core::{DistrictId, EngineCommand, FeedbackId, Principle, ProposalId, StageDef};
use afterlife_protocol::{Principal, Scope};
use serde_json::{Value, json};

use crate::AppDaemonState;

use super::{cmd_id_from, invalid_arg, rpc_err, rpc_ok, run_mcp_cmd, tool_text};

fn parse_optional_str<'a>(args: &'a Value, field: &str) -> Result<Option<&'a str>, String> {
    match args.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| "must be a string".to_string()),
    }
}

fn parse_optional_vec<T: serde::de::DeserializeOwned>(
    args: &Value,
    field: &str,
) -> Result<Vec<T>, String> {
    match args.get(field) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| e.to_string()),
    }
}

pub(super) fn handle_districts_list(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
) -> Value {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return rpc_ok(
            id,
            tool_text(json!({ "error": "insufficient_scope" }), true),
        );
    }
    let include_archived = match args.get("include_archived") {
        None | Some(Value::Null) => false,
        Some(v) => match v.as_bool() {
            Some(b) => b,
            None => return invalid_arg(id, "include_archived", "must be a boolean"),
        },
    };
    match state.ports.store.load_workspace(&principal.owner_id) {
        Ok(snap) => {
            let list: Vec<Value> = snap
                .engine
                .districts
                .iter()
                .filter(|d| (include_archived || !d.archived) && principal.scopes.allows_district(&d.id))
                .map(|d| {
                    json!({
                        "id": d.id,
                        "name": d.name,
                        "active_philosophy_version": d.active_philosophy_version,
                        "philosophy_summary": d.active_philosophy().summary,
                        "active_spec_version": d.active_spec_version,
                        "stage_count": d.stages().len(),
                        "archived": d.archived,
                        "archived_at": d.archived_at,
                    })
                })
                .collect();
            let pending_proposals: Vec<_> = snap
                .engine
                .pending_district_proposals()
                .into_iter()
                .filter(|p| principal.scopes.allows_district_proposal(&p.action))
                .collect();
            rpc_ok(
                id,
                tool_text(
                    json!({
                        "districts": list,
                        "pending_district_proposals": pending_proposals,
                    }),
                    false,
                ),
            )
        }
        Err(e) => rpc_ok(id, tool_text(json!({ "error": e.to_string() }), true)),
    }
}

pub(super) fn handle_districts_propose_create(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
    let (Some(name), Some(summary), Some(rationale), Some(principles_v), Some(stages_v)) = (
        args.get("name").and_then(Value::as_str),
        args.get("summary").and_then(Value::as_str),
        args.get("rationale").and_then(Value::as_str),
        args.get("principles"),
        args.get("stages"),
    ) else {
        return rpc_err(id, -32602, "missing districts_propose_create fields");
    };
    let cmd_id = match cmd_id_from(&args) {
        Ok(cid) => cid,
        Err(msg) => return invalid_arg(id, "command_id", &msg),
    };
    let district_id = match parse_optional_str(&args, "district_id") {
        Ok(v) => v.map(DistrictId::from),
        Err(msg) => return invalid_arg(id, "district_id", &msg),
    };
    let setup_by = match parse_optional_str(&args, "setup_by") {
        Ok(v) => v.map(str::to_string),
        Err(msg) => return invalid_arg(id, "setup_by", &msg),
    };
    let changelog = match parse_optional_str(&args, "changelog") {
        Ok(v) => v.unwrap_or("").to_string(),
        Err(msg) => return invalid_arg(id, "changelog", &msg),
    };
    let principles: Vec<Principle> = match serde_json::from_value(principles_v.clone()) {
        Ok(p) => p,
        Err(e) => return invalid_arg(id, "principles", &e.to_string()),
    };
    let stages: Vec<StageDef> = match serde_json::from_value(stages_v.clone()) {
        Ok(s) => s,
        Err(e) => return invalid_arg(id, "stages", &e.to_string()),
    };
    let anti_patterns: Vec<String> = match parse_optional_vec(&args, "anti_patterns") {
        Ok(v) => v,
        Err(msg) => return invalid_arg(id, "anti_patterns", &msg),
    };
    let decision_heuristics: Vec<String> = match parse_optional_vec(&args, "decision_heuristics") {
        Ok(v) => v,
        Err(msg) => return invalid_arg(id, "decision_heuristics", &msg),
    };
    let source_feedback_ids: Vec<FeedbackId> =
        match parse_optional_vec(&args, "source_feedback_ids") {
            Ok(v) => v,
            Err(msg) => return invalid_arg(id, "source_feedback_ids", &msg),
        };
    run_mcp_cmd(
        state,
        principal,
        id,
        cmd_id,
        EngineCommand::ProposeDistrictCreate {
            district_id,
            name: name.to_string(),
            setup_by,
            summary: summary.to_string(),
            principles,
            anti_patterns,
            decision_heuristics,
            stages,
            changelog,
            rationale: rationale.to_string(),
            source_feedback_ids,
            at: now,
        },
    )
}

pub(super) fn handle_districts_propose_archive(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
    let (Some(did), Some(rationale)) = (
        args.get("district_id").and_then(Value::as_str),
        args.get("rationale").and_then(Value::as_str),
    ) else {
        return rpc_err(id, -32602, "missing districts_propose_archive fields");
    };
    let cmd_id = match cmd_id_from(&args) {
        Ok(cid) => cid,
        Err(msg) => return invalid_arg(id, "command_id", &msg),
    };
    let source_feedback_ids: Vec<FeedbackId> =
        match parse_optional_vec(&args, "source_feedback_ids") {
            Ok(v) => v,
            Err(msg) => return invalid_arg(id, "source_feedback_ids", &msg),
        };
    run_mcp_cmd(
        state,
        principal,
        id,
        cmd_id,
        EngineCommand::ProposeDistrictArchive {
            district_id: DistrictId::from(did),
            rationale: rationale.to_string(),
            source_feedback_ids,
            at: now,
        },
    )
}

pub(super) fn handle_districts_propose_restore(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
    let (Some(did), Some(rationale)) = (
        args.get("district_id").and_then(Value::as_str),
        args.get("rationale").and_then(Value::as_str),
    ) else {
        return rpc_err(id, -32602, "missing districts_propose_restore fields");
    };
    let cmd_id = match cmd_id_from(&args) {
        Ok(cid) => cid,
        Err(msg) => return invalid_arg(id, "command_id", &msg),
    };
    let source_feedback_ids: Vec<FeedbackId> =
        match parse_optional_vec(&args, "source_feedback_ids") {
            Ok(v) => v,
            Err(msg) => return invalid_arg(id, "source_feedback_ids", &msg),
        };
    run_mcp_cmd(
        state,
        principal,
        id,
        cmd_id,
        EngineCommand::ProposeDistrictRestore {
            district_id: DistrictId::from(did),
            rationale: rationale.to_string(),
            source_feedback_ids,
            at: now,
        },
    )
}

pub(super) fn handle_districts_approve_proposal(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
    let Some(pid) = args.get("proposal_id").and_then(Value::as_str) else {
        return rpc_err(id, -32602, "missing proposal_id");
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
        EngineCommand::ApproveDistrictProposal {
            proposal_id: ProposalId::from(pid),
            at: now,
        },
    )
}
