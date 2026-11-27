use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use afterlife_core::{AfterlifeEngine, District, OwnerId, SubMachineTemplate, SubMachineTemplateId};
use afterlife_sm_wasm::DistrictPackage;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::{Deserialize, Serialize};

include!(concat!(env!("OUT_DIR"), "/sm_wasm_embed.rs"));

pub const ENTRY_SKILL_SLUG: &str = "afterlife";
const MANIFEST_FILE: &str = "manifest.json";
const EMBEDDED_SM_MJS: &str = include_str!("embedded_sm.mjs");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedSkillFile {
    pub slug: String,
    pub relative_path: String,
    pub district_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
    #[serde(default)]
    pub executable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillExportBundle {
    pub skills: Vec<ExportedSkillFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillManifestEntry {
    pub slug: String,
    pub relative_path: String,
    pub district_id: Option<String>,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillManifest {
    pub version: u32,
    pub skills: Vec<SkillManifestEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillWriteReport {
    pub out_dir: String,
    pub written: Vec<String>,
    pub unchanged: Vec<String>,
    pub removed: Vec<String>,
}

fn load_sm_wasm_bytes() -> Option<Vec<u8>> {
    if let Ok(path) = std::env::var("AFTERLIFE_SM_WASM_PATH")
        && let Ok(bytes) = fs::read(path.trim())
    {
        return Some(bytes);
    }
    if let Some(embedded) = EMBEDDED_SM_WASM {
        return Some(embedded.to_vec());
    }
    None
}

pub fn export_district_json(
    district: &District,
    templates: &HashMap<SubMachineTemplateId, SubMachineTemplate>,
) -> ExportedSkillFile {
    let mut used_templates = HashMap::new();
    for stage in &district.active_spec().stages {
        if let Some(tid) = &stage.sub_template_id
            && let Some(tpl) = templates.get(tid)
        {
            used_templates.insert(tid.clone(), tpl.clone());
        }
    }
    let pkg = DistrictPackage {
        schema_version: 1,
        district: district.clone(),
        templates: used_templates,
    };
    let content = serde_json::to_string_pretty(&pkg).unwrap_or_else(|_| "{}".to_string()) + "\n";
    let file_id = district.id.as_str();
    ExportedSkillFile {
        slug: ENTRY_SKILL_SLUG.to_string(),
        relative_path: format!("{ENTRY_SKILL_SLUG}/districts/{file_id}.json"),
        district_id: Some(file_id.to_string()),
        title: district.name.clone(),
        content,
        content_base64: None,
        executable: false,
    }
}

pub fn export_sm_runner_script() -> ExportedSkillFile {
    ExportedSkillFile {
        slug: ENTRY_SKILL_SLUG.to_string(),
        relative_path: format!("{ENTRY_SKILL_SLUG}/sm.mjs"),
        district_id: None,
        title: "Afterlife Embedded WASM State Machine Runner".to_string(),
        content: EMBEDDED_SM_MJS.to_string(),
        content_base64: None,
        executable: true,
    }
}

pub fn export_sm_wasm_binary() -> Option<ExportedSkillFile> {
    let bytes = load_sm_wasm_bytes()?;
    Some(ExportedSkillFile {
        slug: ENTRY_SKILL_SLUG.to_string(),
        relative_path: format!("{ENTRY_SKILL_SLUG}/sm.wasm"),
        district_id: None,
        title: "Afterlife Core State Machine (WASM)".to_string(),
        content: String::new(),
        content_base64: Some(BASE64_STANDARD.encode(bytes)),
        executable: false,
    })
}

pub fn export_entry_skill(districts: &[District]) -> ExportedSkillFile {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str("name: afterlife\n");
    out.push_str("description: \"Execute or resume a Linear issue under Afterlife's embedded Rust/WASM State Machine (`sm.mjs` + `sm.wasm` + `districts/*.json`) and post validated `afterlife-signal` comments to Linear.\"\n");
    out.push_str("---\n\n");
    out.push_str("# Afterlife\n\n");
    out.push_str(
        "This single skill embeds the real `afterlife-core` State Machine compiled to WebAssembly (`sm.wasm`), a zero-dependency Node.js CLI wrapper (`sm.mjs`), and compiled District specs (`districts/*.json`). **Never guess stage contracts from prose**—always query `sm.mjs` for the active step and let `sm.mjs advance` validate via `afterlife-core` and emit the Linear comment.\n\n",
    );

    out.push_str("## Bundled Files\n\n");
    out.push_str("- `sm.mjs` — zero-dependency Node.js CLI wrapper around `sm.wasm`\n");
    out.push_str("- `sm.wasm` — compiled `afterlife-core` State Machine (single source of truth)\n");
    out.push_str("- `districts/<DISTRICT_ID>.json` — versioned District + Sub-Machine definitions\n\n");

    out.push_str("## Compact District Index\n\n");
    for d in districts {
        let _ = writeln!(
            out,
            "- `{}` — {} (`v{}` / spec `v{}`): {}",
            d.id,
            d.name,
            d.active_philosophy_version,
            d.active_spec_version,
            d.active_philosophy().summary
        );
    }
    out.push('\n');

    out.push_str("## Workflow (Run `sm.mjs` at Every Step)\n\n");
    out.push_str("1. **Replay Linear comments to locate the exact current step**:\n");
    out.push_str("   ```bash\n   node <skill_dir>/sm.mjs replay --file comments.json\n   ```\n");
    out.push_str("   - `stopped: true` -> pause until a `resumed` signal appears.\n");
    out.push_str("   - `current_context.hard_stop: true` -> **HARD STOP** at `ApprovalGate` (or `Terminal`). Wait for human `approved` or `reworked`.\n");
    out.push_str("   - `last_rework_reason` / `last_sync_error` present -> address it from `current_context`.\n");
    out.push_str("2. **If no signal exists yet, pick a District and post `enter_district`**:\n");
    out.push_str("   ```bash\n   node <skill_dir>/sm.mjs list\n   node <skill_dir>/sm.mjs enter --district DIST-001 --runner \"Cloud Agent\"\n   ```\n");
    out.push_str("   Post `comment_markdown` to the Linear issue, then follow `next_context`.\n");
    out.push_str("3. **Load ONLY the active step's contract before executing**:\n");
    out.push_str("   ```bash\n   node <skill_dir>/sm.mjs context --district DIST-001 --stage discover --sub-stage trace_sources\n   ```\n");
    out.push_str("4. **Validate via `afterlife-core` WASM and emit the transition comment** (pass `--field KEY=VALUE`, `--field-file KEY=PATH`, or `--fields-json`; when `requires_gate_verification` is true, pass `--gate-verdict pass --gate-summary ...` and `--check \"<rubric item>=true\"` for every rubric check; attach PR URLs with `--artifact pull_request:LABEL:URI` before a delivery ApprovalGate):\n");
    out.push_str("   ```bash\n   node <skill_dir>/sm.mjs advance \\\n     --district DIST-001 --stage discover --sub-stage trace_sources \\\n     --field sources_read=\"...\" --field findings=\"...\"\n   ```\n");
    out.push_str("   Post `comment_markdown` to Linear. The Linear comment URL itself satisfies external artifact evidence gates.\n");
    out.push_str("5. **Repeat with `next_context` until `hard_stop: true`.** Never bypass an `ApprovalGate`—both `sm.wasm` and the local Afterlife daemon fail-closed reject it.\n");

    ExportedSkillFile {
        slug: ENTRY_SKILL_SLUG.to_string(),
        relative_path: format!("{ENTRY_SKILL_SLUG}/SKILL.md"),
        district_id: None,
        title: "Afterlife".to_string(),
        content: out,
        content_base64: None,
        executable: false,
    }
}

pub fn export_workspace_skills(
    engine: &AfterlifeEngine,
    district_filter: Option<&str>,
) -> SkillExportBundle {
    let mut active: Vec<District> = engine
        .districts
        .iter()
        .filter(|d| !d.archived)
        .filter(|d| district_filter.is_none_or(|f| d.id.as_str() == f))
        .cloned()
        .collect();
    active.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

    let mut skills = Vec::with_capacity(active.len() + 3);
    skills.push(export_entry_skill(&active));
    skills.push(export_sm_runner_script());
    if let Some(wasm_file) = export_sm_wasm_binary() {
        skills.push(wasm_file);
    }
    for district in &active {
        skills.push(export_district_json(district, &engine.templates));
    }
    SkillExportBundle { skills }
}

pub fn export_builtin_skills(district_filter: Option<&str>) -> SkillExportBundle {
    let engine = AfterlifeEngine::new_for_owner(OwnerId::from("builtin"));
    export_workspace_skills(&engine, district_filter)
}

#[cfg(unix)]
fn set_executable_if_needed(path: &Path, executable: bool) -> Result<(), String> {
    if !executable {
        return Ok(());
    }
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| format!("metadata {}: {e}", path.display()))?
        .permissions();
    if perms.mode() & 0o111 != 0o111 {
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .map_err(|e| format!("chmod {}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_if_needed(_path: &Path, _executable: bool) -> Result<(), String> {
    Ok(())
}

fn atomic_write_if_changed(
    path: &Path,
    content: &[u8],
    executable: bool,
) -> Result<bool, String> {
    if let Ok(existing) = fs::read(path)
        && existing == content
    {
        set_executable_if_needed(path, executable)?;
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create dir {}: {e}", parent.display()))?;
    }
    let tmp = PathBuf::from(format!("{}.tmp.{}", path.display(), std::process::id()));
    fs::write(&tmp, content).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    set_executable_if_needed(&tmp, executable)?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("rename {} -> {}: {e}", tmp.display(), path.display())
    })?;
    Ok(true)
}

fn decode_skill_bytes(skill: &ExportedSkillFile) -> Result<Vec<u8>, String> {
    match &skill.content_base64 {
        Some(b64) => BASE64_STANDARD
            .decode(b64)
            .map_err(|e| format!("decode base64 for {}: {e}", skill.relative_path)),
        None => Ok(skill.content.as_bytes().to_vec()),
    }
}

pub fn write_skill_bundle_atomic(
    out_dir: &Path,
    bundle: &SkillExportBundle,
    prune_stale: bool,
) -> Result<SkillWriteReport, String> {
    fs::create_dir_all(out_dir)
        .map_err(|e| format!("create skills dir {}: {e}", out_dir.display()))?;

    let manifest_path = out_dir.join(MANIFEST_FILE);
    let previous_entries: Vec<SkillManifestEntry> = fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<SkillManifest>(&raw).ok())
        .map(|m| m.skills)
        .unwrap_or_default();

    let mut written = Vec::new();
    let mut unchanged = Vec::new();
    let mut current_paths = HashSet::new();
    let mut current_slugs = HashSet::new();

    for skill in &bundle.skills {
        current_paths.insert(skill.relative_path.clone());
        current_slugs.insert(skill.slug.clone());
        let bytes = decode_skill_bytes(skill)?;
        let target = out_dir.join(&skill.relative_path);
        if atomic_write_if_changed(&target, &bytes, skill.executable)? {
            written.push(skill.relative_path.clone());
        } else {
            unchanged.push(skill.relative_path.clone());
        }
    }

    let mut removed = Vec::new();
    if prune_stale {
        for prev in previous_entries {
            if !current_slugs.contains(&prev.slug) && prev.slug.starts_with("afterlife-") {
                let old_dir = out_dir.join(&prev.slug);
                if old_dir.is_dir() && fs::remove_dir_all(&old_dir).is_ok() {
                    removed.push(prev.slug);
                }
                continue;
            }
            if !current_paths.contains(&prev.relative_path)
                && (prev.relative_path.starts_with("afterlife/")
                    || prev.relative_path.starts_with("afterlife-"))
            {
                let old_file = out_dir.join(&prev.relative_path);
                if old_file.is_file() && fs::remove_file(&old_file).is_ok() {
                    removed.push(prev.relative_path);
                }
            }
        }
    }

    let manifest = SkillManifest {
        version: 3,
        skills: bundle
            .skills
            .iter()
            .map(|s| SkillManifestEntry {
                slug: s.slug.clone(),
                relative_path: s.relative_path.clone(),
                district_id: s.district_id.clone(),
                title: s.title.clone(),
            })
            .collect(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| format!("serialize skill manifest: {e}"))?;
    let _ = atomic_write_if_changed(&manifest_path, &manifest_bytes, false)?;

    Ok(SkillWriteReport {
        out_dir: out_dir.display().to_string(),
        written,
        unchanged,
        removed,
    })
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use serde_json::Value;

    use super::*;
    use crate::linear_signal::{
        LinearSignalV1, SignalAction, SignalSource, extract_linear_signal, format_afterlife_comment,
    };

    #[test]
    fn exports_single_afterlife_skill_with_sm_mjs_wasm_and_district_jsons() {
        let bundle = export_builtin_skills(None);
        assert!(bundle.skills.iter().all(|s| s.slug == ENTRY_SKILL_SLUG));
        assert!(
            bundle
                .skills
                .iter()
                .any(|s| s.relative_path == "afterlife/SKILL.md" && !s.executable)
        );
        assert!(
            bundle
                .skills
                .iter()
                .any(|s| s.relative_path == "afterlife/sm.mjs" && s.executable)
        );
        assert!(
            bundle
                .skills
                .iter()
                .any(|s| s.relative_path == "afterlife/districts/DIST-001.json")
        );
        let skill_md = bundle
            .skills
            .iter()
            .find(|s| s.relative_path == "afterlife/SKILL.md")
            .unwrap();
        assert!(skill_md.content.lines().count() <= 80);
    }

    #[test]
    fn atomic_writer_is_idempotent_and_prunes_legacy_and_archived_files() {
        let tmp = std::env::temp_dir().join(format!(
            "afterlife-single-skill-wasm-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);

        let legacy_dir = tmp.join("afterlife-dist-001-legacy");
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(legacy_dir.join("SKILL.md"), "legacy").unwrap();
        fs::write(
            tmp.join(MANIFEST_FILE),
            serde_json::to_vec(&SkillManifest {
                version: 1,
                skills: vec![SkillManifestEntry {
                    slug: "afterlife-dist-001-legacy".into(),
                    relative_path: "afterlife-dist-001-legacy/SKILL.md".into(),
                    district_id: Some("DIST-001".into()),
                    title: "Legacy".into(),
                }],
            })
            .unwrap(),
        )
        .unwrap();

        let mut engine = AfterlifeEngine::new_for_owner(OwnerId::from("owner-a"));
        let bundle1 = export_workspace_skills(&engine, None);
        let first = write_skill_bundle_atomic(&tmp, &bundle1, true).unwrap();
        assert_eq!(first.written.len(), bundle1.skills.len());
        assert!(first.removed.contains(&"afterlife-dist-001-legacy".to_string()));
        assert!(!legacy_dir.exists());

        let second = write_skill_bundle_atomic(&tmp, &bundle1, true).unwrap();
        assert!(second.written.is_empty());
        assert_eq!(second.unchanged.len(), bundle1.skills.len());

        let archived_id = engine.districts[0].id.as_str().to_string();
        engine.districts[0].archived = true;
        let bundle2 = export_workspace_skills(&engine, None);
        let third = write_skill_bundle_atomic(&tmp, &bundle2, true).unwrap();
        let archived_path = format!("afterlife/districts/{archived_id}.json");
        assert!(third.removed.contains(&archived_path));
        assert!(!tmp.join(&archived_path).exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn embedded_wasm_sm_mjs_walks_core_engine_blocks_approval_bypass_and_replays_rework() {
        let bundle = export_builtin_skills(None);
        if !bundle.skills.iter().any(|s| s.relative_path == "afterlife/sm.wasm") {
            return;
        }
        let tmp = std::env::temp_dir().join(format!(
            "afterlife-sm-wasm-exec-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        write_skill_bundle_atomic(&tmp, &bundle, true).unwrap();
        let sm_mjs = tmp.join("afterlife/sm.mjs");

        let run_sm = |args: &[&str]| -> (bool, Value) {
            let out = Command::new("node")
                .arg(&sm_mjs)
                .args(args)
                .output()
                .expect("node must execute sm.mjs");
            let text = if out.status.success() {
                String::from_utf8_lossy(&out.stdout).to_string()
            } else {
                String::from_utf8_lossy(&out.stderr).to_string()
            };
            let val: Value = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("invalid JSON from sm.mjs ({text}): {e}"));
            (out.status.success(), val)
        };

        let (ok, list_out) = run_sm(&["list"]);
        assert!(ok, "list failed: {list_out}");
        assert!(list_out["districts"].as_array().unwrap().len() >= 5);

        let (ok, enter_out) = run_sm(&["enter", "--district", "DIST-002"]);
        assert!(ok, "enter failed: {enter_out}");
        let mut comments: Vec<Value> = Vec::new();
        let enter_md = enter_out["comment_markdown"].as_str().unwrap().to_string();
        let enter_parsed = extract_linear_signal(&enter_md).unwrap().unwrap();
        assert_eq!(enter_parsed.signal.source, SignalSource::Agent);
        assert_eq!(enter_parsed.signal.action, SignalAction::EnterDistrict);
        comments.push(serde_json::json!({ "body": enter_md }));

        let first_stage = enter_out["current_location"]["stage_id"].as_str().unwrap();
        let (ok_missing, _err_missing) = run_sm(&[
            "advance",
            "--district",
            "DIST-002",
            "--stage",
            first_stage,
        ]);
        assert!(!ok_missing);

        let mut ctx = enter_out["next_context"].clone();
        while !ctx["hard_stop"].as_bool().unwrap() {
            let stage_id = ctx["stage"]["id"].as_str().unwrap().to_string();
            let sub_id = ctx["sub_stage"]["id"].as_str().map(str::to_string);
            let mut cli_args: Vec<String> = vec![
                "advance".into(),
                "--district".into(),
                "DIST-002".into(),
                "--stage".into(),
                stage_id,
            ];
            if let Some(sub) = sub_id {
                cli_args.push("--sub-stage".into());
                cli_args.push(sub);
            }
            for f in ctx["required_step_fields"].as_array().unwrap() {
                let k = f["key"].as_str().unwrap();
                cli_args.push("--field".into());
                cli_args.push(format!("{k}=Verified {k}"));
            }
            if ctx["requires_gate_verification"].as_bool().unwrap() {
                cli_args.push("--gate-verdict".into());
                cli_args.push("pass".into());
                cli_args.push("--gate-summary".into());
                cli_args.push("All checks passed".into());
                if let Some(checks) = ctx["gate_rubric"]["checks"].as_array() {
                    for chk in checks {
                        if let Some(text) = chk.as_str() {
                            cli_args.push("--check".into());
                            cli_args.push(format!("{text}=true"));
                        }
                    }
                }
            }
            let refs: Vec<&str> = cli_args.iter().map(String::as_str).collect();
            let (ok, adv_out) = run_sm(&refs);
            assert!(ok, "advance failed: {adv_out}");
            let md = adv_out["comment_markdown"].as_str().unwrap().to_string();
            let parsed = extract_linear_signal(&md).unwrap().unwrap();
            assert_eq!(parsed.signal.action, SignalAction::AdvanceStage);
            comments.push(serde_json::json!({ "body": md }));
            ctx = adv_out["next_context"].clone();
        }

        assert_eq!(ctx["stage"]["kind"].as_str(), Some("ApprovalGate"));
        let gate_id = ctx["stage"]["id"].as_str().unwrap();
        let (ok_bypass, _err_bypass) = run_sm(&[
            "advance",
            "--district",
            "DIST-002",
            "--stage",
            gate_id,
        ]);
        assert!(!ok_bypass);

        let mut rework_sig = LinearSignalV1::afterlife_event(SignalAction::Reworked, "JOB-099", 50);
        rework_sig.district_id = Some("DIST-002".into());
        rework_sig.stage_id = Some("shape".into());
        rework_sig.reason = Some("Clarify acceptance criteria".into());
        comments.push(serde_json::json!({
            "body": format_afterlife_comment("**Afterlife Rework** -> `shape`", None, &rework_sig)
        }));

        let comments_file = tmp.join("comments.json");
        fs::write(&comments_file, serde_json::to_vec_pretty(&comments).unwrap()).unwrap();
        let (ok_rep, rep_out) = run_sm(&["replay", "--file", comments_file.to_str().unwrap()]);
        assert!(ok_rep, "replay failed: {rep_out}");
        assert_eq!(rep_out["location"]["district_id"].as_str(), Some("DIST-002"));
        assert_eq!(rep_out["location"]["stage_id"].as_str(), Some("shape"));
        assert_eq!(
            rep_out["last_rework_reason"].as_str(),
            Some("Clarify acceptance criteria")
        );
        assert_eq!(rep_out["current_context"]["hard_stop"].as_bool(), Some(false));

        let mut handoff_comments = comments[..comments.len() - 1].to_vec();
        let mut approved_sig =
            LinearSignalV1::afterlife_event(SignalAction::Approved, "JOB-099", 51);
        approved_sig.district_id = Some("DIST-002".into());
        approved_sig.stage_id = Some("shaped".into());
        handoff_comments.push(serde_json::json!({
            "body": format_afterlife_comment("**Afterlife Approved** -> `shaped`", None, &approved_sig)
        }));
        let (ok_enter_d1, enter_d1_out) = run_sm(&["enter", "--district", "DIST-001"]);
        assert!(ok_enter_d1, "enter DIST-001 failed: {enter_d1_out}");
        handoff_comments.push(serde_json::json!({
            "body": enter_d1_out["comment_markdown"].as_str().unwrap()
        }));
        let handoff_file = tmp.join("handoff-comments.json");
        fs::write(
            &handoff_file,
            serde_json::to_vec_pretty(&handoff_comments).unwrap(),
        )
        .unwrap();
        let (ok_handoff, handoff_out) =
            run_sm(&["replay", "--file", handoff_file.to_str().unwrap()]);
        assert!(ok_handoff, "handoff replay failed: {handoff_out}");
        assert_eq!(handoff_out["location"]["state"].as_str(), Some("in_district"));
        assert_eq!(handoff_out["location"]["district_id"].as_str(), Some("DIST-001"));
        assert_eq!(handoff_out["location"]["stage_id"].as_str(), Some("discover"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
