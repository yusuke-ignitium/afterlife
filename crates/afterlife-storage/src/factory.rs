use std::sync::Arc;

use afterlife_protocol::{
    AfterlifeConfig, ArtifactBlobStore, BlobConfig, EventBus, EventBusConfig, PortError,
    StorageConfig, TelemetryConfig, TelemetrySink, WorkspaceStore,
};

use crate::{
    local_adapters::{InMemoryEventBus, JsonlTelemetrySink, LocalFsBlobStore, NoopTelemetrySink},
    sqlite::SqliteWorkspaceStore,
};

pub struct RuntimePorts {
    pub store: Arc<dyn WorkspaceStore>,
    pub blobs: Arc<dyn ArtifactBlobStore>,
    pub telemetry: Arc<dyn TelemetrySink>,
    pub event_bus: Arc<dyn EventBus>,
}

impl RuntimePorts {
    pub fn from_config(cfg: &AfterlifeConfig) -> Result<Self, PortError> {
        let store: Arc<dyn WorkspaceStore> = match &cfg.storage {
            StorageConfig::Sqlite { path } => Arc::new(SqliteWorkspaceStore::open(path)?),
            StorageConfig::Postgres { dsn_env, .. } => {
                return Err(PortError::UnimplementedAdapter(
                    if std::env::var(dsn_env).is_ok() {
                        "postgres_workspace_store (enable sqlx/postgres driver for AWS prod build)"
                    } else {
                        "postgres_workspace_store (missing DSN env var)"
                    },
                ));
            }
        };

        let blobs: Arc<dyn ArtifactBlobStore> = match &cfg.blobs {
            BlobConfig::LocalFs { dir } => Arc::new(LocalFsBlobStore::new(dir)?),
            BlobConfig::S3 { .. } => {
                return Err(PortError::UnimplementedAdapter(
                    "s3_blob_store (enable aws-sdk-s3 feature for AWS prod build)",
                ));
            }
        };

        let telemetry: Arc<dyn TelemetrySink> = match &cfg.telemetry {
            TelemetryConfig::Disabled => Arc::new(NoopTelemetrySink),
            TelemetryConfig::JsonlFile { dir } => Arc::new(JsonlTelemetrySink::new(dir)?),
            TelemetryConfig::S3Jsonl { .. } => {
                return Err(PortError::UnimplementedAdapter(
                    "s3_telemetry_sink (enable aws-sdk-s3 feature for AWS prod build)",
                ));
            }
        };

        let event_bus: Arc<dyn EventBus> = match &cfg.event_bus {
            EventBusConfig::InMemory { capacity } => Arc::new(InMemoryEventBus::new(*capacity)),
            EventBusConfig::PostgresListenNotify { .. } => {
                return Err(PortError::UnimplementedAdapter(
                    "postgres_listen_notify_bus (enable sqlx/postgres feature for AWS prod build)",
                ));
            }
        };

        Ok(Self {
            store,
            blobs,
            telemetry,
            event_bus,
        })
    }
}
