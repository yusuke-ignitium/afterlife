use std::{
    net::SocketAddr,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub const DEFAULT_YOUTUBE_MIRROR_ADDR: &str = "127.0.0.1:47374";
pub const MIN_FRAME_DIM: u32 = 16;
pub const MAX_FRAME_WIDTH: u32 = 2560;
pub const MAX_FRAME_HEIGHT: u32 = 1440;
pub const MAX_FRAME_BYTES: usize = (MAX_FRAME_WIDTH as usize) * (MAX_FRAME_HEIGHT as usize) * 4;
const LIVE_FRAME_FRESH_WINDOW: Duration = Duration::from_secs(3);

struct YoutubeMirrorReceiver {
    activated: AtomicBool,
    enabled: AtomicBool,
    started: AtomicBool,
}

static RECEIVER: OnceLock<YoutubeMirrorReceiver> = OnceLock::new();

fn receiver() -> &'static YoutubeMirrorReceiver {
    RECEIVER.get_or_init(|| YoutubeMirrorReceiver {
        activated: AtomicBool::new(false),
        enabled: AtomicBool::new(false),
        started: AtomicBool::new(false),
    })
}

pub fn activate_receiver(initial_enabled: bool) {
    let r = receiver();
    r.activated.store(true, Ordering::SeqCst);
    apply_enabled_state(r, initial_enabled);
}

pub fn sync_receiver_enabled(enabled: bool) {
    let r = receiver();
    if !r.activated.load(Ordering::SeqCst) {
        return;
    }
    apply_enabled_state(r, enabled);
}

fn apply_enabled_state(r: &YoutubeMirrorReceiver, enabled: bool) {
    r.enabled.store(enabled, Ordering::SeqCst);
    if enabled {
        ensure_server_started(r);
    } else {
        gpui::clear_particle_wave_live_video_frame();
    }
}

fn ensure_server_started(r: &YoutubeMirrorReceiver) {
    if r.started.swap(true, Ordering::SeqCst) {
        return;
    }
    let bind_addr = resolve_bind_addr();
    let _ = std::thread::Builder::new()
        .name("afterlife-youtube-mirror".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[afterlife-ui] youtube mirror runtime error: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                let listener = match tokio::net::TcpListener::bind(bind_addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!(
                            "[afterlife-ui] youtube mirror skipped ({bind_addr} unavailable: {e})"
                        );
                        return;
                    }
                };
                let router = build_router();
                if let Err(e) = axum::serve(listener, router).await {
                    eprintln!("[afterlife-ui] youtube mirror stopped: {e}");
                }
            });
        });
}

pub fn parse_loopback_addr(raw: &str) -> Option<SocketAddr> {
    let addr: SocketAddr = raw.trim().parse().ok()?;
    addr.ip().is_loopback().then_some(addr)
}

fn resolve_bind_addr() -> SocketAddr {
    std::env::var("AFTERLIFE_YOUTUBE_MIRROR_ADDR")
        .ok()
        .and_then(|raw| parse_loopback_addr(&raw))
        .unwrap_or_else(|| {
            DEFAULT_YOUTUBE_MIRROR_ADDR
                .parse()
                .expect("valid default loopback address")
        })
}

pub fn is_allowed_origin(origin: &str) -> bool {
    let trimmed = origin.trim();
    if matches!(
        trimmed,
        "https://www.youtube.com" | "https://m.youtube.com" | "https://music.youtube.com"
    ) {
        return true;
    }
    if let Some(id) = trimmed.strip_prefix("chrome-extension://") {
        return !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    }
    false
}

pub fn validate_frame_dimensions(width: u32, height: u32, byte_len: usize) -> Result<(), String> {
    if !(MIN_FRAME_DIM..=MAX_FRAME_WIDTH).contains(&width) {
        return Err(format!(
            "width {width} out of allowed range {MIN_FRAME_DIM}..={MAX_FRAME_WIDTH}"
        ));
    }
    if !(MIN_FRAME_DIM..=MAX_FRAME_HEIGHT).contains(&height) {
        return Err(format!(
            "height {height} out of allowed range {MIN_FRAME_DIM}..={MAX_FRAME_HEIGHT}"
        ));
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| "frame dimensions overflow".to_string())?;
    if expected > MAX_FRAME_BYTES {
        return Err(format!(
            "frame byte size {expected} exceeds maximum {MAX_FRAME_BYTES}"
        ));
    }
    if byte_len != expected {
        return Err(format!(
            "payload length {byte_len} does not match {width}x{height} RGBA8 ({expected} bytes)"
        ));
    }
    Ok(())
}

