use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use afterlife_core::{
    AfterlifeEngine, ArtifactKind, District, OwnerId, StageDef, StageKind, StepFieldDef,
    SubMachineTemplate, SubMachineTemplateId, SubStageDef, default_step_fields,
};
use serde::{Deserialize, Serialize};

use crate::linear_signal::SIGNAL_FENCE_TAG;

pub const ENTRY_SKILL_SLUG: &str = "afterlife";
const MANIFEST_FILE: &str = "manifest.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedSkillFile {
    pub slug: String,
    pub relative_path: String,
    pub district_id: Option<String>,
    pub title: String,
    pub content: String,
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

fn artifact_kind_name(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::RawNote => "raw_note",
        ArtifactKind::Issue => "issue",
        ArtifactKind::Spec => "spec",
        ArtifactKind::SourceCode => "source_code",
        ArtifactKind::PullRequest => "pull_request",
        ArtifactKind::CiReport => "ci_report",
        ArtifactKind::Snapshot => "snapshot",
        ArtifactKind::VerificationReport => "verification_report",
    }
}

fn stage_kind_name(kind: StageKind) -> &'static str {
    match kind {
        StageKind::Work => "Work",
        StageKind::ApprovalGate => "ApprovalGate",
        StageKind::Terminal => "Terminal",
    }
}

