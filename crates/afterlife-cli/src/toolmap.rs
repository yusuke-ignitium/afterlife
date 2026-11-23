use std::fs;

use afterlife_core::MAX_ARTIFACT_BLOB_BYTES;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde_json::{Map, Value};

use crate::transport::CliError;


pub const CONCEPT: &[&str] = &[
    "Afterlife separates AI execution (Actor::Agent) from human governance (Actor::Human).",
    "Jobs move through Districts governed by versioned Philosophy, ProcedureSpec, and GateRubric.",
    "Agents acquire stage leases (fencing_token), record passing GateVerifications, and stop at Human ApprovalGates.",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FlagKind {
    String,
    U64,
    F32,
    Bool,
    Json,
    CheckPair,
    FieldPair,
}

impl FlagKind {
    pub const fn label(self) -> &'static str {
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FlagTarget {
    McpKey(&'static str),
    LocalKey(&'static str),
}

pub struct FlagSpec {
    pub flag: &'static str,
    pub target: FlagTarget,
    pub kind: FlagKind,
    pub required: bool,
    pub repeatable: bool,
}

impl FlagSpec {
    const fn opt(flag: &'static str, key: &'static str, kind: FlagKind) -> Self {
        Self { flag, target: FlagTarget::McpKey(key), kind, required: false, repeatable: false }
    }
    const fn req(flag: &'static str, key: &'static str, kind: FlagKind) -> Self {
        Self { flag, target: FlagTarget::McpKey(key), kind, required: true, repeatable: false }
    }
    const fn rep(flag: &'static str, key: &'static str, kind: FlagKind) -> Self {
        Self { flag, target: FlagTarget::McpKey(key), kind, required: false, repeatable: true }
    }
    const fn local_opt(flag: &'static str, key: &'static str, kind: FlagKind) -> Self {
        Self { flag, target: FlagTarget::LocalKey(key), kind, required: false, repeatable: false }
    }
}

pub struct PositionalSpec {
    pub placeholder: &'static str,
    pub mcp_key: &'static str,
}

impl PositionalSpec {
    const fn new(placeholder: &'static str, mcp_key: &'static str) -> Self {
        Self { placeholder, mcp_key }
    }
}

pub type PayloadHook = fn(&mut Map<String, Value>, &Map<String, Value>) -> Result<(), CliError>;

pub struct ToolSpec {
    pub path: &'static [&'static str],
    pub mcp_tool: Option<&'static str>,
    pub mutates: bool,
    pub positionals: &'static [PositionalSpec],
    pub flags: &'static [FlagSpec],
    pub default_runner: Option<&'static str>,
    pub payload_hook: Option<PayloadHook>,
}

const FLAG_COMMAND_ID: FlagSpec = FlagSpec::opt("--command-id", "command_id", FlagKind::String);
const FLAG_FENCING_OPT: FlagSpec = FlagSpec::opt("--fencing-token", "fencing_token", FlagKind::U64);
const FLAG_FENCING_REQ: FlagSpec = FlagSpec::req("--fencing-token", "fencing_token", FlagKind::U64);
const FLAG_EXPECTED_REV: FlagSpec =
    FlagSpec::opt("--expected-revision", "expected_revision", FlagKind::U64);

fn prepare_artifact_attach_payload(
    payload: &mut Map<String, Value>,
    local_args: &Map<String, Value>,
) -> Result<(), CliError> {
    let file_path = local_args.get("file").and_then(Value::as_str);
    let has_uri = payload.contains_key("uri");
    let has_content = payload.contains_key("content");
    let count = usize::from(file_path.is_some()) + usize::from(has_uri) + usize::from(has_content);
    if count != 1 {
        return Err(CliError::usage(
            "artifacts attach requires exactly one of --uri, --file, or --content",
        ));
    }
    if let Some(path) = file_path {
        let bytes = fs::read(path)
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

pub const TOOLMAP: &[ToolSpec] = &[
    ToolSpec {
        path: &["toolmap"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["health"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["tools"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["wait"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[
            FlagSpec::opt("--job", "job_id", FlagKind::String),
            FlagSpec::opt("--district", "district_id", FlagKind::String),
            FlagSpec::opt("--until", "until", FlagKind::String),
            FlagSpec::opt("--since-seq", "since_seq", FlagKind::U64),
            FlagSpec::opt("--timeout", "timeout", FlagKind::U64),
            FlagSpec::opt("--timeout-ms", "timeout_ms", FlagKind::U64),
            FlagSpec::opt("--exit-on-stop", "exit_on_stop", FlagKind::Bool),
            FlagSpec::opt("--wake-on-stop", "wake_on_stop", FlagKind::Bool),
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["watch"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[
            FlagSpec::opt("--job", "job_id", FlagKind::String),
            FlagSpec::opt("--district", "district_id", FlagKind::String),
            FlagSpec::opt("--events", "events", FlagKind::String),
            FlagSpec::opt("--since-seq", "since_seq", FlagKind::U64),
            FlagSpec::opt("--debounce-ms", "debounce_ms", FlagKind::U64),
            FlagSpec::opt("--max-events", "max_events", FlagKind::U64),
            FlagSpec::opt("--timeout", "timeout", FlagKind::U64),
            FlagSpec::opt("--timeout-ms", "timeout_ms", FlagKind::U64),
            FlagSpec::opt("--ndjson", "ndjson", FlagKind::Bool),
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["districts"],
        mcp_tool: Some("districts_list"),
        mutates: false,
        positionals: &[],
        flags: &[FlagSpec::opt("--include-archived", "include_archived", FlagKind::Bool)],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["districts", "propose-create"],
        mcp_tool: Some("districts_propose_create"),
        mutates: true,
        positionals: &[],
        flags: &[
            FlagSpec::opt("--district-id", "district_id", FlagKind::String),
            FlagSpec::req("--name", "name", FlagKind::String),
            FlagSpec::opt("--setup-by", "setup_by", FlagKind::String),
            FlagSpec::req("--summary", "summary", FlagKind::String),
            FlagSpec::req("--principles-json", "principles", FlagKind::Json),
            FlagSpec::opt("--anti-patterns-json", "anti_patterns", FlagKind::Json),
            FlagSpec::opt("--heuristics-json", "decision_heuristics", FlagKind::Json),
            FlagSpec::req("--stages-json", "stages", FlagKind::Json),
            FlagSpec::opt("--changelog", "changelog", FlagKind::String),
            FlagSpec::req("--rationale", "rationale", FlagKind::String),
            FlagSpec::rep("--feedback-id", "source_feedback_ids", FlagKind::String),
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["districts", "propose-archive"],
        mcp_tool: Some("districts_propose_archive"),
        mutates: true,
        positionals: &[PositionalSpec::new("district_id", "district_id")],
        flags: &[
            FlagSpec::req("--rationale", "rationale", FlagKind::String),
            FlagSpec::rep("--feedback-id", "source_feedback_ids", FlagKind::String),
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["districts", "propose-restore"],
        mcp_tool: Some("districts_propose_restore"),
        mutates: true,
        positionals: &[PositionalSpec::new("district_id", "district_id")],
        flags: &[
            FlagSpec::req("--rationale", "rationale", FlagKind::String),
            FlagSpec::rep("--feedback-id", "source_feedback_ids", FlagKind::String),
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["districts", "approve-proposal"],
        mcp_tool: Some("districts_approve_proposal"),
        mutates: true,
        positionals: &[PositionalSpec::new("proposal_id", "proposal_id")],
        flags: &[FLAG_COMMAND_ID],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "list"],
        mcp_tool: Some("jobs_list"),
        mutates: false,
        positionals: &[],
        flags: &[
            FlagSpec::opt("--district", "district_id", FlagKind::String),
            FlagSpec::opt("--limit", "limit", FlagKind::U64),
            FlagSpec::opt("--offset", "offset", FlagKind::U64),
            FlagSpec::opt("--include-archived", "include_archived", FlagKind::Bool),
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "context"],
        mcp_tool: Some("job_get_context"),
        mutates: false,
        positionals: &[PositionalSpec::new("job_id", "job_id")],
        flags: &[],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "create"],
        mcp_tool: Some("jobs_create"),
        mutates: true,
        positionals: &[PositionalSpec::new("title", "title")],
        flags: &[
            FlagSpec::opt("--input", "raw_input", FlagKind::String),
            FlagSpec::opt("--district", "target_district_hint", FlagKind::String),
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "update"],
        mcp_tool: Some("jobs_update"),
        mutates: true,
        positionals: &[PositionalSpec::new("job_id", "job_id")],
        flags: &[
            FlagSpec::opt("--title", "title", FlagKind::String),
            FlagSpec::opt("--input", "raw_input", FlagKind::String),
            FlagSpec::opt("--summary", "spec_summary", FlagKind::String),
            FlagSpec::opt("--district", "target_district_hint", FlagKind::String),
            FLAG_EXPECTED_REV,
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "archive"],
        mcp_tool: Some("jobs_archive"),
        mutates: true,
        positionals: &[PositionalSpec::new("job_id", "job_id")],
        flags: &[
            FlagSpec::opt("--reason", "reason", FlagKind::String),
            FLAG_EXPECTED_REV,
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "assign"],
        mcp_tool: Some("jobs_assign"),
        mutates: true,
        positionals: &[
            PositionalSpec::new("job_id", "job_id"),
            PositionalSpec::new("district_id", "district_id"),
        ],
        flags: &[FLAG_EXPECTED_REV, FLAG_COMMAND_ID],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "claim"],
        mcp_tool: Some("jobs_claim"),
        mutates: true,
        positionals: &[PositionalSpec::new("job_id", "job_id")],
        flags: &[
            FlagSpec::opt("--runner", "runner_label", FlagKind::String),
            FlagSpec::opt("--ttl-ms", "ttl_ms", FlagKind::U64),
            FLAG_EXPECTED_REV,
            FLAG_COMMAND_ID,
        ],
        default_runner: Some("CLI Agent"),
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "heartbeat"],
        mcp_tool: Some("jobs_heartbeat"),
        mutates: true,
        positionals: &[PositionalSpec::new("job_id", "job_id")],
        flags: &[
            FLAG_FENCING_REQ,
            FlagSpec::opt("--ttl-ms", "ttl_ms", FlagKind::U64),
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "advance"],
        mcp_tool: Some("jobs_advance"),
        mutates: true,
        positionals: &[PositionalSpec::new("job_id", "job_id")],
        flags: &[
            FlagSpec::rep("--field", "fields", FlagKind::FieldPair),
            FlagSpec::opt("--fields-json", "fields", FlagKind::Json),
            FlagSpec::rep("--artifact-id", "artifact_ids", FlagKind::String),
            FLAG_FENCING_OPT,
            FLAG_EXPECTED_REV,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["jobs", "rework"],
        mcp_tool: Some("jobs_rework"),
        mutates: true,
        positionals: &[PositionalSpec::new("job_id", "job_id")],
        flags: &[
            FlagSpec::opt("--reason", "reason", FlagKind::String),
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["artifacts", "attach"],
        mcp_tool: Some("artifacts_attach"),
        mutates: true,
        positionals: &[PositionalSpec::new("job_id", "job_id")],
        flags: &[
            FlagSpec::req("--kind", "kind", FlagKind::String),
            FlagSpec::req("--label", "label", FlagKind::String),
            FlagSpec::opt("--uri", "uri", FlagKind::String),
            FlagSpec::local_opt("--file", "file", FlagKind::String),
            FlagSpec::opt("--content", "content", FlagKind::String),
            FlagSpec::opt("--sha256", "sha256", FlagKind::String),
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: Some(prepare_artifact_attach_payload),
    },
    ToolSpec {
        path: &["artifacts", "read"],
        mcp_tool: Some("artifacts_read"),
        mutates: false,
        positionals: &[
            PositionalSpec::new("job_id", "job_id"),
            PositionalSpec::new("artifact_id", "artifact_id"),
        ],
        flags: &[],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["gate", "verify"],
        mcp_tool: Some("gate_verify_record"),
        mutates: true,
        positionals: &[PositionalSpec::new("job_id", "job_id")],
        flags: &[
            FlagSpec::req("--verdict", "verdict", FlagKind::String),
            FlagSpec::req("--summary", "summary", FlagKind::String),
            FlagSpec::rep("--check", "checklist_results", FlagKind::CheckPair),
            FLAG_FENCING_OPT,
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["feedback", "submit"],
        mcp_tool: Some("feedback_submit"),
        mutates: true,
        positionals: &[PositionalSpec::new("district_id", "district_id")],
        flags: &[
            FlagSpec::req("--kind", "kind", FlagKind::String),
            FlagSpec::req("--body", "body", FlagKind::String),
            FlagSpec::opt("--job-id", "job_id", FlagKind::String),
            FlagSpec::opt("--stage-id", "stage_id", FlagKind::String),
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["philosophy", "propose"],
        mcp_tool: Some("philosophy_propose"),
        mutates: true,
        positionals: &[PositionalSpec::new("district_id", "district_id")],
        flags: &[
            FlagSpec::req("--base-version", "base_version", FlagKind::U64),
            FlagSpec::opt(
                "--district-name",
                "proposed_district_name",
                FlagKind::String,
            ),
            FlagSpec::req("--summary", "summary", FlagKind::String),
            FlagSpec::req("--rationale", "rationale", FlagKind::String),
            FlagSpec::req("--principles-json", "principles", FlagKind::Json),
            FlagSpec::rep("--anti-pattern", "anti_patterns", FlagKind::String),
            FlagSpec::rep("--heuristic", "decision_heuristics", FlagKind::String),
            FlagSpec::rep("--feedback-id", "source_feedback_ids", FlagKind::String),
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["spec", "propose"],
        mcp_tool: Some("spec_propose"),
        mutates: true,
        positionals: &[PositionalSpec::new("district_id", "district_id")],
        flags: &[
            FlagSpec::req("--base-version", "base_version", FlagKind::U64),
            FlagSpec::opt(
                "--district-name",
                "proposed_district_name",
                FlagKind::String,
            ),
            FlagSpec::req("--changelog", "changelog", FlagKind::String),
            FlagSpec::req("--rationale", "rationale", FlagKind::String),
            FlagSpec::req("--stages-json", "stages", FlagKind::Json),
            FlagSpec::rep("--feedback-id", "source_feedback_ids", FlagKind::String),
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["templates"],
        mcp_tool: Some("templates_list"),
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["template", "propose"],
        mcp_tool: Some("template_propose"),
        mutates: true,
        positionals: &[PositionalSpec::new("template_id", "template_id")],
        flags: &[
            FlagSpec::req("--name", "name", FlagKind::String),
            FlagSpec::req("--description", "description", FlagKind::String),
            FlagSpec::opt("--philosophy-notes", "philosophy_notes", FlagKind::String),
            FlagSpec::req("--rationale", "rationale", FlagKind::String),
            FlagSpec::req("--stages-json", "stages", FlagKind::Json),
            FlagSpec::rep("--feedback-id", "source_feedback_ids", FlagKind::String),
            FLAG_COMMAND_ID,
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["backdrop", "list"],
        mcp_tool: None,
        mutates: false,
        positionals: &[],
        flags: &[],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["backdrop", "add"],
        mcp_tool: None,
        mutates: true,
        positionals: &[PositionalSpec::new("path", "path")],
        flags: &[FlagSpec::opt("--label", "label", FlagKind::String)],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["backdrop", "update"],
        mcp_tool: None,
        mutates: true,
        positionals: &[PositionalSpec::new("id", "id")],
        flags: &[
            FlagSpec::opt("--path", "path", FlagKind::String),
            FlagSpec::opt("--label", "label", FlagKind::String),
            FlagSpec::opt("--index", "index", FlagKind::U64),
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["backdrop", "remove"],
        mcp_tool: None,
        mutates: true,
        positionals: &[PositionalSpec::new("id", "id")],
        flags: &[],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["backdrop", "clear"],
        mcp_tool: None,
        mutates: true,
        positionals: &[],
        flags: &[],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["backdrop", "tuning"],
        mcp_tool: None,
        mutates: true,
        positionals: &[],
        flags: &[
            FlagSpec::opt("--enable-images", "enable_images", FlagKind::Bool),
            FlagSpec::opt("--disable-images", "disable_images", FlagKind::Bool),
            FlagSpec::opt(
                "--enable-apple-music",
                "enable_apple_music",
                FlagKind::Bool,
            ),
            FlagSpec::opt(
                "--disable-apple-music",
                "disable_apple_music",
                FlagKind::Bool,
            ),
            FlagSpec::opt(
                "--enable-youtube-mirror",
                "enable_youtube_mirror",
                FlagKind::Bool,
            ),
            FlagSpec::opt(
                "--disable-youtube-mirror",
                "disable_youtube_mirror",
                FlagKind::Bool,
            ),
            FlagSpec::opt("--tile-size", "tile_size_px", FlagKind::F32),
            FlagSpec::opt("--hold-secs", "image_hold_secs", FlagKind::F32),
            FlagSpec::opt("--image-brightness", "image_brightness", FlagKind::F32),
            FlagSpec::opt("--image-peak", "image_peak_luminance", FlagKind::F32),
            FlagSpec::opt("--image-saturation", "image_saturation", FlagKind::F32),
            FlagSpec::opt("--master-brightness", "master_brightness", FlagKind::F32),
            FlagSpec::opt("--reset", "reset", FlagKind::Bool),
        ],
        default_runner: None,
        payload_hook: None,
    },
    ToolSpec {
        path: &["mcp", "call"],
        mcp_tool: None,
        mutates: true,
        positionals: &[PositionalSpec::new("tool_name", "tool_name")],
        flags: &[FlagSpec::opt("--args-json", "arguments", FlagKind::Json)],
        default_runner: None,
        payload_hook: None,
    },
];

