use afterlife_core::{
    Actor, ArtifactId, ArtifactKind, CommandId, DistrictId, EngineCommand, FencingToken, Job,
    JobEvent, JobId, JobLocation, OwnerId, RunnerId, StageDef, StageKind, SubStageDef,
    TimestampMs,
};
use afterlife_protocol::{
    CommandEnvelope, EventRecord, LinearIssueLink, ParsedLinearComment, PortError, SignalAction,
};

use crate::linear::{
    client::{LinearComment, LinearIssue},
    links::{canonical_linear_url, spawn_followup_job_for_issue},
    sync::{LINEAR_SYNC_CLIENT_ID, LinearSyncContext},
};

const LINEAR_SYNC_LEASE_TTL_MS: u64 = 30_000;

pub enum SignalApplyOutcome {
    Applied,
    DeferredLease,
    Error(String),
}

pub fn dispatch_and_publish(
    ctx: &LinearSyncContext,
    command_id: String,
    actor: Actor,
    command: EngineCommand,
) -> Result<Vec<JobEvent>, PortError> {
    let cmd_id = CommandId::from(command_id);
    let owner_id = actor.owner_id().clone();
    let outcome = ctx.store.execute_command(CommandEnvelope {
        command_id: cmd_id.clone(),
        actor,
        command,
    })?;
    if !outcome.idempotent_replay {
        for (offset, ev) in outcome.events.iter().enumerate() {
            ctx.event_bus.publish(EventRecord {
                seq: outcome.seq_start + offset as u64,
                command_id: cmd_id.clone(),
                owner_id: owner_id.clone(),
                event: ev.clone(),
            });
        }
    }
    Ok(outcome.events)
}

fn find_current_stage_and_sub<'a>(
    snapshot: &'a afterlife_protocol::WorkspaceSnapshot,
    job: &Job,
) -> Option<(&'a StageDef, Option<&'a SubStageDef>)> {
    let JobLocation::InDistrict {
        district_id,
        stage_id,
        sub_stage_id,
    } = &job.location
    else {
        return None;
    };
    let district = snapshot.engine.find_district(district_id)?;
    let spec = job
        .pinned_spec_version
        .and_then(|v| district.spec_at(v))
        .unwrap_or_else(|| district.active_spec());
    let stage = spec.stages.iter().find(|s| &s.id == stage_id)?;
    let sub = sub_stage_id.as_ref().and_then(|sid| {
        stage
            .sub_template_id
            .as_ref()
            .and_then(|tid| snapshot.engine.templates.get(tid))
            .and_then(|tpl| tpl.stages.iter().find(|sub| &sub.id == sid))
    });
    Some((stage, sub))
}

fn claim_short_lease(
    ctx: &LinearSyncContext,
    actor: &Actor,
    job_id: &JobId,
    comment_id: &str,
    runner_label: &str,
    now: TimestampMs,
) -> Result<FencingToken, PortError> {
    let events = dispatch_and_publish(
        ctx,
        format!("linear:cmt:{comment_id}:claim:{now}"),
        actor.clone(),
        EngineCommand::ClaimJob {
            job_id: job_id.clone(),
            runner_id: RunnerId::from(format!("linear-sync:{comment_id}")),
            runner_label: runner_label.to_string(),
            ttl_ms: LINEAR_SYNC_LEASE_TTL_MS,
            expected_revision: None,
            at: now,
        },
    )?;
    events
        .iter()
        .find_map(|ev| match ev {
            JobEvent::JobClaimed { fencing_token, .. } => Some(*fencing_token),
            _ => None,
        })
        .ok_or_else(|| PortError::InvalidInput("claim produced no JobClaimed event".into()))
}

fn release_lease_best_effort(
    ctx: &LinearSyncContext,
    actor: &Actor,
    job_id: &JobId,
    comment_id: &str,
    token: FencingToken,
    now: TimestampMs,
) {
    let _ = dispatch_and_publish(
        ctx,
        format!("linear:cmt:{comment_id}:release:{now}"),
        actor.clone(),
        EngineCommand::ReleaseLease {
            job_id: job_id.clone(),
            fencing_token: token,
            at: now,
        },
    );
}

