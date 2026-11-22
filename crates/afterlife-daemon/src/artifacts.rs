use std::sync::Arc;

use afterlife_core::{
    Artifact, ArtifactId, FencingToken, JobId, JobLocation, MAX_ARTIFACT_BLOB_BYTES, OwnerId,
    TimestampMs, TransitionError, engine::policy, is_valid_lower_sha256, parse_blob_uri,
    sanitize_owner_segment,
};
use afterlife_protocol::{ArtifactBlobStore, Principal, Scope};
use axum::http::StatusCode;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    AppDaemonState,
    rest::{DispatchError, transition_error_to_dispatch},
};

#[derive(Debug)]
pub struct OwnedBlobRead {
    pub sha256: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum ArtifactBlobError {
    NotBlobUri,
    InvalidSha256,
    OwnerMismatch {
        expected: String,
        actual: String,
    },
    DigestMismatch,
    NotFound(String),
    Storage(String),
}

pub fn read_owned_blob(
    blobs: &dyn ArtifactBlobStore,
    owner_id: &OwnerId,
    uri: &str,
) -> Result<OwnedBlobRead, ArtifactBlobError> {
    let blob = parse_blob_uri(uri).ok_or(ArtifactBlobError::NotBlobUri)?;
    let expected_owner = sanitize_owner_segment(owner_id.as_str());
    if blob.owner_segment != expected_owner {
        return Err(ArtifactBlobError::OwnerMismatch {
            expected: expected_owner,
            actual: blob.owner_segment,
        });
    }
    let bytes = read_owned_blob_by_sha256(blobs, owner_id, &blob.sha256)?;
    Ok(OwnedBlobRead {
        sha256: blob.sha256,
        bytes,
    })
}

pub fn read_owned_blob_by_sha256(
    blobs: &dyn ArtifactBlobStore,
    owner_id: &OwnerId,
    sha256: &str,
) -> Result<Vec<u8>, ArtifactBlobError> {
    if !is_valid_lower_sha256(sha256) {
        return Err(ArtifactBlobError::InvalidSha256);
    }
    match blobs.get_blob(owner_id, sha256) {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => Err(ArtifactBlobError::NotFound(sha256.to_string())),
        Err(e) => Err(ArtifactBlobError::Storage(e.to_string())),
    }
}

pub fn verify_blob_reference(
    state: &Arc<AppDaemonState>,
    owner_id: &OwnerId,
    uri: &str,
    provided_sha256: Option<&str>,
) -> Result<(), DispatchError> {
    let read = match read_owned_blob(state.ports.blobs.as_ref(), owner_id, uri) {
        Ok(r) => r,
        Err(ArtifactBlobError::NotBlobUri) => return Ok(()),
        Err(ArtifactBlobError::OwnerMismatch { expected, actual }) => {
            return Err(DispatchError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_input".into(),
                error: format!(
                    "invalid input for 'uri': blob owner '{actual}' does not match authenticated owner '{expected}'"
                ),
                detail: None,
            });
        }
        Err(ArtifactBlobError::NotFound(sha)) => {
            return Err(DispatchError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_input".into(),
                error: format!(
                    "invalid input for 'uri': referenced artifact blob '{sha}' does not exist in owner blob store"
                ),
                detail: None,
            });
        }
        Err(ArtifactBlobError::Storage(msg)) => {
            return Err(DispatchError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "storage_error".into(),
                error: msg,
                detail: None,
            });
        }
        Err(ArtifactBlobError::InvalidSha256 | ArtifactBlobError::DigestMismatch) => {
            return Err(DispatchError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_input".into(),
                error: "invalid input for 'uri': invalid blob digest".into(),
                detail: None,
            });
        }
    };

    if let Some(sha) = provided_sha256
        && !sha.eq_ignore_ascii_case(&read.sha256)
    {
        return Err(DispatchError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_input".into(),
            error: "invalid input for 'sha256': does not match blob URI digest".into(),
            detail: None,
        });
    }

    let mut hasher = Sha256::new();
    hasher.update(&read.bytes);
    let actual_sha = format!("{:x}", hasher.finalize());
    if actual_sha != read.sha256 {
        return Err(DispatchError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_input".into(),
            error: "invalid input for 'uri': stored blob content digest does not match URI".into(),
            detail: None,
        });
    }
    Ok(())
}

