use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use afterlife_core::OwnerId;
use afterlife_protocol::{
    LinearOAuthToken, LinearSyncCursor, LinearSyncStatus, export_workspace_skills,
    write_skill_bundle_atomic,
};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    AppDaemonState,
    auth::{AuthPrincipal, HumanWritePrincipal},
    linear::{
        client::{LINEAR_AUTHORIZE_URL, LINEAR_OAUTH_SCOPES},
        sync::{now_ms, sync_owner_once},
    },
};

const OAUTH_STATE_TTL_MS: u64 = 10 * 60 * 1_000;

#[derive(Clone, Debug)]
pub struct PendingLinearOAuth {
    pub owner_id: OwnerId,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub default_district_id: Option<String>,
    pub redirect_uri: String,
    pub code_verifier: String,
    pub expires_at_ms: u64,
}

#[derive(Default)]
pub struct PendingLinearOAuthStore {
    states: Mutex<HashMap<String, PendingLinearOAuth>>,
}

impl PendingLinearOAuthStore {
    pub fn insert(&self, state: String, entry: PendingLinearOAuth, now: u64) {
        let mut guard = self.states.lock().unwrap();
        guard.retain(|_, v| v.expires_at_ms > now);
        guard.insert(state, entry);
    }

    pub fn take(&self, state: &str, now: u64) -> Option<PendingLinearOAuth> {
        let mut guard = self.states.lock().unwrap();
        guard.retain(|_, v| v.expires_at_ms > now);
        guard.remove(state)
    }
}

