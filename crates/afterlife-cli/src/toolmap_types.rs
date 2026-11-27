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
    pub const fn opt(flag: &'static str, key: &'static str, kind: FlagKind) -> Self {
        Self {
            flag,
            target: FlagTarget::McpKey(key),
            kind,
            required: false,
            repeatable: false,
        }
    }
    pub const fn req(flag: &'static str, key: &'static str, kind: FlagKind) -> Self {
        Self {
            flag,
            target: FlagTarget::McpKey(key),
            kind,
            required: true,
            repeatable: false,
        }
    }
    pub const fn rep(flag: &'static str, key: &'static str, kind: FlagKind) -> Self {
        Self {
            flag,
            target: FlagTarget::McpKey(key),
            kind,
            required: false,
            repeatable: true,
        }
    }
    pub const fn local_opt(flag: &'static str, key: &'static str, kind: FlagKind) -> Self {
        Self {
            flag,
            target: FlagTarget::LocalKey(key),
            kind,
            required: false,
            repeatable: false,
        }
    }
}

pub struct PositionalSpec {
    pub placeholder: &'static str,
    pub mcp_key: &'static str,
}

impl PositionalSpec {
    pub const fn new(placeholder: &'static str, mcp_key: &'static str) -> Self {
        Self {
            placeholder,
            mcp_key,
        }
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

pub const FLAG_COMMAND_ID: FlagSpec = FlagSpec::opt("--command-id", "command_id", FlagKind::String);
pub const FLAG_FENCING_OPT: FlagSpec =
    FlagSpec::opt("--fencing-token", "fencing_token", FlagKind::U64);
pub const FLAG_FENCING_REQ: FlagSpec =
    FlagSpec::req("--fencing-token", "fencing_token", FlagKind::U64);
pub const FLAG_EXPECTED_REV: FlagSpec =
    FlagSpec::opt("--expected-revision", "expected_revision", FlagKind::U64);
pub const FLAG_ANTI_PATTERN: FlagSpec =
    FlagSpec::rep("--anti-pattern", "anti_patterns", FlagKind::String);
pub const FLAG_HEURISTIC: FlagSpec =
    FlagSpec::rep("--heuristic", "decision_heuristics", FlagKind::String);
pub const FLAG_FEEDBACK_ID: FlagSpec =
    FlagSpec::rep("--feedback-id", "source_feedback_ids", FlagKind::String);

pub fn prepare_artifact_attach_payload(
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
