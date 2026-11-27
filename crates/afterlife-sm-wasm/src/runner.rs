use std::collections::{BTreeMap, HashMap};

use afterlife_core::{
    Actor, AfterlifeEngine, ArtifactId, ArtifactKind, District, DistrictId, EngineCommand,
    ExecutionContext, FencingToken, JobEvent, JobId, JobLocation, OwnerId, RunnerId, StageId,
    StageKind, SubMachineTemplate, SubMachineTemplateId, SubStageId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    replay::handle_replay,
    signal::{
        LinearSignalV1, SIGNAL_SCHEMA_VERSION, SignalAction, SignalArtifact, SignalGateReport,
        SignalSource, format_agent_step_comment,
    },
};

const SYNTHETIC_EVIDENCE_URI: &str = "https://linear.app/afterlife/evidence";
pub(crate) const BASE_TS: u64 = 1_800_000_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DistrictPackage {
    pub schema_version: u32,
    pub district: District,
    #[serde(default)]
    pub templates: HashMap<SubMachineTemplateId, SubMachineTemplate>,
}

#[derive(Debug, Deserialize)]
pub struct SmRequest {
    pub districts: Vec<DistrictPackage>,
    #[serde(flatten)]
    pub op: SmOp,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SmOp {
    List,
    Context {
        district_id: String,
        #[serde(default)]
        stage_id: Option<String>,
        #[serde(default)]
        sub_stage_id: Option<String>,
    },
    Enter {
        district_id: String,
        #[serde(default)]
        runner_label: Option<String>,
        #[serde(default)]
        summary: Option<String>,
    },
    Advance {
        district_id: String,
        stage_id: String,
        #[serde(default)]
        sub_stage_id: Option<String>,
        #[serde(default)]
        fields: BTreeMap<String, String>,
        #[serde(default)]
        gate: Option<SignalGateReport>,
        #[serde(default)]
        artifacts: Vec<SignalArtifact>,
        #[serde(default)]
        runner_label: Option<String>,
        #[serde(default)]
        summary: Option<String>,
    },
    Replay {
        comments: Vec<String>,
    },
}

fn build_engine(packages: Vec<DistrictPackage>) -> (AfterlifeEngine, Actor, Actor) {
    let owner = packages
        .first()
        .map(|p| p.district.owner_id.clone())
        .unwrap_or_else(|| OwnerId::from("local-owner"));
    let mut engine = AfterlifeEngine::new_for_owner(owner.clone());
    engine.jobs.clear();
    engine.feedbacks.clear();
    engine.event_log.clear();

    if !packages.is_empty() {
        engine.districts.clear();
        engine.templates.clear();
        engine.template_order.clear();
        for pkg in packages {
            for (tid, tpl) in pkg.templates {
                if !engine.templates.contains_key(&tid) {
                    engine.template_order.push(tid.clone());
                }
                engine.templates.insert(tid, tpl);
            }
            engine.districts.push(pkg.district);
        }
    }

    let agent = Actor::Agent {
        owner_id: owner.clone(),
        client_id: "sm-wasm".into(),
        runner_label: "Cloud Agent".into(),
        token_jti: "sm-wasm-session".into(),
    };
    let human = Actor::Human {
        owner_id: owner,
        email: "you@local".into(),
    };
    (engine, agent, human)
}

pub(crate) fn queue_scratch_job(engine: &mut AfterlifeEngine, agent: &Actor, at: u64) -> Result<JobId, String> {
    let events = engine
        .apply(
            agent,
            EngineCommand::QueueJob {
                title: "Linear Task".into(),
                raw_input: "Synced via Afterlife embedded WASM State Machine".into(),
                target_district_hint: None,
                at,
            },
        )
        .map_err(|e| e.to_string())?;
    events
        .into_iter()
        .find_map(|ev| match ev {
            JobEvent::JobQueued { job_id, .. } => Some(job_id),
            _ => None,
        })
        .ok_or_else(|| "queue produced no JobQueued event".to_string())
}

fn position_job(
    engine: &mut AfterlifeEngine,
    agent: &Actor,
    district_id: &str,
    stage_id: Option<&str>,
    sub_stage_id: Option<&str>,
    at: u64,
) -> Result<JobId, String> {
    let job_id = queue_scratch_job(engine, agent, at)?;
    let did = DistrictId::from(district_id.to_string());
    engine
        .apply(
            agent,
            EngineCommand::AssignJobToDistrict {
                job_id: job_id.clone(),
                district_id: did.clone(),
                expected_revision: None,
                at: at + 1,
            },
        )
        .map_err(|e| e.to_string())?;

    if let Some(target_stage_raw) = stage_id {
        let target_stage_id = StageId::from(target_stage_raw.to_string());
        let district = engine
            .find_district(&did)
            .ok_or_else(|| format!("unknown district_id '{district_id}'"))?
            .clone();
        let stage = district
            .active_spec()
            .stages
            .iter()
            .find(|s| s.id == target_stage_id)
            .ok_or_else(|| format!("unknown stage_id '{target_stage_raw}' in {district_id}"))?;

        let resolved_sub = match &stage.sub_template_id {
            Some(tid) if stage.kind == StageKind::Work => {
                let tpl = engine
                    .templates
                    .get(tid)
                    .ok_or_else(|| format!("missing sub-machine template '{tid}'"))?;
                match sub_stage_id {
                    Some(sub_raw) => {
                        let sid = SubStageId::from(sub_raw.to_string());
                        if !tpl.stages.iter().any(|s| s.id == sid) {
                            return Err(format!(
                                "unknown sub_stage_id '{sub_raw}' in {district_id}/{target_stage_raw}"
                            ));
                        }
                        Some(sid)
                    }
                    None => tpl.stages.first().map(|s| s.id.clone()),
                }
            }
            _ => {
                if let Some(sub_raw) = sub_stage_id {
                    return Err(format!(
                        "stage '{target_stage_raw}' has no sub-stages (got '{sub_raw}')"
                    ));
                }
                None
            }
        };

        let job = engine
            .jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .ok_or_else(|| "scratch job missing".to_string())?;
        job.location = if stage.kind == StageKind::Terminal {
            JobLocation::Completed {
                district_id: did,
                terminal_stage_id: target_stage_id,
                completed_at: at + 2,
            }
        } else {
            JobLocation::InDistrict {
                district_id: did,
                stage_id: target_stage_id,
                sub_stage_id: resolved_sub,
            }
        };
    }

    Ok(job_id)
}

pub(crate) fn format_context_view(engine: &AfterlifeEngine, job_id: &JobId) -> Value {
    let Some(job) = engine.find_job(job_id) else {
        return Value::Null;
    };
    if let JobLocation::Completed {
        district_id,
        terminal_stage_id,
        ..
    } = &job.location
    {
        return json!({
            "district_id": district_id,
            "stage_id": terminal_stage_id,
            "stage_kind": "Terminal",
            "hard_stop": true,
            "instruction": "Terminal stage reached. Job is complete.",
        });
    }
    let Some(ctx) = engine.compile_execution_context(job_id) else {
        return Value::Null;
    };
    context_to_json(engine, &ctx)
}

fn context_to_json(engine: &AfterlifeEngine, ctx: &ExecutionContext) -> Value {
    let is_stage_exit = match (&ctx.stage.sub_template_id, &ctx.sub_stage) {
        (Some(tid), Some(sub)) => engine
            .templates
            .get(tid)
            .and_then(|t| t.stages.last())
            .is_some_and(|last| last.id == sub.id),
        _ => true,
    };
    let hard_stop = matches!(
        ctx.stage.kind,
        StageKind::ApprovalGate | StageKind::Terminal
    );
    let requires_gate =
        is_stage_exit && ctx.gate_rubric.is_some() && ctx.stage.kind == StageKind::Work;
    let instruction = match ctx.stage.kind {
        StageKind::ApprovalGate => {
            "HARD STOP: Human ApprovalGate. Do not advance. Wait for an afterlife approved or reworked signal in Linear."
        }
        StageKind::Terminal => "Terminal stage reached. Job is complete.",
        StageKind::Work => {
            "Execute this single step, then run `sm.mjs advance` to validate via afterlife-core WASM and generate the Linear signal comment."
        }
    };
    json!({
        "district_id": ctx.district_id,
        "district_name": ctx.district_name,
        "stopped": ctx.stopped,
        "stopped_reason": ctx.stopped_reason,
        "philosophy": ctx.philosophy,
        "stage": ctx.stage,
        "sub_stage": ctx.sub_stage,
        "gate_rubric": ctx.gate_rubric,
        "required_step_fields": ctx.required_step_fields,
        "required_artifact_kinds": ctx.required_artifact_kinds,
        "is_stage_exit": is_stage_exit,
        "requires_gate_verification": requires_gate,
        "hard_stop": hard_stop,
        "instruction": instruction,
    })
}

pub(crate) fn claim_and_advance_in_engine(
    engine: &mut AfterlifeEngine,
    agent: &Actor,
    job_id: &JobId,
    fields: BTreeMap<String, String>,
    gate: Option<SignalGateReport>,
    artifacts: &[SignalArtifact],
    runner_label: &str,
    at: u64,
) -> Result<(), String> {
    let ctx = engine
        .compile_execution_context(job_id)
        .ok_or_else(|| "job is not in an active district stage".to_string())?;

    let claim_events = engine
        .apply(
            agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("sm-wasm-runner"),
                runner_label: runner_label.to_string(),
                ttl_ms: 60_000,
                expected_revision: None,
                at,
            },
        )
        .map_err(|e| e.to_string())?;
    let token: FencingToken = claim_events
        .iter()
        .find_map(|ev| match ev {
            JobEvent::JobClaimed { fencing_token, .. } => Some(*fencing_token),
            _ => None,
        })
        .ok_or_else(|| "claim produced no fencing_token".to_string())?;

