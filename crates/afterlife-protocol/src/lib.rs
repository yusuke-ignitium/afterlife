pub mod auth;
pub mod backdrop;
pub mod config;
pub mod envelope;
pub mod linear;
pub mod ports;
pub mod skill_export;

pub use afterlife_sm_wasm::signal as linear_signal;

pub use auth::{Principal, RegisteredClient, Scope, ScopeSet, StoredRefreshToken};
pub use backdrop::{
    BackdropConfig, BackdropGlassAppearance, BackdropImageEntry, BackdropStore, BackdropTuning,
    BackdropTuningPatch, DEFAULT_APPLE_MUSIC_ARTWORK_ENABLED, DEFAULT_FADE_UI_TRANSPARENCY_PCT,
    DEFAULT_IMAGE_BRIGHTNESS, DEFAULT_IMAGE_ENABLED, DEFAULT_IMAGE_HOLD_SECS,
    DEFAULT_IMAGE_PEAK_LUMINANCE, DEFAULT_IMAGE_SATURATION, DEFAULT_MASTER_BRIGHTNESS,
    DEFAULT_TILE_SIZE_PX, DEFAULT_YOUTUBE_MIRROR_ENABLED, MAX_FADE_UI_AFTER_SECS,
    MAX_FADE_UI_TRANSPARENCY_PCT, default_backdrop_data_dir, resolve_tile_size_for_display,
    sanitize_display_tile_sizes, sanitize_fade_ui_after_secs, sanitize_fade_ui_transparency_pct,
};
pub use config::{
    AfterlifeConfig, AuthConfig, BlobConfig, EventBusConfig, LocalHumanHttpPolicy, ServerConfig,
    StorageConfig, TelemetryConfig,
};
pub use envelope::{CommandEnvelope, CommandOutcome, EventRecord, WorkspaceSnapshot};
pub use linear::{
    LinearIssueLink, LinearOAuthToken, LinearSyncCursor, LinearSyncReport, LinearSyncStatus,
    LinearSyncStore,
};
pub use linear_signal::{
    LinearSignalV1, ParsedLinearComment, SIGNAL_FENCE_TAG, SIGNAL_SCHEMA_VERSION, SignalAction,
    SignalArtifact, SignalGateReport, SignalSource, extract_linear_signal,
    format_afterlife_comment,
};
pub use ports::{ArtifactBlobStore, BlobRef, EventBus, PortError, TelemetrySink, WorkspaceStore};
pub use skill_export::{
    ENTRY_SKILL_SLUG, ExportedSkillFile, SkillExportBundle, SkillManifest, SkillManifestEntry,
    SkillWriteReport, export_builtin_skills, export_district_json, export_entry_skill,
    export_sm_runner_script, export_sm_wasm_binary, export_workspace_skills,
    write_skill_bundle_atomic,
};

#[cfg(test)]
mod appearance_compat_tests;