fn slugify_ascii(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

pub fn district_skill_slug(district: &District) -> String {
    let id_slug = slugify_ascii(district.id.as_str());
    let cleaned_name = district
        .name
        .split_once('-')
        .map(|(left, right)| {
            if left.to_ascii_lowercase().contains("district") {
                right.trim()
            } else {
                district.name.as_str()
            }
        })
        .unwrap_or(district.name.as_str());
    let name_slug = slugify_ascii(cleaned_name);
    if name_slug.is_empty() {
        format!("afterlife-{id_slug}")
    } else {
        format!("afterlife-{id_slug}-{name_slug}")
    }
}

fn yaml_escape(input: &str) -> String {
    let single_line = input.replace(['\r', '\n'], " ");
    let escaped = single_line.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn write_fields_and_artifacts(
    out: &mut String,
    fields: &[StepFieldDef],
    required_artifacts: &[ArtifactKind],
    indent: &str,
) {
    let resolved = if fields.is_empty() {
        default_step_fields()
    } else {
        fields.to_vec()
    };
    let _ = writeln!(
        out,
        "{indent}- **Step Fields** (provide in JSON `fields` or `## <key>` Markdown headings in the Linear comment):"
    );
    for f in &resolved {
        let req = if f.required { "required" } else { "optional" };
        let _ = writeln!(
            out,
            "{indent}  - `{}` ({}, {}): {}",
            f.key, f.label, req, f.prompt
        );
    }
    if !required_artifacts.is_empty() {
        let names: Vec<&str> = required_artifacts
            .iter()
            .copied()
            .map(artifact_kind_name)
            .collect();
        let _ = writeln!(
            out,
            "{indent}- **Required Artifact Kinds**: `{}` (external URLs such as a PR link or the Linear comment URL itself satisfy this gate)",
            names.join("`, `")
        );
    }
}

fn write_sub_stage(out: &mut String, sub: &SubStageDef) {
    let _ = writeln!(out, "#### Sub-Stage `{}` — {}", sub.id, sub.label);
    let _ = writeln!(out, "- **Procedure**: {}", sub.procedure);
    if !sub.gate_checks.is_empty() {
        let _ = writeln!(out, "- **Sub-Gate Checks**:");
        for chk in &sub.gate_checks {
            let _ = writeln!(out, "  - {chk}");
        }
    }
    write_fields_and_artifacts(
        out,
        &sub.record_fields,
        &sub.required_artifact_kinds,
        "",
    );
    out.push('\n');
}

fn write_stage(
    out: &mut String,
    stage: &StageDef,
    templates: &HashMap<SubMachineTemplateId, SubMachineTemplate>,
) {
    let _ = writeln!(
        out,
        "### Stage `{}` — {} (`{}`)",
        stage.id,
        stage.label,
        stage_kind_name(stage.kind)
    );
    let _ = writeln!(
        out,
        "- **Runner**: {} (`{:?}`)",
        stage.runner, stage.runner_kind
    );
    let _ = writeln!(out, "- **Procedure**: {}", stage.procedure);
    if !stage.next.is_empty() {
        let nexts: Vec<&str> = stage.next.iter().map(|s| s.as_str()).collect();
        let _ = writeln!(out, "- **Next**: `{}`", nexts.join("`, `"));
    }
    if let Some(rework) = &stage.rework_to {
        let _ = writeln!(out, "- **Rework Target**: `{rework}`");
    }
    if let Some(rubric) = &stage.gate_rubric {
        let _ = writeln!(
            out,
            "- **Gate Rubric** (`requires_human_approval: {}`, `auto_verify_by_llm: {}`):",
            rubric.requires_human_approval, rubric.auto_verify_by_llm
        );
        for chk in &rubric.checks {
            let _ = writeln!(out, "  - {chk}");
        }
    }
    if stage.kind == StageKind::ApprovalGate {
        out.push_str(
            "- **HARD STOP**: Human governance boundary. Never approve or advance past this stage. Stop work and wait for an `afterlife` signal (`approved` or `reworked`) in the Linear issue comments.\n\n",
        );
        return;
    }
    if stage.kind == StageKind::Terminal {
        out.push_str("- **Terminal Stage**: Job completion boundary.\n\n");
        return;
    }

    if let Some(tpl_id) = &stage.sub_template_id
        && let Some(tpl) = templates.get(tpl_id)
    {
        let _ = writeln!(
            out,
            "- **Embedded Sub-Machine**: `{}` — {} ({})",
            tpl.id, tpl.name, tpl.description
        );
        if !tpl.philosophy_notes.trim().is_empty() {
            let _ = writeln!(out, "- **Sub-Machine Notes**: {}", tpl.philosophy_notes);
        }
        out.push('\n');
        for sub in &tpl.stages {
            write_sub_stage(out, sub);
        }
    } else {
        write_fields_and_artifacts(
            out,
            &stage.record_fields,
            &stage.required_artifact_kinds,
            "",
        );
        out.push('\n');
    }
}

pub fn export_district_skill(
    district: &District,
    templates: &HashMap<SubMachineTemplateId, SubMachineTemplate>,
) -> ExportedSkillFile {
    let slug = district_skill_slug(district);
    let phil = district.active_philosophy();
    let spec = district.active_spec();
    let first_stage = spec
        .stages
        .first()
        .map(|s| s.id.as_str())
        .unwrap_or("start");
    let first_sub_stage = spec
        .stages
        .first()
        .and_then(|s| s.sub_template_id.as_ref())
        .and_then(|tid| templates.get(tid))
        .and_then(|tpl| tpl.stages.first())
        .map(|sub| sub.id.as_str());

    let description = format!(
        "Execute a Linear issue under Afterlife District {} ({}) using its embedded Philosophy, State Machine, Gate Rubric, and Linear signal protocol.",
        district.id, district.name
    );

    let mut out = String::new();
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "name: {slug}");
    let _ = writeln!(out, "description: {}", yaml_escape(&description));
    let _ = writeln!(out, "---\n");
    let _ = writeln!(out, "# {} — {}\n", district.id, district.name);
    let _ = writeln!(
        out,
        "- **District ID**: `{}`\n- **Philosophy Version**: `v{}`\n- **Procedure Spec Version**: `v{}`\n- **Setup By**: {}\n",
        district.id, phil.version, spec.version, district.setup_by
    );

    out.push_str("## Philosophy\n\n");
    let _ = writeln!(out, "{}\n", phil.summary);
    if !phil.principles.is_empty() {
        out.push_str("### Principles\n\n");
        for p in &phil.principles {
            let _ = writeln!(
                out,
                "- **{} — {}**: {} *(Why: {})*",
                p.id, p.title, p.statement, p.rationale
            );
        }
        out.push('\n');
    }
    if !phil.anti_patterns.is_empty() {
        out.push_str("### Anti-Patterns\n\n");
        for ap in &phil.anti_patterns {
            let _ = writeln!(out, "- {ap}");
        }
        out.push('\n');
    }
    if !phil.decision_heuristics.is_empty() {
        out.push_str("### Decision Heuristics\n\n");
        for h in &phil.decision_heuristics {
            let _ = writeln!(out, "- {h}");
        }
        out.push('\n');
    }

    out.push_str("## Embedded State Machine\n\n");
    let stage_flow: Vec<String> = spec
        .stages
        .iter()
        .map(|s| format!("`{}` ({})", s.id, stage_kind_name(s.kind)))
        .collect();
    let _ = writeln!(out, "**Flow**: {}\n", stage_flow.join(" -> "));

    for stage in &spec.stages {
        write_stage(&mut out, stage, templates);
    }

    out.push_str("## Linear Signal Protocol\n\n");
    out.push_str(
        "Post a Linear comment containing a fenced `afterlife-signal` JSON block at each transition. The local Afterlife Linear sync bridge reads comments on started tickets, validates every contract rule fail-closed, attaches the Linear comment URL (plus any PR links you provide) as stage evidence, and posts human `approved`, `reworked`, `stopped`, or `sync_error` responses back to the same Linear issue.\n\n",
    );
    out.push_str("### 1. Entering this District\n\n");
    let _ = writeln!(out, "```{SIGNAL_FENCE_TAG}");
    let _ = writeln!(
        out,
        "{{\n  \"version\": 1,\n  \"source\": \"agent\",\n  \"action\": \"enter_district\",\n  \"district_id\": \"{}\",\n  \"runner_label\": \"Cloud Agent\",\n  \"summary\": \"Entering {} at stage {}\"\n}}",
        district.id, district.id, first_stage
    );
    out.push_str("```\n\n");

    out.push_str("### 2. Advancing a Stage / Sub-Stage\n\n");
    out.push_str(
        "Put long-form step field content under `## <field_key>` Markdown headings in the same Linear comment (or inside JSON `fields`). Include `gate` whenever the current stage has a Gate Rubric, and include PR links under `artifacts` (`kind: \"pull_request\"`) before entering a delivery ApprovalGate.\n\n",
    );
    let _ = writeln!(out, "```{SIGNAL_FENCE_TAG}");
    let sub_line = match first_sub_stage {
        Some(sub) => format!("  \"sub_stage_id\": \"{sub}\",\n"),
        None => String::new(),
    };
    let _ = writeln!(
        out,
        "{{\n  \"version\": 1,\n  \"source\": \"agent\",\n  \"action\": \"advance_stage\",\n  \"district_id\": \"{}\",\n  \"stage_id\": \"{}\",\n{}  \"runner_label\": \"Cloud Agent\",\n  \"summary\": \"Completed stage with verified evidence\",\n  \"gate\": {{\n    \"verdict\": \"pass\",\n    \"summary\": \"All rubric checks verified\",\n    \"checks\": [[\"Rubric check\", true]]\n  }},\n  \"artifacts\": []\n}}",
        district.id, first_stage, sub_line
    );
    out.push_str("```\n\n");

    out.push_str("### 3. Governance Boundary & Rework\n\n");
    out.push_str(
        "- Stop immediately upon reaching any `ApprovalGate` stage.\n- Before acting on an existing Linear issue, read its comments in order and inspect the latest `afterlife-signal` block.\n- If the latest signal has `\"source\": \"afterlife\"` and `\"action\": \"reworked\"`, resume from its `stage_id` addressing `reason`.\n- If the latest signal has `\"source\": \"afterlife\"` and `\"action\": \"sync_error\"`, resolve the contract error in `reason` and post a corrected `advance_stage` signal.\n- If the latest signal has `\"source\": \"afterlife\"` and `\"action\": \"stopped\"`, pause work until a `resumed` signal appears.\n",
    );

    ExportedSkillFile {
        relative_path: format!("{slug}/SKILL.md"),
        slug,
        district_id: Some(district.id.as_str().to_string()),
        title: district.name.clone(),
        content: out,
    }
}

pub fn export_entry_skill(districts: &[District]) -> ExportedSkillFile {
    let description = "Work on a Linear issue under Afterlife's Districts, Philosophy, and State Machines by selecting or resuming the matching District skill and posting afterlife-signal comments to Linear.";

    let mut out = String::new();
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "name: {ENTRY_SKILL_SLUG}");
    let _ = writeln!(out, "description: {}", yaml_escape(description));
    let _ = writeln!(out, "---\n");
    out.push_str("# Afterlife\n\n");
    out.push_str(
        "Follow Afterlife's versioned Philosophy, embedded State Machines, and human ApprovalGate boundaries when executing Linear tickets from environments that cannot reach the local Afterlife daemon directly.\n\n",
    );
    out.push_str("## Execution Protocol\n\n");
    out.push_str(
        "1. **Inspect the Linear issue comments in chronological order** and locate the latest ` ```afterlife-signal ` JSON block:\n   - **No signal yet**: Select the fitting District below, post an `enter_district` signal comment, and follow that District's skill.\n   - **`source: \"agent\"`**: Continue from the next stage/sub-stage in that District's State Machine.\n   - **`source: \"afterlife\"`, `action: \"reworked\"`**: Resume from `stage_id` and address the human `reason`.\n   - **`source: \"afterlife\"`, `action: \"sync_error\"`**: Fix the missing field, gate check, or stage mismatch reported in `reason` and post a corrected signal.\n   - **`source: \"afterlife\"`, `action: \"stopped\"`**: Pause work until a `resumed` signal appears.\n   - **`source: \"afterlife\"`, `action: \"approved\"`**: Verify whether the ticket reached a Terminal stage or entered a downstream stage.\n2. **Advance step by step.** Never skip stages or sub-stages. Provide every required step field via `## <field_key>` Markdown headings or JSON `fields`, and include `gate` whenever a stage has a Gate Rubric.\n3. **Always stop at every `ApprovalGate`.** Never approve or advance past a human ApprovalGate. Attach a `pull_request` artifact before entering a merge/ship ApprovalGate.\n\n",
    );

    out.push_str("## Available Districts\n\n");
    for district in districts {
        let slug = district_skill_slug(district);
        let phil = district.active_philosophy();
        let flow: Vec<&str> = district
            .active_spec()
            .stages
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        let _ = writeln!(
            out,
            "### `{}` — {} (`{slug}`)\n- **Skill File**: `{slug}/SKILL.md`\n- **Philosophy (v{})**: {}\n- **Stages (v{})**: `{}`\n",
            district.id,
            district.name,
            phil.version,
            phil.summary,
            district.active_spec_version,
            flow.join("` -> `")
        );
    }

    out.push_str("## Signal Format Reference\n\n");
    let _ = writeln!(out, "```{SIGNAL_FENCE_TAG}");
    out.push_str("{\n  \"version\": 1,\n  \"source\": \"agent\",\n  \"action\": \"enter_district | advance_stage | attach_artifact\",\n  \"district_id\": \"DIST-001\",\n  \"stage_id\": \"discover\",\n  \"sub_stage_id\": \"trace_sources\",\n  \"runner_label\": \"Cloud Agent\",\n  \"summary\": \"One-line outcome summary\",\n  \"fields\": {},\n  \"gate\": {\n    \"verdict\": \"pass\",\n    \"summary\": \"Verification summary\",\n    \"checks\": [[\"Check description\", true]]\n  },\n  \"artifacts\": [\n    {\n      \"kind\": \"pull_request\",\n      \"label\": \"org/repo#123\",\n      \"uri\": \"https://github.com/org/repo/pull/123\"\n    }\n  ]\n}\n");
    out.push_str("```\n");

    ExportedSkillFile {
        slug: ENTRY_SKILL_SLUG.to_string(),
        relative_path: format!("{ENTRY_SKILL_SLUG}/SKILL.md"),
        district_id: None,
        title: "Afterlife".to_string(),
        content: out,
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

    let mut skills = Vec::with_capacity(active.len() + 1);
    if district_filter.is_none() {
        skills.push(export_entry_skill(&active));
    }
    for district in &active {
        skills.push(export_district_skill(district, &engine.templates));
    }
    SkillExportBundle { skills }
}

pub fn export_builtin_skills(district_filter: Option<&str>) -> SkillExportBundle {
    let engine = AfterlifeEngine::new_for_owner(OwnerId::from("builtin"));
    export_workspace_skills(&engine, district_filter)
}

fn atomic_write_if_changed(path: &Path, content: &[u8]) -> Result<bool, String> {
    if let Ok(existing) = fs::read(path)
        && existing == content
    {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create dir {}: {e}", parent.display()))?;
    }
    let tmp = PathBuf::from(format!("{}.tmp.{}", path.display(), std::process::id()));
    fs::write(&tmp, content).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("rename {} -> {}: {e}", tmp.display(), path.display())
    })?;
    Ok(true)
}

