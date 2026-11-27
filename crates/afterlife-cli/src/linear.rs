use std::process::Command;

use serde_json::{Map, Value, json};

use crate::transport::{CliError, get_json, post_json};

fn open_in_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status();
    #[cfg(not(target_os = "macos"))]
    let status = Command::new("xdg-open").arg(url).status();
    match status {
        Ok(st) if st.success() => Ok(()),
        Ok(st) => Err(format!("browser open exited with {st}")),
        Err(e) => Err(format!("failed launching browser: {e}")),
    }
}

pub fn run_linear_command(sub: &str, payload: &Map<String, Value>) -> Result<Value, CliError> {
    match sub {
        "status" => get_json("/v1/linear/status"),
        "connect" => {
            let client_id = payload
                .get("client_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| CliError::usage("--client-id is required"))?;
            let client_secret = payload.get("client_secret").and_then(Value::as_str);
            let default_district_id = payload.get("default_district").and_then(Value::as_str);
            let open_browser = payload
                .get("open")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let mut resp = post_json(
                "/v1/linear/oauth/start",
                &json!({
                    "client_id": client_id,
                    "client_secret": client_secret,
                    "default_district_id": default_district_id,
                }),
            )?;

            if open_browser
                && let Some(url) = resp.get("authorize_url").and_then(Value::as_str)
            {
                match open_in_browser(url) {
                    Ok(()) => {
                        resp["browser_opened"] = Value::Bool(true);
                    }
                    Err(err) => {
                        resp["browser_opened"] = Value::Bool(false);
                        resp["browser_open_error"] = Value::String(err);
                    }
                }
            }
            Ok(resp)
        }
        "sync" => post_json("/v1/linear/sync", &json!({})),
        "disconnect" => post_json("/v1/linear/disconnect", &json!({})),
        other => Err(CliError::usage(format!(
            "unknown linear subcommand `{other}`"
        ))),
    }
}
