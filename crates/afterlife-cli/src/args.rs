use std::collections::{BTreeMap, HashSet};

use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    toolmap::{CONCEPT, FlagKind, FlagTarget, TOOLMAP, ToolSpec},
    transport::{CliError, get_json, mcp_call_tool, mcp_rpc},
};

impl ToolSpec {
    pub fn format_cmd(&self) -> String {
        let mut parts: Vec<String> = vec!["afterlife".to_string()];
        for p in self.path {
            parts.push((*p).to_string());
        }
        for pos in self.positionals {
            parts.push(format!("<{}>", pos.placeholder));
        }
        for fl in self.flags {
            let suffix = if fl.repeatable { "..." } else { "" };
            let token = if fl.kind == FlagKind::Bool {
                fl.flag.to_string()
            } else {
                format!("{} {}{}", fl.flag, fl.kind.label(), suffix)
            };
            if fl.required {
                parts.push(token);
            } else {
                parts.push(format!("[{token}]"));
            }
        }
        parts.join(" ")
    }
}

pub fn build_toolmap() -> Value {
    let health = get_json("/v1/health").ok();
    let live_tools = mcp_rpc("tools/list", json!({}))
        .ok()
        .and_then(|v| v.get("tools").and_then(Value::as_array).cloned());
    let live_districts = mcp_call_tool("districts_list", json!({}))
        .ok()
        .and_then(|v| v.get("districts").and_then(Value::as_array).cloned());

    let mut live_tool_map: BTreeMap<String, Value> = BTreeMap::new();
    if let Some(ref tools) = live_tools {
        for t in tools {
            if let Some(name) = t.get("name").and_then(Value::as_str) {
                live_tool_map.insert(name.to_string(), t.clone());
            }
        }
    }

    let mut mapped_tools: HashSet<String> = HashSet::new();
    let mut entries = Vec::with_capacity(TOOLMAP.len());
    for spec in TOOLMAP {
        let mut entry = json!({
            "cmd": spec.format_cmd(),
            "mcp_tool": spec.mcp_tool,
            "mutates": spec.mutates,
        });
        if let Some(tool_name) = spec.mcp_tool {
            mapped_tools.insert(tool_name.to_string());
            if let Some(schema) = live_tool_map.get(tool_name) {
                entry["schema"] = schema["inputSchema"].clone();
            }
        }
        entries.push(entry);
    }

    let unmapped_server_tools: Vec<Value> = live_tool_map
        .iter()
        .filter(|(name, _)| !mapped_tools.contains(*name))
        .map(|(name, schema)| {
            json!({
                "mcp_tool": name,
                "cmd": format!("afterlife mcp call {name} --args-json JSON"),
                "schema": schema["inputSchema"],
            })
        })
        .collect();

    let districts_compact: Vec<Value> = live_districts
        .unwrap_or_default()
        .into_iter()
        .map(|d| {
            json!({
                "id": d["id"],
                "name": d["name"],
                "active_philosophy_version": d["active_philosophy_version"],
                "active_spec_version": d["active_spec_version"],
                "stage_count": d["stage_count"],
            })
        })
        .collect();

    json!({
        "concept": CONCEPT,
        "live": health.is_some(),
        "human_http_policy": health
            .as_ref()
            .and_then(|h| h.get("human_http_policy"))
            .cloned()
            .unwrap_or(Value::Null),
        "limits": health
            .as_ref()
            .and_then(|h| h.get("policy_limits"))
            .cloned()
            .unwrap_or(Value::Null),
        "districts": districts_compact,
        "toolmap": entries,
        "unmapped_server_tools": unmapped_server_tools,
    })
}

