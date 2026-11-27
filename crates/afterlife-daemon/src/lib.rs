pub mod artifacts;
pub mod auth;
#[cfg(test)]
mod governance_tests;
pub mod linear;
pub mod mcp;
pub mod metrics;
pub mod rest;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

use afterlife_protocol::{AfterlifeConfig, StorageConfig};
use afterlife_storage::RuntimePorts;
use axum::{
    Router,
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};

use crate::{auth::AuthBroker, linear::LinearBridge, metrics::DaemonMetrics};

pub struct AppDaemonState {
    pub config: AfterlifeConfig,
    pub ports: RuntimePorts,
    pub auth: AuthBroker,
    pub linear: LinearBridge,
    pub metrics: Arc<DaemonMetrics>,
}

fn resolve_data_dir(config: &AfterlifeConfig) -> Option<PathBuf> {
    match &config.storage {
        StorageConfig::Sqlite { path } => Path::new(path).parent().map(Path::to_path_buf),
        StorageConfig::Postgres { .. } => None,
    }
}

fn resolve_log_dir(config: &AfterlifeConfig) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("AFTERLIFE_LOG_DIR") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    resolve_data_dir(config).map(|p| p.join("logs"))
}

impl AppDaemonState {
    pub fn from_config(config: AfterlifeConfig) -> Result<Arc<Self>, String> {
        let ports = RuntimePorts::from_config(&config).map_err(|e| e.to_string())?;
        let auth = AuthBroker::new(config.auth.clone());
        let linear = LinearBridge::default();
        let log_dir = resolve_log_dir(&config);
        let metrics = DaemonMetrics::new(log_dir.as_deref());
        Ok(Arc::new(Self {
            config,
            ports,
            auth,
            linear,
            metrics,
        }))
    }

    pub fn skills_dir(&self) -> Option<PathBuf> {
        resolve_data_dir(&self.config).map(|p| p.join("skills"))
    }
}

async fn metrics_middleware(
    State(state): State<Arc<AppDaemonState>>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();
    let skip_record = path == "/v1/metrics" || path == "/v1/metrics/reset";

    if !skip_record {
        state.metrics.in_flight.fetch_add(1, Ordering::Relaxed);
    }
    let started = Instant::now();
    let resp = next.run(req).await;
    let latency_us = started.elapsed().as_micros() as u64;
    if !skip_record {
        state.metrics.in_flight.fetch_sub(1, Ordering::Relaxed);
        state
            .metrics
            .record_http(&method, &path, resp.status().as_u16(), latency_us);
    }
    resp
}

pub fn build_router(state: Arc<AppDaemonState>) -> Router {
    Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(auth::well_known_protected_resource),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(auth::well_known_authorization_server),
        )
        .route("/oauth/register", post(auth::register_client))
        .route("/oauth/authorize", get(auth::authorize_get))
        .route("/oauth/token", post(auth::token_post))
        .route("/oauth/revoke", post(auth::revoke_post))
        .route(
            "/oauth/linear/callback",
            get(linear::oauth::linear_oauth_callback_get),
        )
        .route("/v1/health", get(rest::health))
        .route("/v1/metrics", get(rest::get_metrics))
        .route("/v1/metrics/reset", post(rest::reset_metrics))
        .route("/v1/workspace", get(rest::get_workspace))
        .route("/v1/blobs/{sha256}", get(rest::get_blob))
        .route("/v1/jobs/{job_id}/context", get(rest::get_job_context))
        .route("/v1/commands", post(rest::post_command))
        .route("/v1/events", get(rest::sse_events))
        .route("/v1/linear/status", get(linear::oauth::linear_status_get))
        .route(
            "/v1/linear/oauth/start",
            post(linear::oauth::linear_oauth_start_post),
        )
        .route("/v1/linear/sync", post(linear::oauth::linear_sync_post))
        .route(
            "/v1/linear/disconnect",
            post(linear::oauth::linear_disconnect_post),
        )
        .route(
            "/v1/districts/skills",
            get(linear::oauth::districts_skills_get),
        )
        .route(
            "/v1/districts/skills/export",
            post(linear::oauth::districts_skills_export_post),
        )
        .route(
            "/mcp",
            post(mcp::mcp_post)
                .get(mcp::mcp_get)
                .delete(mcp::mcp_delete),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics_middleware,
        ))
        .with_state(state)
}

#[cfg(test)]
mod lib_tests;
