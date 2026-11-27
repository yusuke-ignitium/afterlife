use std::sync::{Arc, Mutex};

use afterlife_core::{
    Actor, ArtifactKind, CommandId, DistrictId, EngineCommand, JobLocation, OwnerId, RunnerId,
    StageKind,
};
use afterlife_protocol::{
    CommandEnvelope, LinearOAuthToken, LinearSyncStore, SignalAction, SignalSource, WorkspaceStore,
    extract_linear_signal,
};
use afterlife_storage::{InMemoryEventBus, SqliteLinearStore, SqliteWorkspaceStore};

use crate::linear::{
    client::{LinearApi, LinearApiError, LinearComment, LinearIssue, LinearTokenResponse, LinearViewer},
    sync::{LinearSyncContext, sync_owner_once},
};

struct MockLinearState {
    issues: Vec<LinearIssue>,
    posted_comments: Vec<(String, String)>,
    refresh_calls: usize,
    next_comment_seq: usize,
}

struct MockLinearApi {
    state: Mutex<MockLinearState>,
}

impl MockLinearApi {
    fn new(issues: Vec<LinearIssue>) -> Self {
        Self {
            state: Mutex::new(MockLinearState {
                issues,
                posted_comments: Vec::new(),
                refresh_calls: 0,
                next_comment_seq: 1,
            }),
        }
    }

    fn push_comment(&self, issue_id: &str, comment: LinearComment) {
        let mut st = self.state.lock().unwrap();
        if let Some(issue) = st.issues.iter_mut().find(|i| i.id == issue_id) {
            issue.comments.push(comment);
        }
    }

    fn posted_comments(&self) -> Vec<(String, String)> {
        self.state.lock().unwrap().posted_comments.clone()
    }

    fn refresh_calls(&self) -> usize {
        self.state.lock().unwrap().refresh_calls
    }
}

impl LinearApi for MockLinearApi {
    fn exchange_code(
        &self,
        _client_id: &str,
        _client_secret: Option<&str>,
        _code: &str,
        _redirect_uri: &str,
        _code_verifier: &str,
    ) -> Result<LinearTokenResponse, LinearApiError> {
        Ok(LinearTokenResponse {
            access_token: "acc-initial".into(),
            refresh_token: Some("ref-initial".into()),
            expires_in: Some(86_400),
            scope: Some("read,comments:create".into()),
            token_type: Some("Bearer".into()),
        })
    }

    fn refresh_token(
        &self,
        _client_id: &str,
        _client_secret: Option<&str>,
        _refresh_token: &str,
    ) -> Result<LinearTokenResponse, LinearApiError> {
        let mut st = self.state.lock().unwrap();
        st.refresh_calls += 1;
        Ok(LinearTokenResponse {
            access_token: format!("acc-refreshed-{}", st.refresh_calls),
            refresh_token: Some(format!("ref-rotated-{}", st.refresh_calls)),
            expires_in: Some(86_400),
            scope: Some("read,comments:create".into()),
            token_type: Some("Bearer".into()),
        })
    }

    fn fetch_viewer(&self, _access_token: &str) -> Result<LinearViewer, LinearApiError> {
        Ok(LinearViewer {
            id: "viewer-1".into(),
            name: "Alice".into(),
            email: Some("alice@example.com".into()),
        })
    }

    fn fetch_open_assigned_issues(
        &self,
        _access_token: &str,
    ) -> Result<(LinearViewer, Vec<LinearIssue>), LinearApiError> {
        let st = self.state.lock().unwrap();
        Ok((
            LinearViewer {
                id: "viewer-1".into(),
                name: "Alice".into(),
                email: Some("alice@example.com".into()),
            },
            st.issues.clone(),
        ))
    }

    fn create_comment(
        &self,
        _access_token: &str,
        issue_id: &str,
        body: &str,
    ) -> Result<LinearComment, LinearApiError> {
        let mut st = self.state.lock().unwrap();
        let seq = st.next_comment_seq;
        st.next_comment_seq += 1;
        st.posted_comments
            .push((issue_id.to_string(), body.to_string()));
        let comment = LinearComment {
            id: format!("lin-out-{seq}"),
            body: body.to_string(),
            url: format!("https://linear.app/acme/issue/ENG-1#comment-out-{seq}"),
            created_at: format!("2026-11-27T00:00:{seq:02}Z"),
            author_name: Some("Afterlife".into()),
        };
        if let Some(issue) = st.issues.iter_mut().find(|i| i.id == issue_id) {
            issue.comments.push(comment.clone());
        }
        Ok(comment)
    }
}

