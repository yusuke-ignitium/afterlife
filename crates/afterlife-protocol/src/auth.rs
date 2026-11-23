use std::collections::BTreeSet;
use std::fmt;

use afterlife_core::{Actor, DistrictId, DistrictProposalAction, OwnerId, TimestampMs};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Scope {
    JobsRead,
    JobsCreate,
    JobsWork,
    ApprovalsDecide,
    DistrictsAdmin,
    DistrictScoped(DistrictId),
}

impl Scope {
    pub fn as_scope_string(&self) -> String {
        match self {
            Self::JobsRead => "jobs:read".into(),
            Self::JobsCreate => "jobs:create".into(),
            Self::JobsWork => "jobs:work".into(),
            Self::ApprovalsDecide => "approvals:decide".into(),
            Self::DistrictsAdmin => "districts:admin".into(),
            Self::DistrictScoped(id) => format!("district:{id}"),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "jobs:read" => Some(Self::JobsRead),
            "jobs:create" => Some(Self::JobsCreate),
            "jobs:work" => Some(Self::JobsWork),
            "approvals:decide" => Some(Self::ApprovalsDecide),
            "districts:admin" => Some(Self::DistrictsAdmin),
            other => other
                .strip_prefix("district:")
                .filter(|id| !id.is_empty())
                .map(|id| Self::DistrictScoped(DistrictId::from(id))),
        }
    }

    pub fn is_human_only(&self) -> bool {
        matches!(self, Self::ApprovalsDecide | Self::DistrictsAdmin)
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_scope_string())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeSet {
    scopes: BTreeSet<Scope>,
}

impl ScopeSet {
    pub fn new(scopes: impl IntoIterator<Item = Scope>) -> Self {
        Self {
            scopes: scopes.into_iter().collect(),
        }
    }

    pub fn parse_space_separated(raw: &str) -> Self {
        Self {
            scopes: raw.split_whitespace().filter_map(Scope::parse).collect(),
        }
    }

    pub fn to_space_separated(&self) -> String {
        self.scopes
            .iter()
            .map(Scope::as_scope_string)
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn human_full() -> Self {
        Self::new([
            Scope::JobsRead,
            Scope::JobsCreate,
            Scope::JobsWork,
            Scope::ApprovalsDecide,
            Scope::DistrictsAdmin,
        ])
    }

    pub fn agent_default() -> Self {
        Self::new([Scope::JobsRead, Scope::JobsCreate, Scope::JobsWork])
    }

    pub fn without_human_only(&self) -> Self {
        Self {
            scopes: self
                .scopes
                .iter()
                .filter(|s| !s.is_human_only())
                .cloned()
                .collect(),
        }
    }

    pub fn contains(&self, scope: &Scope) -> bool {
        self.scopes.contains(scope)
    }

    pub fn is_district_restricted(&self) -> bool {
        self.scopes
            .iter()
            .any(|s| matches!(s, Scope::DistrictScoped(_)))
    }

    pub fn allows_workspace_structure(&self) -> bool {
        !self.is_district_restricted()
    }

    pub fn allows_district(&self, district_id: &DistrictId) -> bool {
        if !self.is_district_restricted() {
            return true;
        }
        self.scopes
            .contains(&Scope::DistrictScoped(district_id.clone()))
    }

    pub fn allows_district_proposal(&self, action: &DistrictProposalAction) -> bool {
        match action {
            DistrictProposalAction::Create { .. } => self.allows_workspace_structure(),
            DistrictProposalAction::Archive { district_id }
            | DistrictProposalAction::Restore { district_id } => self.allows_district(district_id),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Scope> {
        self.scopes.iter()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub owner_id: OwnerId,
    pub actor: Actor,
    pub scopes: ScopeSet,
    pub token_jti: String,
    pub expires_at: TimestampMs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredClient {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub is_agent_client: bool,
    pub created_at: TimestampMs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredRefreshToken {
    pub token_hash: String,
    pub owner_id: OwnerId,
    pub actor: Actor,
    pub client_id: String,
    pub scopes: ScopeSet,
    pub expires_at: TimestampMs,
    pub rotated: bool,
}