fn parse_flag_value(kind: FlagKind, flag: &str, raw: &str) -> Result<Value, CliError> {
    match kind {
        FlagKind::String => Ok(Value::String(raw.to_string())),
        FlagKind::U64 => raw
            .parse::<u64>()
            .map(|n| json!(n))
            .map_err(|_| CliError::usage(format!("{flag} expects U64, got `{raw}`"))),
        FlagKind::F32 => raw
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|n| json!(n))
            .ok_or_else(|| CliError::usage(format!("{flag} expects F32, got `{raw}`"))),
        FlagKind::Bool => Ok(Value::Bool(true)),
        FlagKind::Json => serde_json::from_str::<Value>(raw)
            .map_err(|e| CliError::usage(format!("{flag} expects JSON: {e}"))),
        FlagKind::CheckPair => {
            let (label, b) = raw
                .rsplit_once('=')
                .ok_or_else(|| CliError::usage(format!("{flag} expects LABEL=true|false")))?;
            let passed = match b {
                "true" => true,
                "false" => false,
                _ => {
                    return Err(CliError::usage(format!(
                        "{flag} expects true|false, got `{b}`"
                    )));
                }
            };
            Ok(json!([label, passed]))
        }
        FlagKind::FieldPair => {
            let (k, v) = raw
                .split_once('=')
                .ok_or_else(|| CliError::usage(format!("{flag} expects KEY=VALUE")))?;
            if k.trim().is_empty() {
                return Err(CliError::usage(format!("{flag} key must not be empty")));
            }
            Ok(json!([k, v]))
        }
    }
}

pub fn parse_spec_args(
    spec: &ToolSpec,
    rest: &[String],
) -> Result<(Map<String, Value>, Map<String, Value>), CliError> {
    if rest.len() < spec.positionals.len() {
        return Err(CliError::usage(format!("usage: {}", spec.format_cmd())));
    }

    let mut out = Map::new();
    let mut local = Map::new();
    for (idx, pos) in spec.positionals.iter().enumerate() {
        out.insert(pos.mcp_key.to_string(), json!(rest[idx]));
    }

    let flag_args = &rest[spec.positionals.len()..];
    let mut i = 0usize;
    while i < flag_args.len() {
        let token = flag_args[i].as_str();
        let Some(fspec) = spec.flags.iter().find(|f| f.flag == token) else {
            return Err(CliError::usage(format!(
                "unknown flag `{token}` (usage: {})",
                spec.format_cmd()
            )));
        };
        let (dest, key) = match fspec.target {
            FlagTarget::McpKey(k) => (&mut out, k),
            FlagTarget::LocalKey(k) => (&mut local, k),
        };
        if fspec.kind == FlagKind::Bool {
            dest.insert(key.to_string(), Value::Bool(true));
            i += 1;
            continue;
        }
        let raw = flag_args
            .get(i + 1)
            .ok_or_else(|| CliError::usage(format!("missing value for {token}")))?;
        let val = parse_flag_value(fspec.kind, token, raw)?;
        if fspec.kind == FlagKind::FieldPair {
            let pair = val.as_array().unwrap();
            let k = pair[0].as_str().unwrap().to_string();
            let v = pair[1].as_str().unwrap().to_string();
            let map = dest
                .entry(key.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            let Some(obj) = map.as_object_mut() else {
                return Err(CliError::usage(format!(
                    "{token} requires `{key}` to be a JSON object"
                )));
            };
            obj.insert(k, Value::String(v));
        } else if fspec.repeatable {
            dest.entry(key.to_string())
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .unwrap()
                .push(val);
        } else {
            dest.insert(key.to_string(), val);
        }
        i += 2;
    }

    for fspec in spec.flags {
        let present = match fspec.target {
            FlagTarget::McpKey(k) => out.contains_key(k),
            FlagTarget::LocalKey(k) => local.contains_key(k),
        };
        if fspec.required && !present {
            return Err(CliError::usage(format!(
                "missing required flag {} (usage: {})",
                fspec.flag,
                spec.format_cmd()
            )));
        }
    }

    if let Some(default_runner) = spec.default_runner {
        out.entry("runner_label".to_string())
            .or_insert_with(|| json!(default_runner));
    }

    if spec.mutates && spec.mcp_tool.is_some() {
        out.entry("command_id".to_string())
            .or_insert_with(|| json!(Uuid::now_v7().to_string()));
    }

    Ok((out, local))
}

pub fn find_spec(args: &[String]) -> Option<(&'static ToolSpec, usize)> {
    let mut best: Option<(&'static ToolSpec, usize)> = None;
    for spec in TOOLMAP {
        let n = spec.path.len();
        if args.len() >= n
            && spec.path.iter().enumerate().all(|(i, p)| args[i] == *p)
            && best.is_none_or(|(_, prev_n)| n > prev_n)
        {
            best = Some((spec, n));
        }
    }
    best
}