fn setup_ctx(
    issues: Vec<LinearIssue>,
    token_expires_at: Option<u64>,
) -> (LinearSyncContext, Arc<MockLinearApi>, OwnerId) {
    let owner_id = OwnerId::from("local-owner");
    let store: Arc<dyn WorkspaceStore> = Arc::new(SqliteWorkspaceStore::open_in_memory().unwrap());
    let linear: Arc<dyn LinearSyncStore> = Arc::new(SqliteLinearStore::open_in_memory().unwrap());
    let event_bus = Arc::new(InMemoryEventBus::new(64));
    let api = Arc::new(MockLinearApi::new(issues));

    linear
        .save_token(&LinearOAuthToken {
            owner_id: owner_id.clone(),
            client_id: "client-1".into(),
            client_secret: None,
            access_token: "acc-1".into(),
            refresh_token: Some("ref-1".into()),
            expires_at_ms: token_expires_at,
            scope: "read,comments:create".into(),
            viewer_id: None,
            viewer_name: None,
            viewer_email: None,
            default_district_id: Some("DIST-002".into()),
            updated_at_ms: 1_000_000,
        })
        .unwrap();

    let ctx = LinearSyncContext {
        store,
        linear,
        event_bus,
        api: api.clone(),
        refresh_lock: Arc::new(Mutex::new(())),
    };
    (ctx, api, owner_id)
}

