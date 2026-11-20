use afterlife_core::{Actor, AfterlifeEngine, CommandId, EngineCommand, JobEvent, OwnerId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub command_id: CommandId,
    pub actor: Actor,
    pub command: EngineCommand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    pub seq: u64,
    pub command_id: CommandId,
    pub owner_id: OwnerId,
    pub event: JobEvent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandOutcome {
    pub command_id: CommandId,
    pub seq_start: u64,
    pub seq_end: u64,
    pub events: Vec<JobEvent>,
    pub idempotent_replay: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub owner_id: OwnerId,
    pub latest_seq: u64,
    pub engine: AfterlifeEngine,
}