#[allow(clippy::too_many_arguments)]
pub fn apply_agent_signal(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    link: &mut LinearIssueLink,
    issue: &LinearIssue,
    comment: &LinearComment,
    parsed: &ParsedLinearComment,
    default_district_id: Option<&str>,
    now: TimestampMs,
) -> SignalApplyOutcome {
    let snapshot = match ctx.store.load_workspace(owner_id) {
        Ok(s) => s,
        Err(e) => return SignalApplyOutcome::Error(format!("load workspace failed: {e}")),
    };
    let Some(job) = snapshot.engine.find_job(&link.job_id).cloned() else {
        return SignalApplyOutcome::Error(format!("linked job {} not found", link.job_id));
    };
    if job.stopped && !matches!(job.location, JobLocation::Archived) {
        return SignalApplyOutcome::Error(
            "job is currently stopped by human governance; wait for a resumed signal".into(),
        );
    }
    if let Some(ref lease) = job.lease
        && lease.is_active_at(now)
        && lease.holder_client_id.as_deref() != Some(LINEAR_SYNC_CLIENT_ID)
    {
        return SignalApplyOutcome::DeferredLease;
    }

    let runner_label = parsed
        .signal
        .runner_label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(comment.author_name.as_deref())
        .unwrap_or("Cloud Agent")
        .to_string();

    let actor = Actor::Agent {
        owner_id: owner_id.clone(),
        client_id: LINEAR_SYNC_CLIENT_ID.into(),
        runner_label: runner_label.clone(),
        token_jti: format!("linear-comment:{}", comment.id),
    };

    match parsed.signal.action {
        SignalAction::EnterDistrict => apply_enter_district(
            ctx,
            owner_id,
            &actor,
            &job,
            link,
            issue,
            &comment.id,
            parsed,
            default_district_id,
            now,
        ),
        SignalAction::AttachArtifact => {
            let token =
                match claim_short_lease(ctx, &actor, &link.job_id, &comment.id, &runner_label, now) {
                    Ok(t) => t,
                    Err(PortError::Domain(afterlife_core::TransitionError::AlreadyLeased { .. })) => {
                        return SignalApplyOutcome::DeferredLease;
                    }
                    Err(e) => return SignalApplyOutcome::Error(e.to_string()),
                };
            let result = attach_signal_artifacts(
                ctx,
                &actor,
                &link.job_id,
                comment,
                issue,
                parsed,
                Some(token),
                now,
            );
            release_lease_best_effort(ctx, &actor, &link.job_id, &comment.id, token, now);
            match result {
                Ok(_) => SignalApplyOutcome::Applied,
                Err(e) => SignalApplyOutcome::Error(e),
            }
        }
        SignalAction::AdvanceStage => apply_advance_stage(
            ctx,
            owner_id,
            &actor,
            &job,
            link,
            issue,
            comment,
            parsed,
            &runner_label,
            default_district_id,
            now,
        ),
        other => SignalApplyOutcome::Error(format!(
            "action '{other:?}' is reserved for Afterlife governance responses and cannot be submitted by agent"
        )),
    }
}