#[test]
fn started_issue_syncs_through_required_artifact_gate_blocks_approval_bypass_and_writes_back_rework()
{
    let issue_started = LinearIssue {
        id: "iss-started-1".into(),
        identifier: "ENG-101".into(),
        title: "Shape new auth flow".into(),
        description: Some("Clarify OAuth flow".into()),
        url: "https://linear.app/acme/issue/ENG-101".into(),
        state_name: "In Progress".into(),
        state_type: "started".into(),
        comments: vec![LinearComment {
            id: "cmt-1".into(),
            body: r#"Starting discovery.

```afterlife-signal
{
  "version": 1,
  "source": "agent",
  "action": "enter_district",
  "district_id": "DIST-002",
  "runner_label": "Cursor Cloud Agent"
}
```"#
            .into(),
            url: "https://linear.app/acme/issue/ENG-101#comment-1".into(),
            created_at: "2026-11-27T01:00:00Z".into(),
            author_name: Some("Cursor".into()),
        }],
    };
    let issue_todo = LinearIssue {
        id: "iss-todo-2".into(),
        identifier: "ENG-102".into(),
        title: "Open todo ticket".into(),
        description: None,
        url: "https://linear.app/acme/issue/ENG-102".into(),
        state_name: "Todo".into(),
        state_type: "unstarted".into(),
        comments: vec![],
    };
    let issue_done = LinearIssue {
        id: "iss-done-3".into(),
        identifier: "ENG-103".into(),
        title: "Already completed".into(),
        description: None,
        url: "https://linear.app/acme/issue/ENG-103".into(),
        state_name: "Done".into(),
        state_type: "completed".into(),
        comments: vec![],
    };
    let issue_canceled = LinearIssue {
        id: "iss-canceled-4".into(),
        identifier: "ENG-104".into(),
        title: "Canceled task".into(),
        description: None,
        url: "https://linear.app/acme/issue/ENG-104".into(),
        state_name: "Canceled".into(),
        state_type: "canceled".into(),
        comments: vec![],
    };
    let issue_duplicate = LinearIssue {
        id: "iss-dup-5".into(),
        identifier: "ENG-105".into(),
        title: "Duplicate task".into(),
        description: None,
        url: "https://linear.app/acme/issue/ENG-105".into(),
        state_name: "Duplicate".into(),
        state_type: "canceled".into(),
        comments: vec![],
    };

    let (ctx, api, owner_id) = setup_ctx(
        vec![
            issue_started,
            issue_todo,
            issue_done,
            issue_canceled,
            issue_duplicate,
        ],
        None,
    );
    let now = 2_000_000_000_000u64;

    let r1 = sync_owner_once(&ctx, &owner_id, now).unwrap();
    assert_eq!(r1.open_issues_seen, 2);
    assert_eq!(r1.jobs_created, 2);
    assert_eq!(r1.signals_applied, 1);

    let link = ctx
        .linear
        .find_link_by_issue(&owner_id, "iss-started-1")
        .unwrap()
        .unwrap();
    assert!(
        ctx.linear
            .find_link_by_issue(&owner_id, "iss-todo-2")
            .unwrap()
            .is_some()
    );
    assert!(
        ctx.linear
            .find_link_by_issue(&owner_id, "iss-done-3")
            .unwrap()
            .is_none()
    );
    assert!(
        ctx.linear
            .find_link_by_issue(&owner_id, "iss-canceled-4")
            .unwrap()
            .is_none()
    );
    assert!(
        ctx.linear
            .find_link_by_issue(&owner_id, "iss-dup-5")
            .unwrap()
            .is_none()
    );

    let mut step_idx = 2usize;
    loop {
        let snap = ctx.store.load_workspace(&owner_id).unwrap();
        let job = snap.engine.find_job(&link.job_id).unwrap();
        let JobLocation::InDistrict {
            district_id,
            stage_id,
            sub_stage_id,
        } = &job.location
        else {
            panic!("expected job in district");
        };
        let district = snap.engine.find_district(district_id).unwrap();
        let stage = district
            .active_spec()
            .stages
            .iter()
            .find(|s| &s.id == stage_id)
            .unwrap();
        if stage.kind == StageKind::ApprovalGate {
            break;
        }
        let sub = sub_stage_id.as_ref().and_then(|sid| {
            stage
                .sub_template_id
                .as_ref()
                .and_then(|tid| snap.engine.templates.get(tid))
                .and_then(|tpl| tpl.stages.iter().find(|s| &s.id == sid))
        });
        let fields = match sub {
            Some(s) if !s.record_fields.is_empty() => s.record_fields.clone(),
            _ if !stage.record_fields.is_empty() => stage.record_fields.clone(),
            _ => afterlife_core::default_step_fields(),
        };
        let mut md = String::new();
        for f in &fields {
            md.push_str(&format!("## {}\nVerified content for {}\n\n", f.key, f.key));
        }
        let gate_json = if let Some(ref rubric) = stage.gate_rubric {
            let checks: Vec<serde_json::Value> = rubric
                .checks
                .iter()
                .map(|c| serde_json::json!([c, true]))
                .collect();
            serde_json::json!({
                "verdict": "pass",
                "summary": "All checks verified",
                "checks": checks
            })
        } else {
            serde_json::Value::Null
        };
        let mut sig_obj = serde_json::json!({
            "version": 1,
            "source": "agent",
            "action": "advance_stage",
            "district_id": district_id.as_str(),
            "stage_id": stage_id.as_str(),
            "runner_label": "Cursor Cloud Agent"
        });
        if let Some(sub_id) = sub_stage_id {
            sig_obj["sub_stage_id"] = serde_json::Value::String(sub_id.as_str().to_string());
        }
        if !gate_json.is_null() {
            sig_obj["gate"] = gate_json;
        }
        md.push_str("```afterlife-signal\n");
        md.push_str(&serde_json::to_string_pretty(&sig_obj).unwrap());
        md.push_str("\n```\n");

        api.push_comment(
            "iss-started-1",
            LinearComment {
                id: format!("cmt-{step_idx}"),
                body: md,
                url: format!("https://linear.app/acme/issue/ENG-101#comment-{step_idx}"),
                created_at: format!("2026-11-27T01:{step_idx:02}:00Z"),
                author_name: Some("Cursor".into()),
            },
        );
        let rep = sync_owner_once(&ctx, &owner_id, now + (step_idx as u64) * 1_000).unwrap();
        assert_eq!(rep.signals_applied, 1, "step {step_idx} failed: {rep:?}");
        assert_eq!(rep.sync_errors_posted, 0);
        step_idx += 1;
    }

    let snap = ctx.store.load_workspace(&owner_id).unwrap();
    let job = snap.engine.find_job(&link.job_id).unwrap();
    assert!(
        job.artifacts
            .iter()
            .any(|a| a.kind == ArtifactKind::Spec && a.uri.starts_with("https://linear.app/"))
    );

    let gate_stage_id = match &job.location {
        JobLocation::InDistrict { stage_id, .. } => stage_id.as_str().to_string(),
        other => panic!("expected approval gate, got {other:?}"),
    };
    api.push_comment(
        "iss-started-1",
        LinearComment {
            id: "cmt-bypass".into(),
            body: format!(
                "```afterlife-signal\n{{\"version\":1,\"source\":\"agent\",\"action\":\"advance_stage\",\"district_id\":\"DIST-002\",\"stage_id\":\"{gate_stage_id}\"}}\n```"
            ),
            url: "https://linear.app/acme/issue/ENG-101#comment-bypass".into(),
            created_at: "2026-11-27T02:00:00Z".into(),
            author_name: Some("Cursor".into()),
        },
    );
    let bypass_rep = sync_owner_once(&ctx, &owner_id, now + 90_000).unwrap();
    assert_eq!(bypass_rep.signals_applied, 0);
    assert_eq!(bypass_rep.sync_errors_posted, 1);

    let local_only = ctx
        .store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("local-only-job"),
            actor: Actor::local_human(owner_id.clone()),
            command: EngineCommand::QueueJob {
                title: "Local private job".into(),
                raw_input: "Do not mirror to Linear".into(),
                target_district_hint: Some(DistrictId::from("DIST-001")),
                at: now + 91_000,
            },
        })
        .unwrap();
    assert!(!local_only.events.is_empty());

    ctx.store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("human-rework-1"),
            actor: Actor::local_human(owner_id.clone()),
            command: EngineCommand::ReworkJob {
                job_id: link.job_id.clone(),
                expected_revision: None,
                fencing_token: None,
                reason: Some("Tighten acceptance criteria".into()),
                at: now + 95_000,
            },
        })
        .unwrap();

    let out_rep = sync_owner_once(&ctx, &owner_id, now + 100_000).unwrap();
    assert_eq!(out_rep.outbound_events_posted, 1);

    let posted = api.posted_comments();
    let (_, last_body) = posted.last().unwrap();
    let parsed_out = extract_linear_signal(last_body).unwrap().unwrap();
    assert_eq!(parsed_out.signal.source, SignalSource::Afterlife);
    assert_eq!(parsed_out.signal.action, SignalAction::Reworked);
    assert_eq!(
        parsed_out.signal.reason.as_deref(),
        Some("Tighten acceptance criteria")
    );

    let echo_rep = sync_owner_once(&ctx, &owner_id, now + 105_000).unwrap();
    assert_eq!(echo_rep.signals_applied, 0);
    assert_eq!(echo_rep.sync_errors_posted, 0);
    assert_eq!(echo_rep.outbound_events_posted, 0);
}

