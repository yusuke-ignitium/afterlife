use std::{
    io::{BufRead, BufReader, Write},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Map, Value, json};

use crate::{
    events::{
        EXIT_CODE_HUMAN_STOPPED, EXIT_CODE_TIMEOUT, EventFilter, EventPredicate,
        SSE_BACKLOG_LIMIT, SseLineParser, WaitDecision, build_wait_summary, build_watch_line,
        evaluate_event_record, evaluate_workspace_precheck,
    },
    transport::{CliError, get_json, open_sse_reader},
};

const INITIAL_BACKOFF_MS: u64 = 25;
const MAX_BACKOFF_MS: u64 = 2_000;

fn parse_deadline(payload: &Map<String, Value>) -> Option<Instant> {
    if let Some(ms) = payload.get("timeout_ms").and_then(Value::as_u64) {
        return Some(Instant::now() + Duration::from_millis(ms));
    }
    payload
        .get("timeout")
        .and_then(Value::as_u64)
        .map(|s| Instant::now() + Duration::from_secs(s))
}

fn parse_filter(payload: &Map<String, Value>, default_preds: &str) -> Result<EventFilter, CliError> {
    let raw_preds = payload
        .get("until")
        .or_else(|| payload.get("events"))
        .and_then(Value::as_str)
        .unwrap_or(default_preds);
    let predicates = EventPredicate::parse_csv(raw_preds).map_err(CliError::usage)?;
    let job_id = payload
        .get("job_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let district_id = payload
        .get("district_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let wake_on_stop = payload
        .get("wake_on_stop")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let exit_on_stop = payload
        .get("exit_on_stop")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(EventFilter {
        job_id,
        district_id,
        predicates,
        wake_on_stop,
        exit_on_stop,
    })
}

fn remaining_until(deadline: Option<Instant>) -> Option<Duration> {
    deadline.map(|d| d.saturating_duration_since(Instant::now()))
}

fn is_expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|d| Instant::now() >= d)
}

fn sleep_backoff(backoff: &mut Duration, deadline: Option<Instant>) -> bool {
    let wait_dur = match remaining_until(deadline) {
        Some(rem) if rem.is_zero() => return false,
        Some(rem) => rem.min(*backoff),
        None => *backoff,
    };
    thread::sleep(wait_dur);
    let next_ms = (backoff.as_millis() as u64).saturating_mul(2).min(MAX_BACKOFF_MS);
    *backoff = Duration::from_millis(next_ms);
    !is_expired(deadline)
}

fn resolve_initial_cursor(
    since_seq: Option<u64>,
    filter: &EventFilter,
) -> Result<Result<(u64, Option<Value>), (Value, u8)>, CliError> {
    let ws = get_json("/v1/workspace")?;
    let latest_seq = ws.get("latest_seq").and_then(Value::as_u64).unwrap_or(0);
    let should_precheck = match since_seq {
        None => true,
        Some(since) => latest_seq.saturating_sub(since) > SSE_BACKLOG_LIMIT || latest_seq <= since,
    };
    if should_precheck {
        let decision = evaluate_workspace_precheck(&ws, filter);
        match decision {
            WaitDecision::Matched { .. } => {
                let summary = build_wait_summary(&decision, Some(&ws), true);
                return Ok(Err((summary, 0)));
            }
            WaitDecision::HumanStopped { .. } => {
                let summary = build_wait_summary(&decision, Some(&ws), true);
                return Ok(Err((summary, EXIT_CODE_HUMAN_STOPPED)));
            }
            WaitDecision::Ignore => {}
        }
    }
    let cursor = match since_seq {
        None => latest_seq,
        Some(since) if latest_seq.saturating_sub(since) > SSE_BACKLOG_LIMIT => latest_seq,
        Some(since) => since,
    };
    Ok(Ok((cursor, Some(ws))))
}

