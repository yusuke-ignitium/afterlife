use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use afterlife_core::{JobEvent, JobId, JobLocation, OwnerId, StageKind, TimestampMs};
use afterlife_protocol::{
    EventBus, LinearIssueLink, LinearOAuthToken, LinearSignalV1, LinearSyncCursor,
    LinearSyncReport, LinearSyncStore, SignalAction, SignalSource, WorkspaceSnapshot,
    WorkspaceStore, extract_linear_signal, format_afterlife_comment,
};

use crate::linear::{
    client::{LinearApi, LinearApiError},
    inbound::{SignalApplyOutcome, apply_agent_signal},
    links::ensure_issue_linked,
};

pub const LINEAR_SYNC_CLIENT_ID: &str = "linear-sync";
const TOKEN_REFRESH_LEAD_MS: u64 = 5 * 60 * 1_000;

pub fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone)]
pub struct LinearSyncContext {
    pub store: Arc<dyn WorkspaceStore>,
    pub linear: Arc<dyn LinearSyncStore>,
    pub event_bus: Arc<dyn EventBus>,
    pub api: Arc<dyn LinearApi>,
    pub refresh_lock: Arc<Mutex<()>>,
}

fn ensure_valid_access_token(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    force_refresh: bool,
    now: TimestampMs,
) -> Result<Option<LinearOAuthToken>, String> {
    let _guard = ctx.refresh_lock.lock().unwrap();
    let Some(mut token) = ctx
        .linear
        .load_token(owner_id)
        .map_err(|e| format!("load linear token: {e}"))?
    else {
        return Ok(None);
    };

    let needs_refresh = force_refresh
        || token
            .expires_at_ms
            .is_some_and(|exp| exp <= now.saturating_add(TOKEN_REFRESH_LEAD_MS));

    if !needs_refresh {
        return Ok(Some(token));
    }

    let Some(ref refresh_tok) = token.refresh_token else {
        if force_refresh {
            return Err(
                "Linear access token expired and no refresh_token is stored; reconnect Linear OAuth"
                    .into(),
            );
        }
        return Ok(Some(token));
    };

    match ctx.api.refresh_token(
        &token.client_id,
        token.client_secret.as_deref(),
        refresh_tok,
    ) {
        Ok(resp) => {
            token.access_token = resp.access_token;
            if let Some(next_refresh) = resp.refresh_token {
                token.refresh_token = Some(next_refresh);
            }
            token.expires_at_ms = resp
                .expires_in
                .map(|secs| now.saturating_add(secs.saturating_mul(1_000)));
            if let Some(scope) = resp.scope {
                token.scope = scope;
            }
            token.updated_at_ms = now;
            ctx.linear
                .save_token(&token)
                .map_err(|e| format!("save refreshed linear token: {e}"))?;
            Ok(Some(token))
        }
        Err(LinearApiError::InvalidGrant(msg)) => Err(format!(
            "Linear refresh token expired or revoked ({msg}); reconnect Linear OAuth"
        )),
        Err(err) => Err(err.to_string()),
    }
}

fn post_sync_error_comment(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    access_token: &str,
    link: &LinearIssueLink,
    comment_id: &str,
    error_reason: &str,
    now: TimestampMs,
) {
    let mut sig = LinearSignalV1::afterlife_event(SignalAction::SyncError, link.job_id.as_str(), 0);
    sig.reason = Some(error_reason.to_string());
    let body = format_afterlife_comment(
        "**Afterlife Sync Error**",
        Some(&format!(
            "Failed to apply signal from comment `{comment_id}`:\n\n> {error_reason}"
        )),
        &sig,
    );
    if let Ok(created) = ctx.api.create_comment(access_token, &link.linear_issue_id, &body) {
        let _ = ctx
            .linear
            .mark_comment_processed(owner_id, &created.id, "outbound_afterlife", now);
    }
}

fn location_stage_ids(loc: &JobLocation) -> (Option<String>, Option<String>, Option<String>) {
    match loc {
        JobLocation::InDistrict {
            district_id,
            stage_id,
            sub_stage_id,
        } => (
            Some(district_id.as_str().to_string()),
            Some(stage_id.as_str().to_string()),
            sub_stage_id.as_ref().map(|s| s.as_str().to_string()),
        ),
        JobLocation::Completed {
            district_id,
            terminal_stage_id,
            ..
        } => (
            Some(district_id.as_str().to_string()),
            Some(terminal_stage_id.as_str().to_string()),
            None,
        ),
        _ => (None, None, None),
    }
}

fn is_approval_gate_location(snapshot: &WorkspaceSnapshot, loc: &JobLocation) -> bool {
    let JobLocation::InDistrict {
        district_id,
        stage_id,
        ..
    } = loc
    else {
        return false;
    };
    snapshot
        .engine
        .find_district(district_id)
        .and_then(|d| {
            d.spec_history
                .iter()
                .flat_map(|s| s.stages.iter())
                .find(|st| &st.id == stage_id)
        })
        .is_some_and(|st| st.kind == StageKind::ApprovalGate)
}