    let mut attached: Vec<(ArtifactId, ArtifactKind)> = Vec::new();
    for art in artifacts {
        let evs = engine
            .apply(
                agent,
                EngineCommand::AttachArtifact {
                    job_id: job_id.clone(),
                    kind: art.kind,
                    label: art.label.clone(),
                    uri: art.uri.clone(),
                    sha256: art.sha256.clone(),
                    fencing_token: Some(token),
                    at: at + 1,
                },
            )
            .map_err(|e| e.to_string())?;
        if let Some(aid) = evs.iter().find_map(|ev| match ev {
            JobEvent::ArtifactAttached { artifact_id, .. } => Some(artifact_id.clone()),
            _ => None,
        }) {
            attached.push((aid, art.kind));
        }
    }

    if !ctx.required_artifact_kinds.is_empty()
        && !attached
            .iter()
            .any(|(_, k)| ctx.required_artifact_kinds.contains(k))
    {
        let fallback_kind = ctx.required_artifact_kinds[0];
        let evs = engine
            .apply(
                agent,
                EngineCommand::AttachArtifact {
                    job_id: job_id.clone(),
                    kind: fallback_kind,
                    label: "Linear comment evidence".into(),
                    uri: SYNTHETIC_EVIDENCE_URI.into(),
                    sha256: None,
                    fencing_token: Some(token),
                    at: at + 2,
                },
            )
            .map_err(|e| e.to_string())?;
        if let Some(aid) = evs.iter().find_map(|ev| match ev {
            JobEvent::ArtifactAttached { artifact_id, .. } => Some(artifact_id.clone()),
            _ => None,
        }) {
            attached.push((aid, fallback_kind));
        }
    }

