use serde::{Deserialize, Serialize};

use crate::ids::OwnerId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    Human {
        owner_id: OwnerId,
        email: String,
    },
    Agent {
        owner_id: OwnerId,
        client_id: String,
        runner_label: String,
        token_jti: String,
    },
}

impl Actor {
    pub fn local_human(owner_id: impl Into<OwnerId>) -> Self {
        let owner_id = owner_id.into();
        let email = if owner_id.as_str().contains('@') {
            owner_id.as_str().to_string()
        } else {
            format!("{}@local", owner_id.as_str())
        };
        Self::Human { owner_id, email }
    }

    pub fn owner_id(&self) -> &OwnerId {
        match self {
            Self::Human { owner_id, .. } | Self::Agent { owner_id, .. } => owner_id,
        }
    }

    pub fn is_human(&self) -> bool {
        matches!(self, Self::Human { .. })
    }

    pub fn display_label(&self) -> &str {
        match self {
            Self::Human { email, .. } => email.as_str(),
            Self::Agent { runner_label, .. } => runner_label.as_str(),
        }
    }
}
