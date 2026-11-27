use afterlife_core::{JobId, OwnerId};
use serde::{Deserialize, Serialize};

use crate::ports::PortError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearOAuthToken {
    pub owner_id: OwnerId,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at_ms: Option<u64>,
    pub scope: String,
    #[serde(default)]
    pub viewer_id: Option<String>,
    #[serde(default)]
    pub viewer_name: Option<String>,
    #[serde(default)]
    pub viewer_email: Option<String>,
    #[serde(default)]
    pub default_district_id: Option<String>,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearIssueLink {
    pub owner_id: OwnerId,
    pub linear_issue_id: String,
    pub linear_identifier: String,
    pub linear_url: String,
    pub job_id: JobId,
    pub last_synced_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearSyncCursor {
    pub owner_id: OwnerId,
    pub last_outbound_seq: u64,
    #[serde(default)]
    pub last_sync_at_ms: Option<u64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl LinearSyncCursor {
    pub fn initial(owner_id: OwnerId) -> Self {
        Self {
            owner_id,
            last_outbound_seq: 0,
            last_sync_at_ms: None,
            last_error: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearSyncStatus {
    pub connected: bool,
    pub client_id: Option<String>,
    pub viewer_name: Option<String>,
    pub viewer_email: Option<String>,
    pub scope: Option<String>,
    pub expires_at_ms: Option<u64>,
    pub default_district_id: Option<String>,
    pub linked_issues: Vec<LinearIssueLink>,
    pub last_outbound_seq: u64,
    pub last_sync_at_ms: Option<u64>,
    pub last_error: Option<String>,
    pub redirect_uri: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearSyncReport {
    pub open_issues_seen: usize,
    pub jobs_created: usize,
    pub signals_applied: usize,
    pub signals_deferred_lease: usize,
    pub sync_errors_posted: usize,
    pub outbound_events_posted: usize,
}

pub trait LinearSyncStore: Send + Sync {
    fn load_token(&self, owner_id: &OwnerId) -> Result<Option<LinearOAuthToken>, PortError>;
    fn save_token(&self, token: &LinearOAuthToken) -> Result<(), PortError>;
    fn delete_token(&self, owner_id: &OwnerId) -> Result<(), PortError>;

    fn list_links(&self, owner_id: &OwnerId) -> Result<Vec<LinearIssueLink>, PortError>;
    fn find_link_by_issue(
        &self,
        owner_id: &OwnerId,
        linear_issue_id: &str,
    ) -> Result<Option<LinearIssueLink>, PortError>;
    fn find_link_by_job(
        &self,
        owner_id: &OwnerId,
        job_id: &JobId,
    ) -> Result<Option<LinearIssueLink>, PortError>;
    fn upsert_link(&self, link: &LinearIssueLink) -> Result<(), PortError>;

    fn is_comment_processed(&self, owner_id: &OwnerId, comment_id: &str) -> Result<bool, PortError>;
    fn mark_comment_processed(
        &self,
        owner_id: &OwnerId,
        comment_id: &str,
        outcome: &str,
        processed_at_ms: u64,
    ) -> Result<(), PortError>;

    fn load_cursor(&self, owner_id: &OwnerId) -> Result<LinearSyncCursor, PortError>;
    fn save_cursor(&self, cursor: &LinearSyncCursor) -> Result<(), PortError>;
}