pub fn write_skill_bundle_atomic(
    out_dir: &Path,
    bundle: &SkillExportBundle,
    prune_stale: bool,
) -> Result<SkillWriteReport, String> {
    fs::create_dir_all(out_dir)
        .map_err(|e| format!("create skills dir {}: {e}", out_dir.display()))?;

    let manifest_path = out_dir.join(MANIFEST_FILE);
    let previous_slugs: Vec<String> = fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<SkillManifest>(&raw).ok())
        .map(|m| m.skills.into_iter().map(|e| e.slug).collect())
        .unwrap_or_default();

    let mut written = Vec::new();
    let mut unchanged = Vec::new();
    let mut current_slugs = HashSet::new();

    for skill in &bundle.skills {
        current_slugs.insert(skill.slug.clone());
        let target = out_dir.join(&skill.relative_path);
        if atomic_write_if_changed(&target, skill.content.as_bytes())? {
            written.push(skill.relative_path.clone());
        } else {
            unchanged.push(skill.relative_path.clone());
        }
    }

    let mut removed = Vec::new();
    if prune_stale {
        for old_slug in previous_slugs {
            if !current_slugs.contains(&old_slug)
                && (old_slug == ENTRY_SKILL_SLUG || old_slug.starts_with("afterlife-"))
            {
                let old_dir = out_dir.join(&old_slug);
                if old_dir.is_dir() && fs::remove_dir_all(&old_dir).is_ok() {
                    removed.push(old_slug);
                }
            }
        }
    }

    let manifest = SkillManifest {
        version: 1,
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
    let _ = atomic_write_if_changed(&manifest_path, &manifest_bytes)?;

    Ok(SkillWriteReport {
        out_dir: out_dir.display().to_string(),
        written,
        unchanged,
        removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_entry_and_all_active_district_skills() {
        let bundle = export_builtin_skills(None);
        assert!(bundle.skills.len() >= 2);
        assert_eq!(bundle.skills[0].slug, ENTRY_SKILL_SLUG);
        assert_eq!(bundle.skills[0].relative_path, "afterlife/SKILL.md");
        assert!(bundle.skills[0].content.contains("enter_district"));
        assert!(bundle.skills[0].content.contains("ApprovalGate"));

        let dist1 = bundle
            .skills
            .iter()
            .find(|s| s.district_id.as_deref() == Some("DIST-001"))
            .expect("DIST-001 skill exported");
        assert!(dist1.slug.starts_with("afterlife-dist-001"));
        assert!(dist1.content.contains("## Philosophy"));
        assert!(dist1.content.contains("## Embedded State Machine"));
        assert!(dist1.content.contains("```afterlife-signal"));
        assert!(dist1.content.contains("HARD STOP"));
    }

    #[test]
    fn atomic_writer_is_idempotent_and_prunes_archived_districts() {
        let tmp = std::env::temp_dir().join(format!(
            "afterlife-skill-export-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);

        let mut engine = AfterlifeEngine::new_for_owner(OwnerId::from("owner-a"));
        let bundle1 = export_workspace_skills(&engine, None);
        let first = write_skill_bundle_atomic(&tmp, &bundle1, true).unwrap();
        assert_eq!(first.written.len(), bundle1.skills.len());
        assert!(first.unchanged.is_empty());

        let second = write_skill_bundle_atomic(&tmp, &bundle1, true).unwrap();
        assert!(second.written.is_empty());
        assert_eq!(second.unchanged.len(), bundle1.skills.len());

        let archived_slug = district_skill_slug(&engine.districts[0]);
        engine.districts[0].archived = true;
        let bundle2 = export_workspace_skills(&engine, None);
        let third = write_skill_bundle_atomic(&tmp, &bundle2, true).unwrap();
        assert!(third.removed.contains(&archived_slug));
        assert!(!tmp.join(&archived_slug).exists());

        let _ = fs::remove_dir_all(&tmp);
    }
}
