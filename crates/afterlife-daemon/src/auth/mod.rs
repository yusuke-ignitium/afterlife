mod oauth_routes;

pub use oauth_routes::{
    AuthorizeQuery, RegisterClientRequest, TokenRequest, authorize_get, register_client,
    revoke_post, token_post, well_known_authorization_server, well_known_protected_resource,
};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use afterlife_core::{Actor, OwnerId, TimestampMs};
use afterlife_protocol::{
    AuthConfig, LocalHumanHttpPolicy, Principal, ScopeSet, WorkspaceStore,
};
use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AppDaemonState;

type HmacSha256 = Hmac<Sha256>;

pub fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(super) fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

pub(super) fn sha256_b64url(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    URL_SAFE_NO_PAD.encode(h.finalize())
}

pub(super) fn sha256_hex(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    format!("{:x}", h.finalize())
}

#[derive(Clone, Debug)]
pub struct PendingAuthCode {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub owner_id: OwnerId,
    pub actor: Actor,
    pub scopes: ScopeSet,
    pub expires_at: TimestampMs,
}

#[derive(Clone, Debug)]
pub struct PendingGoogleState {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub client_state: Option<String>,
    pub requested_scopes: ScopeSet,
    pub is_agent_client: bool,
    pub client_name: String,
    pub expires_at: TimestampMs,
}

pub struct AuthBroker {
    pub config: AuthConfig,
    jwt_secret: Vec<u8>,
    pub pending_codes: Mutex<HashMap<String, PendingAuthCode>>,
    pub pending_google_states: Mutex<HashMap<String, PendingGoogleState>>,
}

#[derive(Serialize, Deserialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    jti: String,
    exp: u64,
    iat: u64,
    scope: String,
    actor: Actor,
}

impl AuthBroker {
    pub fn new(config: AuthConfig) -> Self {
        let secret_env = match &config {
            AuthConfig::LocalDev { jwt_secret_env, .. }
            | AuthConfig::GoogleOidc { jwt_secret_env, .. } => jwt_secret_env.as_str(),
        };
        let jwt_secret = std::env::var(secret_env)
            .map(|s| s.into_bytes())
            .unwrap_or_else(|_| {
                let mut key = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                key
            });
        Self {
            config,
            jwt_secret,
            pending_codes: Mutex::new(HashMap::new()),
            pending_google_states: Mutex::new(HashMap::new()),
        }
    }

    pub fn issuer(&self) -> &str {
        match &self.config {
            AuthConfig::LocalDev { issuer, .. } | AuthConfig::GoogleOidc { issuer, .. } => issuer,
        }
    }

    pub(super) fn access_ttl_secs(&self) -> u64 {
        match &self.config {
            AuthConfig::LocalDev {
                access_token_ttl_secs,
                ..
            }
            | AuthConfig::GoogleOidc {
                access_token_ttl_secs,
                ..
            } => *access_token_ttl_secs,
        }
    }

    pub(super) fn refresh_ttl_secs(&self) -> u64 {
        match &self.config {
            AuthConfig::LocalDev {
                refresh_token_ttl_secs,
                ..
            }
            | AuthConfig::GoogleOidc {
                refresh_token_ttl_secs,
                ..
            } => *refresh_token_ttl_secs,
        }
    }