fn extract_origin(headers: &HeaderMap) -> Option<&str> {
    headers.get("origin").and_then(|v| v.to_str().ok())
}

fn attach_cors_headers(mut resp: Response, origin: Option<&str>) -> Response {
    let allow_origin = origin
        .filter(|o| is_allowed_origin(o))
        .and_then(|o| HeaderValue::from_str(o).ok())
        .unwrap_or_else(|| HeaderValue::from_static("*"));
    let headers = resp.headers_mut();
    headers.insert(
        HeaderName::from_static("access-control-allow-origin"),
        allow_origin,
    );
    headers.insert(
        HeaderName::from_static("access-control-allow-methods"),
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        HeaderName::from_static("access-control-allow-headers"),
        HeaderValue::from_static("Content-Type"),
    );
    headers.insert(
        HeaderName::from_static("access-control-allow-private-network"),
        HeaderValue::from_static("true"),
    );
    headers.insert(
        HeaderName::from_static("access-control-max-age"),
        HeaderValue::from_static("86400"),
    );
    headers.insert(
        HeaderName::from_static("vary"),
        HeaderValue::from_static("Origin"),
    );
    resp
}

fn reject_disallowed_origin(headers: &HeaderMap) -> Option<Response> {
    let origin = extract_origin(headers)?;
    if is_allowed_origin(origin) {
        return None;
    }
    Some(
        (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "origin not allowed" })),
        )
            .into_response(),
    )
}

fn current_priority(enabled: bool, apple_music_active: bool) -> &'static str {
    if !enabled {
        "disabled"
    } else if apple_music_active {
        "apple_music"
    } else {
        "youtube"
    }
}

fn is_youtube_live_active() -> bool {
    let (_, frame) = gpui::particle_wave_live_video_frame();
    frame.is_some_and(|f| f.updated_at.elapsed() <= LIVE_FRAME_FRESH_WINDOW)
}

fn build_status_payload(enabled: bool, accepted: Option<bool>) -> Value {
    let apple_music_active = gpui::particle_wave_override_image_active();
    let (grid_w, grid_h) = gpui::particle_wave_grid_dimensions();
    let priority = current_priority(enabled, apple_music_active);
    let mut payload = json!({
        "enabled": enabled,
        "apple_music_active": apple_music_active,
        "youtube_active": enabled && is_youtube_live_active(),
        "priority": priority,
        "grid_w": grid_w,
        "grid_h": grid_h,
        "max_width": MAX_FRAME_WIDTH,
        "max_height": MAX_FRAME_HEIGHT,
    });
    if let Some(acc) = accepted
        && let Some(obj) = payload.as_object_mut()
    {
        obj.insert("accepted".to_string(), Value::Bool(acc));
    }
    payload
}

#[derive(Deserialize)]
struct FrameQuery {
    w: Option<u32>,
    h: Option<u32>,
}

fn is_encoded_image_request(headers: &HeaderMap, query: &FrameQuery) -> bool {
    if query.w.is_none() || query.h.is_none() {
        return true;
    }
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| {
            let lower = ct.to_ascii_lowercase();
            lower.starts_with("image/jpeg")
                || lower.starts_with("image/webp")
                || lower.starts_with("image/png")
        })
}

async fn handle_preflight(headers: HeaderMap) -> Response {
    if let Some(rejected) = reject_disallowed_origin(&headers) {
        return rejected;
    }
    let origin = extract_origin(&headers);
    attach_cors_headers(StatusCode::NO_CONTENT.into_response(), origin)
}

async fn handle_status(headers: HeaderMap) -> Response {
    if let Some(rejected) = reject_disallowed_origin(&headers) {
        return rejected;
    }
    let origin = extract_origin(&headers);
    let enabled = receiver().enabled.load(Ordering::SeqCst);
    let body = build_status_payload(enabled, None);
    attach_cors_headers((StatusCode::OK, Json(body)).into_response(), origin)
}

