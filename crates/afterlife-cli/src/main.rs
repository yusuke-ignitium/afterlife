use std::{
    collections::{BTreeMap, HashSet},
    env,
    fs,
    path::Path,
    process::ExitCode,
};

use afterlife_core::MAX_ARTIFACT_BLOB_BYTES;
use afterlife_protocol::{BackdropStore, BackdropTuningPatch};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde_json::{Map, Value, json};
use uuid::Uuid;

const CONCEPT: &[&str] = &[
    "Afterlife separates AI execution (Actor::Agent) from human governance (Actor::Human).",
    "Jobs move through Districts governed by versioned Philosophy, ProcedureSpec, and GateRubric.",
    "Agents acquire stage leases (fencing_token), record passing GateVerifications, and stop at Human ApprovalGates.",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum FlagKind {
    String,
    U64,
    F32,
    Bool,
    Json,
    CheckPair,
    FieldPair,
}

impl FlagKind {
    const fn label(self) -> &'static str {
        match self {
            Self::String => "STR",
            Self::U64 => "U64",
            Self::F32 => "F32",
            Self::Bool => "",
            Self::Json => "JSON",
            Self::CheckPair => "LABEL=BOOL",
            Self::FieldPair => "KEY=VALUE",
        }
    }
}

struct FlagSpec {
    flag: &'static str,
    mcp_key: &'static str,
    kind: FlagKind,
    required: bool,
    repeatable: bool,
}

struct PositionalSpec {
    placeholder: &'static str,
    mcp_key: &'static str,
}

struct ToolSpec {
    path: &'static [&'static str],
    mcp_tool: Option<&'static str>,
    mutates: bool,
    positionals: &'static [PositionalSpec],
    flags: &'static [FlagSpec],
    default_runner: Option<&'static str>,
}

const FLAG_COMMAND_ID: FlagSpec = FlagSpec {
    flag: "--command-id",
    mcp_key: "command_id",
    kind: FlagKind::String,
    required: false,
    repeatable: false,
};

const FLAG_FENCING_OPT: FlagSpec = FlagSpec {
    flag: "--fencing-token",
    mcp_key: "fencing_token",
    kind: FlagKind::U64,
    required: false,
    repeatable: false,
};

const FLAG_FENCING_REQ: FlagSpec = FlagSpec {
    flag: "--fencing-token",
    mcp_key: "fencing_token",
    kind: FlagKind::U64,
    required: true,
    repeatable: false,
};

const FLAG_EXPECTED_REV: FlagSpec = FlagSpec {
    flag: "--expected-revision",
    mcp_key: "expected_revision",
    kind: FlagKind::U64,
    required: false,
    repeatable: false,
};