    pub fn mint_access_token(
        &self,
        owner_id: &OwnerId,
        actor: &Actor,
        requested_scopes: &ScopeSet,
    ) -> (String, String, u64) {
        let effective_scopes = if actor.is_human() {
            requested_scopes.clone()
        } else {
            requested_scopes.without_human_only()
        };
        let iat = now_ms() / 1_000;
        let ttl = self.access_ttl_secs();
        let exp = iat + ttl;
        let jti = Uuid::new_v4().to_string();

        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        let claims = JwtClaims {
            iss: self.issuer().to_string(),
            sub: owner_id.as_str().to_string(),
            jti: jti.clone(),
            exp,
            iat,
            scope: effective_scopes.to_space_separated(),
            actor: actor.clone(),
        };
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        let signing_input = format!("{header}.{payload}");
        let mut mac =
            HmacSha256::new_from_slice(&self.jwt_secret).expect("HMAC accepts any key size");
        mac.update(signing_input.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        (format!("{signing_input}.{sig}"), jti, ttl)
    }

    pub fn verify_access_token(
        &self,
        token: &str,
        store: &dyn WorkspaceStore,
    ) -> Result<Principal, (StatusCode, &'static str)> {
        let mut parts = token.split('.');
        let (Some(header), Some(payload), Some(sig), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err((StatusCode::UNAUTHORIZED, "malformed bearer token"));
        };
        let signing_input = format!("{header}.{payload}");
        let mut mac =
            HmacSha256::new_from_slice(&self.jwt_secret).expect("HMAC accepts any key size");
        mac.update(signing_input.as_bytes());
        let expected_sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        if expected_sig != sig {
            return Err((StatusCode::UNAUTHORIZED, "invalid token signature"));
        }
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token encoding"))?;
        let claims: JwtClaims = serde_json::from_slice(&payload_bytes)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token claims"))?;

        let now_sec = now_ms() / 1_000;
        if claims.exp <= now_sec {
            return Err((StatusCode::UNAUTHORIZED, "access token expired"));
        }
        if store.is_token_jti_revoked(&claims.jti).unwrap_or(true) {
            return Err((StatusCode::UNAUTHORIZED, "access token revoked"));
        }

        let scopes = ScopeSet::parse_space_separated(&claims.scope);
        let scopes = if claims.actor.is_human() {
            scopes
        } else {
            scopes.without_human_only()
        };

        Ok(Principal {
            owner_id: OwnerId::new(claims.sub),
            actor: claims.actor,
            scopes,
            token_jti: claims.jti,
            expires_at: claims.exp * 1_000,
        })
    }

    pub fn allows_local_human_http(&self) -> bool {
        matches!(
            self.config,
            AuthConfig::LocalDev {
                human_http_policy: LocalHumanHttpPolicy::Enabled,
                ..
            }
        )
    }

    pub fn human_http_policy_label(&self) -> &'static str {
        match self.config {
            AuthConfig::LocalDev {
                human_http_policy: LocalHumanHttpPolicy::Enabled,
                ..
            } => "enabled",
            AuthConfig::LocalDev {
                human_http_policy: LocalHumanHttpPolicy::DesktopOnly,
                ..
            } => "desktop_only",
            AuthConfig::GoogleOidc { .. } => "oidc_required",
        }
    }

    pub fn default_local_principal(&self) -> Option<Principal> {
        match &self.config {
            AuthConfig::LocalDev {
                default_owner_id,
                default_email,
                human_http_policy: LocalHumanHttpPolicy::Enabled,
                ..
            } => {
                let owner_id = OwnerId::new(default_owner_id.clone());
                Some(Principal {
                    owner_id: owner_id.clone(),
                    actor: Actor::Human {
                        owner_id,
                        email: default_email.clone(),
                    },
                    scopes: ScopeSet::human_full(),
                    token_jti: "local-dev-session".into(),
                    expires_at: now_ms() + 3_600_000,
                })
            }
            AuthConfig::LocalDev {
                default_owner_id,
                human_http_policy: LocalHumanHttpPolicy::DesktopOnly,
                ..
            } => {
                let owner_id = OwnerId::new(default_owner_id.clone());
                Some(Principal {
                    owner_id: owner_id.clone(),
                    actor: Actor::Agent {
                        owner_id,
                        client_id: "local-readonly".into(),
                        runner_label: "Local Read-Only Observer".into(),
                        token_jti: "local-readonly-session".into(),
                    },
                    scopes: ScopeSet::parse_space_separated("jobs:read"),
                    token_jti: "local-readonly-session".into(),
                    expires_at: now_ms() + 3_600_000,
                })
            }
            AuthConfig::GoogleOidc { .. } => None,
        }
    }

    pub fn default_local_human_write_principal(&self) -> Option<Principal> {
        if self.allows_local_human_http() {
            self.default_local_principal()
        } else {
            None
        }
    }

    pub fn default_local_agent_principal(&self) -> Option<Principal> {
        match &self.config {
            AuthConfig::LocalDev {
                default_owner_id, ..
            } => {
                let owner_id = OwnerId::new(default_owner_id.clone());
                Some(Principal {
                    owner_id: owner_id.clone(),
                    actor: Actor::Agent {
                        owner_id,
                        client_id: "local-mcp".into(),
                        runner_label: "Local MCP Agent".into(),
                        token_jti: "local-mcp-session".into(),
                    },
                    scopes: ScopeSet::agent_default(),
                    token_jti: "local-mcp-session".into(),
                    expires_at: now_ms() + 3_600_000,
                })
            }
            AuthConfig::GoogleOidc { .. } => None,
        }
    }
}

