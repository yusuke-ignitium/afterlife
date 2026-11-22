use crate::{
    engine::{AfterlifeEngine, command::EngineCommand, policy::validate_lease_ttl},
    error::TransitionError,
    ids::FeedbackId,
};

fn require_non_empty(field: &str, value: &str) -> Result<(), TransitionError> {
    if value.trim().is_empty() {
        return Err(TransitionError::InvalidInput {
            field: field.into(),
            reason: "must not be empty".into(),
        });
    }
    Ok(())
}

fn validate_artifact_uri(uri: &str) -> Result<(), TransitionError> {
    require_non_empty("uri", uri)?;
    let lower = uri.trim().to_ascii_lowercase();
    for forbidden in ["javascript:", "data:", "vbscript:"] {
        if lower.starts_with(forbidden) {
            return Err(TransitionError::InvalidInput {
                field: "uri".into(),
                reason: format!("forbidden URI scheme '{forbidden}'"),
            });
        }
    }
    Ok(())
}

fn validate_sha256(sha: &str) -> Result<(), TransitionError> {
    let valid = sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(TransitionError::InvalidInput {
            field: "sha256".into(),
            reason: "must be a 64-character hexadecimal SHA-256 digest".into(),
        });
    }
    Ok(())
}

fn validate_feedback_refs(
    engine: &AfterlifeEngine,
    ids: &[FeedbackId],
) -> Result<(), TransitionError> {
    for fid in ids {
        if !engine.feedbacks.iter().any(|f| &f.id == fid) {
            return Err(TransitionError::InvalidInput {
                field: "source_feedback_ids".into(),
                reason: format!("unknown feedback id '{fid}'"),
            });
        }
    }
    Ok(())
}

pub fn validate_command(
    engine: &AfterlifeEngine,
    cmd: &EngineCommand,
) -> Result<(), TransitionError> {
    match cmd {
        EngineCommand::QueueJob {
            title,
            target_district_hint,
            ..
        } => {
            require_non_empty("title", title)?;
            if let Some(did) = target_district_hint {
                if engine.find_district(did).is_none() {
                    return Err(TransitionError::DistrictNotFound(did.clone()));
                }
            }
            Ok(())
        }
        EngineCommand::UpdateJob {
            title,
            raw_input,
            spec_summary,
            target_district_hint,
            ..
        } => {
            if title.is_none()
                && raw_input.is_none()
                && spec_summary.is_none()
                && target_district_hint.is_none()
            {
                return Err(TransitionError::InvalidInput {
                    field: "update".into(),
                    reason: "at least one field (title, raw_input, spec_summary, target_district_hint) must be provided".into(),
                });
            }
            if let Some(t) = title {
                require_non_empty("title", t)?;
            }
            if let Some(did) = target_district_hint {
                if engine.find_district(did).is_none() {
                    return Err(TransitionError::DistrictNotFound(did.clone()));
                }
            }
            Ok(())
        }
        EngineCommand::ArchiveJob { reason, .. } => {
            if let Some(r) = reason {
                require_non_empty("reason", r)?;
            }
            Ok(())
        }
        EngineCommand::ClaimJob {
            runner_label,
            ttl_ms,
            ..
        } => {
            require_non_empty("runner_label", runner_label)?;
            validate_lease_ttl(*ttl_ms)?;
            Ok(())
        }
        EngineCommand::HeartbeatLease { ttl_ms, .. } => {
            validate_lease_ttl(*ttl_ms)?;
            Ok(())
        }
        EngineCommand::AttachArtifact {
            label, uri, sha256, ..
        } => {
            require_non_empty("label", label)?;
            validate_artifact_uri(uri)?;
            if let Some(h) = sha256 {
                validate_sha256(h)?;
            }
            Ok(())
        }
        EngineCommand::RecordGateVerification { summary, .. } => {
            require_non_empty("summary", summary)?;
            Ok(())
        }
        EngineCommand::SubmitFeedback {
            district_id,
            job_id,
            body,
            ..
        } => {
            require_non_empty("body", body)?;
            if engine.find_district(district_id).is_none() {
                return Err(TransitionError::DistrictNotFound(district_id.clone()));
            }
            if let Some(jid) = job_id {
                if engine.find_job(jid).is_none() {
                    return Err(TransitionError::JobNotFound(jid.clone()));
                }
            }
            Ok(())
        }
        EngineCommand::ProposePhilosophyUpdate {
            summary,
            principles,
            rationale,
            source_feedback_ids,
            ..
        } => {
            require_non_empty("summary", summary)?;
            require_non_empty("rationale", rationale)?;
            if principles.is_empty() {
                return Err(TransitionError::InvalidInput {
                    field: "principles".into(),
                    reason: "must contain at least one principle".into(),
                });
            }
            for (idx, p) in principles.iter().enumerate() {
                require_non_empty(&format!("principles[{idx}].id"), &p.id)?;
                require_non_empty(&format!("principles[{idx}].title"), &p.title)?;
                require_non_empty(&format!("principles[{idx}].statement"), &p.statement)?;
            }
            validate_feedback_refs(engine, source_feedback_ids)?;
            Ok(())
        }
        EngineCommand::ProposeSpecUpdate {
            changelog,
            rationale,
            source_feedback_ids,
            ..
        } => {
            require_non_empty("changelog", changelog)?;
            require_non_empty("rationale", rationale)?;
            validate_feedback_refs(engine, source_feedback_ids)?;
            Ok(())
        }
        EngineCommand::AdvanceJob { fields, .. } => {
            for k in fields.keys() {
                require_non_empty("fields", k)?;
            }
            Ok(())
        }
        EngineCommand::ApproveJob { note, .. } => {
            if let Some(n) = note {
                require_non_empty("note", n)?;
            }
            Ok(())
        }
        EngineCommand::ReworkJob { reason, .. } => {
            if let Some(r) = reason {
                require_non_empty("reason", r)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
