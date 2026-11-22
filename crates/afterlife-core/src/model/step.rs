use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    ids::{ArtifactId, DistrictId, StageId, SubStageId, TimestampMs},
    model::actor::Actor,
};

fn default_required() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepFieldDef {
    pub key: String,
    pub label: String,
    pub prompt: String,
    #[serde(default = "default_required")]
    pub required: bool,
}

impl StepFieldDef {
    pub fn required(
        key: impl Into<String>,
        label: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            prompt: prompt.into(),
            required: true,
        }
    }

    pub fn optional(
        key: impl Into<String>,
        label: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            prompt: prompt.into(),
            required: false,
        }
    }
}

pub const STEP_FIELD_DECISION_LABEL: &str = "Decision & Rationale";
pub const STEP_FIELD_EXECUTION_LABEL: &str = "Execution & Outcome";
pub const STEP_FIELD_NOTE_LABEL: &str = "Approval Note";
pub const STEP_FIELD_REASON_LABEL: &str = "Rework Reason";

pub fn builtin_step_field_label(key: &str) -> Option<&'static str> {
    match key {
        "decision" => Some(STEP_FIELD_DECISION_LABEL),
        "execution" => Some(STEP_FIELD_EXECUTION_LABEL),
        "note" => Some(STEP_FIELD_NOTE_LABEL),
        "reason" => Some(STEP_FIELD_REASON_LABEL),
        _ => None,
    }
}

pub fn default_step_fields() -> Vec<StepFieldDef> {
    vec![
        StepFieldDef::required(
            "decision",
            STEP_FIELD_DECISION_LABEL,
            "Record what was decided in this step and why (rationale, assumptions, and rejected alternatives).",
        ),
        StepFieldDef::required(
            "execution",
            STEP_FIELD_EXECUTION_LABEL,
            "Record what was executed, changed, and verified (targets, steps, and observed outcomes).",
        ),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepRecordOutcome {
    Advanced,
    Approved,
    Reworked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    pub district_id: DistrictId,
    pub stage_id: StageId,
    pub sub_stage_id: Option<SubStageId>,
    pub revision: u64,
    pub actor: Actor,
    pub outcome: StepRecordOutcome,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub artifact_ids: Vec<ArtifactId>,
    pub recorded_at: TimestampMs,
}
