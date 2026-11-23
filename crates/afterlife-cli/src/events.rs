use std::collections::BTreeSet;

use serde_json::{Value, json};

pub const EXIT_CODE_HUMAN_STOPPED: u8 = 3;
pub const EXIT_CODE_TIMEOUT: u8 = 124;
pub const SSE_BACKLOG_LIMIT: u64 = 500;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventPredicate {
    Approved,
    Reworked,
    Resumed,
    Stopped,
    Queued,
    Assigned,
    Claimed,
    LeaseReleased,
    ProposalResolved,
    Any,
    EventType(String),
}

impl EventPredicate {
    pub fn parse_csv(raw: &str) -> Result<Vec<Self>, String> {
        let mut out = Vec::new();
        for token in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let lower = token.to_ascii_lowercase();
            let pred = match lower.as_str() {
                "approved" | "approval_resolved" => Self::Approved,
                "reworked" | "job_reworked" => Self::Reworked,
                "resumed" | "job_resumed" => Self::Resumed,
                "stopped" | "job_stopped" => Self::Stopped,
                "queued" | "job_queued" => Self::Queued,
                "assigned" => Self::Assigned,
                "claimed" | "job_claimed" => Self::Claimed,
                "lease_released" | "job_lease_released" => Self::LeaseReleased,
                "proposal_resolved" => Self::ProposalResolved,
                "any" | "any-change" | "any_change" => Self::Any,
                other if other.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') => {
                    Self::EventType(other.to_string())
                }
                other => {
                    return Err(format!("invalid event predicate '{other}'"));
                }
            };
            if !out.contains(&pred) {
                out.push(pred);
            }
        }
        if out.is_empty() {
            return Err("at least one predicate must be specified".into());
        }
        Ok(out)
    }

    pub fn includes_stopped(preds: &[Self]) -> bool {
        preds.iter().any(|p| {
            matches!(p, Self::Stopped)
                || matches!(p, Self::EventType(t) if t == "job_stopped")
        })
    }
}

#[derive(Clone, Debug)]
pub struct EventFilter {
    pub job_id: Option<String>,
    pub district_id: Option<String>,
    pub predicates: Vec<EventPredicate>,
    pub wake_on_stop: bool,
    pub exit_on_stop: bool,
}

