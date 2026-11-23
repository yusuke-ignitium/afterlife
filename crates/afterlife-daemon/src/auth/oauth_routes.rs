use std::sync::Arc;

use afterlife_core::{Actor, OwnerId};
use afterlife_protocol::{
    AuthConfig, LocalHumanHttpPolicy, RegisteredClient, ScopeSet, StoredRefreshToken,
};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppDaemonState;

use super::{
    PendingAuthCode, PendingGoogleState, now_ms, random_token, sha256_b64url, sha256_hex,
};

pub async fn well_known_protected_resource(
    State(state): State<Arc<AppDaemonState>>,
) -> impl IntoResponse {
    let base = &state.config.server.public_base_url;
    Json(serde_json::json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [state.auth.issuer()],
        "scopes_supported": [
            "jobs:read",
            "jobs:create",
            "jobs:work"
        ],
        "bearer_methods_supported": ["header"]
    }))
}

pub async fn well_known_authorization_server(
    State(state): State<Arc<AppDaemonState>>,
) -> impl IntoResponse {
    let base = &state.config.server.public_base_url;
    Json(serde_json::json!({
        "issuer": state.auth.issuer(),
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "revocation_endpoint": format!("{base}/oauth/revoke"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": [
            "jobs:read",
            "jobs:create",
            "jobs:work",
            "approvals:decide",
            "districts:admin"
        ]
    }))
}

#[derive(Deserialize)]
pub struct RegisterClientRequest {
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub grant_types: Vec<String>,
    #[serde(default = "default_true")]
    pub is_agent_client: bool,
}

fn default_true() -> bool {
    true
}

pub async fn register_client(
    State(state): State<Arc<AppDaemonState>>,
    Json(req): Json<RegisterClientRequest>,
) -> Response {
    if req.redirect_uris.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid_redirect_uri" })),
        )
            .into_response();
    }
    if !req.is_agent_client
        && matches!(
            state.config.auth,
            AuthConfig::LocalDev {
                human_http_policy: LocalHumanHttpPolicy::DesktopOnly,
                ..
            }
        )
    {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "human_client_registration_disabled",
                "detail": "Embedded desktop mode restricts Human authority to the native UI; register with is_agent_client=true."
            })),
        )
            .into_response();
    }
    let client_id = format!("af_client_{}", Uuid::new_v4().simple());
    let grant_types = if req.grant_types.is_empty() {
        vec!["authorization_code".into(), "refresh_token".into()]
    } else {
        req.grant_types
    };
    let client = RegisteredClient {
        client_id: client_id.clone(),
        client_name: req.client_name.unwrap_or_else(|| "MCP Agent Client".into()),
        redirect_uris: req.redirect_uris.clone(),
        grant_types: grant_types.clone(),
        is_agent_client: req.is_agent_client,
        created_at: now_ms(),
    };
    if let Err(e) = state.ports.store.register_oauth_client(client) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response();
    }
    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "client_id": client_id,
            "redirect_uris": req.redirect_uris,
            "grant_types": grant_types,
            "token_endpoint_auth_method": "none"
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct AuthorizeQuery {
    pub client_id: String,
    pub redirect_uri: String,
    pub response_type: String,
    pub code_challenge: String,
    pub code_challenge_method: Option<String>,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub auto_approve: Option<u8>,
}