async fn handle_frame(
    headers: HeaderMap,
    Query(query): Query<FrameQuery>,
    body: Bytes,
) -> Response {
    if let Some(rejected) = reject_disallowed_origin(&headers) {
        return rejected;
    }
    let origin = extract_origin(&headers);
    let enabled = receiver().enabled.load(Ordering::SeqCst);
    if is_encoded_image_request(&headers, &query) {
        if body.is_empty() || body.len() > MAX_FRAME_BYTES {
            return attach_cors_headers(
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "encoded frame payload size out of bounds" })),
                )
                    .into_response(),
                origin,
            );
        }
        if enabled {
            let decoded = tokio::task::spawn_blocking(move || {
                gpui::set_particle_wave_live_video_encoded(
                    &body,
                    MIN_FRAME_DIM,
                    MAX_FRAME_WIDTH,
                    MAX_FRAME_HEIGHT,
                )
            })
            .await;
            match decoded {
                Ok(Ok(_)) => {}
                Ok(Err(msg)) => {
                    return attach_cors_headers(
                        (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response(),
                        origin,
                    );
                }
                Err(e) => {
                    return attach_cors_headers(
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": format!("decode worker failed: {e}") })),
                        )
                            .into_response(),
                        origin,
                    );
                }
            }
        }
    } else {
        let (w, h) = (query.w.unwrap_or(0), query.h.unwrap_or(0));
        if let Err(msg) = validate_frame_dimensions(w, h, body.len()) {
            return attach_cors_headers(
                (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response(),
                origin,
            );
        }
        if enabled {
            gpui::set_particle_wave_live_video_frame(w, h, body.to_vec());
        }
    }
    let payload = build_status_payload(enabled, Some(enabled));
    attach_cors_headers((StatusCode::OK, Json(payload)).into_response(), origin)
}

async fn handle_stop(headers: HeaderMap) -> Response {
    if let Some(rejected) = reject_disallowed_origin(&headers) {
        return rejected;
    }
    let origin = extract_origin(&headers);
    gpui::clear_particle_wave_live_video_frame();
    attach_cors_headers(
        (StatusCode::OK, Json(json!({ "stopped": true }))).into_response(),
        origin,
    )
}

