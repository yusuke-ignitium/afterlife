use std::collections::BTreeMap;

use afterlife_core::{ArtifactKind, GateVerdict};
use serde::{Deserialize, Serialize};

pub const SIGNAL_FENCE_TAG: &str = "afterlife-signal";
pub const SIGNAL_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalSource {
    Agent,
    Afterlife,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalAction {
    EnterDistrict,
    AdvanceStage,
    AttachArtifact,
    Approved,
    Reworked,
    Stopped,
    Resumed,
    SyncError,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalGateReport {
    pub verdict: GateVerdict,
    pub summary: String,
    #[serde(default)]
    pub checks: Vec<(String, bool)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalArtifact {
    pub kind: ArtifactKind,
    pub label: String,
    pub uri: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearSignalV1 {
    pub version: u32,
    pub source: SignalSource,
    pub action: SignalAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub district_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_stage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_stage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<SignalGateReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<SignalArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub afterlife_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub afterlife_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl LinearSignalV1 {
    pub fn afterlife_event(action: SignalAction, job_id: impl Into<String>, seq: u64) -> Self {
        Self {
            version: SIGNAL_SCHEMA_VERSION,
            source: SignalSource::Afterlife,
            action,
            district_id: None,
            stage_id: None,
            sub_stage_id: None,
            target_stage_id: None,
            runner_label: None,
            summary: None,
            fields: BTreeMap::new(),
            gate: None,
            artifacts: Vec::new(),
            afterlife_job_id: Some(job_id.into()),
            afterlife_seq: Some(seq),
            reason: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedLinearComment {
    pub signal: LinearSignalV1,
    pub markdown_sections: BTreeMap<String, String>,
    pub prose: String,
}

impl ParsedLinearComment {
    pub fn merged_fields(&self) -> BTreeMap<String, String> {
        let mut merged = self.markdown_sections.clone();
        for (key, value) in &self.signal.fields {
            if !value.trim().is_empty() {
                merged.insert(key.clone(), value.trim().to_string());
            }
        }
        merged
    }
}

fn is_signal_fence_open(line: &str) -> bool {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let tag = rest.trim();
        return tag.eq_ignore_ascii_case(SIGNAL_FENCE_TAG);
    }
    false
}

fn is_fence_close(line: &str) -> bool {
    line.trim() == "```"
}

fn parse_section_heading_key(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("## ")?;
    let key = rest.trim().trim_matches('`').trim();
    if key.is_empty() {
        return None;
    }
    let valid = key
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    valid.then(|| key.to_string())
}

fn parse_markdown_sections(prose: &str) -> BTreeMap<String, String> {
    let mut sections = BTreeMap::new();
    let mut current_key: Option<String> = None;
    let mut buffer = Vec::<&str>::new();
    let mut in_code_fence = false;

    for line in prose.lines() {
        if line.trim_start().starts_with("```") {
            in_code_fence = !in_code_fence;
        }
        if !in_code_fence && let Some(next_key) = parse_section_heading_key(line) {
            if let Some(prev_key) = current_key.take() {
                let body = buffer.join("\n").trim().to_string();
                if !body.is_empty() {
                    sections.insert(prev_key, body);
                }
            }
            buffer.clear();
            current_key = Some(next_key);
            continue;
        }
        if current_key.is_some() {
            buffer.push(line);
        }
    }

    if let Some(last_key) = current_key {
        let body = buffer.join("\n").trim().to_string();
        if !body.is_empty() {
            sections.insert(last_key, body);
        }
    }

    sections
}

pub fn extract_linear_signal(body: &str) -> Result<Option<ParsedLinearComment>, String> {
    let mut in_signal = false;
    let mut found_signal = false;
    let mut signal_lines = Vec::<&str>::new();
    let mut prose_lines = Vec::<&str>::new();

    for line in body.lines() {
        if !in_signal && !found_signal && is_signal_fence_open(line) {
            in_signal = true;
            continue;
        }
        if in_signal {
            if is_fence_close(line) {
                in_signal = false;
                found_signal = true;
            } else {
                signal_lines.push(line);
            }
            continue;
        }
        prose_lines.push(line);
    }

    if in_signal {
        return Err("unclosed ```afterlife-signal code block".into());
    }
    if !found_signal {
        return Ok(None);
    }

    let raw_json = signal_lines.join("\n");
    let signal: LinearSignalV1 = serde_json::from_str(raw_json.trim())
        .map_err(|e| format!("invalid afterlife-signal JSON: {e}"))?;
    if signal.version != SIGNAL_SCHEMA_VERSION {
        return Err(format!(
            "unsupported afterlife-signal version {} (expected {})",
            signal.version, SIGNAL_SCHEMA_VERSION
        ));
    }

    let prose = prose_lines.join("\n").trim().to_string();
    let markdown_sections = parse_markdown_sections(&prose);

    Ok(Some(ParsedLinearComment {
        signal,
        markdown_sections,
        prose,
    }))
}

pub fn format_afterlife_comment(
    headline: &str,
    details: Option<&str>,
    signal: &LinearSignalV1,
) -> String {
    let payload = serde_json::to_string_pretty(signal).unwrap_or_else(|_| "{}".into());
    let mut out = String::new();
    out.push_str(headline.trim());
    out.push('\n');
    if let Some(text) = details.map(str::trim).filter(|s| !s.is_empty()) {
        out.push('\n');
        out.push_str(text);
        out.push('\n');
    }
    out.push('\n');
    out.push_str("```");
    out.push_str(SIGNAL_FENCE_TAG);
    out.push('\n');
    out.push_str(&payload);
    out.push_str("\n```\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_signal_and_merges_markdown_sections() {
        let comment = r#"Completed discovery pass.

## findings
Verified primary sources and isolated probe behavior.

## sources_read
- crates/afterlife-core/src/model/artifact_uri.rs

```afterlife-signal
{
  "version": 1,
  "source": "agent",
  "action": "advance_stage",
  "district_id": "DIST-001",
  "stage_id": "discover",
  "sub_stage_id": "trace_sources",
  "runner_label": "Cursor Cloud Agent",
  "fields": {
    "findings": "Explicit JSON override"
  }
}
```
"#;
        let parsed = extract_linear_signal(comment).unwrap().unwrap();
        assert_eq!(parsed.signal.source, SignalSource::Agent);
        assert_eq!(parsed.signal.action, SignalAction::AdvanceStage);
        assert_eq!(parsed.signal.district_id.as_deref(), Some("DIST-001"));
        assert_eq!(parsed.signal.stage_id.as_deref(), Some("discover"));
        assert_eq!(parsed.signal.sub_stage_id.as_deref(), Some("trace_sources"));

        let merged = parsed.merged_fields();
        assert_eq!(
            merged.get("findings").map(String::as_str),
            Some("Explicit JSON override")
        );
        assert_eq!(
            merged.get("sources_read").map(String::as_str),
            Some("- crates/afterlife-core/src/model/artifact_uri.rs")
        );
    }

    #[test]
    fn returns_none_for_plain_comment() {
        assert_eq!(
            extract_linear_signal("Just a regular human comment.").unwrap(),
            None
        );
    }

    #[test]
    fn rejects_invalid_json_or_unclosed_fence() {
        assert!(extract_linear_signal("```afterlife-signal\n{bad\n```").is_err());
        assert!(extract_linear_signal("```afterlife-signal\n{\"version\":1}").is_err());
    }

    #[test]
    fn formats_and_round_trips_afterlife_comment() {
        let mut sig = LinearSignalV1::afterlife_event(SignalAction::Reworked, "JOB-042", 77);
        sig.district_id = Some("DIST-001".into());
        sig.stage_id = Some("build".into());
        sig.reason = Some("Missing regression test".into());
        let formatted = format_afterlife_comment(
            "**Afterlife Rework** -> `build`",
            Some("Reason: Missing regression test"),
            &sig,
        );
        let parsed = extract_linear_signal(&formatted).unwrap().unwrap();
        assert_eq!(parsed.signal, sig);
    }
}