pub async fn authorize_get(
    State(state): State<Arc<AppDaemonState>>,
    Query(q): Query<AuthorizeQuery>,
) -> Response {
    if q.response_type != "code" {
        return (StatusCode::BAD_REQUEST, "unsupported_response_type").into_response();
    }
    if q.code_challenge_method.as_deref().unwrap_or("S256") != "S256" {
        return (StatusCode::BAD_REQUEST, "pkce_s256_required").into_response();
    }
    let Some(client) = state
        .ports
        .store
        .get_oauth_client(&q.client_id)
        .ok()
        .flatten()
    else {
        return (StatusCode::BAD_REQUEST, "unknown_client_id").into_response();
    };
    if !client.redirect_uris.iter().any(|u| u == &q.redirect_uri) {
        return (StatusCode::BAD_REQUEST, "redirect_uri_mismatch").into_response();
    }

    let raw_scopes = q
        .scope
        .as_deref()
        .map(ScopeSet::parse_space_separated)
        .unwrap_or_else(ScopeSet::agent_default);
    let effective_scopes = if client.is_agent_client {
        raw_scopes.without_human_only()
    } else {
        raw_scopes
    };

    match &state.auth.config {
        AuthConfig::LocalDev {
            default_owner_id,
            default_email,
            human_http_policy,
            ..
        } => {
            if !client.is_agent_client && *human_http_policy == LocalHumanHttpPolicy::DesktopOnly {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "human_oauth_disabled" })),
                )
                    .into_response();
            }
            let owner_id = OwnerId::new(default_owner_id.clone());
            let actor = if client.is_agent_client {
                Actor::Agent {
                    owner_id: owner_id.clone(),
                    client_id: client.client_id.clone(),
                    runner_label: client.client_name.clone(),
                    token_jti: "pending".into(),
                }
            } else {
                Actor::Human {
                    owner_id: owner_id.clone(),
                    email: default_email.clone(),
                }
            };

            let code = random_token(24);
            state.auth.pending_codes.lock().unwrap().insert(
                code.clone(),
                PendingAuthCode {
                    client_id: client.client_id.clone(),
                    redirect_uri: q.redirect_uri.clone(),
                    code_challenge: q.code_challenge.clone(),
                    owner_id,
                    actor,
                    scopes: effective_scopes.clone(),
                    expires_at: now_ms() + 120_000,
                },
            );

            let mut redirect_target = format!("{}?code={}", q.redirect_uri, code);
            if let Some(st) = &q.state {
                redirect_target.push_str(&format!("&state={st}"));
            }

            if q.auto_approve == Some(1) {
                return Redirect::to(&redirect_target).into_response();
            }

            Html(format!(
                "<!doctype html><html><body style='font-family:system-ui;background:#0d1017;color:#e6edf3;padding:32px'>\
                 <h2>Afterlife OAuth 2.1 Consent (LocalDev)</h2>\
                 <p>Client: <b>{}</b> ({})</p>\
                 <p>Effective Scopes: <code>{}</code></p>\
                 <p style='color:#f59e0b'>Note: Human approval gates (approvals:decide) are structurally withheld from AI agents.</p>\
                 <a href='{}' style='display:inline-block;padding:10px 18px;background:#3b82f6;color:#fff;border-radius:8px;text-decoration:none'>Approve &amp; Connect</a>\
                 </body></html>",
                client.client_name,
                client.client_id,
                effective_scopes.to_space_separated(),
                redirect_target
            ))
            .into_response()
        }
        AuthConfig::GoogleOidc {
            google_client_id,
            hosted_domain,
            ..
        } => {
            let broker_state = random_token(24);
            state.auth.pending_google_states.lock().unwrap().insert(
                broker_state.clone(),
                PendingGoogleState {
                    client_id: client.client_id.clone(),
                    redirect_uri: q.redirect_uri.clone(),
                    code_challenge: q.code_challenge.clone(),
                    client_state: q.state.clone(),
                    requested_scopes: effective_scopes,
                    is_agent_client: client.is_agent_client,
                    client_name: client.client_name.clone(),
                    expires_at: now_ms() + 300_000,
                },
            );
            let callback_url = format!(
                "{}/oauth/google/callback",
                state.config.server.public_base_url
            );
            let google_url = format!(
                "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&hd={}&state={}",
                google_client_id, callback_url, hosted_domain, broker_state
            );
            Redirect::to(&google_url).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub client_id: String,
    pub code: Option<String>,
    pub code_verifier: Option<String>,
    pub redirect_uri: Option<String>,
    pub refresh_token: Option<String>,
}

