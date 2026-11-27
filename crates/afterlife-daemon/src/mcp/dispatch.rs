use std::sync::Arc;

use afterlife_protocol::Principal;
use serde_json::Value;

use crate::{AppDaemonState, auth::now_ms};

use super::{district_tools, governance_tools, jobs_tools, rpc_err};

pub(super) fn handle_tool_call(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    id: Value,
    name: &str,
    args: Value,
) -> Value {
    let now = now_ms();
    match name {
        "districts_list" => district_tools::handle_districts_list(state, principal, id, args),
        "districts_export_skills" => {
            district_tools::handle_districts_export_skills(state, principal, id, args)
        }
        "districts_propose_create" => {
            district_tools::handle_districts_propose_create(state, principal, id, args, now)
        }
        "districts_propose_archive" => {
            district_tools::handle_districts_propose_archive(state, principal, id, args, now)
        }
        "districts_propose_restore" => {
            district_tools::handle_districts_propose_restore(state, principal, id, args, now)
        }
        "districts_approve_proposal" => {
            district_tools::handle_districts_approve_proposal(state, principal, id, args, now)
        }
        "jobs_list" => jobs_tools::handle_jobs_list(state, principal, id, args),
        "job_get_context" => jobs_tools::handle_job_get_context(state, principal, id, args),
        "jobs_create" => jobs_tools::handle_jobs_create(state, principal, id, args, now),
        "jobs_update" => jobs_tools::handle_jobs_update(state, principal, id, args, now),
        "jobs_archive" => jobs_tools::handle_jobs_archive(state, principal, id, args, now),
        "jobs_assign" => jobs_tools::handle_jobs_assign(state, principal, id, args, now),
        "jobs_claim" => jobs_tools::handle_jobs_claim(state, principal, id, args, now),
        "jobs_heartbeat" => jobs_tools::handle_jobs_heartbeat(state, principal, id, args, now),
        "jobs_advance" => jobs_tools::handle_jobs_advance(state, principal, id, args, now),
        "jobs_rework" => jobs_tools::handle_jobs_rework(state, principal, id, args, now),
        "artifacts_attach" => governance_tools::handle_artifacts_attach(state, principal, id, args, now),
        "artifacts_read" => governance_tools::handle_artifacts_read(state, principal, id, args),
        "gate_verify_record" => governance_tools::handle_gate_verify_record(state, principal, id, args, now),
        "feedback_submit" => governance_tools::handle_feedback_submit(state, principal, id, args, now),
        "philosophy_propose" => governance_tools::handle_philosophy_propose(state, principal, id, args, now),
        "spec_propose" => governance_tools::handle_spec_propose(state, principal, id, args, now),
        "templates_list" => governance_tools::handle_templates_list(state, principal, id),
        "template_propose" => governance_tools::handle_template_propose(state, principal, id, args, now),
        other => rpc_err(id, -32601, format!("unknown tool: {other}")),
    }
}