    if let Some(g) = gate {
        engine
            .apply(
                agent,
                EngineCommand::RecordGateVerification {
                    job_id: job_id.clone(),
                    verdict: g.verdict,
                    summary: g.summary,
                    checklist_results: g.checks,
                    fencing_token: Some(token),
                    at: at + 3,
                },
            )
            .map_err(|e| e.to_string())?;
    }

    let artifact_ids: Vec<ArtifactId> = attached.into_iter().map(|(id, _)| id).collect();
    engine
        .apply(
            agent,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields,
                artifact_ids,
                expected_revision: None,
                fencing_token: Some(token),
                at: at + 4,
            },
        )
        .map_err(|e| e.to_string())?;
    let _ = engine.apply(
        agent,
        EngineCommand::ReleaseLease {
            job_id: job_id.clone(),
            fencing_token: token,
            at: at + 5,
        },
    );
    Ok(())
}

pub(crate) fn location_json(loc: &JobLocation) -> Value {
    match loc {
        JobLocation::Queue => json!({ "state": "queue" }),
        JobLocation::InDistrict {
            district_id,
            stage_id,
            sub_stage_id,
        } => json!({
            "state": "in_district",
            "district_id": district_id,
            "stage_id": stage_id,
            "sub_stage_id": sub_stage_id,
        }),
        JobLocation::Completed {
            district_id,
            terminal_stage_id,
            ..
        } => json!({
            "state": "completed",
            "district_id": district_id,
            "stage_id": terminal_stage_id,
            "sub_stage_id": Value::Null,
        }),
        JobLocation::Archived => json!({ "state": "archived" }),
    }
}