const TOOLMAP: &[ToolSpec] = &[
    ToolSpec {
        path: &["toolmap"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
    },
    ToolSpec {
        path: &["health"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
    },
    ToolSpec {
        path: &["tools"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
    },
    ToolSpec {
        path: &["districts"],
        mcp_tool: Some("districts_list"),
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
    },
    ToolSpec {
        path: &["jobs", "list"],
        mcp_tool: Some("jobs_list"),
        mutates: false,
        positionals: &[],
        flags: &[
            FlagSpec {
                flag: "--district",
                mcp_key: "district_id",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--limit",
                mcp_key: "limit",
                kind: FlagKind::U64,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--offset",
                mcp_key: "offset",
                kind: FlagKind::U64,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--include-archived",
                mcp_key: "include_archived",
                kind: FlagKind::Bool,
                required: false,
                repeatable: false,
            },
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["jobs", "context"],
        mcp_tool: Some("job_get_context"),
        mutates: false,
        positionals: &[PositionalSpec {
            placeholder: "job_id",
            mcp_key: "job_id",
        }],
        flags: &[],
        default_runner: None,
    },
    ToolSpec {
        path: &["jobs", "create"],
        mcp_tool: Some("jobs_create"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "title",
            mcp_key: "title",
        }],
        flags: &[
            FlagSpec {
                flag: "--input",
                mcp_key: "raw_input",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--district",
                mcp_key: "target_district_hint",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["jobs", "update"],
        mcp_tool: Some("jobs_update"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "job_id",
            mcp_key: "job_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--title",
                mcp_key: "title",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--input",
                mcp_key: "raw_input",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--summary",
                mcp_key: "spec_summary",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--district",
                mcp_key: "target_district_hint",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FLAG_EXPECTED_REV,
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["jobs", "archive"],
        mcp_tool: Some("jobs_archive"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "job_id",
            mcp_key: "job_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--reason",
                mcp_key: "reason",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FLAG_EXPECTED_REV,
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["jobs", "assign"],
        mcp_tool: Some("jobs_assign"),
        mutates: true,
        positionals: &[
            PositionalSpec {
                placeholder: "job_id",
                mcp_key: "job_id",
            },
            PositionalSpec {
                placeholder: "district_id",
                mcp_key: "district_id",
            },
        ],
        flags: &[FLAG_EXPECTED_REV, FLAG_COMMAND_ID],
        default_runner: None,
    },
    ToolSpec {
        path: &["jobs", "claim"],
        mcp_tool: Some("jobs_claim"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "job_id",
            mcp_key: "job_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--runner",
                mcp_key: "runner_label",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--ttl-ms",
                mcp_key: "ttl_ms",
                kind: FlagKind::U64,
                required: false,
                repeatable: false,
            },
            FLAG_EXPECTED_REV,
            FLAG_COMMAND_ID,
        ],
        default_runner: Some("afterlife-cli"),
    },
    ToolSpec {
        path: &["jobs", "heartbeat"],
        mcp_tool: Some("jobs_heartbeat"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "job_id",
            mcp_key: "job_id",
        }],
        flags: &[
            FLAG_FENCING_REQ,
            FlagSpec {
                flag: "--ttl-ms",
                mcp_key: "ttl_ms",
                kind: FlagKind::U64,
                required: false,
                repeatable: false,
            },
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["jobs", "advance"],
        mcp_tool: Some("jobs_advance"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "job_id",
            mcp_key: "job_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--field",
                mcp_key: "fields",
                kind: FlagKind::FieldPair,
                required: false,
                repeatable: true,
            },
            FlagSpec {
                flag: "--fields-json",
                mcp_key: "fields",
                kind: FlagKind::Json,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--artifact-id",
                mcp_key: "artifact_ids",
                kind: FlagKind::String,
                required: false,
                repeatable: true,
            },
            FLAG_FENCING_OPT,
            FLAG_EXPECTED_REV,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["jobs", "rework"],
        mcp_tool: Some("jobs_rework"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "job_id",
            mcp_key: "job_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--reason",
                mcp_key: "reason",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["artifacts", "attach"],
        mcp_tool: Some("artifacts_attach"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "job_id",
            mcp_key: "job_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--kind",
                mcp_key: "kind",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--label",
                mcp_key: "label",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--uri",
                mcp_key: "uri",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--file",
                mcp_key: "file",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--content",
                mcp_key: "content",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--sha256",
                mcp_key: "sha256",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["artifacts", "read"],
        mcp_tool: Some("artifacts_read"),
        mutates: false,
        positionals: &[
            PositionalSpec {
                placeholder: "job_id",
                mcp_key: "job_id",
            },
            PositionalSpec {
                placeholder: "artifact_id",
                mcp_key: "artifact_id",
            },
        ],
        flags: &[],
        default_runner: None,
    },
    ToolSpec {
        path: &["gate", "verify"],
        mcp_tool: Some("gate_verify_record"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "job_id",
            mcp_key: "job_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--verdict",
                mcp_key: "verdict",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--summary",
                mcp_key: "summary",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--check",
                mcp_key: "checklist_results",
                kind: FlagKind::CheckPair,
                required: false,
                repeatable: true,
            },
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["feedback", "submit"],
        mcp_tool: Some("feedback_submit"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "district_id",
            mcp_key: "district_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--kind",
                mcp_key: "kind",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--body",
                mcp_key: "body",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--job-id",
                mcp_key: "job_id",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--stage-id",
                mcp_key: "stage_id",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["philosophy", "propose"],
        mcp_tool: Some("philosophy_propose"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "district_id",
            mcp_key: "district_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--base-version",
                mcp_key: "base_version",
                kind: FlagKind::U64,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--summary",
                mcp_key: "summary",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--rationale",
                mcp_key: "rationale",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--principles-json",
                mcp_key: "principles",
                kind: FlagKind::Json,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--anti-pattern",
                mcp_key: "anti_patterns",
                kind: FlagKind::String,
                required: false,
                repeatable: true,
            },
            FlagSpec {
                flag: "--heuristic",
                mcp_key: "decision_heuristics",
                kind: FlagKind::String,
                required: false,
                repeatable: true,
            },
            FlagSpec {
                flag: "--feedback-id",
                mcp_key: "source_feedback_ids",
                kind: FlagKind::String,
                required: false,
                repeatable: true,
            },
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["spec", "propose"],
        mcp_tool: Some("spec_propose"),
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "district_id",
            mcp_key: "district_id",
        }],
        flags: &[
            FlagSpec {
                flag: "--base-version",
                mcp_key: "base_version",
                kind: FlagKind::U64,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--changelog",
                mcp_key: "changelog",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--rationale",
                mcp_key: "rationale",
                kind: FlagKind::String,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--stages-json",
                mcp_key: "stages",
                kind: FlagKind::Json,
                required: true,
                repeatable: false,
            },
            FlagSpec {
                flag: "--feedback-id",
                mcp_key: "source_feedback_ids",
                kind: FlagKind::String,
                required: false,
                repeatable: true,
            },
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["backdrop", "list"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
    },
    ToolSpec {
        path: &["backdrop", "add"],
        mcp_tool: None,
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "path",
            mcp_key: "path",
        }],
        flags: &[FlagSpec {
            flag: "--label",
            mcp_key: "label",
            kind: FlagKind::String,
            required: false,
            repeatable: false,
        }],
        default_runner: None,
    },
    ToolSpec {
        path: &["backdrop", "update"],
        mcp_tool: None,
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "id",
            mcp_key: "id",
        }],
        flags: &[
            FlagSpec {
                flag: "--path",
                mcp_key: "path",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--label",
                mcp_key: "label",
                kind: FlagKind::String,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--index",
                mcp_key: "index",
                kind: FlagKind::U64,
                required: false,
                repeatable: false,
            },
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["backdrop", "remove"],
        mcp_tool: None,
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "id",
            mcp_key: "id",
        }],
        flags: &[],
        default_runner: None,
    },
    ToolSpec {
        path: &["backdrop", "clear"],
        mcp_tool: None,
        mutates: true,
        positionals: &[],
        flags: &[],
        default_runner: None,
    },
    ToolSpec {
        path: &["backdrop", "tuning"],
        mcp_tool: None,
        mutates: true,
        positionals: &[],
        flags: &[
            FlagSpec {
                flag: "--enable-images",
                mcp_key: "enable_images",
                kind: FlagKind::Bool,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--disable-images",
                mcp_key: "disable_images",
                kind: FlagKind::Bool,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--enable-apple-music",
                mcp_key: "enable_apple_music",
                kind: FlagKind::Bool,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--disable-apple-music",
                mcp_key: "disable_apple_music",
                kind: FlagKind::Bool,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--enable-youtube-mirror",
                mcp_key: "enable_youtube_mirror",
                kind: FlagKind::Bool,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--disable-youtube-mirror",
                mcp_key: "disable_youtube_mirror",
                kind: FlagKind::Bool,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--tile-size",
                mcp_key: "tile_size_px",
                kind: FlagKind::F32,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--hold-secs",
                mcp_key: "image_hold_secs",
                kind: FlagKind::F32,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--image-brightness",
                mcp_key: "image_brightness",
                kind: FlagKind::F32,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--image-peak",
                mcp_key: "image_peak_luminance",
                kind: FlagKind::F32,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--image-saturation",
                mcp_key: "image_saturation",
                kind: FlagKind::F32,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--master-brightness",
                mcp_key: "master_brightness",
                kind: FlagKind::F32,
                required: false,
                repeatable: false,
            },
            FlagSpec {
                flag: "--reset",
                mcp_key: "reset",
                kind: FlagKind::Bool,
                required: false,
                repeatable: false,
            },
        ],
        default_runner: None,
    },
    ToolSpec {
        path: &["mcp", "call"],
        mcp_tool: None,
        mutates: true,
        positionals: &[PositionalSpec {
            placeholder: "tool_name",
            mcp_key: "tool_name",
        }],
        flags: &[FlagSpec {
            flag: "--args-json",
            mcp_key: "arguments",
            kind: FlagKind::Json,
            required: false,
            repeatable: false,
        }],
        default_runner: None,
    },
];

impl ToolSpec {
    fn format_cmd(&self) -> String {
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

struct CliError {
    exit_code: u8,
    payload: Value,
}

impl CliError {
    fn usage(msg: impl Into<String>) -> Self {
        Self {
            exit_code: 2,
            payload: json!({ "error": "invalid_usage", "message": msg.into() }),
        }
    }

    fn transport(msg: impl Into<String>) -> Self {
        Self {
            exit_code: 1,
            payload: json!({ "error": "transport_error", "message": msg.into() }),
        }
    }

    fn remote(payload: Value) -> Self {
        Self {
            exit_code: 1,
            payload,
        }
    }

    fn local(msg: impl Into<String>) -> Self {
        Self {
            exit_code: 1,
            payload: json!({ "error": "backdrop_error", "message": msg.into() }),
        }
    }
}

fn get_json(path: &str) -> Result<Value, CliError> {
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

fn mcp_rpc(method: &str, params: Value) -> Result<Value, CliError> {
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

fn mcp_call_tool(name: &str, arguments: Value) -> Result<Value, CliError> {
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

fn build_toolmap() -> Value {
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

fn parse_spec_args(spec: &ToolSpec, rest: &[String]) -> Result<Map<String, Value>, CliError> {
    if rest.len() < spec.positionals.len() {
        return Err(CliError::usage(format!("usage: {}", spec.format_cmd())));
    }

    let mut out = Map::new();
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
        if fspec.kind == FlagKind::Bool {
            out.insert(fspec.mcp_key.to_string(), Value::Bool(true));
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
            let map = out
                .entry(fspec.mcp_key.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            let Some(obj) = map.as_object_mut() else {
                return Err(CliError::usage(format!(
                    "{token} requires `{}` to be a JSON object",
                    fspec.mcp_key
                )));
            };
            obj.insert(k, Value::String(v));
        } else if fspec.repeatable {
            out.entry(fspec.mcp_key.to_string())
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .unwrap()
                .push(val);
        } else {
            out.insert(fspec.mcp_key.to_string(), val);
        }
        i += 2;
    }

    for fspec in spec.flags {
        if fspec.required && !out.contains_key(fspec.mcp_key) {
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

    Ok(out)
}

fn find_spec(args: &[String]) -> Option<(&'static ToolSpec, usize)> {
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

fn prepare_artifact_attach_payload(payload: &mut Map<String, Value>) -> Result<(), CliError> {
    let file_path = payload
        .remove("file")
        .and_then(|v| v.as_str().map(str::to_string));
    let has_uri = payload.contains_key("uri");
    let has_content = payload.contains_key("content");
    let count = usize::from(file_path.is_some()) + usize::from(has_uri) + usize::from(has_content);
    if count != 1 {
        return Err(CliError::usage(
            "artifacts attach requires exactly one of --uri, --file, or --content",
        ));
    }
    if let Some(path) = file_path {
        let bytes = fs::read(&path)
            .map_err(|e| CliError::usage(format!("failed reading --file `{path}`: {e}")))?;
        if bytes.len() > MAX_ARTIFACT_BLOB_BYTES {
            return Err(CliError::usage(format!(
                "--file `{path}` exceeds maximum blob size of {} bytes (got {} bytes)",
                MAX_ARTIFACT_BLOB_BYTES,
                bytes.len()
            )));
        }
        match String::from_utf8(bytes) {
            Ok(text) => {
                payload.insert("content".to_string(), Value::String(text));
            }
            Err(e) => {
                payload.insert(
                    "content_base64".to_string(),
                    Value::String(BASE64_STANDARD.encode(e.into_bytes())),
                );
            }
        }
    }
    Ok(())
}

fn run(args: &[String]) -> Result<Option<Value>, CliError> {
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        print_usage();
        return Ok(None);
    }

    if args[0] == "guide" {
        return Ok(Some(build_toolmap()));
    }

    let Some((spec, consumed)) = find_spec(args) else {
        return Err(CliError::usage(format!(
            "unknown command `{}` (run `afterlife toolmap`)",
            args[0]
        )));
    };
    let rest = &args[consumed..];

    if let Some(tool_name) = spec.mcp_tool {
        let mut payload = parse_spec_args(spec, rest)?;
        if tool_name == "artifacts_attach" {
            prepare_artifact_attach_payload(&mut payload)?;
        }
        return mcp_call_tool(tool_name, Value::Object(payload)).map(Some);
    }

    match spec.path {
        ["toolmap"] => Ok(Some(build_toolmap())),
        ["health"] => get_json("/v1/health").map(Some),
        ["tools"] => mcp_rpc("tools/list", json!({})).map(Some),
        ["backdrop", sub] => {
            let payload = parse_spec_args(spec, rest)?;
            run_backdrop_command(sub, &payload).map(Some)
        }
        ["mcp", "call"] => {
            let mut payload = parse_spec_args(spec, rest)?;
            let tool_name = payload
                .remove("tool_name")
                .and_then(|v| v.as_str().map(str::to_string))
                .ok_or_else(|| CliError::usage("missing <tool_name>"))?;
            let arguments = payload.remove("arguments").unwrap_or_else(|| json!({}));
            mcp_call_tool(&tool_name, arguments).map(Some)
        }
        _ => Err(CliError::usage("unhandled command")),
    }
}

fn run_backdrop_command(sub: &str, payload: &Map<String, Value>) -> Result<Value, CliError> {
    let store = BackdropStore::open_default();
    let config_path = store.config_path().to_string_lossy().into_owned();
    match sub {
        "list" => {
            let cfg = store.load().map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "count": cfg.images.len(),
                "images": cfg.images,
                "tuning": cfg.tuning,
            }))
        }
        "add" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| CliError::usage("missing <path>"))?;
            let label = payload.get("label").and_then(|v| v.as_str());
            let (entry, cfg) = store
                .add_image(Path::new(path), label)
                .map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "added": entry,
                "count": cfg.images.len(),
                "images": cfg.images,
            }))
        }
        "update" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| CliError::usage("missing <id>"))?;
            let new_path = payload.get("path").and_then(|v| v.as_str()).map(Path::new);
            let new_label = payload.get("label").and_then(|v| v.as_str());
            let new_index = payload
                .get("index")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let (entry, cfg) = store
                .update_image(id, new_path, new_label, new_index)
                .map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "updated": entry,
                "count": cfg.images.len(),
                "images": cfg.images,
            }))
        }
        "remove" => {
            let id = payload
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| CliError::usage("missing <id>"))?;
            let (removed, cfg) = store.remove_image(id).map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "removed": removed,
                "count": cfg.images.len(),
                "images": cfg.images,
            }))
        }
        "clear" => {
            let (removed_count, cfg) = store.clear_images().map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "removed_count": removed_count,
                "count": cfg.images.len(),
                "images": cfg.images,
            }))
        }
        "tuning" => {
            let enable_images = payload
                .get("enable_images")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let disable_images = payload
                .get("disable_images")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if enable_images && disable_images {
                return Err(CliError::usage(
                    "cannot pass both --enable-images and --disable-images",
                ));
            }
            let image_enabled = if enable_images {
                Some(true)
            } else if disable_images {
                Some(false)
            } else {
                None
            };
            let enable_apple_music = payload
                .get("enable_apple_music")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let disable_apple_music = payload
                .get("disable_apple_music")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if enable_apple_music && disable_apple_music {
                return Err(CliError::usage(
                    "cannot pass both --enable-apple-music and --disable-apple-music",
                ));
            }
            let apple_music_artwork_enabled = if enable_apple_music {
                Some(true)
            } else if disable_apple_music {
                Some(false)
            } else {
                None
            };
            let enable_youtube_mirror = payload
                .get("enable_youtube_mirror")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let disable_youtube_mirror = payload
                .get("disable_youtube_mirror")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if enable_youtube_mirror && disable_youtube_mirror {
                return Err(CliError::usage(
                    "cannot pass both --enable-youtube-mirror and --disable-youtube-mirror",
                ));
            }
            let youtube_mirror_enabled = if enable_youtube_mirror {
                Some(true)
            } else if disable_youtube_mirror {
                Some(false)
            } else {
                None
            };
            let tile_size_px = payload
                .get("tile_size_px")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let image_hold_secs = payload
                .get("image_hold_secs")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let image_brightness = payload
                .get("image_brightness")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let image_peak = payload
                .get("image_peak_luminance")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let image_saturation = payload
                .get("image_saturation")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let master_brightness = payload
                .get("master_brightness")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let reset = payload
                .get("reset")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let cfg = store
                .update_tuning(BackdropTuningPatch {
                    image_enabled,
                    apple_music_artwork_enabled,
                    youtube_mirror_enabled,
                    tile_size_px,
                    image_hold_secs,
                    image_brightness,
                    image_peak_luminance: image_peak,
                    image_saturation,
                    master_brightness,
                    reset,
                })
                .map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "tuning": cfg.tuning,
            }))
        }
        _ => Err(CliError::usage(format!(
            "unknown backdrop subcommand `{sub}`"
        ))),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(val)) => {
            println!("{}", serde_json::to_string_pretty(&val).unwrap());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{}", serde_json::to_string_pretty(&err.payload).unwrap());
            ExitCode::from(err.exit_code)
        }
    }
}