fn resolve_target_district(
    parsed: &ParsedLinearComment,
    default_district_id: Option<&str>,
) -> Option<DistrictId> {
    parsed
        .signal
        .district_id
        .as_deref()
        .or(default_district_id)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| DistrictId::from(s.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn apply_enter_district(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    actor: &Actor,
    job: &Job,
    link: &mut LinearIssueLink,
    issue: &LinearIssue,
    comment_id: &str,
    parsed: &ParsedLinearComment,
    default_district_id: Option<&str>,
    now: TimestampMs,
) -> SignalApplyOutcome {
    let Some(target_district) = resolve_target_district(parsed, default_district_id) else {
        return SignalApplyOutcome::Error("enter_district signal requires district_id".into());
    };
    match &job.location {
        JobLocation::InDistrict { district_id, .. } | JobLocation::Completed { district_id, .. }
            if district_id == &target_district =>
        {
            SignalApplyOutcome::Applied
        }
        JobLocation::Queue => match dispatch_and_publish(
            ctx,
            format!("linear:cmt:{comment_id}:enter"),
            actor.clone(),
            EngineCommand::AssignJobToDistrict {
                job_id: link.job_id.clone(),
                district_id: target_district,
                expected_revision: None,
                at: now,
            },
        ) {
            Ok(_) => SignalApplyOutcome::Applied,
            Err(e) => SignalApplyOutcome::Error(e.to_string()),
        },
        JobLocation::Completed { .. } | JobLocation::Archived => match spawn_followup_job_for_issue(
            ctx,
            owner_id,
            actor,
            job,
            link,
            issue,
            comment_id,
            &target_district,
            now,
        ) {
            Ok(_) => SignalApplyOutcome::Applied,
            Err(e) => SignalApplyOutcome::Error(e),
        },
        other => SignalApplyOutcome::Error(format!(
            "cannot enter_district {target_district}: job is currently in {other:?}"
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_advance_stage(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    actor: &Actor,
    job: &Job,
    link: &mut LinearIssueLink,
    issue: &LinearIssue,
    comment: &LinearComment,
    parsed: &ParsedLinearComment,
    runner_label: &str,
    default_district_id: Option<&str>,
    now: TimestampMs,
) -> SignalApplyOutcome {
    match &job.location {
        JobLocation::Queue => {
            let Some(target_district) = resolve_target_district(parsed, default_district_id) else {
                return SignalApplyOutcome::Error(
                    "job is still in Queue; specify district_id or post enter_district first"
                        .into(),
                );
            };
            if let Err(e) = dispatch_and_publish(
                ctx,
                format!("linear:cmt:{}:auto-enter", comment.id),
                actor.clone(),
                EngineCommand::AssignJobToDistrict {
                    job_id: link.job_id.clone(),
                    district_id: target_district,
                    expected_revision: None,
                    at: now,
                },
            ) {
                return SignalApplyOutcome::Error(e.to_string());
            }
        }
        JobLocation::Completed { .. } | JobLocation::Archived => {
            let Some(target_district) = resolve_target_district(parsed, default_district_id) else {
                return SignalApplyOutcome::Error(format!(
                    "job {} is not active; specify district_id or post enter_district to spawn a follow-up job",
                    link.job_id
                ));
            };
            if let JobLocation::Completed { district_id, .. } = &job.location
                && district_id == &target_district
            {
                return SignalApplyOutcome::Error(format!(
                    "job {} is already completed in {district_id}",
                    link.job_id
                ));
            }
            if let Err(e) = spawn_followup_job_for_issue(
                ctx,
                owner_id,
                actor,
                job,
                link,
                issue,
                &comment.id,
                &target_district,
                now,
            ) {
                return SignalApplyOutcome::Error(e);
            }
        }
        JobLocation::InDistrict { .. } => {}
    }

    let fresh_snapshot = match ctx.store.load_workspace(owner_id) {
        Ok(s) => s,
        Err(e) => return SignalApplyOutcome::Error(format!("reload workspace failed: {e}")),
    };
    let Some(fresh_job) = fresh_snapshot.engine.find_job(&link.job_id).cloned() else {
        return SignalApplyOutcome::Error(format!("linked job {} not found", link.job_id));
    };
    let JobLocation::InDistrict {
        district_id,
        stage_id,
        sub_stage_id,
    } = &fresh_job.location
    else {
        return SignalApplyOutcome::Error(format!(
            "job {} is not in an active district stage ({:?})",
            link.job_id, fresh_job.location
        ));
    };

    if let Some(sig_dist) = parsed.signal.district_id.as_deref()
        && sig_dist != district_id.as_str()
    {
        return SignalApplyOutcome::Error(format!(
            "district_id mismatch: signal has '{sig_dist}', job is in '{district_id}'"
        ));
    }
    if let Some(sig_stage) = parsed.signal.stage_id.as_deref()
        && sig_stage != stage_id.as_str()
    {
        return SignalApplyOutcome::Error(format!(
            "stage_id mismatch: signal has '{sig_stage}', job is at '{stage_id}'"
        ));
    }
    if let Some(sig_sub) = parsed.signal.sub_stage_id.as_deref()
        && sub_stage_id.as_ref().map(|s| s.as_str()) != Some(sig_sub)
    {
        return SignalApplyOutcome::Error(format!(
            "sub_stage_id mismatch: signal has '{sig_sub}', job is at {:?}",
            sub_stage_id.as_ref().map(|s| s.as_str())
        ));
    }

    let Some((stage_def, sub_def)) = find_current_stage_and_sub(&fresh_snapshot, &fresh_job) else {
        return SignalApplyOutcome::Error(format!(
            "stage definition not found for {district_id}/{stage_id}"
        ));
    };
    if stage_def.kind == StageKind::ApprovalGate {
        return SignalApplyOutcome::Error(format!(
            "stage '{stage_id}' is a human ApprovalGate; agent cannot advance past it"
        ));
    }

    let token = match claim_short_lease(ctx, actor, &link.job_id, &comment.id, runner_label, now) {
        Ok(t) => t,
        Err(PortError::Domain(afterlife_core::TransitionError::AlreadyLeased { .. })) => {
            return SignalApplyOutcome::DeferredLease;
        }
        Err(e) => return SignalApplyOutcome::Error(e.to_string()),
    };

    let step_result = execute_advance_steps(
        ctx,
        actor,
        &link.job_id,
        stage_def,
        sub_def,
        comment,
        issue,
        parsed,
        token,
        now,
    );
    release_lease_best_effort(ctx, actor, &link.job_id, &comment.id, token, now);
    match step_result {
        Ok(()) => SignalApplyOutcome::Applied,
        Err(e) => SignalApplyOutcome::Error(e),
    }
}

fn extract_attached_id(events: &[JobEvent]) -> Option<ArtifactId> {
    events.iter().find_map(|ev| match ev {
        JobEvent::ArtifactAttached { artifact_id, .. } => Some(artifact_id.clone()),
        _ => None,
    })
}

fn evidence_uri_for(comment: &LinearComment, issue: &LinearIssue) -> String {
    if !comment.url.trim().is_empty() {
        canonical_linear_url(&comment.url)
    } else {
        canonical_linear_url(&issue.url)
    }
}

#[allow(clippy::too_many_arguments)]
fn attach_one(
    ctx: &LinearSyncContext,
    cmd_id: String,
    actor: &Actor,
    job_id: &JobId,
    kind: ArtifactKind,
    label: String,
    uri: String,
    sha256: Option<String>,
    token: Option<FencingToken>,
    now: TimestampMs,
) -> Result<Option<(ArtifactId, ArtifactKind)>, String> {
    let events = dispatch_and_publish(
        ctx,
        cmd_id,
        actor.clone(),
        EngineCommand::AttachArtifact {
            job_id: job_id.clone(),
            kind,
            label: label.clone(),
            uri,
            sha256,
            fencing_token: token,
            at: now,
        },
    )
    .map_err(|e| format!("attach artifact '{label}': {e}"))?;
    Ok(extract_attached_id(&events).map(|id| (id, kind)))
}

#[allow(clippy::too_many_arguments)]
fn attach_signal_artifacts(
    ctx: &LinearSyncContext,
    actor: &Actor,
    job_id: &JobId,
    comment: &LinearComment,
    issue: &LinearIssue,
    parsed: &ParsedLinearComment,
    token: Option<FencingToken>,
    now: TimestampMs,
) -> Result<Vec<(ArtifactId, ArtifactKind)>, String> {
    let mut attached = Vec::new();
    for (idx, art) in parsed.signal.artifacts.iter().enumerate() {
        if let Some(pair) = attach_one(
            ctx,
            format!("linear:cmt:{}:art:{idx}", comment.id),
            actor,
            job_id,
            art.kind,
            art.label.clone(),
            art.uri.clone(),
            art.sha256.clone(),
            token,
            now,
        )? {
            attached.push(pair);
        }
    }
    if attached.is_empty() && parsed.signal.action == SignalAction::AttachArtifact {
        let uri = evidence_uri_for(comment, issue);
        if !uri.trim().is_empty()
            && let Some(pair) = attach_one(
                ctx,
                format!("linear:cmt:{}:art:comment", comment.id),
                actor,
                job_id,
                ArtifactKind::RawNote,
                format!("{} comment", issue.identifier),
                uri,
                None,
                token,
                now,
            )?
        {
            attached.push(pair);
        }
    }
    Ok(attached)
}

#[allow(clippy::too_many_arguments)]
fn execute_advance_steps(
    ctx: &LinearSyncContext,
    actor: &Actor,
    job_id: &JobId,
    stage_def: &StageDef,
    sub_def: Option<&SubStageDef>,
    comment: &LinearComment,
    issue: &LinearIssue,
    parsed: &ParsedLinearComment,
    token: FencingToken,
    now: TimestampMs,
) -> Result<(), String> {
    let mut attached =
        attach_signal_artifacts(ctx, actor, job_id, comment, issue, parsed, Some(token), now)?;

    let required_kinds: &[ArtifactKind] = match sub_def {
        Some(sub) => &sub.required_artifact_kinds,
        None => &stage_def.required_artifact_kinds,
    };
    let evidence_uri = evidence_uri_for(comment, issue);
    if !required_kinds.is_empty()
        && !attached.iter().any(|(_, k)| required_kinds.contains(k))
        && !evidence_uri.trim().is_empty()
    {
        let fallback_kind = required_kinds[0];
        let step_name = sub_def
            .map(|s| s.id.as_str())
            .unwrap_or(stage_def.id.as_str());
        if let Some(pair) = attach_one(
            ctx,
            format!("linear:cmt:{}:art:linear-evidence", comment.id),
            actor,
            job_id,
            fallback_kind,
            format!("{} ({step_name})", issue.identifier),
            evidence_uri,
            None,
            Some(token),
            now,
        )? {
            attached.push(pair);
        }
    }

    if let Some(ref gate) = parsed.signal.gate {
        dispatch_and_publish(
            ctx,
            format!("linear:cmt:{}:gate", comment.id),
            actor.clone(),
            EngineCommand::RecordGateVerification {
                job_id: job_id.clone(),
                verdict: gate.verdict,
                summary: gate.summary.clone(),
                checklist_results: gate.checks.clone(),
                fencing_token: Some(token),
                at: now,
            },
        )
        .map_err(|e| format!("record gate verification: {e}"))?;
    }

    let artifact_ids: Vec<ArtifactId> = attached.into_iter().map(|(id, _)| id).collect();
    let fields = parsed.merged_fields();
    dispatch_and_publish(
        ctx,
        format!("linear:cmt:{}:advance", comment.id),
        actor.clone(),
        EngineCommand::AdvanceJob {
            job_id: job_id.clone(),
            fields,
            artifact_ids,
            expected_revision: None,
            fencing_token: Some(token),
            at: now,
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
