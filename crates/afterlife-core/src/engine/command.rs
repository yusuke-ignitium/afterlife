use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    ids::{
        ArtifactId, DistrictId, FeedbackId, FencingToken, JobId, PhilosophyVersion, ProposalId,
        RunnerId, SpecVersion, StageId, SubMachineTemplateId, TimestampMs,
    },
    model::{
        district::StageDef,
        feedback::{FeedbackKind, GateVerdict},
        job::ArtifactKind,
        philosophy::Principle,
        sub_machine::SubStageDef,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum EngineCommand {
    QueueJob {
        title: String,
        raw_input: String,
        target_district_hint: Option<DistrictId>,
        at: TimestampMs,
    },
    UpdateJob {
        job_id: JobId,
        title: Option<String>,
        raw_input: Option<String>,
        spec_summary: Option<String>,
        target_district_hint: Option<DistrictId>,
        expected_revision: Option<u64>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    },
    ArchiveJob {
        job_id: JobId,
        reason: Option<String>,
        expected_revision: Option<u64>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    },
    PullNextJob {
        district_id: DistrictId,
        at: TimestampMs,
    },
    AssignJobToDistrict {
        job_id: JobId,
        district_id: DistrictId,
        expected_revision: Option<u64>,
        at: TimestampMs,
    },
    ClaimJob {
        job_id: JobId,
        runner_id: RunnerId,
        runner_label: String,
        ttl_ms: TimestampMs,
        expected_revision: Option<u64>,
        at: TimestampMs,
    },
    HeartbeatLease {
        job_id: JobId,
        fencing_token: FencingToken,
        ttl_ms: TimestampMs,
        at: TimestampMs,
    },
    ReleaseLease {
        job_id: JobId,
        fencing_token: FencingToken,
        at: TimestampMs,
    },
    AdvanceJob {
        job_id: JobId,
        #[serde(default)]
        fields: BTreeMap<String, String>,
        #[serde(default)]
        artifact_ids: Vec<ArtifactId>,
        expected_revision: Option<u64>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    },
    ApproveJob {
        job_id: JobId,
        note: Option<String>,
        expected_revision: Option<u64>,
        at: TimestampMs,
    },
    ReworkJob {
        job_id: JobId,
        expected_revision: Option<u64>,
        fencing_token: Option<FencingToken>,
        reason: Option<String>,
        at: TimestampMs,
    },
    StopJob {
        job_id: JobId,
        reason: Option<String>,
        expected_revision: Option<u64>,
        at: TimestampMs,
    },
    ResumeJob {
        job_id: JobId,
        expected_revision: Option<u64>,
        at: TimestampMs,
    },
    AttachArtifact {
        job_id: JobId,
        kind: ArtifactKind,
        label: String,
        uri: String,
        sha256: Option<String>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    },
    RecordGateVerification {
        job_id: JobId,
        verdict: GateVerdict,
        summary: String,
        checklist_results: Vec<(String, bool)>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    },
    SubmitFeedback {
        district_id: DistrictId,
        job_id: Option<JobId>,
        stage_id: Option<StageId>,
        kind: FeedbackKind,
        body: String,
        at: TimestampMs,
    },
    ProposePhilosophyUpdate {
        district_id: DistrictId,
        base_version: PhilosophyVersion,
        #[serde(default)]
        proposed_district_name: Option<String>,
        summary: String,
        principles: Vec<Principle>,
        anti_patterns: Vec<String>,
        decision_heuristics: Vec<String>,
        rationale: String,
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    },
    ApprovePhilosophyProposal {
        proposal_id: ProposalId,
        at: TimestampMs,
    },
    ProposeSpecUpdate {
        district_id: DistrictId,
        base_version: SpecVersion,
        #[serde(default)]
        proposed_district_name: Option<String>,
        stages: Vec<StageDef>,
        changelog: String,
        rationale: String,
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    },
    ApproveSpecProposal {
        proposal_id: ProposalId,
        at: TimestampMs,
    },
    SetStageSubTemplate {
        district_id: DistrictId,
        stage_id: StageId,
        template_id: Option<SubMachineTemplateId>,
        at: TimestampMs,
    },
    ProposeTemplateUpdate {
        template_id: SubMachineTemplateId,
        name: String,
        description: String,
        philosophy_notes: String,
        stages: Vec<SubStageDef>,
        rationale: String,
        #[serde(default)]
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    },
    ApproveTemplateProposal {
        proposal_id: ProposalId,
        at: TimestampMs,
    },
    SyncBuiltinTemplates {
        #[serde(default)]
        template_ids: Option<Vec<SubMachineTemplateId>>,
        at: TimestampMs,
    },
    ProposeDistrictCreate {
        #[serde(default)]
        district_id: Option<DistrictId>,
        name: String,
        #[serde(default)]
        setup_by: Option<String>,
        summary: String,
        principles: Vec<Principle>,
        #[serde(default)]
        anti_patterns: Vec<String>,
        #[serde(default)]
        decision_heuristics: Vec<String>,
        stages: Vec<StageDef>,
        #[serde(default)]
        changelog: String,
        rationale: String,
        #[serde(default)]
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    },
    ProposeDistrictArchive {
        district_id: DistrictId,
        rationale: String,
        #[serde(default)]
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    },
    ProposeDistrictRestore {
        district_id: DistrictId,
        rationale: String,
        #[serde(default)]
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    },
    ApproveDistrictProposal {
        proposal_id: ProposalId,
        at: TimestampMs,
    },
}