pub enum ArtifactPayloadSource<'a> {
    Uri(&'a str),
    Utf8(&'a str),
    Base64(&'a str),
}

pub enum ArtifactServiceError {
    InvalidArg { field: &'static str, reason: String },
    Dispatch(DispatchError),
}

pub fn resolve_or_store_artifact_source(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    job_id: &JobId,
    label: &str,
    source: ArtifactPayloadSource<'_>,
    provided_sha: Option<String>,
    fencing_token: Option<FencingToken>,
    now: TimestampMs,
) -> Result<(String, Option<String>), ArtifactServiceError> {
    let raw_bytes = match source {
        ArtifactPayloadSource::Uri(uri) => return Ok((uri.to_string(), provided_sha)),
        ArtifactPayloadSource::Utf8(text) => text.as_bytes().to_vec(),
        ArtifactPayloadSource::Base64(b64) => BASE64_STANDARD.decode(b64).map_err(|e| {
            ArtifactServiceError::InvalidArg {
                field: "content_base64",
                reason: format!("invalid base64 payload: {e}"),
            }
        })?,
    };

    if !principal.scopes.contains(&Scope::JobsWork) {
        return Err(ArtifactServiceError::Dispatch(DispatchError {
            status: StatusCode::FORBIDDEN,
            code: "insufficient_scope".into(),
            error: "insufficient_scope".into(),
            detail: None,
        }));
    }
    if raw_bytes.len() > MAX_ARTIFACT_BLOB_BYTES {
        return Err(ArtifactServiceError::InvalidArg {
            field: "content",
            reason: format!(
                "payload exceeds maximum size of {} bytes (got {} bytes)",
                MAX_ARTIFACT_BLOB_BYTES,
                raw_bytes.len()
            ),
        });
    }
    if let Some(ref sha) = provided_sha {
        let mut hasher = Sha256::new();
        hasher.update(&raw_bytes);
        let computed = format!("{:x}", hasher.finalize());
        if !sha.eq_ignore_ascii_case(&computed) {
            return Err(ArtifactServiceError::InvalidArg {
                field: "sha256",
                reason: "provided sha256 does not match payload digest".into(),
            });
        }
    }

    let snap = state
        .ports
        .store
        .load_workspace(&principal.owner_id)
        .map_err(|e| {
            ArtifactServiceError::Dispatch(DispatchError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "storage_error".into(),
                error: e.to_string(),
                detail: None,
            })
        })?;
    let Some(job) = snap.engine.find_job(job_id) else {
        return Err(ArtifactServiceError::Dispatch(
            transition_error_to_dispatch(TransitionError::JobNotFound(job_id.clone())),
        ));
    };
    if let JobLocation::InDistrict {
        district_id,
        stage_id,
        ..
    } = &job.location
        && let Some(stage) = snap.engine.find_stage_for_job(job, district_id, stage_id)
    {
        policy::require_stage_authority(&principal.actor, district_id, stage, "attach artifact")
            .map_err(|te| ArtifactServiceError::Dispatch(transition_error_to_dispatch(te)))?;
    }
    policy::require_mutation_lease(&principal.actor, job, fencing_token, now)
        .map_err(|te| ArtifactServiceError::Dispatch(transition_error_to_dispatch(te)))?;

    let blob_ref = state
        .ports
        .blobs
        .put_blob(&principal.owner_id, label, &raw_bytes)
        .map_err(|e| {
            ArtifactServiceError::Dispatch(DispatchError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "storage_error".into(),
                error: e.to_string(),
                detail: None,
            })
        })?;
    Ok((blob_ref.uri, Some(blob_ref.sha256)))
}

pub fn read_job_artifact(
    state: &Arc<AppDaemonState>,
    principal: &Principal,
    job_id: &JobId,
    artifact_id: &ArtifactId,
) -> Result<Value, DispatchError> {
    if !principal.scopes.contains(&Scope::JobsRead) {
        return Err(DispatchError {
            status: StatusCode::FORBIDDEN,
            code: "insufficient_scope".into(),
            error: "insufficient_scope".into(),
            detail: None,
        });
    }
    let snap = state
        .ports
        .store
        .load_workspace(&principal.owner_id)
        .map_err(|e| DispatchError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "storage_error".into(),
            error: e.to_string(),
            detail: None,
        })?;
    let Some(job) = snap.engine.find_job(job_id) else {
        return Err(DispatchError {
            status: StatusCode::NOT_FOUND,
            code: "job_not_found".into(),
            error: format!("job not found: {job_id}"),
            detail: None,
        });
    };
    let Some(art) = job.artifacts.iter().find(|a| &a.id == artifact_id) else {
        return Err(DispatchError {
            status: StatusCode::NOT_FOUND,
            code: "artifact_not_found".into(),
            error: format!("artifact not found: {artifact_id}"),
            detail: None,
        });
    };
    format_artifact_read_response(state.ports.blobs.as_ref(), &principal.owner_id, art)
}

fn format_artifact_read_response(
    blobs: &dyn ArtifactBlobStore,
    owner_id: &OwnerId,
    art: &Artifact,
) -> Result<Value, DispatchError> {
    match read_owned_blob(blobs, owner_id, &art.uri) {
        Ok(read) => {
            let size_bytes = read.bytes.len() as u64;
            let body = match String::from_utf8(read.bytes) {
                Ok(text) => json!({
                    "storage": "blob",
                    "artifact": art,
                    "sha256": read.sha256,
                    "size_bytes": size_bytes,
                    "encoding": "utf-8",
                    "content": text,
                }),
                Err(e) => json!({
                    "storage": "blob",
                    "artifact": art,
                    "sha256": read.sha256,
                    "size_bytes": size_bytes,
                    "encoding": "base64",
                    "content_base64": BASE64_STANDARD.encode(e.into_bytes()),
                }),
            };
            Ok(body)
        }
        Err(ArtifactBlobError::NotBlobUri) => Ok(json!({
            "storage": "external",
            "artifact": art,
        })),
        Err(ArtifactBlobError::OwnerMismatch { .. }) => Err(DispatchError {
            status: StatusCode::FORBIDDEN,
            code: "owner_mismatch".into(),
            error: "blob belongs to a different owner".into(),
            detail: None,
        }),
        Err(ArtifactBlobError::NotFound(sha)) => Err(DispatchError {
            status: StatusCode::NOT_FOUND,
            code: "blob_not_found".into(),
            error: format!("artifact blob missing: {sha}"),
            detail: None,
        }),
        Err(ArtifactBlobError::Storage(msg)) => Err(DispatchError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "storage_error".into(),
            error: msg,
            detail: None,
        }),
        Err(ArtifactBlobError::InvalidSha256 | ArtifactBlobError::DigestMismatch) => {
            Err(DispatchError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_sha256".into(),
                error: "invalid artifact blob digest".into(),
                detail: None,
            })
        }
    }
}