fn unauthorized_with_resource_metadata(state: &AppDaemonState, error: &str) -> Response {
    let mut resp = (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": error })),
    )
        .into_response();
    let www = format!(
        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
        state.config.server.public_base_url
    );
    if let Ok(hv) = www.parse() {
        resp.headers_mut().insert(header::WWW_AUTHENTICATE, hv);
    }
    resp
}

pub struct AuthPrincipal(pub Principal);

impl FromRequestParts<Arc<AppDaemonState>> for AuthPrincipal {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppDaemonState>,
    ) -> Result<Self, Self::Rejection> {
        if let Some(auth_val) = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        {
            if let Some(token) = auth_val.strip_prefix("Bearer ") {
                return state
                    .auth
                    .verify_access_token(token.trim(), state.ports.store.as_ref())
                    .map(AuthPrincipal)
                    .map_err(|(code, msg)| {
                        let mut resp =
                            (code, Json(serde_json::json!({ "error": msg }))).into_response();
                        let www = format!(
                            "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
                            state.config.server.public_base_url
                        );
                        if let Ok(hv) = www.parse() {
                            resp.headers_mut().insert(header::WWW_AUTHENTICATE, hv);
                        }
                        resp
                    });
            }
        }

        if let Some(local) = state.auth.default_local_principal() {
            return Ok(AuthPrincipal(local));
        }

        Err(unauthorized_with_resource_metadata(
            state,
            "authentication_required",
        ))
    }
}

pub struct HumanWritePrincipal(pub Principal);

impl FromRequestParts<Arc<AppDaemonState>> for HumanWritePrincipal {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppDaemonState>,
    ) -> Result<Self, Self::Rejection> {
        if let Some(auth_val) = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        {
            if let Some(token) = auth_val.strip_prefix("Bearer ") {
                return state
                    .auth
                    .verify_access_token(token.trim(), state.ports.store.as_ref())
                    .map(HumanWritePrincipal)
                    .map_err(|(code, msg)| {
                        let mut resp =
                            (code, Json(serde_json::json!({ "error": msg }))).into_response();
                        let www = format!(
                            "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
                            state.config.server.public_base_url
                        );
                        if let Ok(hv) = www.parse() {
                            resp.headers_mut().insert(header::WWW_AUTHENTICATE, hv);
                        }
                        resp
                    });
            }
        }

        if let Some(local_human) = state.auth.default_local_human_write_principal() {
            return Ok(HumanWritePrincipal(local_human));
        }

        let err_code = match state.config.auth {
            AuthConfig::LocalDev {
                human_http_policy: LocalHumanHttpPolicy::DesktopOnly,
                ..
            } => "human_http_disabled",
            _ => "authentication_required",
        };
        Err(unauthorized_with_resource_metadata(state, err_code))
    }
}

pub struct McpPrincipal(pub Principal);

impl FromRequestParts<Arc<AppDaemonState>> for McpPrincipal {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppDaemonState>,
    ) -> Result<Self, Self::Rejection> {
        if let Some(auth_val) = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        {
            if let Some(token) = auth_val.strip_prefix("Bearer ") {
                return state
                    .auth
                    .verify_access_token(token.trim(), state.ports.store.as_ref())
                    .map(McpPrincipal)
                    .map_err(|(code, msg)| {
                        let mut resp =
                            (code, Json(serde_json::json!({ "error": msg }))).into_response();
                        let www = format!(
                            "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
                            state.config.server.public_base_url
                        );
                        if let Ok(hv) = www.parse() {
                            resp.headers_mut().insert(header::WWW_AUTHENTICATE, hv);
                        }
                        resp
                    });
            }
        }

        if let Some(local_agent) = state.auth.default_local_agent_principal() {
            return Ok(McpPrincipal(local_agent));
        }

        Err(unauthorized_with_resource_metadata(
            state,
            "authentication_required",
        ))
    }
}