#[test]
fn active_local_lease_defers_linear_signal_and_token_rotates_when_expiring() {
    let issue = LinearIssue {
        id: "iss-lease-1".into(),
        identifier: "ENG-202".into(),
        title: "Coexist with local runner".into(),
        description: None,
        url: "https://linear.app/acme/issue/ENG-202".into(),
        state_name: "In Progress".into(),
        state_type: "started".into(),
        comments: vec![],
    };

    let now = 2_000_000_000_000u64;
    let (ctx, api, owner_id) = setup_ctx(vec![issue], Some(now + 60_000));

    let first = sync_owner_once(&ctx, &owner_id, now).unwrap();
    assert_eq!(first.jobs_created, 1);
    assert_eq!(api.refresh_calls(), 1);
    let refreshed = ctx.linear.load_token(&owner_id).unwrap().unwrap();
    assert_eq!(refreshed.access_token, "acc-refreshed-1");
    assert_eq!(refreshed.refresh_token.as_deref(), Some("ref-rotated-1"));

    let link = ctx
        .linear
        .find_link_by_issue(&owner_id, "iss-lease-1")
        .unwrap()
        .unwrap();

    let local_agent = Actor::Agent {
        owner_id: owner_id.clone(),
        client_id: "local-mcp".into(),
        runner_label: "Local Claude".into(),
        token_jti: "local-1".into(),
    };
    let claim_out = ctx
        .store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("local-claim"),
            actor: local_agent.clone(),
            command: EngineCommand::ClaimJob {
                job_id: link.job_id.clone(),
                runner_id: RunnerId::from("local-claude"),
                runner_label: "Local Claude".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: now + 1_000,
            },
        })
        .unwrap();
    let token = claim_out
        .events
        .iter()
        .find_map(|e| match e {
            afterlife_core::JobEvent::JobClaimed { fencing_token, .. } => Some(*fencing_token),
            _ => None,
        })
        .unwrap();

    api.push_comment(
        "iss-lease-1",
        LinearComment {
            id: "cmt-deferred".into(),
            body: "```afterlife-signal\n{\"version\":1,\"source\":\"agent\",\"action\":\"enter_district\",\"district_id\":\"DIST-001\"}\n```".into(),
            url: "https://linear.app/acme/issue/ENG-202#comment-1".into(),
            created_at: "2026-11-27T03:00:00Z".into(),
            author_name: Some("Cloud".into()),
        },
    );

    let deferred = sync_owner_once(&ctx, &owner_id, now + 2_000).unwrap();
    assert_eq!(deferred.signals_deferred_lease, 1);
    assert_eq!(deferred.signals_applied, 0);

    ctx.store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("local-release"),
            actor: local_agent,
            command: EngineCommand::ReleaseLease {
                job_id: link.job_id.clone(),
                fencing_token: token,
                at: now + 3_000,
            },
        })
        .unwrap();

    let applied = sync_owner_once(&ctx, &owner_id, now + 4_000).unwrap();
    assert_eq!(applied.signals_deferred_lease, 0);
    assert_eq!(applied.signals_applied, 1);

    let snap = ctx.store.load_workspace(&owner_id).unwrap();
    let job = snap.engine.find_job(&link.job_id).unwrap();
    assert!(
        job.title
            .starts_with("[ENG-202](https://linear.app/acme/issue/ENG-202)")
    );
    assert!(
        job.raw_input
            .starts_with("Linear: [ENG-202](https://linear.app/acme/issue/ENG-202)")
    );
    assert!(matches!(
        &job.location,
        JobLocation::InDistrict { district_id, .. } if district_id.as_str() == "DIST-001"
    ));
}

