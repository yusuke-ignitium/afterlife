mod args;
mod backdrop;
mod events;
mod events_runner;
mod toolmap;
mod transport;

use std::{env, process::ExitCode};

use serde_json::{Value, json};

use crate::{
    args::{build_toolmap, find_spec, parse_spec_args},
    backdrop::run_backdrop_command,
    events_runner::{run_wait_command, run_watch_command},
    toolmap::{CONCEPT, TOOLMAP},
    transport::{CliError, get_json, mcp_call_tool, mcp_rpc},
};

enum CliOutput {
    Silent,
    Json(Value),
    JsonWithExit(Value, u8),
}

fn print_usage() {
    for line in CONCEPT {
        eprintln!("{line}");
    }
    eprintln!();
    for spec in TOOLMAP {
        eprintln!("  {}", spec.format_cmd());
    }
    eprintln!("\nENV: AFTERLIFE_URL (default http://127.0.0.1:4319), AFTERLIFE_TOKEN");
}

fn run(args: &[String]) -> Result<CliOutput, CliError> {
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        print_usage();
        return Ok(CliOutput::Silent);
    }

    if args[0] == "guide" {
        return Ok(CliOutput::Json(build_toolmap()));
    }

    let Some((spec, consumed)) = find_spec(args) else {
        return Err(CliError::usage(format!(
            "unknown command `{}` (run `afterlife toolmap`)",
            args[0]
        )));
    };
    let rest = &args[consumed..];

    if let Some(tool_name) = spec.mcp_tool {
        let (mut payload, local_args) = parse_spec_args(spec, rest)?;
        if let Some(hook) = spec.payload_hook {
            hook(&mut payload, &local_args)?;
        }
        return mcp_call_tool(tool_name, Value::Object(payload)).map(CliOutput::Json);
    }

    match spec.path {
        ["toolmap"] => Ok(CliOutput::Json(build_toolmap())),
        ["health"] => get_json("/v1/health").map(CliOutput::Json),
        ["tools"] => mcp_rpc("tools/list", json!({})).map(CliOutput::Json),
        ["wait"] => {
            let (payload, _) = parse_spec_args(spec, rest)?;
            let (val, code) = run_wait_command(&payload)?;
            Ok(CliOutput::JsonWithExit(val, code))
        }
        ["watch"] => {
            let (payload, _) = parse_spec_args(spec, rest)?;
            run_watch_command(&payload)?;
            Ok(CliOutput::Silent)
        }
        ["backdrop", sub] => {
            let (payload, _) = parse_spec_args(spec, rest)?;
            run_backdrop_command(sub, &payload).map(CliOutput::Json)
        }
        ["mcp", "call"] => {
            let (mut payload, _) = parse_spec_args(spec, rest)?;
            let tool_name = payload
                .remove("tool_name")
                .and_then(|v| v.as_str().map(str::to_string))
                .ok_or_else(|| CliError::usage("missing <tool_name>"))?;
            let arguments = payload.remove("arguments").unwrap_or_else(|| json!({}));
            mcp_call_tool(&tool_name, arguments).map(CliOutput::Json)
        }
        _ => Err(CliError::usage("unhandled command")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(CliOutput::Silent) => ExitCode::SUCCESS,
        Ok(CliOutput::Json(val)) => {
            println!("{}", serde_json::to_string_pretty(&val).unwrap());
            ExitCode::SUCCESS
        }
        Ok(CliOutput::JsonWithExit(val, code)) => {
            println!("{}", serde_json::to_string_pretty(&val).unwrap());
            ExitCode::from(code)
        }
        Err(err) => {
            eprintln!("{}", serde_json::to_string_pretty(&err.payload).unwrap());
            ExitCode::from(err.exit_code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifacts_attach_local_file_flag_runs_via_payload_hook_without_leaking_file_key() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("afterlife_cli_hook_{nanos}.json"));
        std::fs::write(&path, r#"{"ok":true}"#).unwrap();

        let args: Vec<String> = [
            "artifacts",
            "attach",
            "JOB-001",
            "--kind",
            "snapshot",
            "--label",
            "snap.json",
            "--file",
            path.to_str().unwrap(),
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let (spec, consumed) = find_spec(&args).unwrap();
        let (mut payload, local_args) = parse_spec_args(spec, &args[consumed..]).unwrap();
        assert!(!payload.contains_key("file"));
        assert!(local_args.contains_key("file"));

        let hook = spec.payload_hook.expect("declarative payload_hook");
        hook(&mut payload, &local_args).unwrap();
        assert!(!payload.contains_key("file"));
        assert_eq!(payload["content"], json!(r#"{"ok":true}"#));

        let _ = std::fs::remove_file(&path);
    }
}
