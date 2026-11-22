pub mod engine;
pub mod error;
pub mod fixtures;
pub mod ids;
pub mod model;

pub use engine::{
    AfterlifeEngine, EngineCommand, validate_stages, validate_template, validate_workspace,
};
pub use error::{TransitionError, ValidationError};
pub use ids::{
    ArtifactId, CommandId, DistrictId, FeedbackId, FencingToken, JobId, LeaseId, OwnerId,
    PhilosophyVersion, ProposalId, RunnerId, SpecVersion, StageId, SubMachineTemplateId,
    SubStageId, TimestampMs,
};
pub use model::{
    Actor, Artifact, ArtifactKind, District, ExecutionContext, Feedback, FeedbackKind, GateRubric,
    GateVerdict, GateVerification, InlineLink, Job, JobEvent, JobLease, JobLocation,
    MAX_ARTIFACT_BLOB_BYTES, ParsedArtifactUri, ParsedBlobUri, ParsedInlineText, Philosophy,
    PhilosophyProposal, Principle, ProcedureSpec, ProposalStatus, RunnerKind, SpecProposal,
    StageDef, StageKind, STEP_FIELD_DECISION_LABEL, STEP_FIELD_EXECUTION_LABEL,
    STEP_FIELD_NOTE_LABEL, STEP_FIELD_REASON_LABEL, StepFieldDef, StepRecord, StepRecordOutcome,
    SubMachineTemplate, SubStageDef, TemplateProposal, builtin_step_field_label,
    default_step_fields, format_blob_uri, is_openable_url, is_valid_lower_sha256,
    parse_artifact_uri, parse_blob_uri, parse_inline_links, sanitize_blob_filename,
    sanitize_owner_segment,
};

#[cfg(test)]
mod tests;