#[test]
fn canonical_linear_url_strips_japanese_slug_and_upgrades_legacy_queue_job_links() {
    use afterlife_core::parse_inline_links;

    let slugged = "https://linear.app/acme/issue/ENG-853/日本語のタイトル slug";
    let issue = LinearIssue {
        id: "iss-slug-1".into(),
        identifier: "ENG-853".into(),
        title: "Replace docs link".into(),
        description: Some("Body text".into()),
        url: slugged.into(),
        state_name: "In Progress".into(),
        state_type: "started".into(),
        comments: vec![],
    };

    let now = 2_000_000_000_000u64;
    let (ctx, _api, owner_id) = setup_ctx(vec![issue], None);

    let legacy_out = ctx
        .store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("legacy-queue"),
            actor: Actor::local_human(owner_id.clone()),
            command: EngineCommand::QueueJob {
                title: "[ENG-853] Replace docs link".into(),
                raw_input: format!("Linear: ENG-853 ({slugged})\nState: In Progress\n"),
                target_district_hint: None,
                at: now - 1_000,
            },
        })
        .unwrap();
    let legacy_job_id = legacy_out
        .events
        .iter()
        .find_map(|e| match e {
            afterlife_core::JobEvent::JobQueued { job_id, .. } => Some(job_id.clone()),
            _ => None,
        })
        .unwrap();
    ctx.linear
        .upsert_link(&afterlife_protocol::LinearIssueLink {
            owner_id: owner_id.clone(),
            linear_issue_id: "iss-slug-1".into(),
            linear_identifier: "ENG-853".into(),
            linear_url: slugged.into(),
            job_id: legacy_job_id.clone(),
            last_synced_at_ms: now - 1_000,
        })
        .unwrap();

    sync_owner_once(&ctx, &owner_id, now).unwrap();

    let snap = ctx.store.load_workspace(&owner_id).unwrap();
    let job = snap.engine.find_job(&legacy_job_id).unwrap();
    let title_links = parse_inline_links(&job.title);
    assert_eq!(title_links.links.len(), 1);
    assert_eq!(title_links.links[0].label, "ENG-853");
    assert_eq!(
        title_links.links[0].url,
        "https://linear.app/acme/issue/ENG-853"
    );
    let body_links = parse_inline_links(&job.raw_input);
    assert_eq!(body_links.links.len(), 1);
    assert_eq!(
        body_links.links[0].url,
        "https://linear.app/acme/issue/ENG-853"
    );
    assert!(job.artifacts.iter().any(|a| {
        a.kind == ArtifactKind::Issue && a.uri == "https://linear.app/acme/issue/ENG-853"
    }));
}
