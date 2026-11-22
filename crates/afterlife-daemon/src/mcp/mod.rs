mod dispatch;
mod governance_tools;
mod jobs_tools;
mod schemas;

use std::{sync::Arc, time::Instant};

use afterlife_core::{CommandId, EngineCommand, FencingToken};
use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{AppDaemonState, auth::McpPrincipal, rest::dispatch_command};

pub use schemas::mcp_tool_definitions;

#[derive(Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

pub(super) fn rpc_ok(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

pub(super) fn rpc_err(id: Value, code: i32, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

pub(super) fn tool_text(val: Value, is_error: bool) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&val).unwrap_or_default()
        }],
        "isError": is_error
    })
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
            let res = dispatch::handle_tool_call(&state, &principal, id, name, args);
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
    if let Some(raw) = text
        && let Ok(parsed) = serde_json::from_str::<Value>(raw)
    {
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
    ("error", None)
}

pub(super) fn invalid_arg(id: Value, field: &str, reason: &str) -> Value {
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

pub(super) fn parse_optional_u64(args: &Value, field: &str) -> Result<Option<u64>, String> {
    match args.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("expected non-negative integer for '{field}', got {v}")),
    }
}

pub(super) fn parse_required_u64(args: &Value, field: &str) -> Result<u64, String> {
    match args.get(field) {
        None | Some(Value::Null) => Err(format!("missing required integer field '{field}'")),
        Some(v) => v
            .as_u64()
            .ok_or_else(|| format!("expected non-negative integer for '{field}', got {v}")),
    }
}

pub(super) fn parse_optional_fencing_token(args: &Value) -> Result<Option<FencingToken>, String> {
    parse_optional_u64(args, "fencing_token").map(|opt| opt.map(FencingToken::new))
}

pub(super) fn cmd_id_from(args: &Value) -> Result<CommandId, String> {
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

pub(super) fn run_mcp_cmd(
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
