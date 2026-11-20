use serde::{Deserialize, Serialize};

use crate::{
    ids::{SubMachineTemplateId, SubStageId},
    model::runner::RunnerKind,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubStageDef {
    pub id: SubStageId,
    pub label: String,
    pub runner: String,
    pub runner_kind: RunnerKind,
    pub procedure: String,
    pub gate_checks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubMachineTemplate {
    pub id: SubMachineTemplateId,
    pub name: String,
    pub description: String,
    pub philosophy_notes: String,
    pub stages: Vec<SubStageDef>,
}