fn build_outbound_comment(
    snapshot: &WorkspaceSnapshot,
    seq: u64,
    event: &JobEvent,
) -> Option<(JobId, String)> {
    match event {
        JobEvent::JobReworked {
            actor,
            job_id,
            to,
            reason,
            ..
        } if actor.is_human() => {
            let (district_id, stage_id, sub_stage_id) = location_stage_ids(to);
            let mut sig =
                LinearSignalV1::afterlife_event(SignalAction::Reworked, job_id.as_str(), seq);
            sig.district_id = district_id;
            sig.stage_id = stage_id.clone();
            sig.sub_stage_id = sub_stage_id;
            sig.reason = reason.clone();
            let headline = format!(
                "**Afterlife Rework** -> `{}`",
                stage_id.as_deref().unwrap_or("rework")
            );
            let details = reason
                .as_deref()
                .filter(|r| !r.trim().is_empty())
                .map(|r| format!("Reason: {r}"));
            Some((
                job_id.clone(),
                format_afterlife_comment(&headline, details.as_deref(), &sig),
            ))
        }
        JobEvent::JobTransitioned {
            actor,
            job_id,
            from,
            to,
            ..
        } if actor.is_human() && is_approval_gate_location(snapshot, from) => {
            let (district_id, stage_id, sub_stage_id) = location_stage_ids(to);
            let mut sig =
                LinearSignalV1::afterlife_event(SignalAction::Approved, job_id.as_str(), seq);
            sig.district_id = district_id;
            sig.stage_id = stage_id.clone();
            sig.sub_stage_id = sub_stage_id;
            let headline = format!(
                "**Afterlife Approved** -> `{}`",
                stage_id.as_deref().unwrap_or("completed")
            );
            Some((
                job_id.clone(),
                format_afterlife_comment(&headline, None, &sig),
            ))
        }
        JobEvent::JobStopped {
            actor,
            job_id,
            reason,
            ..
        } if actor.is_human() => {
            let mut sig =
                LinearSignalV1::afterlife_event(SignalAction::Stopped, job_id.as_str(), seq);
            sig.reason = reason.clone();
            let details = reason
                .as_deref()
                .filter(|r| !r.trim().is_empty())
                .map(|r| format!("Reason: {r}"));
            Some((
                job_id.clone(),
                format_afterlife_comment("**Afterlife Stopped**", details.as_deref(), &sig),
            ))
        }
        JobEvent::JobResumed { actor, job_id, .. } if actor.is_human() => {
            let sig = LinearSignalV1::afterlife_event(SignalAction::Resumed, job_id.as_str(), seq);
            Some((
                job_id.clone(),
                format_afterlife_comment("**Afterlife Resumed**", None, &sig),
            ))
        }
        _ => None,
    }
}

pub fn sync_outbound_events_once(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    now: TimestampMs,
) -> Result<usize, String> {
    let Some(token) = ensure_valid_access_token(ctx, owner_id, false, now)? else {
        return Ok(0);
    };
    let mut cursor = ctx
        .linear
        .load_cursor(owner_id)
        .map_err(|e| format!("load linear cursor: {e}"))?;
    let events = ctx
        .store
        .list_events_since(owner_id, cursor.last_outbound_seq, 200)
        .map_err(|e| format!("read list_events_since: {e}"))?;
    if events.is_empty() {
        return Ok(0);
    }

    let snapshot = ctx
        .store
        .load_workspace(owner_id)
        .map_err(|e| format!("load workspace: {e}"))?;
    let mut posted = 0usize;

    for record in events {
        if let Some((job_id, body)) = build_outbound_comment(&snapshot, record.seq, &record.event)
            && let Some(link) = ctx
                .linear
                .find_link_by_job(owner_id, &job_id)
                .map_err(|e| format!("lookup linear link by job: {e}"))?
        {
            let created = match ctx
                .api
                .create_comment(&token.access_token, &link.linear_issue_id, &body)
            {
                Ok(c) => c,
                Err(LinearApiError::Unauthorized(_)) => {
                    let refreshed = ensure_valid_access_token(ctx, owner_id, true, now)?
                        .ok_or_else(|| "linear disconnected during refresh".to_string())?;
                    ctx.api
                        .create_comment(&refreshed.access_token, &link.linear_issue_id, &body)
                        .map_err(|e| e.to_string())?
                }
                Err(e) => return Err(e.to_string()),
            };
            ctx.linear
                .mark_comment_processed(owner_id, &created.id, "outbound_afterlife", now)
                .map_err(|e| format!("mark outbound comment processed: {e}"))?;
            posted += 1;
        }
        cursor.last_outbound_seq = record.seq;
        ctx.linear
            .save_cursor(&cursor)
            .map_err(|e| format!("save outbound cursor: {e}"))?;
    }

    Ok(posted)
}