pub fn build_router() -> Router {
    Router::new()
        .route(
            "/v1/youtube-mirror/status",
            get(handle_status).options(handle_preflight),
        )
        .route(
            "/v1/youtube-mirror/frame",
            post(handle_frame).options(handle_preflight),
        )
        .route(
            "/v1/youtube-mirror/stop",
            post(handle_stop).options(handle_preflight),
        )
        .layer(DefaultBodyLimit::max(MAX_FRAME_BYTES + 1024))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_origin_accepts_youtube_and_chrome_extensions_only() {
        assert!(is_allowed_origin("https://www.youtube.com"));
        assert!(is_allowed_origin("https://m.youtube.com"));
        assert!(is_allowed_origin("https://music.youtube.com"));
        assert!(is_allowed_origin(
            "chrome-extension://abcdefghijklmnopqrstuvwxyzabcdef"
        ));
        assert!(!is_allowed_origin("https://evil.example.com"));
        assert!(!is_allowed_origin("http://www.youtube.com"));
        assert!(!is_allowed_origin("chrome-extension://"));
        assert!(!is_allowed_origin("chrome-extension://bad/path"));
    }

    #[test]
    fn parse_loopback_addr_rejects_non_loopback_binds() {
        assert!(parse_loopback_addr("127.0.0.1:47374").is_some());
        assert!(parse_loopback_addr("[::1]:47374").is_some());
        assert!(parse_loopback_addr("0.0.0.0:47374").is_none());
        assert!(parse_loopback_addr("192.168.1.10:47374").is_none());
        assert!(parse_loopback_addr("not-an-addr").is_none());
    }

    #[test]
    fn validate_frame_dimensions_accepts_up_to_2560x1440_and_rejects_mismatches() {
        assert!(validate_frame_dimensions(16, 16, 16 * 16 * 4).is_ok());
        assert!(validate_frame_dimensions(1344, 756, 1344 * 756 * 4).is_ok());
        assert!(validate_frame_dimensions(2560, 1440, 2560 * 1440 * 4).is_ok());
        assert!(validate_frame_dimensions(15, 16, 15 * 16 * 4).is_err());
        assert!(validate_frame_dimensions(2561, 1440, 2561 * 1440 * 4).is_err());
        assert!(validate_frame_dimensions(128, 128, 128 * 128 * 3).is_err());
    }

    #[test]
    fn http_endpoints_ingest_frames_report_grid_and_respect_priority() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let _ = axum::serve(listener, build_router()).await;
            });

            receiver().enabled.store(true, Ordering::SeqCst);
            gpui::set_particle_wave_override_image(None);
            gpui::set_particle_wave_grid_dimensions(1200, 754);
            gpui::clear_particle_wave_live_video_frame();

            let base = format!("http://{addr}");
            for path in ["status", "frame", "stop"] {
                let (status, pna, allow_origin) = tokio::task::spawn_blocking({
                    let url = format!("{base}/v1/youtube-mirror/{path}");
                    move || {
                        let resp = ureq::request("OPTIONS", &url)
                            .set("Origin", "https://www.youtube.com")
                            .set("Access-Control-Request-Method", "POST")
                            .set("Access-Control-Request-Private-Network", "true")
                            .call()
                            .unwrap();
                        (
                            resp.status(),
                            resp.header("access-control-allow-private-network")
                                .map(|s| s.to_string()),
                            resp.header("access-control-allow-origin")
                                .map(|s| s.to_string()),
                        )
                    }
                })
                .await
                .unwrap();
                assert_eq!(status, 204);
                assert_eq!(pna.as_deref(), Some("true"));
                assert_eq!(allow_origin.as_deref(), Some("https://www.youtube.com"));
            }

            let status_resp = tokio::task::spawn_blocking({
                let base = base.clone();
                move || {
                    let raw = ureq::get(&format!("{base}/v1/youtube-mirror/status"))
                        .set("Origin", "https://www.youtube.com")
                        .call()
                        .unwrap()
                        .into_string()
                        .unwrap();
                    serde_json::from_str::<Value>(&raw).unwrap()
                }
            })
            .await
            .unwrap();
            assert_eq!(status_resp["enabled"], json!(true));
            assert_eq!(status_resp["priority"], json!("youtube"));
            assert_eq!(status_resp["grid_w"], json!(1200));
            assert_eq!(status_resp["grid_h"], json!(754));

            let rgba = vec![128u8; 32 * 18 * 4];
            let frame_resp = tokio::task::spawn_blocking({
                let base = base.clone();
                move || {
                    let raw = ureq::post(&format!("{base}/v1/youtube-mirror/frame?w=32&h=18"))
                        .set("Origin", "https://www.youtube.com")
                        .send_bytes(&rgba)
                        .unwrap()
                        .into_string()
                        .unwrap();
                    serde_json::from_str::<Value>(&raw).unwrap()
                }
            })
            .await
            .unwrap();
            assert_eq!(frame_resp["accepted"], json!(true));
            assert_eq!(frame_resp["youtube_active"], json!(true));
            let (_, stored) = gpui::particle_wave_live_video_frame();
            let stored = stored.expect("live frame stored");
            assert_eq!(stored.width, 32);
            assert_eq!(stored.height, 18);

            let png_bytes: &[u8] = include_bytes!("../../assets/afterlife-icon.png");
            let encoded_resp = tokio::task::spawn_blocking({
                let base = base.clone();
                move || {
                    let raw = ureq::post(&format!("{base}/v1/youtube-mirror/frame"))
                        .set(
                            "Origin",
                            "chrome-extension://abcdefghijklmnopqrstuvwxyzabcdef",
                        )
                        .set("Content-Type", "image/png")
                        .send_bytes(png_bytes)
                        .unwrap()
                        .into_string()
                        .unwrap();
                    serde_json::from_str::<Value>(&raw).unwrap()
                }
            })
            .await
            .unwrap();
            assert_eq!(encoded_resp["accepted"], json!(true));
            let (_, decoded_stored) = gpui::particle_wave_live_video_frame();
            let decoded_stored = decoded_stored.expect("encoded live frame decoded");
            assert_eq!(decoded_stored.width, 1024);
            assert_eq!(decoded_stored.height, 1024);

            gpui::set_particle_wave_override_image(Some("/tmp/fake-artwork.jpg".to_string()));
            let priority_resp = tokio::task::spawn_blocking({
                let base = base.clone();
                move || {
                    let raw = ureq::get(&format!("{base}/v1/youtube-mirror/status"))
                        .set("Origin", "https://www.youtube.com")
                        .call()
                        .unwrap()
                        .into_string()
                        .unwrap();
                    serde_json::from_str::<Value>(&raw).unwrap()
                }
            })
            .await
            .unwrap();
            assert_eq!(priority_resp["apple_music_active"], json!(true));
            assert_eq!(priority_resp["priority"], json!("apple_music"));
            gpui::set_particle_wave_override_image(None);

            let forbidden_status = tokio::task::spawn_blocking({
                let base = base.clone();
                move || match ureq::get(&format!("{base}/v1/youtube-mirror/status"))
                    .set("Origin", "https://malicious.example.com")
                    .call()
                {
                    Ok(resp) => resp.status(),
                    Err(ureq::Error::Status(code, _)) => code,
                    Err(_) => 0,
                }
            })
            .await
            .unwrap();
            assert_eq!(forbidden_status, 403);

            let _ = tokio::task::spawn_blocking({
                let base = base.clone();
                move || {
                    ureq::post(&format!("{base}/v1/youtube-mirror/stop"))
                        .set("Origin", "https://www.youtube.com")
                        .call()
                        .unwrap()
                }
            })
            .await
            .unwrap();
            let (_, cleared) = gpui::particle_wave_live_video_frame();
            assert!(cleared.is_none());

            server.abort();
        });
    }
}
