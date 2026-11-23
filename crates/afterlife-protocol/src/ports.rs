use std::fmt;

use afterlife_core::{OwnerId, TimestampMs, TransitionError};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::{
    auth::{RegisteredClient, StoredRefreshToken},
    envelope::{CommandEnvelope, CommandOutcome, EventRecord, WorkspaceSnapshot},
};

#[derive(Clone, Debug)]
pub enum PortError {
    Domain(TransitionError),
    InvalidInput(String),
    IdempotencyConflict { command_id: String },
    Storage(String),
    NotFound(String),
    UnimplementedAdapter(&'static str),
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(e) => write!(f, "{e}"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::IdempotencyConflict { command_id } => {
                write!(
                    f,
                    "idempotency conflict: command_id '{command_id}' was already used with a different payload"
                )
            }
            Self::Storage(msg) => write!(f, "storage error: {msg}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::UnimplementedAdapter(name) => {
                write!(
                    f,
                    "production adapter '{name}' requires cloud feature/runtime wiring"
                )
            }
        }
    }
}

impl std::error::Error for PortError {}

impl From<TransitionError> for PortError {
    fn from(value: TransitionError) -> Self {
        Self::Domain(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    pub uri: String,
    pub sha256: String,
    pub size_bytes: u64,
}

pub trait WorkspaceStore: Send + Sync {
    fn load_workspace(&self, owner_id: &OwnerId) -> Result<WorkspaceSnapshot, PortError>;

    fn execute_command(&self, env: CommandEnvelope) -> Result<CommandOutcome, PortError>;

    fn list_events_since(
        &self,
        owner_id: &OwnerId,
        since_seq: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>, PortError>;

    fn register_oauth_client(&self, client: RegisteredClient) -> Result<(), PortError>;

    fn get_oauth_client(&self, client_id: &str) -> Result<Option<RegisteredClient>, PortError>;

    fn store_refresh_token(&self, token: StoredRefreshToken) -> Result<(), PortError>;

    fn consume_refresh_token(
        &self,
        token_hash: &str,
        now_ms: TimestampMs,
    ) -> Result<Option<StoredRefreshToken>, PortError>;

    fn revoke_token_jti(&self, jti: &str, expires_at: TimestampMs) -> Result<(), PortError>;

    fn is_token_jti_revoked(&self, jti: &str) -> Result<bool, PortError>;
}

pub trait ArtifactBlobStore: Send + Sync {
    fn put_blob(
        &self,
        owner_id: &OwnerId,
        filename: &str,
        bytes: &[u8],
    ) -> Result<BlobRef, PortError>;

    fn get_blob(&self, owner_id: &OwnerId, sha256: &str) -> Result<Option<Vec<u8>>, PortError>;
}

pub trait TelemetrySink: Send + Sync {
    fn emit(&self, record: &EventRecord) -> Result<(), PortError>;
}

pub trait EventBus: Send + Sync {
    fn publish(&self, record: EventRecord);
    fn subscribe(&self) -> broadcast::Receiver<EventRecord>;
    fn subscriber_count(&self) -> usize {
        0
    }
}