impl EventFilter {
    pub fn allows_stop_as_match(&self) -> bool {
        self.wake_on_stop || EventPredicate::includes_stopped(&self.predicates)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseFrame {
    pub id: Option<String>,
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct SseLineParser {
    id: Option<String>,
    event: Option<String>,
    data_lines: Vec<String>,
}

impl SseLineParser {
    pub fn push_line(&mut self, raw_line: &str) -> Option<SseFrame> {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            return self.flush();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, val) = match line.split_once(':') {
            Some((k, v)) => (k, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "id" => self.id = Some(val.to_string()),
            "event" => self.event = Some(val.to_string()),
            "data" => self.data_lines.push(val.to_string()),
            _ => {}
        }
        None
    }

    pub fn flush(&mut self) -> Option<SseFrame> {
        if self.data_lines.is_empty() {
            self.id = None;
            self.event = None;
            return None;
        }
        let frame = SseFrame {
            id: self.id.take(),
            event: self.event.take(),
            data: self.data_lines.join("\n"),
        };
        self.data_lines.clear();
        Some(frame)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum WaitDecision {
    Ignore,
    Matched {
        outcome: String,
        matched_event: String,
        job_id: Option<String>,
        seq: u64,
        event: Value,
    },
    HumanStopped {
        job_id: Option<String>,
        seq: u64,
        event: Value,
    },
}

fn event_job_id(ev: &Value) -> Option<&str> {
    ev.get("job_id").and_then(Value::as_str)
}

fn event_matches_district(ev: &Value, district_id: &str) -> bool {
    if ev.get("district_id").and_then(Value::as_str) == Some(district_id) {
        return true;
    }
    if ev
        .get("to")
        .and_then(|v| v.get("district_id"))
        .and_then(Value::as_str)
        == Some(district_id)
    {
        return true;
    }
    if ev
        .get("from")
        .and_then(|v| v.get("district_id"))
        .and_then(Value::as_str)
        == Some(district_id)
    {
        return true;
    }
    false
}

fn classify_event_outcome(ev: &Value) -> &'static str {
    let ev_type = ev.get("type").and_then(Value::as_str).unwrap_or("");
    match ev_type {
        "job_stopped" => "stopped",
        "job_resumed" => "resumed",
        "job_reworked" => "reworked",
        "job_queued" => "queued",
        "job_claimed" => "claimed",
        "job_lease_released" => "lease_released",
        "philosophy_updated"
        | "spec_updated"
        | "template_updated"
        | "builtin_templates_synced"
        | "district_created"
        | "district_archived"
        | "district_restored" => "proposal_resolved",
        "step_recorded" => match ev.get("outcome").and_then(Value::as_str) {
            Some("approved") => "approved",
            Some("reworked") => "reworked",
            _ => "step_recorded",
        },
        "job_transitioned" => {
            let from_state = ev
                .get("from")
                .and_then(|v| v.get("state"))
                .and_then(Value::as_str);
            let from_stage = ev
                .get("from")
                .and_then(|v| v.get("stage_id"))
                .and_then(Value::as_str);
            let to_state = ev
                .get("to")
                .and_then(|v| v.get("state"))
                .and_then(Value::as_str);
            if from_stage == Some("approve") || to_state == Some("completed") {
                "approved"
            } else if from_state == Some("queue") && to_state == Some("in_district") {
                "assigned"
            } else {
                "transitioned"
            }
        }
        _ => "event",
    }
}

fn predicate_matches_event(pred: &EventPredicate, ev: &Value) -> bool {
    let ev_type = ev.get("type").and_then(Value::as_str).unwrap_or("");
    let outcome = classify_event_outcome(ev);
    match pred {
        EventPredicate::Approved => outcome == "approved",
        EventPredicate::Reworked => outcome == "reworked",
        EventPredicate::Resumed => ev_type == "job_resumed",
        EventPredicate::Stopped => ev_type == "job_stopped",
        EventPredicate::Queued => ev_type == "job_queued",
        EventPredicate::Assigned => outcome == "assigned",
        EventPredicate::Claimed => ev_type == "job_claimed",
        EventPredicate::LeaseReleased => ev_type == "job_lease_released",
        EventPredicate::ProposalResolved => outcome == "proposal_resolved",
        EventPredicate::Any => ev_type != "job_stopped",
        EventPredicate::EventType(expected) => ev_type == expected,
    }
}

pub fn evaluate_event_record(record: &Value, filter: &EventFilter) -> WaitDecision {
    let seq = record.get("seq").and_then(Value::as_u64).unwrap_or(0);
    let Some(ev) = record.get("event") else {
        return WaitDecision::Ignore;
    };
    let ev_type = ev.get("type").and_then(Value::as_str).unwrap_or("");
    let ev_job = event_job_id(ev).map(str::to_string);

    if let Some(ref want_job) = filter.job_id {
        if ev_job.as_deref() != Some(want_job.as_str()) {
            return WaitDecision::Ignore;
        }
    }
    if let Some(ref want_dist) = filter.district_id {
        if !event_matches_district(ev, want_dist) {
            return WaitDecision::Ignore;
        }
    }

    if ev_type == "job_stopped" {
        if filter.allows_stop_as_match() {
            return WaitDecision::Matched {
                outcome: "stopped".into(),
                matched_event: ev_type.into(),
                job_id: ev_job,
                seq,
                event: ev.clone(),
            };
        }
        if filter.exit_on_stop {
            return WaitDecision::HumanStopped {
                job_id: ev_job,
                seq,
                event: ev.clone(),
            };
        }
        return WaitDecision::Ignore;
    }

    let matched = filter
        .predicates
        .iter()
        .any(|p| predicate_matches_event(p, ev));
    if !matched {
        return WaitDecision::Ignore;
    }
    WaitDecision::Matched {
        outcome: classify_event_outcome(ev).into(),
        matched_event: ev_type.into(),
        job_id: ev_job,
        seq,
        event: ev.clone(),
    }
}

pub fn find_job_in_workspace<'a>(workspace: &'a Value, job_id: &str) -> Option<&'a Value> {
    workspace
        .get("engine")
        .and_then(|e| e.get("jobs"))
        .and_then(Value::as_array)?
        .iter()
        .find(|j| j.get("id").and_then(Value::as_str) == Some(job_id))
}

fn is_job_at_approval_gate(workspace: &Value, job: &Value) -> bool {
    let loc = job.get("location");
    let state = loc.and_then(|l| l.get("state")).and_then(Value::as_str);
    if state != Some("in_district") {
        return false;
    }
    let Some(did) = loc.and_then(|l| l.get("district_id")).and_then(Value::as_str) else {
        return false;
    };
    let Some(sid) = loc.and_then(|l| l.get("stage_id")).and_then(Value::as_str) else {
        return false;
    };
    let Some(districts) = workspace
        .get("engine")
        .and_then(|e| e.get("districts"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    let Some(dist) = districts
        .iter()
        .find(|d| d.get("id").and_then(Value::as_str) == Some(did))
    else {
        return false;
    };
    let target_ver = job
        .get("pinned_spec_version")
        .and_then(Value::as_u64)
        .or_else(|| dist.get("active_spec_version").and_then(Value::as_u64));
    let Some(specs) = dist.get("spec_history").and_then(Value::as_array) else {
        return false;
    };
    let spec = specs
        .iter()
        .find(|s| s.get("version").and_then(Value::as_u64) == target_ver)
        .or_else(|| specs.last());
    let Some(stages) = spec.and_then(|s| s.get("stages")).and_then(Value::as_array) else {
        return false;
    };
    stages
        .iter()
        .find(|st| st.get("id").and_then(Value::as_str) == Some(sid))
        .and_then(|st| st.get("kind"))
        .and_then(Value::as_str)
        == Some("ApprovalGate")
}

pub fn evaluate_workspace_precheck(workspace: &Value, filter: &EventFilter) -> WaitDecision {
    let latest_seq = workspace
        .get("latest_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    if let Some(ref job_id) = filter.job_id {
        let Some(job) = find_job_in_workspace(workspace, job_id) else {
            return WaitDecision::Ignore;
        };
        let stopped = job.get("stopped").and_then(Value::as_bool).unwrap_or(false);
        if stopped {
            if filter.allows_stop_as_match() {
                return WaitDecision::Matched {
                    outcome: "stopped".into(),
                    matched_event: "workspace_state".into(),
                    job_id: Some(job_id.clone()),
                    seq: latest_seq,
                    event: json!({ "type": "job_stopped", "job_id": job_id }),
                };
            }
            if filter.exit_on_stop {
                return WaitDecision::HumanStopped {
                    job_id: Some(job_id.clone()),
                    seq: latest_seq,
                    event: json!({ "type": "job_stopped", "job_id": job_id }),
                };
            }
            return WaitDecision::Ignore;
        }

        let loc_state = job
            .get("location")
            .and_then(|l| l.get("state"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let preds: BTreeSet<&EventPredicate> = filter.predicates.iter().collect();
        if preds.contains(&EventPredicate::Approved) && loc_state == "completed" {
            return WaitDecision::Matched {
                outcome: "approved".into(),
                matched_event: "workspace_state".into(),
                job_id: Some(job_id.clone()),
                seq: latest_seq,
                event: json!({ "type": "job_transitioned", "job_id": job_id, "to": job["location"] }),
            };
        }
        let last_step_outcome = job
            .get("step_records")
            .and_then(Value::as_array)
            .and_then(|arr| arr.last())
            .and_then(|rec| rec.get("outcome"))
            .and_then(Value::as_str);
        if preds.contains(&EventPredicate::Reworked)
            && last_step_outcome == Some("reworked")
            && !is_job_at_approval_gate(workspace, job)
        {
            return WaitDecision::Matched {
                outcome: "reworked".into(),
                matched_event: "workspace_state".into(),
                job_id: Some(job_id.clone()),
                seq: latest_seq,
                event: json!({ "type": "job_reworked", "job_id": job_id, "to": job["location"] }),
            };
        }
        if preds.contains(&EventPredicate::Queued) && loc_state == "queue" {
            return WaitDecision::Matched {
                outcome: "queued".into(),
                matched_event: "workspace_state".into(),
                job_id: Some(job_id.clone()),
                seq: latest_seq,
                event: json!({ "type": "job_queued", "job_id": job_id }),
            };
        }
        return WaitDecision::Ignore;
    }

    if filter.predicates.contains(&EventPredicate::Queued) {
        if let Some(jobs) = workspace
            .get("engine")
            .and_then(|e| e.get("jobs"))
            .and_then(Value::as_array)
        {
            for job in jobs {
                let loc_state = job
                    .get("location")
                    .and_then(|l| l.get("state"))
                    .and_then(Value::as_str);
                if loc_state != Some("queue") {
                    continue;
                }
                if let Some(ref did) = filter.district_id {
                    let hint = job.get("target_district_hint").and_then(Value::as_str);
                    if hint != Some(did.as_str()) {
                        continue;
                    }
                }
                let jid = job.get("id").and_then(Value::as_str).map(str::to_string);
                return WaitDecision::Matched {
                    outcome: "queued".into(),
                    matched_event: "workspace_state".into(),
                    job_id: jid.clone(),
                    seq: latest_seq,
                    event: json!({ "type": "job_queued", "job_id": jid }),
                };
            }
        }
    }

    WaitDecision::Ignore
}

pub fn build_wait_summary(
    decision: &WaitDecision,
    workspace: Option<&Value>,
    already_satisfied: bool,
) -> Value {
    let (matched_event, outcome, job_id, seq, event) = match decision {
        WaitDecision::Matched {
            outcome,
            matched_event,
            job_id,
            seq,
            event,
        } => (matched_event.as_str(), outcome.as_str(), job_id.as_deref(), *seq, event),
        WaitDecision::HumanStopped { job_id, seq, event } => {
            ("job_stopped", "stopped", job_id.as_deref(), *seq, event)
        }
        WaitDecision::Ignore => return json!({}),
    };

    let job_obj = job_id.and_then(|jid| workspace.and_then(|ws| find_job_in_workspace(ws, jid)));
    let location = job_obj
        .and_then(|j| j.get("location"))
        .cloned()
        .unwrap_or(Value::Null);
    let stopped = job_obj
        .and_then(|j| j.get("stopped"))
        .and_then(Value::as_bool)
        .unwrap_or(outcome == "stopped");
    let next_action_hint = match outcome {
        "stopped" => "human_stopped",
        "approved" if location.get("state").and_then(Value::as_str) == Some("completed") => {
            "job_completed"
        }
        "approved" | "reworked" | "resumed" | "assigned" | "queued" => "claim_and_continue",
        "proposal_resolved" => "reload_context",
        _ => "observe_event",
    };

    json!({
        "matched_event": matched_event,
        "outcome": outcome,
        "job_id": job_id,
        "location": location,
        "stopped": stopped,
        "next_action_hint": next_action_hint,
        "seq": seq,
        "already_satisfied": already_satisfied,
        "event": event,
    })
}

pub fn build_watch_line(decision: &WaitDecision, record: &Value) -> Value {
    match decision {
        WaitDecision::Matched {
            outcome,
            matched_event,
            job_id,
            seq,
            event,
        } => json!({
            "seq": seq,
            "event": matched_event,
            "outcome": outcome,
            "job_id": job_id,
            "command_id": record.get("command_id").cloned().unwrap_or(Value::Null),
            "payload": event,
        }),
        WaitDecision::HumanStopped { job_id, seq, event } => json!({
            "seq": seq,
            "event": "job_stopped",
            "outcome": "stopped",
            "job_id": job_id,
            "command_id": record.get("command_id").cloned().unwrap_or(Value::Null),
            "payload": event,
        }),
        WaitDecision::Ignore => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_parser_ignores_keepalive_comments_and_assembles_frames() {
        let mut parser = SseLineParser::default();
        assert!(parser.push_line(": keep-alive\n").is_none());
        assert!(parser.push_line("\n").is_none());
        assert!(parser.push_line("id: 42\n").is_none());
        assert!(parser.push_line("event: afterlife_event\n").is_none());
        assert!(parser.push_line("data: {\"seq\":42}\n").is_none());
        let frame = parser.push_line("\n").expect("frame emitted on blank line");
        assert_eq!(frame.id.as_deref(), Some("42"));
        assert_eq!(frame.event.as_deref(), Some("afterlife_event"));
        assert_eq!(frame.data, "{\"seq\":42}");
    }

    #[test]
    fn human_stop_does_not_spuriously_wake_and_honors_exit_or_wake_flags() {
        let stopped_rec = json!({
            "seq": 10,
            "event": { "type": "job_stopped", "job_id": "JOB-001", "revision": 5 }
        });

        let normal = EventFilter {
            job_id: Some("JOB-001".into()),
            district_id: None,
            predicates: EventPredicate::parse_csv("approved,reworked,resumed").unwrap(),
            wake_on_stop: false,
            exit_on_stop: false,
        };
        assert_eq!(evaluate_event_record(&stopped_rec, &normal), WaitDecision::Ignore);

        let any_without_stop = EventFilter {
            job_id: Some("JOB-001".into()),
            district_id: None,
            predicates: EventPredicate::parse_csv("any").unwrap(),
            wake_on_stop: false,
            exit_on_stop: false,
        };
        assert_eq!(
            evaluate_event_record(&stopped_rec, &any_without_stop),
            WaitDecision::Ignore
        );

        let exit_on_stop = EventFilter {
            exit_on_stop: true,
            ..normal.clone()
        };
        assert!(matches!(
            evaluate_event_record(&stopped_rec, &exit_on_stop),
            WaitDecision::HumanStopped { seq: 10, .. }
        ));

        let wake_on_stop = EventFilter {
            wake_on_stop: true,
            ..normal
        };
        assert!(matches!(
            evaluate_event_record(&stopped_rec, &wake_on_stop),
            WaitDecision::Matched { ref outcome, seq: 10, .. } if outcome == "stopped"
        ));
    }

    #[test]
    fn precheck_catches_already_completed_or_reworked_or_stopped_job() {
        let ws_done = json!({
            "latest_seq": 99,
            "engine": {
                "districts": [],
                "jobs": [{
                    "id": "JOB-005",
                    "stopped": false,
                    "location": { "state": "completed", "district_id": "DIST-001", "terminal_stage_id": "done", "completed_at": 1 }
                }]
            }
        });
        let filter = EventFilter {
            job_id: Some("JOB-005".into()),
            district_id: None,
            predicates: EventPredicate::parse_csv("approved,reworked").unwrap(),
            wake_on_stop: false,
            exit_on_stop: true,
        };
        let dec = evaluate_workspace_precheck(&ws_done, &filter);
        assert!(matches!(
            dec,
            WaitDecision::Matched { ref outcome, seq: 99, .. } if outcome == "approved"
        ));

        let ws_stopped = json!({
            "latest_seq": 100,
            "engine": {
                "districts": [],
                "jobs": [{
                    "id": "JOB-005",
                    "stopped": true,
                    "location": { "state": "in_district", "district_id": "DIST-001", "stage_id": "build", "sub_stage_id": null }
                }]
            }
        });
        assert!(matches!(
            evaluate_workspace_precheck(&ws_stopped, &filter),
            WaitDecision::HumanStopped { seq: 100, .. }
        ));
    }
}