pub async fn token_post(
    State(state): State<Arc<AppDaemonState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Response {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let req: TokenRequest = if ct.starts_with("application/x-www-form-urlencoded") {
        match <axum::Form<TokenRequest> as axum::extract::FromRequest<()>>::from_request(
            request,
            &(),
        )
        .await
        {
            Ok(axum::Form(v)) => v,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "invalid_request" })),
                )
                    .into_response();
            }
        }
    } else if ct.starts_with("application/json") || ct.is_empty() {
        match <Json<TokenRequest> as axum::extract::FromRequest<()>>::from_request(request, &())
            .await
        {
            Ok(Json(v)) => v,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "invalid_request" })),
                )
                    .into_response();
            }
        }
    } else {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(serde_json::json!({ "error": "unsupported_media_type" })),
        )
            .into_response();
    };
    match req.grant_type.as_str() {
        "authorization_code" => {
            let (Some(code), Some(verifier), Some(redirect_uri)) =
                (req.code, req.code_verifier, req.redirect_uri)
            else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "invalid_request" })),
                )
                    .into_response();
            };

            let pending = state.auth.pending_codes.lock().unwrap().remove(&code);
            let Some(pending) = pending else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "invalid_grant" })),
                )
                    .into_response();
            };
            if pending.expires_at <= now_ms()
                || pending.client_id != req.client_id
                || pending.redirect_uri != redirect_uri
            {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "invalid_grant" })),
                )
                    .into_response();
            }
            if sha256_b64url(&verifier) != pending.code_challenge {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "invalid_pkce_verifier" })),
                )
                    .into_response();
            }

            issue_token_pair(
                &state,
                &pending.owner_id,
                pending.actor,
                &pending.scopes,
                &pending.client_id,
            )
        }
        "refresh_token" => {
            let Some(raw_refresh) = req.refresh_token else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "invalid_request" })),
                )
                    .into_response();
            };
            let hash = sha256_hex(&raw_refresh);
            let consumed = match state.ports.store.consume_refresh_token(&hash, now_ms()) {
                Ok(Some(t)) if t.client_id == req.client_id => t,
                _ => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "invalid_refresh_token" })),
                    )
                        .into_response();
                }
            };
            issue_token_pair(
                &state,
                &consumed.owner_id,
                consumed.actor,
                &consumed.scopes,
                &consumed.client_id,
            )
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "unsupported_grant_type" })),
        )
            .into_response(),
    }
}

fn issue_token_pair(
    state: &Arc<AppDaemonState>,
    owner_id: &OwnerId,
    mut actor: Actor,
    scopes: &ScopeSet,
    client_id: &str,
) -> Response {
    let jti_preview = Uuid::new_v4().to_string();
    if let Actor::Agent {
        ref mut token_jti, ..
    } = actor
    {
        *token_jti = jti_preview;
    }

    let (access_token, _jti, expires_in) = state.auth.mint_access_token(owner_id, &actor, scopes);

    let effective_scopes = if actor.is_human() {
        scopes.clone()
    } else {
        scopes.without_human_only()
    };

    let raw_refresh = random_token(32);
    let stored = StoredRefreshToken {
        token_hash: sha256_hex(&raw_refresh),
        owner_id: owner_id.clone(),
        actor,
        client_id: client_id.to_string(),
        scopes: effective_scopes.clone(),
        expires_at: now_ms() + state.auth.refresh_ttl_secs() * 1_000,
        rotated: false,
    };
    if let Err(e) = state.ports.store.store_refresh_token(stored) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    Json(serde_json::json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": expires_in,
        "refresh_token": raw_refresh,
        "scope": effective_scopes.to_space_separated()
    }))
    .into_response()
}

pub async fn revoke_post(State(state): State<Arc<AppDaemonState>>, headers: HeaderMap) -> Response {
    if let Some(tok) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        if let Ok(principal) = state
            .auth
            .verify_access_token(tok.trim(), state.ports.store.as_ref())
        {
            let _ = state
                .ports
                .store
                .revoke_token_jti(&principal.token_jti, principal.expires_at);
        }
    }
    StatusCode::OK.into_response()
}
