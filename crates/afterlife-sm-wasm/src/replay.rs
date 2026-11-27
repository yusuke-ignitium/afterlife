use afterlife_core::{
    Actor, AfterlifeEngine, DistrictId, EngineCommand, JobLocation,
};
use serde_json::{Value, json};

use crate::{
    runner::{
        BASE_TS, claim_and_advance_in_engine, format_context_view, location_json, queue_scratch_job,
    },
    signal::{SignalAction, SignalSource, extract_linear_signal},
};

pub fn handle_replay(
    engine: &mut AfterlifeEngine,
    agent: &Actor,
    human: &Actor,
    comments: Vec<String>,
) -> Result<Value, String> {
    let job_id = queue_scratch_job(engine, agent, BASE_TS)?;
    let mut ts = BASE_TS + 100;
    let mut signals_seen = 0usize;
    let mut last_rework_reason: Option<String> = None;
    let mut last_sync_error: Option<String> = None;

    for body in comments {
        let Ok(Some(parsed)) = extract_linear_signal(&body) else {
            continue;
        };
        signals_seen += 1;
        ts += 100;
        let sig = &parsed.signal;
        match (sig.source, sig.action) {
            (SignalSource::Agent, SignalAction::EnterDistrict) => {
                if let Some(did) = sig.district_id.as_deref()
                    && engine
                        .apply(
                            agent,
                            EngineCommand::AssignJobToDistrict {
                                job_id: job_id.clone(),
                                district_id: DistrictId::from(did.to_string()),
                                expected_revision: None,
                                at: ts,
                            },
                        )
                        .is_ok()
                {
                    last_sync_error = None;
                }
            }
            (SignalSource::Agent, SignalAction::AdvanceStage) => {
                if matches!(
                    engine.find_job(&job_id).map(|j| &j.location),
                    Some(JobLocation::Queue)
                ) && let Some(did) = sig.district_id.as_deref()
                {
                    let _ = engine.apply(
                        agent,
                        EngineCommand::AssignJobToDistrict {
                            job_id: job_id.clone(),
                            district_id: DistrictId::from(did.to_string()),
                            expected_revision: None,
                            at: ts,
                        },
                    );
                }
                let runner = sig.runner_label.as_deref().unwrap_or("Cloud Agent");
                if claim_and_advance_in_engine(
                    engine,
                    agent,
                    &job_id,
                    parsed.merged_fields(),
                    sig.gate.clone(),
                    &sig.artifacts,
                    runner,
                    ts + 10,
                )
                .is_ok()
                {
                    last_sync_error = None;
                }
            }
            (SignalSource::Afterlife, SignalAction::Reworked) => {
                if engine
                    .apply(
                        human,
                        EngineCommand::ReworkJob {
                            job_id: job_id.clone(),
                            expected_revision: None,
                            fencing_token: None,
                            reason: sig.reason.clone(),
                            at: ts,
                        },
                    )
                    .is_ok()
                {
                    last_rework_reason = sig.reason.clone();
                    last_sync_error = None;
                }
            }
            (SignalSource::Afterlife, SignalAction::Approved) => {
                if engine
                    .apply(
                        human,
                        EngineCommand::ApproveJob {
                            job_id: job_id.clone(),
                            note: sig.summary.clone(),
                            expected_revision: None,
                            at: ts,
                        },
                    )
                    .is_ok()
                {
                    last_rework_reason = None;
                    last_sync_error = None;
                }
            }
            (SignalSource::Afterlife, SignalAction::Stopped) => {
                let _ = engine.apply(
                    human,
                    EngineCommand::StopJob {
                        job_id: job_id.clone(),
                        reason: sig.reason.clone(),
                        expected_revision: None,
                        at: ts,
                    },
                );
            }
            (SignalSource::Afterlife, SignalAction::Resumed) => {
                let _ = engine.apply(
                    human,
                    EngineCommand::ResumeJob {
                        job_id: job_id.clone(),
                        expected_revision: None,
                        at: ts,
                    },
                );
            }
            (SignalSource::Afterlife, SignalAction::SyncError) => {
                last_sync_error = sig.reason.clone();
            }
            _ => {}
        }
    }

    let job = engine.find_job(&job_id).unwrap().clone();
    Ok(json!({
        "ok": true,
        "signals_seen": signals_seen,
        "location": location_json(&job.location),
        "stopped": job.stopped,
        "stop_reason": job.stopped_reason,
        "last_rework_reason": last_rework_reason,
        "last_sync_error": last_sync_error,
        "current_context": format_context_view(engine, &job_id),
    }))
}
