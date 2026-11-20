use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AfterlifeConfig {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub storage: StorageConfig,
    pub blobs: BlobConfig,
    pub telemetry: TelemetryConfig,
    pub event_bus: EventBusConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub public_base_url: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalHumanHttpPolicy {
    #[default]
    Enabled,
    DesktopOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AuthConfig {
    LocalDev {
        default_owner_id: String,
        default_email: String,
        issuer: String,
        jwt_secret_env: String,
        access_token_ttl_secs: u64,
        refresh_token_ttl_secs: u64,
        #[serde(default)]
        human_http_policy: LocalHumanHttpPolicy,
    },
    GoogleOidc {
        issuer: String,
        google_client_id: String,
        google_client_secret_env: String,
        hosted_domain: String,
        jwt_secret_env: String,
        access_token_ttl_secs: u64,
        refresh_token_ttl_secs: u64,
        #[serde(default)]
        allowed_emails: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum StorageConfig {
    Sqlite {
        path: String,
    },
    Postgres {
        dsn_env: String,
        max_connections: u32,
        rls_enforced: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum BlobConfig {
    LocalFs { dir: String },
    S3 {
        bucket: String,
        prefix: String,
        region: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum TelemetryConfig {
    Disabled,
    JsonlFile { dir: String },
    S3Jsonl {
        bucket: String,
        prefix: String,
        region: String,
        flush_interval_secs: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum EventBusConfig {
    InMemory { capacity: usize },
    PostgresListenNotify { channel: String },
}

impl AfterlifeConfig {
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("failed to read config {}: {e}", path.as_ref().display()))?;
        toml::from_str(&raw)
            .map_err(|e| format!("invalid TOML config {}: {e}", path.as_ref().display()))
    }

    pub fn local_default(data_dir: &str) -> Self {
        Self {
            server: ServerConfig {
                bind_addr: "127.0.0.1:4319".into(),
                public_base_url: "http://127.0.0.1:4319".into(),
            },
            auth: AuthConfig::LocalDev {
                default_owner_id: "local-owner".into(),
                default_email: "you@local".into(),
                issuer: "http://127.0.0.1:4319".into(),
                jwt_secret_env: "AFTERLIFE_JWT_SECRET".into(),
                access_token_ttl_secs: 900,
                refresh_token_ttl_secs: 2_592_000,
                human_http_policy: LocalHumanHttpPolicy::Enabled,
            },
            storage: StorageConfig::Sqlite {
                path: format!("{data_dir}/afterlife.db"),
            },
            blobs: BlobConfig::LocalFs {
                dir: format!("{data_dir}/artifacts"),
            },
            telemetry: TelemetryConfig::JsonlFile {
                dir: format!("{data_dir}/telemetry"),
            },
            event_bus: EventBusConfig::InMemory { capacity: 1024 },
        }
    }

    pub fn desktop_embedded(data_dir: &str) -> Self {
        let mut cfg = Self::local_default(data_dir);
        if let AuthConfig::LocalDev {
            ref mut human_http_policy,
            ..
        } = cfg.auth
        {
            *human_http_policy = LocalHumanHttpPolicy::DesktopOnly;
        }
        cfg
    }
}