fn random_b64url(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

fn pkce_challenge(verifier: &str) -> String {
    let mut h = Sha256::new();
    h.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(h.finalize())
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn redirect_uri_for_base(public_base_url: &str) -> String {
    format!("{}/oauth/linear/callback", public_base_url.trim_end_matches('/'))
}

#[derive(Clone, Debug, Deserialize)]
pub struct LinearOAuthStartRequest {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub default_district_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinearOAuthStartResponse {
    pub authorize_url: String,
    pub redirect_uri: String,
    pub state: String,
}

pub fn begin_linear_oauth_flow(
    state: &AppDaemonState,
    owner_id: OwnerId,
    req: LinearOAuthStartRequest,
) -> Result<LinearOAuthStartResponse, String> {
    let client_id = req.client_id.trim().to_string();
    if client_id.is_empty() {
        return Err("client_id must not be empty".into());
    }
    let client_secret = req
        .client_secret
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let default_district_id = req
        .default_district_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let redirect_uri = redirect_uri_for_base(&state.config.server.public_base_url);
    let oauth_state = random_b64url(24);
    let code_verifier = random_b64url(32);
    let code_challenge = pkce_challenge(&code_verifier);
    let now = now_ms();

    state.linear.pending_oauth.insert(
        oauth_state.clone(),
        PendingLinearOAuth {
            owner_id,
            client_id: client_id.clone(),
            client_secret,
            default_district_id,
            redirect_uri: redirect_uri.clone(),
            code_verifier,
            expires_at_ms: now.saturating_add(OAUTH_STATE_TTL_MS),
        },
        now,
    );

    let authorize_url = format!(
        "{LINEAR_AUTHORIZE_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        percent_encode(&client_id),
        percent_encode(&redirect_uri),
        percent_encode(LINEAR_OAUTH_SCOPES),
        percent_encode(&oauth_state),
        percent_encode(&code_challenge),
    );

    Ok(LinearOAuthStartResponse {
        authorize_url,
        redirect_uri,
        state: oauth_state,
    })
}

pub fn build_linear_status(state: &AppDaemonState, owner_id: &OwnerId) -> Result<LinearSyncStatus, String> {
    let token = state
        .ports
        .linear
        .load_token(owner_id)
        .map_err(|e| e.to_string())?;
    let links = state
        .ports
        .linear
        .list_links(owner_id)
        .map_err(|e| e.to_string())?;
    let cursor = state
        .ports
        .linear
        .load_cursor(owner_id)
        .unwrap_or_else(|_| LinearSyncCursor::initial(owner_id.clone()));

    Ok(LinearSyncStatus {
        connected: token.is_some(),
        client_id: token.as_ref().map(|t| t.client_id.clone()),
        viewer_name: token.as_ref().and_then(|t| t.viewer_name.clone()),
        viewer_email: token.as_ref().and_then(|t| t.viewer_email.clone()),
        scope: token.as_ref().map(|t| t.scope.clone()),
        expires_at_ms: token.as_ref().and_then(|t| t.expires_at_ms),
        default_district_id: token.as_ref().and_then(|t| t.default_district_id.clone()),
        linked_issues: links,
        last_outbound_seq: cursor.last_outbound_seq,
        last_sync_at_ms: cursor.last_sync_at_ms,
        last_error: cursor.last_error,
        redirect_uri: redirect_uri_for_base(&state.config.server.public_base_url),
    })
}

pub async fn linear_status_get(
    State(state): State<Arc<AppDaemonState>>,
    AuthPrincipal(principal): AuthPrincipal,
) -> Response {
    match build_linear_status(&state, &principal.owner_id) {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

pub async fn linear_oauth_start_post(
    State(state): State<Arc<AppDaemonState>>,
    AuthPrincipal(principal): AuthPrincipal,
    Json(req): Json<LinearOAuthStartRequest>,
) -> Response {
    match begin_linear_oauth_flow(&state, principal.owner_id, req) {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct LinearCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

fn callback_html(title: &str, message: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>body{{font-family:-apple-system,BlinkMacSystemFont,sans-serif;background:#0e1014;color:#eef2f7;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0}}\
         .card{{background:#171b22;border:1px solid #2b3240;border-radius:14px;padding:28px 32px;max-width:460px;box-shadow:0 16px 40px rgba(0,0,0,.4)}}\
         h1{{margin:0 0 10px;font-size:18px}}p{{margin:0;color:#b4bfce;line-height:1.5;font-size:14px}}</style></head>\
         <body><div class=\"card\"><h1>{title}</h1><p>{message}</p></div></body></html>"
    ))
}

pub async fn linear_oauth_callback_get(
    State(state): State<Arc<AppDaemonState>>,
    Query(q): Query<LinearCallbackQuery>,
) -> Response {
    if let Some(err) = q.error {
        let detail = q.error_description.unwrap_or(err);
        return (
            StatusCode::BAD_REQUEST,
            callback_html("Linear Authorization Cancelled", &detail),
        )
            .into_response();
    }
    let (Some(code), Some(oauth_state)) = (q.code, q.state) else {
        return (
            StatusCode::BAD_REQUEST,
            callback_html("Invalid Callback", "Missing OAuth code or state parameter."),
        )
            .into_response();
    };
    let now = now_ms();
    let Some(pending) = state.linear.pending_oauth.take(&oauth_state, now) else {
        return (
            StatusCode::BAD_REQUEST,
            callback_html(
                "Expired OAuth Session",
                "The Linear OAuth state was expired or already used. Please restart connection from Afterlife.",
            ),
        )
            .into_response();
    };

    let daemon = state.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let token_resp = daemon
            .linear
            .api
            .exchange_code(
                &pending.client_id,
                pending.client_secret.as_deref(),
                &code,
                &pending.redirect_uri,
                &pending.code_verifier,
            )
            .map_err(|e| e.to_string())?;
        let viewer = daemon
            .linear
            .api
            .fetch_viewer(&token_resp.access_token)
            .map_err(|e| e.to_string())?;

        let latest_seq = daemon
            .ports
            .store
            .load_workspace(&pending.owner_id)
            .map(|w| w.latest_seq)
            .unwrap_or(0);
        let mut cursor = daemon
            .ports
            .linear
            .load_cursor(&pending.owner_id)
            .unwrap_or_else(|_| LinearSyncCursor::initial(pending.owner_id.clone()));
        if cursor.last_outbound_seq == 0 {
            cursor.last_outbound_seq = latest_seq;
        }
        cursor.last_error = None;
        let _ = daemon.ports.linear.save_cursor(&cursor);

        let stored = LinearOAuthToken {
            owner_id: pending.owner_id.clone(),
            client_id: pending.client_id,
            client_secret: pending.client_secret,
            access_token: token_resp.access_token,
            refresh_token: token_resp.refresh_token,
            expires_at_ms: token_resp
                .expires_in
                .map(|s| now.saturating_add(s.saturating_mul(1_000))),
            scope: token_resp
                .scope
                .unwrap_or_else(|| LINEAR_OAUTH_SCOPES.to_string()),
            viewer_id: Some(viewer.id),
            viewer_name: Some(viewer.name.clone()),
            viewer_email: viewer.email,
            default_district_id: pending.default_district_id,
            updated_at_ms: now,
        };
        daemon
            .ports
            .linear
            .save_token(&stored)
            .map_err(|e| e.to_string())?;

        let ctx = daemon.linear.sync_context(&daemon);
        let _ = sync_owner_once(&ctx, &pending.owner_id, now);
        Ok(viewer.name)
    })
    .await;

    match result {
        Ok(Ok(name)) => (
            StatusCode::OK,
            callback_html(
                "Linear Connected",
                &format!(
                    "Connected to Linear as {name}. Your started assigned tickets will now sync with Afterlife. You can close this tab."
                ),
            ),
        )
            .into_response(),
        Ok(Err(err)) => (
            StatusCode::BAD_GATEWAY,
            callback_html("Linear Token Exchange Failed", &err),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            callback_html("Internal Error", &join_err.to_string()),
        )
            .into_response(),
    }
}

pub async fn linear_sync_post(
    State(state): State<Arc<AppDaemonState>>,
    AuthPrincipal(principal): AuthPrincipal,
) -> Response {
    let daemon = state.clone();
    let owner_id = principal.owner_id;
    let res = tokio::task::spawn_blocking(move || {
        let ctx = daemon.linear.sync_context(&daemon);
        sync_owner_once(&ctx, &owner_id, now_ms())
    })
    .await;
    match res {
        Ok(Ok(report)) => (StatusCode::OK, Json(report)).into_response(),
        Ok(Err(err)) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": err })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn linear_disconnect_post(
    State(state): State<Arc<AppDaemonState>>,
    HumanWritePrincipal(principal): HumanWritePrincipal,
) -> Response {
    match state.ports.linear.delete_token(&principal.owner_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "disconnected": true })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct ExportSkillsQuery {
    pub district_id: Option<String>,
}

pub async fn districts_skills_get(
    State(state): State<Arc<AppDaemonState>>,
    AuthPrincipal(principal): AuthPrincipal,
    Query(q): Query<ExportSkillsQuery>,
) -> Response {
    match state.ports.store.load_workspace(&principal.owner_id) {
        Ok(snapshot) => {
            let bundle = export_workspace_skills(&snapshot.engine, q.district_id.as_deref());
            (StatusCode::OK, Json(bundle)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn districts_skills_export_post(
    State(state): State<Arc<AppDaemonState>>,
    AuthPrincipal(principal): AuthPrincipal,
) -> Response {
    let Some(skills_dir) = state.skills_dir() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "skills output directory unavailable for current storage backend" })),
        )
            .into_response();
    };
    match state.ports.store.load_workspace(&principal.owner_id) {
        Ok(snapshot) => {
            let bundle = export_workspace_skills(&snapshot.engine, None);
            match write_skill_bundle_atomic(&skills_dir, &bundle, true) {
                Ok(report) => (StatusCode::OK, Json(report)).into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e })),
                )
                    .into_response(),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
