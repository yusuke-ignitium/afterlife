use afterlife_core::{
    Actor, ArtifactKind, DistrictId, EngineCommand, JobEvent, JobId, JobLocation, OwnerId,
    RunnerId, TimestampMs,
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

fn sanitize_url_chars(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.trim().chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '\t' | '\n' | '\r' => {}
            other => out.push(other),
        }
    }
    out
}

pub fn canonical_linear_url(raw: &str) -> String {
    let sanitized = sanitize_url_chars(raw);
    let (base, fragment) = match sanitized.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (sanitized.as_str(), None),
    };
    let (path_part, query) = match base.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (base, None),
    };
    if let Some(rest) = path_part.strip_prefix("https://linear.app/") {
        let segs: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() >= 3 && segs[1] == "issue" {
            let mut canonical = format!("https://linear.app/{}/issue/{}", segs[0], segs[2]);
            if let Some(q) = query {
                canonical.push('?');
                canonical.push_str(q);
            }
            if let Some(f) = fragment {
                canonical.push('#');
                canonical.push_str(f);
            }
            return canonical;
        }
    }
    sanitized
}

fn format_issue_title(issue: &LinearIssue, url: &str) -> String {
    if url.is_empty() {
        format!("[{}] {}", issue.identifier, issue.title)
    } else {
        format!("[{}]({}) {}", issue.identifier, url, issue.title)
    }
}

fn format_issue_raw_input(issue: &LinearIssue, url: &str) -> String {
    let header = if url.is_empty() {
        format!("Linear: {}\nState: {}\n", issue.identifier, issue.state_name)
    } else {
        format!(
            "Linear: [{}]({})\nState: {}\n",
            issue.identifier, url, issue.state_name
        )
    };
    let mut raw_input = header;
    if let Some(desc) = issue.description.as_deref().map(str::trim)
        && !desc.is_empty()
    {
        raw_input.push('\n');
        raw_input.push_str(desc);
    }
    raw_input
}

fn attach_issue_artifact_with_short_lease(
    ctx: &LinearSyncContext,
    actor: &Actor,
    job_id: &JobId,
    issue_id: &str,
    identifier: &str,
    url: &str,
    cmd_suffix: &str,
    now: TimestampMs,
) {
    if url.trim().is_empty() {
        return;
    }
    let Ok(claim_events) = dispatch_and_publish(
        ctx,
        format!("linear:issue-claim:{issue_id}:{cmd_suffix}:{now}"),
        actor.clone(),
        EngineCommand::ClaimJob {
            job_id: job_id.clone(),
            runner_id: RunnerId::from(format!("linear-link:{issue_id}")),
            runner_label: "Linear Sync".into(),
            ttl_ms: 30_000,
            expected_revision: None,
            at: now,
        },
    ) else {
        return;
    };
    let Some(token) = claim_events.iter().find_map(|ev| match ev {
        JobEvent::JobClaimed { fencing_token, .. } => Some(*fencing_token),
        _ => None,
    }) else {
        return;
    };
    let _ = dispatch_and_publish(
        ctx,
        format!("linear:issue-art:{issue_id}:{cmd_suffix}"),
        actor.clone(),
        EngineCommand::AttachArtifact {
            job_id: job_id.clone(),
            kind: ArtifactKind::Issue,
            label: identifier.to_string(),
            uri: url.to_string(),
            sha256: None,
            fencing_token: Some(token),
            at: now,
        },
    );
    let _ = dispatch_and_publish(
        ctx,
        format!("linear:issue-release:{issue_id}:{cmd_suffix}:{now}"),
        actor.clone(),
        EngineCommand::ReleaseLease {
            job_id: job_id.clone(),
            fencing_token: token,
            at: now,
        },
    );
}

