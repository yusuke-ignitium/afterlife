use std::sync::Arc;

use afterlife_core::{
    ArtifactId, ArtifactKind, DistrictId, EngineCommand, FeedbackId, FeedbackKind, GateVerdict,
    JobId, PhilosophyVersion, Principle, SpecVersion, StageDef, StageId, SubMachineTemplateId,
    SubStageDef,
};
use afterlife_protocol::{Principal, Scope};
use serde_json::{Value, json};

use crate::{
    AppDaemonState,
    artifacts::{
        ArtifactPayloadSource, ArtifactServiceError, read_job_artifact,
        resolve_or_store_artifact_source,
    },
};

use super::{
    cmd_id_from, invalid_arg, parse_optional_fencing_token, parse_required_u64, rpc_err, rpc_ok,
    run_mcp_cmd, tool_text,
};

fn parse_optional_str<'a>(args: &'a Value, field: &str) -> Result<Option<&'a str>, String> {
    match args.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| "must be a string".to_string()),
    }
}

pub(super) fn handle_artifacts_attach(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
    let (Some(jid), Some(kind_v), Some(label)) = (
        args.get("job_id").and_then(|v| v.as_str()),
        args.get("kind"),
        args.get("label").and_then(|v| v.as_str()),
    ) else {
        return rpc_err(id, -32602, "missing required artifact fields");
    };
    if label.trim().is_empty() {
        return invalid_arg(id, "label", "must not be empty");
    }
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
    let provided_sha = match parse_optional_str(&args, "sha256") {
        Ok(v) => v.map(str::to_string),
        Err(msg) => return invalid_arg(id, "sha256", &msg),
    };
    let uri_arg = match parse_optional_str(&args, "uri") {
        Ok(v) => v,
        Err(msg) => return invalid_arg(id, "uri", &msg),
    };
    let content_arg = match parse_optional_str(&args, "content") {
        Ok(v) => v,
        Err(msg) => return invalid_arg(id, "content", &msg),
    };
    let content_b64_arg = match parse_optional_str(&args, "content_base64") {
        Ok(v) => v,
        Err(msg) => return invalid_arg(id, "content_base64", &msg),
    };

    let source = match (uri_arg, content_arg, content_b64_arg) {
        (Some(u), None, None) => ArtifactPayloadSource::Uri(u),
        (None, Some(c), None) => ArtifactPayloadSource::Utf8(c),
        (None, None, Some(b)) => ArtifactPayloadSource::Base64(b),
        _ => {
            return invalid_arg(
                id,
                "uri",
                "provide exactly one of 'uri', 'content', or 'content_base64'",
            );
        }
    };

    let job_id = JobId::from(jid);
    let (final_uri, final_sha) = match resolve_or_store_artifact_source(
        state,
        principal,
        &job_id,
        label,
        source,
        provided_sha,
        fencing_token,
        now,
    ) {
        Ok(pair) => pair,
        Err(ArtifactServiceError::InvalidArg { field, reason }) => {
            return invalid_arg(id, field, &reason);
        }
        Err(ArtifactServiceError::Dispatch(err)) => {
            return rpc_ok(id, tool_text(err.to_json(), true));
        }
    };

    run_mcp_cmd(
        state,
        principal,
        id,
        cmd_id,
        EngineCommand::AttachArtifact {
            job_id,
            kind,
            label: label.to_string(),
            uri: final_uri,
            sha256: final_sha,
            fencing_token,
            at: now,
        },
    )
}

pub(super) fn handle_artifacts_read(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
) -> Value {
    let (Some(jid), Some(aid)) = (
        args.get("job_id").and_then(|v| v.as_str()),
        args.get("artifact_id").and_then(|v| v.as_str()),
    ) else {
        return rpc_err(id, -32602, "missing job_id or artifact_id");
    };
    match read_job_artifact(state, principal, &JobId::from(jid), &ArtifactId::from(aid)) {
        Ok(body) => rpc_ok(id, tool_text(body, false)),
        Err(err) => rpc_ok(id, tool_text(err.to_json(), true)),
    }
}

pub(super) fn handle_gate_verify_record(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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

pub(super) fn handle_feedback_submit(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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

pub(super) fn handle_philosophy_propose(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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
    let proposed_district_name = match parse_optional_str(&args, "proposed_district_name") {
        Ok(v) => v.map(str::to_string),
        Err(msg) => return invalid_arg(id, "proposed_district_name", &msg),
    };
    run_mcp_cmd(
        state,
        principal,
        id,
        cmd_id,
        EngineCommand::ProposePhilosophyUpdate {
            district_id: DistrictId::from(did),
            base_version: PhilosophyVersion::new(base),
            proposed_district_name,
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

pub(super) fn handle_spec_propose(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
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
    let proposed_district_name = match parse_optional_str(&args, "proposed_district_name") {
        Ok(v) => v.map(str::to_string),
        Err(msg) => return invalid_arg(id, "proposed_district_name", &msg),
    };
    run_mcp_cmd(
        state,
        principal,
        id,
        cmd_id,
        EngineCommand::ProposeSpecUpdate {
            district_id: DistrictId::from(did),
            base_version: SpecVersion::new(base),
            proposed_district_name,
            stages,
            changelog: changelog.to_string(),
            rationale: rationale.to_string(),
            source_feedback_ids,
            at: now,
        },
    )
}

pub(super) fn handle_templates_list(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
) -> Value {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return rpc_err(id, -32001, "insufficient_scope: jobs:read required");
    }
    match state.ports.store.load_workspace(&principal.owner_id) {
        Ok(snap) => {
            let ordered: Vec<_> = snap
                .engine
                .template_order
                .iter()
                .filter_map(|tid| snap.engine.templates.get(tid))
                .collect();
            let outdated = snap.engine.outdated_builtin_template_ids();
            let pending = snap.engine.pending_template_proposals();
            rpc_ok(
                id,
                tool_text(
                    json!({
                        "template_order": snap.engine.template_order,
                        "templates": ordered,
                        "outdated_builtin_template_ids": outdated,
                        "pending_template_proposals": pending,
                        "latest_seq": snap.latest_seq,
                    }),
                    false,
                ),
            )
        }
        Err(e) => rpc_err(id, -32000, e.to_string()),
    }
}

pub(super) fn handle_template_propose(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    args: Value,
    now: u64,
) -> Value {
    let (Some(tid), Some(name), Some(description), Some(rationale), Some(stages_v)) = (
        args.get("template_id").and_then(|v| v.as_str()),
        args.get("name").and_then(|v| v.as_str()),
        args.get("description").and_then(|v| v.as_str()),
        args.get("rationale").and_then(|v| v.as_str()),
        args.get("stages"),
    ) else {
        return rpc_err(id, -32602, "missing template_propose fields");
    };
    let cmd_id = match cmd_id_from(&args) {
        Ok(cid) => cid,
        Err(msg) => return invalid_arg(id, "command_id", &msg),
    };
    let philosophy_notes = match parse_optional_str(&args, "philosophy_notes") {
        Ok(v) => v.unwrap_or("").to_string(),
        Err(msg) => return invalid_arg(id, "philosophy_notes", &msg),
    };
    let stages: Vec<SubStageDef> = match serde_json::from_value(stages_v.clone()) {
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
        EngineCommand::ProposeTemplateUpdate {
            template_id: SubMachineTemplateId::from(tid),
            name: name.to_string(),
            description: description.to_string(),
            philosophy_notes,
            stages,
            rationale: rationale.to_string(),
            source_feedback_ids,
            at: now,
        },
    )
}
