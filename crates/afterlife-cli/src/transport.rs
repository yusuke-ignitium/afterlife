use std::{env, io::Read, time::Duration};

use serde_json::{Value, json};

#[derive(Debug)]
pub struct CliError {
    pub exit_code: u8,
    pub payload: Value,
}

impl CliError {
    pub fn usage(msg: impl Into<String>) -> Self {
        Self {
            exit_code: 2,
            payload: json!({ "error": "invalid_usage", "message": msg.into() }),
        }
    }

    pub fn transport(msg: impl Into<String>) -> Self {
        Self {
            exit_code: 1,
            payload: json!({ "error": "transport_error", "message": msg.into() }),
        }
    }

    pub fn remote(payload: Value) -> Self {
        Self {
            exit_code: 1,
            payload,
        }
    }

    pub fn local(msg: impl Into<String>) -> Self {
        Self {
            exit_code: 1,
            payload: json!({ "error": "backdrop_error", "message": msg.into() }),
        }
    }
}

fn base_url() -> String {
    env::var("AFTERLIFE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:4319".into())
}

fn bearer_token() -> Option<String> {
    env::var("AFTERLIFE_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn get_json(path: &str) -> Result<Value, CliError> {
    let url = format!("{}{}", base_url().trim_end_matches('/'), path);
    let mut req = ureq::get(&url);
    if let Some(tok) = bearer_token() {
        req = req.set("Authorization", &format!("Bearer {tok}"));
    }
    match req.call() {
        Ok(resp) => {
            let body = resp
                .into_string()
                .map_err(|e| CliError::transport(format!("failed reading {url}: {e}")))?;
            serde_json::from_str::<Value>(&body)
                .map_err(|e| CliError::transport(format!("invalid JSON from {url}: {e}")))
        }
        Err(ureq::Error::Status(status, resp)) => {
            let raw = resp.into_string().unwrap_or_default();
            let parsed = serde_json::from_str::<Value>(&raw)
                .unwrap_or_else(|_| json!({ "status": status, "error": raw }));
            Err(CliError::remote(parsed))
        }
        Err(e) => Err(CliError::transport(format!("GET {url} failed: {e}"))),
    }
}

pub fn open_sse_reader(
    since_seq: u64,
    read_timeout: Option<Duration>,
) -> Result<Box<dyn Read + Send + Sync>, CliError> {
    let url = format!(
        "{}/v1/events?since_seq={since_seq}",
        base_url().trim_end_matches('/')
    );
    let mut builder = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5));
    if let Some(rt) = read_timeout {
        builder = builder.timeout_read(rt.max(Duration::from_millis(50)));
    }
    let agent = builder.build();
    let mut req = agent.get(&url).set("Accept", "text/event-stream");
    if let Some(tok) = bearer_token() {
        req = req.set("Authorization", &format!("Bearer {tok}"));
    }
    match req.call() {
        Ok(resp) => Ok(resp.into_reader()),
        Err(ureq::Error::Status(status, resp)) => {
            let raw = resp.into_string().unwrap_or_default();
            let parsed = serde_json::from_str::<Value>(&raw)
                .unwrap_or_else(|_| json!({ "status": status, "error": raw }));
            Err(CliError::remote(parsed))
        }
        Err(e) => Err(CliError::transport(format!("SSE {url} failed: {e}"))),
    }
}

pub fn mcp_rpc(method: &str, params: Value) -> Result<Value, CliError> {
    let url = format!("{}/mcp", base_url().trim_end_matches('/'));
    let mut req = ureq::post(&url).set("Content-Type", "application/json");
    if let Some(tok) = bearer_token() {
        req = req.set("Authorization", &format!("Bearer {tok}"));
    }
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    })
    .to_string();

    match req.send_string(&body) {
        Ok(resp) => {
            let raw = resp
                .into_string()
                .map_err(|e| CliError::transport(format!("failed reading {url}: {e}")))?;
            let rpc: Value = serde_json::from_str(&raw)
                .map_err(|e| CliError::transport(format!("invalid JSON-RPC from {url}: {e}")))?;
            if let Some(err) = rpc.get("error") {
                return Err(CliError::remote(err.clone()));
            }
            Ok(rpc.get("result").cloned().unwrap_or(Value::Null))
        }
        Err(ureq::Error::Status(status, resp)) => {
            let raw = resp.into_string().unwrap_or_default();
            let parsed = serde_json::from_str::<Value>(&raw)
                .unwrap_or_else(|_| json!({ "status": status, "error": raw }));
            Err(CliError::remote(parsed))
        }
        Err(e) => Err(CliError::transport(format!("POST {url} failed: {e}"))),
    }
}

pub fn mcp_call_tool(name: &str, arguments: Value) -> Result<Value, CliError> {
    let result = mcp_rpc(
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    )?;
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str);

    let decoded = match text {
        Some(t) => serde_json::from_str::<Value>(t).unwrap_or_else(|_| json!({ "text": t })),
        None => result,
    };
    if is_error {
        Err(CliError::remote(decoded))
    } else {
        Ok(decoded)
    }
}
