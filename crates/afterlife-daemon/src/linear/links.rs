use afterlife_core::{
    Actor, ArtifactKind, DistrictId, EngineCommand, JobEvent, OwnerId, TimestampMs,
};
use afterlife_protocol::{
    LinearIssueLink, LinearOAuthToken, SignalSource, extract_linear_signal,
};

use crate::linear::{
    client::{LinearComment, LinearIssue},
    inbound::dispatch_and_publish,
    sync::{LINEAR_SYNC_CLIENT_ID, LinearSyncContext},
};

fn first_agent_district_hint(comments: &[LinearComment]) -> Option<DistrictId> {
    for cmt in comments {
        if let Ok(Some(parsed)) = extract_linear_signal(&cmt.body)
            && parsed.signal.source == SignalSource::Agent
            && let Some(did) = parsed.signal.district_id
            && !did.trim().is_empty()
        {
            return Some(DistrictId::from(did.trim().to_string()));
        }
    }
    None
}

pub fn ensure_issue_linked(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    token: &LinearOAuthToken,
    issue: &LinearIssue,
    now: TimestampMs,
) -> Result<(LinearIssueLink, bool), String> {
    if let Some(mut existing) = ctx
        .linear
        .find_link_by_issue(owner_id, &issue.id)
        .map_err(|e| format!("lookup linear link: {e}"))?
    {
        existing.linear_identifier = issue.identifier.clone();
        existing.linear_url = issue.url.clone();
        existing.last_synced_at_ms = now;
        ctx.linear
            .upsert_link(&existing)
            .map_err(|e| format!("update linear link: {e}"))?;
        return Ok((existing, false));
    }

    let hint = first_agent_district_hint(&issue.comments)
        .or_else(|| token.default_district_id.clone().map(DistrictId::from));
    let mut raw_input = format!(
        "Linear: {} ({})\nState: {}\n",
        issue.identifier, issue.url, issue.state_name
    );
    if let Some(desc) = issue.description.as_deref().map(str::trim)
        && !desc.is_empty()
    {
        raw_input.push('\n');
        raw_input.push_str(desc);
    }

    let actor = Actor::Agent {
        owner_id: owner_id.clone(),
        client_id: LINEAR_SYNC_CLIENT_ID.into(),
        runner_label: "Linear Sync".into(),
        token_jti: format!("linear-issue:{}", issue.id),
    };
    let events = dispatch_and_publish(
        ctx,
        format!("linear:queue:{}", issue.id),
        actor.clone(),
        EngineCommand::QueueJob {
            title: format!("[{}] {}", issue.identifier, issue.title),
            raw_input,
            target_district_hint: hint,
            at: now,
        },
    )
    .map_err(|e| format!("queue job for {}: {e}", issue.identifier))?;

    let job_id = events
        .iter()
        .find_map(|ev| match ev {
            JobEvent::JobQueued { job_id, .. } => Some(job_id.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            format!(
                "queue command produced no JobQueued event for {}",
                issue.identifier
            )
        })?;

    if !issue.url.trim().is_empty() {
        let _ = dispatch_and_publish(
            ctx,
            format!("linear:issue-art:{}", issue.id),
            actor,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::Issue,
                label: issue.identifier.clone(),
                uri: issue.url.clone(),
                sha256: None,
                fencing_token: None,
                at: now,
            },
        );
    }

    let link = LinearIssueLink {
        owner_id: owner_id.clone(),
        linear_issue_id: issue.id.clone(),
        linear_identifier: issue.identifier.clone(),
        linear_url: issue.url.clone(),
        job_id,
        last_synced_at_ms: now,
    };
    ctx.linear
        .upsert_link(&link)
        .map_err(|e| format!("insert linear link: {e}"))?;
    Ok((link, true))
}