pub fn run_wait_command(payload: &Map<String, Value>) -> Result<(Value, u8), CliError> {
    let filter = parse_filter(payload, "approved,reworked,resumed")?;
    let since_seq = payload.get("since_seq").and_then(Value::as_u64);
    let deadline = parse_deadline(payload);

    let mut last_seq = match resolve_initial_cursor(since_seq, &filter)? {
        Ok((cursor, _)) => cursor,
        Err(done) => return Ok(done),
    };

    let mut backoff = Duration::from_millis(INITIAL_BACKOFF_MS);
    loop {
        if is_expired(deadline) {
            return Ok((
                json!({ "timed_out": true, "last_seq": last_seq }),
                EXIT_CODE_TIMEOUT,
            ));
        }

        let read_timeout = remaining_until(deadline);
        let stream = match open_sse_reader(last_seq, read_timeout) {
            Ok(reader) => {
                backoff = Duration::from_millis(INITIAL_BACKOFF_MS);
                reader
            }
            Err(_) => {
                if !sleep_backoff(&mut backoff, deadline) {
                    return Ok((
                        json!({ "timed_out": true, "last_seq": last_seq }),
                        EXIT_CODE_TIMEOUT,
                    ));
                }
                if let Ok(ws) = get_json("/v1/workspace") {
                    let latest = ws.get("latest_seq").and_then(Value::as_u64).unwrap_or(last_seq);
                    if latest.saturating_sub(last_seq) > SSE_BACKLOG_LIMIT {
                        let dec = evaluate_workspace_precheck(&ws, &filter);
                        match dec {
                            WaitDecision::Matched { .. } => {
                                return Ok((build_wait_summary(&dec, Some(&ws), true), 0));
                            }
                            WaitDecision::HumanStopped { .. } => {
                                return Ok((
                                    build_wait_summary(&dec, Some(&ws), true),
                                    EXIT_CODE_HUMAN_STOPPED,
                                ));
                            }
                            WaitDecision::Ignore => {
                                last_seq = latest;
                            }
                        }
                    }
                }
                continue;
            }
        };

        let mut reader = BufReader::new(stream);
        let mut parser = SseLineParser::default();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Some(frame) = parser.push_line(&line)
                        && let Ok(record) = serde_json::from_str::<Value>(&frame.data)
                    {
                        let rec_seq = record.get("seq").and_then(Value::as_u64).unwrap_or(0);
                        if rec_seq > last_seq.saturating_add(1)
                            && rec_seq.saturating_sub(last_seq) > SSE_BACKLOG_LIMIT
                        {
                            if let Ok(ws) = get_json("/v1/workspace") {
                                let dec = evaluate_workspace_precheck(&ws, &filter);
                                match dec {
                                    WaitDecision::Matched { .. } => {
                                        return Ok((build_wait_summary(&dec, Some(&ws), true), 0));
                                    }
                                    WaitDecision::HumanStopped { .. } => {
                                        return Ok((
                                            build_wait_summary(&dec, Some(&ws), true),
                                            EXIT_CODE_HUMAN_STOPPED,
                                        ));
                                    }
                                    WaitDecision::Ignore => {}
                                }
                            }
                        }
                        if rec_seq > last_seq {
                            last_seq = rec_seq;
                        }
                        let decision = evaluate_event_record(&record, &filter);
                        match decision {
                            WaitDecision::Matched { .. } => {
                                let ws = get_json("/v1/workspace").ok();
                                return Ok((build_wait_summary(&decision, ws.as_ref(), false), 0));
                            }
                            WaitDecision::HumanStopped { .. } => {
                                let ws = get_json("/v1/workspace").ok();
                                return Ok((
                                    build_wait_summary(&decision, ws.as_ref(), false),
                                    EXIT_CODE_HUMAN_STOPPED,
                                ));
                            }
                            WaitDecision::Ignore => {}
                        }
                    }
                    if is_expired(deadline) {
                        return Ok((
                            json!({ "timed_out": true, "last_seq": last_seq }),
                            EXIT_CODE_TIMEOUT,
                        ));
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    if is_expired(deadline) {
                        return Ok((
                            json!({ "timed_out": true, "last_seq": last_seq }),
                            EXIT_CODE_TIMEOUT,
                        ));
                    }
                    break;
                }
                Err(_) => break,
            }
        }

        if !sleep_backoff(&mut backoff, deadline) {
            return Ok((
                json!({ "timed_out": true, "last_seq": last_seq }),
                EXIT_CODE_TIMEOUT,
            ));
        }
    }
}

pub fn run_watch_command(payload: &Map<String, Value>) -> Result<(), CliError> {
    let mut filter = parse_filter(payload, "any")?;
    filter.wake_on_stop = filter.predicates.contains(&EventPredicate::Any);
    let since_seq_opt = payload.get("since_seq").and_then(Value::as_u64);
    let debounce_ms = payload
        .get("debounce_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_events = payload.get("max_events").and_then(Value::as_u64);
    let deadline = parse_deadline(payload);

    let mut last_seq = match since_seq_opt {
        Some(s) => s,
        None => get_json("/v1/workspace")?
            .get("latest_seq")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    };

    let mut emitted_count: u64 = 0;
    let mut last_emit_at: Option<Instant> = None;
    let mut backoff = Duration::from_millis(INITIAL_BACKOFF_MS);
    let stdout = std::io::stdout();

    loop {
        if is_expired(deadline) {
            return Ok(());
        }
        let stream = match open_sse_reader(last_seq, remaining_until(deadline)) {
            Ok(reader) => {
                backoff = Duration::from_millis(INITIAL_BACKOFF_MS);
                reader
            }
            Err(_) => {
                if !sleep_backoff(&mut backoff, deadline) {
                    return Ok(());
                }
                continue;
            }
        };

        let mut reader = BufReader::new(stream);
        let mut parser = SseLineParser::default();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Some(frame) = parser.push_line(&line)
                        && let Ok(record) = serde_json::from_str::<Value>(&frame.data)
                    {
                        let rec_seq = record.get("seq").and_then(Value::as_u64).unwrap_or(0);
                        if rec_seq > last_seq {
                            last_seq = rec_seq;
                        }
                        let decision = evaluate_event_record(&record, &filter);
                        if !matches!(decision, WaitDecision::Ignore) {
                            if debounce_ms > 0
                                && let Some(prev) = last_emit_at
                            {
                                let gap = Duration::from_millis(debounce_ms);
                                let elapsed = prev.elapsed();
                                if elapsed < gap {
                                    thread::sleep(gap - elapsed);
                                }
                            }
                            let out_val = build_watch_line(&decision, &record);
                            let mut lock = stdout.lock();
                            let _ = writeln!(lock, "{}", serde_json::to_string(&out_val).unwrap());
                            let _ = lock.flush();
                            last_emit_at = Some(Instant::now());
                            emitted_count += 1;
                            if max_events.is_some_and(|max| emitted_count >= max) {
                                return Ok(());
                            }
                        }
                    }
                    if is_expired(deadline) {
                        return Ok(());
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    if is_expired(deadline) {
                        return Ok(());
                    }
                    break;
                }
                Err(_) => break,
            }
        }

        if !sleep_backoff(&mut backoff, deadline) {
            return Ok(());
        }
    }
}
