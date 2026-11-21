pub mod auth;
pub mod backdrop;
pub mod config;
pub mod envelope;
pub mod ports;

pub use auth::{Principal, RegisteredClient, Scope, ScopeSet, StoredRefreshToken};
pub use backdrop::{
    BackdropConfig, BackdropGlassAppearance, BackdropImageEntry, BackdropStore, BackdropTuning,
    BackdropTuningPatch, DEFAULT_APPLE_MUSIC_ARTWORK_ENABLED, DEFAULT_IMAGE_BRIGHTNESS,
    DEFAULT_IMAGE_ENABLED, DEFAULT_IMAGE_HOLD_SECS, DEFAULT_IMAGE_PEAK_LUMINANCE,
    DEFAULT_IMAGE_SATURATION, DEFAULT_MASTER_BRIGHTNESS, DEFAULT_TILE_SIZE_PX,
    DEFAULT_YOUTUBE_MIRROR_ENABLED, default_backdrop_data_dir, resolve_tile_size_for_display,
    sanitize_display_tile_sizes,
};
pub use config::{
    AfterlifeConfig, AuthConfig, BlobConfig, EventBusConfig, LocalHumanHttpPolicy, ServerConfig,
    StorageConfig, TelemetryConfig,
};
pub use envelope::{CommandEnvelope, CommandOutcome, EventRecord, WorkspaceSnapshot};
pub use ports::{ArtifactBlobStore, BlobRef, EventBus, PortError, TelemetrySink, WorkspaceStore};
