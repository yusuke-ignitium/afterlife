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

pub fn default_step_fields() -> Vec<StepFieldDef> {
    vec![
        StepFieldDef::required(
            "decision",
            "判断と根拠",
            "このステップで何を根拠にどう判断したか（採用理由・前提・見送った選択肢）を記録する。",
        ),
        StepFieldDef::required(
            "execution",
            "実行内容と結果",
            "実際に何を実行・変更・確認したか（対象・手順・観測結果）を記録する。",
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
