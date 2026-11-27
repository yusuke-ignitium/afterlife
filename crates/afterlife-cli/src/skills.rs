use std::path::PathBuf;

use afterlife_protocol::{
    SkillExportBundle, default_backdrop_data_dir, export_builtin_skills, write_skill_bundle_atomic,
};
use serde_json::{Map, Value, json};

use crate::transport::{CliError, get_json};

fn fetch_live_bundle(district: Option<&str>) -> Result<SkillExportBundle, CliError> {
    let path = match district {
        Some(did) => format!("/v1/districts/skills?district_id={did}"),
        None => "/v1/districts/skills".to_string(),
    };
    let val = get_json(&path)?;
    serde_json::from_value::<SkillExportBundle>(val)
        .map_err(|e| CliError::transport(format!("invalid skills payload from daemon: {e}")))
}

pub fn run_skills_command(sub: &str, payload: &Map<String, Value>) -> Result<Value, CliError> {
    if sub != "export" {
        return Err(CliError::usage(format!("unknown skills subcommand `{sub}`")));
    }

    let district = payload.get("district").and_then(Value::as_str);
    let source = payload
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("auto");
    let print_only = payload
        .get("print")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let (bundle, source_used) = match source {
        "builtin" => (export_builtin_skills(district), "builtin"),
        "live" => (fetch_live_bundle(district)?, "live"),
        "auto" => match fetch_live_bundle(district) {
            Ok(b) => (b, "live"),
            Err(_) => (export_builtin_skills(district), "builtin"),
        },
        other => {
            return Err(CliError::usage(format!(
                "--source must be auto, live, or builtin (got `{other}`)"
            )));
        }
    };

    if print_only {
        return Ok(json!({
            "source": source_used,
            "skills": bundle.skills,
        }));
    }

    let out_dir = match payload.get("out").and_then(Value::as_str) {
        Some(dir) if !dir.trim().is_empty() => PathBuf::from(dir.trim()),
        _ => default_backdrop_data_dir().join("skills"),
    };
    let report = write_skill_bundle_atomic(&out_dir, &bundle, district.is_none())
        .map_err(CliError::local)?;

    Ok(json!({
        "source": source_used,
        "skill_count": bundle.skills.len(),
        "out_dir": report.out_dir,
        "written": report.written,
        "unchanged": report.unchanged,
        "removed": report.removed,
    }))
}