pub fn sync_owner_once(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    now: TimestampMs,
) -> Result<LinearSyncReport, String> {
    let Some(mut token) = ensure_valid_access_token(ctx, owner_id, false, now)? else {
        return Ok(LinearSyncReport::default());
    };

    let (viewer, issues) = match ctx.api.fetch_started_assigned_issues(&token.access_token) {
        Ok(res) => res,
        Err(LinearApiError::Unauthorized(_)) => {
            token = ensure_valid_access_token(ctx, owner_id, true, now)?
                .ok_or_else(|| "linear disconnected during refresh".to_string())?;
            ctx.api
                .fetch_started_assigned_issues(&token.access_token)
                .map_err(|e| e.to_string())?
        }
        Err(e) => {
            let mut cursor = ctx
                .linear
                .load_cursor(owner_id)
                .unwrap_or_else(|_| LinearSyncCursor::initial(owner_id.clone()));
            cursor.last_error = Some(e.to_string());
            let _ = ctx.linear.save_cursor(&cursor);
            return Err(e.to_string());
        }
    };

    if token.viewer_id.as_deref() != Some(viewer.id.as_str())
        || token.viewer_name.as_deref() != Some(viewer.name.as_str())
        || token.viewer_email != viewer.email
    {
        token.viewer_id = Some(viewer.id);
        token.viewer_name = Some(viewer.name);
        token.viewer_email = viewer.email;
        token.updated_at_ms = now;
        let _ = ctx.linear.save_token(&token);
    }

    let mut report = LinearSyncReport::default();

    for issue in &issues {
        if !issue.state_type.eq_ignore_ascii_case("started") {
            continue;
        }
        report.started_issues_seen += 1;
        let (link, created) = ensure_issue_linked(ctx, owner_id, &token, issue, now)?;
        if created {
            report.jobs_created += 1;
        }

        for comment in &issue.comments {
            if ctx
                .linear
                .is_comment_processed(owner_id, &comment.id)
                .map_err(|e| format!("check processed comment: {e}"))?
            {
                continue;
            }
            match extract_linear_signal(&comment.body) {
                Ok(None) => {
                    ctx.linear
                        .mark_comment_processed(owner_id, &comment.id, "ignored_plain", now)
                        .map_err(|e| format!("mark plain comment: {e}"))?;
                }
                Err(parse_err) => {
                    post_sync_error_comment(
                        ctx,
                        owner_id,
                        &token.access_token,
                        &link,
                        &comment.id,
                        &parse_err,
                        now,
                    );
                    ctx.linear
                        .mark_comment_processed(owner_id, &comment.id, "sync_error", now)
                        .map_err(|e| format!("mark error comment: {e}"))?;
                    report.sync_errors_posted += 1;
                }
                Ok(Some(parsed)) => {
                    if parsed.signal.source == SignalSource::Afterlife {
                        ctx.linear
                            .mark_comment_processed(
                                owner_id,
                                &comment.id,
                                "ignored_afterlife",
                                now,
                            )
                            .map_err(|e| format!("mark afterlife comment: {e}"))?;
                        continue;
                    }
                    match apply_agent_signal(
                        ctx,
                        owner_id,
                        &link,
                        issue,
                        comment,
                        &parsed,
                        token.default_district_id.as_deref(),
                        now,
                    ) {
                        SignalApplyOutcome::Applied => {
                            ctx.linear
                                .mark_comment_processed(owner_id, &comment.id, "applied", now)
                                .map_err(|e| format!("mark applied comment: {e}"))?;
                            report.signals_applied += 1;
                        }
                        SignalApplyOutcome::DeferredLease => {
                            report.signals_deferred_lease += 1;
                            break;
                        }
                        SignalApplyOutcome::Error(err_msg) => {
                            post_sync_error_comment(
                                ctx,
                                owner_id,
                                &token.access_token,
                                &link,
                                &comment.id,
                                &err_msg,
                                now,
                            );
                            ctx.linear
                                .mark_comment_processed(owner_id, &comment.id, "sync_error", now)
                                .map_err(|e| format!("mark error comment: {e}"))?;
                            report.sync_errors_posted += 1;
                        }
                    }
                }
            }
        }
    }

    report.outbound_events_posted = sync_outbound_events_once(ctx, owner_id, now)?;

    let mut cursor = ctx
        .linear
        .load_cursor(owner_id)
        .unwrap_or_else(|_| LinearSyncCursor::initial(owner_id.clone()));
    cursor.last_sync_at_ms = Some(now);
    cursor.last_error = None;
    let _ = ctx.linear.save_cursor(&cursor);

    Ok(report)
}