fn upgrade_existing_link_display(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    actor: &Actor,
    existing: &LinearIssueLink,
    issue: &LinearIssue,
    url: &str,
    now: TimestampMs,
) {
    if url.is_empty() {
        return;
    }
    let Ok(snapshot) = ctx.store.load_workspace(owner_id) else {
        return;
    };
    let Some(job) = snapshot.engine.find_job(&existing.job_id) else {
        return;
    };

    let expected_prefix = format!("[{}](", issue.identifier);
    let legacy_raw_prefix = format!("Linear: {} (", issue.identifier);
    let needs_text_upgrade = matches!(job.location, JobLocation::Queue)
        && (!job.title.contains(&expected_prefix)
            || job.raw_input.starts_with(&legacy_raw_prefix)
            || job.spec_summary.starts_with(&legacy_raw_prefix));

    if needs_text_upgrade {
        let new_title = format_issue_title(issue, url);
        let new_raw = format_issue_raw_input(issue, url);
        let _ = dispatch_and_publish(
            ctx,
            format!("linear:upgrade-link:{}", issue.id),
            actor.clone(),
            EngineCommand::UpdateJob {
                job_id: existing.job_id.clone(),
                title: Some(new_title),
                raw_input: Some(new_raw.clone()),
                spec_summary: Some(new_raw),
                target_district_hint: None,
                expected_revision: None,
                fencing_token: None,
                at: now,
            },
        );
    }

    let has_issue_artifact = job.artifacts.iter().any(|a| a.kind == ArtifactKind::Issue);
    let has_active_foreign_lease = job
        .lease
        .as_ref()
        .is_some_and(|l| l.is_active_at(now) && l.holder_client_id.as_deref() != Some(LINEAR_SYNC_CLIENT_ID));
    if !has_issue_artifact && !has_active_foreign_lease && !job.stopped {
        attach_issue_artifact_with_short_lease(
            ctx,
            actor,
            &existing.job_id,
            &issue.id,
            &issue.identifier,
            url,
            "canonical",
            now,
        );
    }
}

pub fn ensure_issue_linked(
    ctx: &LinearSyncContext,
    owner_id: &OwnerId,
    token: &LinearOAuthToken,
    issue: &LinearIssue,
    now: TimestampMs,
) -> Result<(LinearIssueLink, bool), String> {
    let url = canonical_linear_url(&issue.url);
    let actor = Actor::Agent {
        owner_id: owner_id.clone(),
        client_id: LINEAR_SYNC_CLIENT_ID.into(),
        runner_label: "Linear Sync".into(),
        token_jti: format!("linear-issue:{}", issue.id),
    };

    if let Some(mut existing) = ctx
        .linear
        .find_link_by_issue(owner_id, &issue.id)
        .map_err(|e| format!("lookup linear link: {e}"))?
    {
        existing.linear_identifier = issue.identifier.clone();
        existing.linear_url = url.clone();
        existing.last_synced_at_ms = now;
        ctx.linear
            .upsert_link(&existing)
            .map_err(|e| format!("update linear link: {e}"))?;
        upgrade_existing_link_display(ctx, owner_id, &actor, &existing, issue, &url, now);
        return Ok((existing, false));
    }

    let hint = first_agent_district_hint(&issue.comments)
        .or_else(|| token.default_district_id.clone().map(DistrictId::from));
    let title = format_issue_title(issue, &url);
    let raw_input = format_issue_raw_input(issue, &url);

    let events = dispatch_and_publish(
        ctx,
        format!("linear:queue:{}", issue.id),
        actor.clone(),
        EngineCommand::QueueJob {
            title,
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

    attach_issue_artifact_with_short_lease(
        ctx,
        &actor,
        &job_id,
        &issue.id,
        &issue.identifier,
        &url,
        "initial",
        now,
    );

    let link = LinearIssueLink {
        owner_id: owner_id.clone(),
        linear_issue_id: issue.id.clone(),
        linear_identifier: issue.identifier.clone(),
        linear_url: url,
        job_id,
        last_synced_at_ms: now,
    };
    ctx.linear
        .upsert_link(&link)
        .map_err(|e| format!("insert linear link: {e}"))?;
    Ok((link, true))
}