pub fn handle_request(req: SmRequest) -> Result<Value, String> {
    let (mut engine, agent, human) = build_engine(req.districts);
    match req.op {
        SmOp::List => {
            let mut active: Vec<&District> =
                engine.districts.iter().filter(|d| !d.archived).collect();
            active.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
            let list: Vec<Value> = active
                .into_iter()
                .map(|d| {
                    let stages: Vec<Value> = d
                        .active_spec()
                        .stages
                        .iter()
                        .map(|s| {
                            let subs: Vec<&str> = s
                                .sub_template_id
                                .as_ref()
                                .and_then(|tid| engine.templates.get(tid))
                                .map(|tpl| tpl.stages.iter().map(|sub| sub.id.as_str()).collect())
                                .unwrap_or_default();
                            json!({
                                "id": s.id,
                                "kind": s.kind,
                                "sub_stages": subs,
                            })
                        })
                        .collect();
                    json!({
                        "district_id": d.id,
                        "name": d.name,
                        "philosophy_version": d.active_philosophy_version,
                        "spec_version": d.active_spec_version,
                        "philosophy_summary": d.active_philosophy().summary,
                        "stages": stages,
                    })
                })
                .collect();
            Ok(json!({ "ok": true, "districts": list }))
        }
        SmOp::Context {
            district_id,
            stage_id,
            sub_stage_id,
        } => {
            let job_id = position_job(
                &mut engine,
                &agent,
                &district_id,
                stage_id.as_deref(),
                sub_stage_id.as_deref(),
                BASE_TS,
            )?;
            let mut view = format_context_view(&engine, &job_id);
            if let Some(obj) = view.as_object_mut() {
                obj.insert("ok".into(), Value::Bool(true));
            }
            Ok(view)
        }
        SmOp::Enter {
            district_id,
            runner_label,
            summary,
        } => {
            let job_id = position_job(&mut engine, &agent, &district_id, None, None, BASE_TS)?;
            let district_name = engine
                .find_district(&DistrictId::from(district_id.clone()))
                .map(|d| d.name.clone())
                .unwrap_or_else(|| district_id.clone());
            let runner = runner_label
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Cloud Agent")
                .to_string();
            let loc = engine.find_job(&job_id).unwrap().location.clone();
            let first_stage = match &loc {
                JobLocation::InDistrict { stage_id, .. } => stage_id.as_str().to_string(),
                _ => "start".into(),
            };
            let summary_text = summary
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("Entering {district_id} at {first_stage}"));
            let signal = LinearSignalV1 {
                version: SIGNAL_SCHEMA_VERSION,
                source: SignalSource::Agent,
                action: SignalAction::EnterDistrict,
                district_id: Some(district_id.clone()),
                stage_id: None,
                sub_stage_id: None,
                target_stage_id: None,
                runner_label: Some(runner),
                summary: Some(summary_text.clone()),
                fields: BTreeMap::new(),
                gate: None,
                artifacts: Vec::new(),
                afterlife_job_id: None,
                afterlife_seq: None,
                reason: None,
            };
            let headline = format!("**Afterlife Enter** `{district_id}` ({district_name})");
            let comment =
                format_agent_step_comment(&headline, Some(&summary_text), &BTreeMap::new(), &signal);
            Ok(json!({
                "ok": true,
                "comment_markdown": comment,
                "signal": signal,
                "current_location": location_json(&loc),
                "next_context": format_context_view(&engine, &job_id),
            }))
        }
        SmOp::Advance {
            district_id,
            stage_id,
            sub_stage_id,
            fields,
            gate,
            artifacts,
            runner_label,
            summary,
        } => {
            let job_id = position_job(
                &mut engine,
                &agent,
                &district_id,
                Some(&stage_id),
                sub_stage_id.as_deref(),
                BASE_TS,
            )?;
            let resolved_sub = match &engine.find_job(&job_id).unwrap().location {
                JobLocation::InDistrict { sub_stage_id, .. } => {
                    sub_stage_id.as_ref().map(|s| s.as_str().to_string())
                }
                _ => None,
            };
            let runner = runner_label
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Cloud Agent")
                .to_string();

            claim_and_advance_in_engine(
                &mut engine,
                &agent,
                &job_id,
                fields.clone(),
                gate.clone(),
                &artifacts,
                &runner,
                BASE_TS + 10,
            )?;

            let next_loc = engine.find_job(&job_id).unwrap().location.clone();
            let next_ctx = format_context_view(&engine, &job_id);
            let hard_stop = next_ctx
                .get("hard_stop")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let signal = LinearSignalV1 {
                version: SIGNAL_SCHEMA_VERSION,
                source: SignalSource::Agent,
                action: SignalAction::AdvanceStage,
                district_id: Some(district_id.clone()),
                stage_id: Some(stage_id.clone()),
                sub_stage_id: resolved_sub.clone(),
                target_stage_id: None,
                runner_label: Some(runner),
                summary: summary.clone(),
                fields: BTreeMap::new(),
                gate,
                artifacts,
                afterlife_job_id: None,
                afterlife_seq: None,
                reason: None,
            };
            let step_label = match &resolved_sub {
                Some(sub) => format!("{stage_id}/{sub}"),
                None => stage_id.clone(),
            };
            let headline = format!("**Afterlife Advance** `{district_id}` `{step_label}`");
            let comment =
                format_agent_step_comment(&headline, summary.as_deref(), &fields, &signal);

            Ok(json!({
                "ok": true,
                "comment_markdown": comment,
                "signal": signal,
                "next_location": location_json(&next_loc),
                "hard_stop": hard_stop,
                "next_context": next_ctx,
            }))
        }
        SmOp::Replay { comments } => handle_replay(&mut engine, &agent, &human, comments),
    }
}
