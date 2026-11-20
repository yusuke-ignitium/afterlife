use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, SystemTime},
};

use afterlife_protocol::default_backdrop_data_dir;

const MAX_CACHE_ENTRIES: usize = 200;
const MIN_IMAGE_BYTES: usize = 64;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const HTTP_READ_TIMEOUT: Duration = Duration::from_secs(6);
const ITUNES_ARTWORK_DIM: &str = "1200x1200bb.jpg";
const MIN_ITUNES_SEARCH_SCORE: i32 = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MusicTrackEvent {
    pub is_playing: bool,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub store_url: Option<String>,
    pub persistent_id: Option<String>,
}

enum WorkerMsg {
    InitialSync {
        generation: u64,
    },
    TrackEvent {
        generation: u64,
        event: MusicTrackEvent,
    },
    ClearActiveKey,
}

struct AppleMusicWatcher {
    activated: AtomicBool,
    enabled: AtomicBool,
    generation: AtomicU64,
    tx: Mutex<Option<mpsc::Sender<WorkerMsg>>>,
    #[cfg(target_os = "macos")]
    observer_ptr: Mutex<Option<usize>>,
}

static WATCHER: OnceLock<AppleMusicWatcher> = OnceLock::new();

fn watcher() -> &'static AppleMusicWatcher {
    WATCHER.get_or_init(|| AppleMusicWatcher {
        activated: AtomicBool::new(false),
        enabled: AtomicBool::new(false),
        generation: AtomicU64::new(0),
        tx: Mutex::new(None),
        #[cfg(target_os = "macos")]
        observer_ptr: Mutex::new(None),
    })
}

pub fn activate_watcher(initial_enabled: bool) {
    let w = watcher();
    w.activated.store(true, Ordering::SeqCst);
    apply_enabled_state(w, initial_enabled);
}

pub fn sync_watcher_enabled(enabled: bool) {
    let w = watcher();
    if !w.activated.load(Ordering::SeqCst) {
        return;
    }
    apply_enabled_state(w, enabled);
}

fn ensure_worker_tx(w: &AppleMusicWatcher) -> mpsc::Sender<WorkerMsg> {
    let mut guard = w.tx.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref tx) = *guard {
        return tx.clone();
    }
    let (tx, rx) = mpsc::channel::<WorkerMsg>();
    let _ = std::thread::Builder::new()
        .name("afterlife-apple-music".into())
        .spawn(move || run_worker_loop(rx));
    *guard = Some(tx.clone());
    tx
}

fn apply_enabled_state(w: &AppleMusicWatcher, enabled: bool) {
    let prev = w.enabled.swap(enabled, Ordering::SeqCst);
    if prev == enabled {
        return;
    }
    if enabled {
        let generation = w.generation.fetch_add(1, Ordering::SeqCst) + 1;
        #[cfg(target_os = "macos")]
        macos_bridge::register_observer(w);
        let tx = ensure_worker_tx(w);
        let _ = tx.send(WorkerMsg::InitialSync { generation });
    } else {
        w.generation.fetch_add(1, Ordering::SeqCst);
        gpui::set_particle_wave_override_image(None);
        #[cfg(target_os = "macos")]
        macos_bridge::unregister_observer(w);
        if let Ok(guard) = w.tx.lock()
            && let Some(ref tx) = *guard
        {
            let _ = tx.send(WorkerMsg::ClearActiveKey);
        }
    }
}

#[cfg(target_os = "macos")]
fn handle_notification_event(event: MusicTrackEvent) {
    let w = watcher();
    if !w.enabled.load(Ordering::SeqCst) {
        return;
    }
    let generation = w.generation.fetch_add(1, Ordering::SeqCst) + 1;
    if !event.is_playing {
        gpui::set_particle_wave_override_image(None);
        if let Ok(guard) = w.tx.lock()
            && let Some(ref tx) = *guard
        {
            let _ = tx.send(WorkerMsg::ClearActiveKey);
        }
        return;
    }
    let tx = ensure_worker_tx(w);
    let _ = tx.send(WorkerMsg::TrackEvent { generation, event });
}

fn run_worker_loop(rx: mpsc::Receiver<WorkerMsg>) {
    let cache_dir = artwork_cache_dir();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(HTTP_CONNECT_TIMEOUT)
        .timeout_read(HTTP_READ_TIMEOUT)
        .user_agent("Afterlife/1.0")
        .build();
    let mut active_playing_key: Option<String> = None;

    while let Ok(msg) = rx.recv() {
        let w = watcher();
        match msg {
            WorkerMsg::ClearActiveKey => {
                active_playing_key = None;
            }
            WorkerMsg::InitialSync { generation } => {
                if !is_generation_current(w, generation) {
                    continue;
                }
                let Some(event) = query_current_track_if_running() else {
                    continue;
                };
                if !is_generation_current(w, generation) {
                    continue;
                }
                if !event.is_playing {
                    active_playing_key = None;
                    gpui::set_particle_wave_override_image(None);
                    continue;
                }
                process_playing_event(
                    w,
                    &agent,
                    &cache_dir,
                    generation,
                    &event,
                    &mut active_playing_key,
                );
            }
            WorkerMsg::TrackEvent { generation, event } => {
                if !is_generation_current(w, generation) {
                    continue;
                }
                process_playing_event(
                    w,
                    &agent,
                    &cache_dir,
                    generation,
                    &event,
                    &mut active_playing_key,
                );
            }
        }
    }
}

fn is_generation_current(w: &AppleMusicWatcher, generation: u64) -> bool {
    w.enabled.load(Ordering::SeqCst) && w.generation.load(Ordering::SeqCst) == generation
}

fn process_playing_event(
    w: &AppleMusicWatcher,
    agent: &ureq::Agent,
    cache_dir: &Path,
    generation: u64,
    event: &MusicTrackEvent,
    active_playing_key: &mut Option<String>,
) {
    if event.name.trim().is_empty() && event.album.trim().is_empty() && event.store_url.is_none() {
        return;
    }
    let key = track_cache_key(event);
    if active_playing_key.as_deref() == Some(key.as_str()) {
        return;
    }
    let resolved = resolve_artwork_path(agent, cache_dir, &key, event, generation);
    if !is_generation_current(w, generation) {
        return;
    }
    match resolved {
        Some(path) => {
            *active_playing_key = Some(key);
            gpui::set_particle_wave_override_image(Some(path.to_string_lossy().into_owned()));
        }
        None => {
            *active_playing_key = None;
            gpui::set_particle_wave_override_image(None);
        }
    }
}

fn artwork_cache_dir() -> PathBuf {
    default_backdrop_data_dir().join("music_artwork_cache")
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn track_identity_string(event: &MusicTrackEvent) -> String {
    let artist = normalize_match_text(&event.artist);
    let album = normalize_match_text(&event.album);
    let name = normalize_match_text(&event.name);
    if !name.is_empty() || !album.is_empty() {
        return format!("meta:{artist}|{album}|{name}");
    }
    if let Some(ref url) = event.store_url
        && let Some(id) = extract_adam_id(url)
    {
        return format!("adam:{id}");
    }
    if let Some(ref pid) = event.persistent_id {
        let trimmed = pid.trim();
        if !trimmed.is_empty() {
            return format!("pid:{}", trimmed.to_ascii_lowercase());
        }
    }
    String::new()
}

pub fn track_cache_key(event: &MusicTrackEvent) -> String {
    let id = track_identity_string(event);
    format!("{:016x}", fnv1a_64(id.as_bytes()))
}

pub fn extract_adam_id(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((_, query)) = trimmed.split_once('?') {
        for key in ["i", "adamId", "p", "id"] {
            for pair in query.split('&') {
                if let Some((k, v)) = pair.split_once('=')
                    && k == key
                {
                    let digits: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if digits.len() >= 5 {
                        return Some(digits);
                    }
                }
            }
        }
    }
    let path_part = trimmed.split('?').next().unwrap_or(trimmed);
    for seg in path_part.rsplit('/') {
        let stripped = seg.strip_prefix("id").unwrap_or(seg);
        if stripped.len() >= 5 && stripped.chars().all(|c| c.is_ascii_digit()) {
            return Some(stripped.to_string());
        }
    }
    None
}

pub fn upscale_itunes_artwork_url(url: &str) -> String {
    if let Some(idx) = url.rfind('/') {
        let tail = &url[idx + 1..];
        if tail.contains("bb.") || tail.ends_with(".jpg") || tail.ends_with(".png") {
            return format!("{}/{}", &url[..idx], ITUNES_ARTWORK_DIM);
        }
    }
    url.to_string()
}

pub fn detect_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < MIN_IMAGE_BYTES || bytes.len() > MAX_IMAGE_BYTES {
        return None;
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("png");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    None
}

fn find_cached_artwork(cache_dir: &Path, key: &str) -> Option<PathBuf> {
    for ext in ["jpg", "png", "webp"] {
        let path = cache_dir.join(format!("{key}.{ext}"));
        if let Ok(meta) = fs::metadata(&path)
            && meta.is_file()
            && meta.len() >= MIN_IMAGE_BYTES as u64
        {
            return Some(path);
        }
    }
    None
}

fn save_cached_artwork(
    cache_dir: &Path,
    key: &str,
    bytes: &[u8],
    generation: u64,
) -> Option<PathBuf> {
    let ext = detect_image_extension(bytes)?;
    fs::create_dir_all(cache_dir).ok()?;
    let target = cache_dir.join(format!("{key}.{ext}"));
    let tmp = cache_dir.join(format!(
        "{key}.{ext}.tmp.{}.{generation}",
        std::process::id()
    ));
    if fs::write(&tmp, bytes).is_err() {
        let _ = fs::remove_file(&tmp);
        return None;
    }
    if fs::rename(&tmp, &target).is_err() {
        let _ = fs::remove_file(&tmp);
        return None;
    }
    prune_cache_dir(cache_dir, MAX_CACHE_ENTRIES);
    Some(target)
}

pub fn prune_cache_dir(cache_dir: &Path, max_entries: usize) {
    let Ok(rd) = fs::read_dir(cache_dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, PathBuf)> = rd
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str())?;
            if !matches!(ext, "jpg" | "png" | "webp") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((mtime, path))
        })
        .collect();
    if files.len() <= max_entries {
        return;
    }
    files.sort_by_key(|(mtime, _)| *mtime);
    let remove_count = files.len() - max_entries;
    for (_, path) in files.into_iter().take(remove_count) {
        let _ = fs::remove_file(path);
    }
}

fn resolve_artwork_path(
    agent: &ureq::Agent,
    cache_dir: &Path,
    key: &str,
    event: &MusicTrackEvent,
    generation: u64,
) -> Option<PathBuf> {
    if let Some(cached) = find_cached_artwork(cache_dir, key) {
        return Some(cached);
    }

    if let Some(ref url) = event.store_url
        && let Some(adam_id) = extract_adam_id(url)
        && let Some(art_url) = lookup_itunes_artwork_by_id(agent, &adam_id)
        && let Some(bytes) = download_image_bytes(agent, &art_url)
        && let Some(saved) = save_cached_artwork(cache_dir, key, &bytes, generation)
    {
        return Some(saved);
    }

    if let Some(bytes) = fetch_embedded_music_artwork(&event.name)
        && let Some(saved) = save_cached_artwork(cache_dir, key, &bytes, generation)
    {
        return Some(saved);
    }

    if let Some(art_url) = search_itunes_artwork(agent, event)
        && let Some(bytes) = download_image_bytes(agent, &art_url)
        && let Some(saved) = save_cached_artwork(cache_dir, key, &bytes, generation)
    {
        return Some(saved);
    }

    None
}

fn percent_encode_query(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

fn fetch_json(agent: &ureq::Agent, url: &str) -> Option<serde_json::Value> {
    let resp = agent.get(url).call().ok()?;
    let mut body = String::new();
    resp.into_reader()
        .take(2 * 1024 * 1024)
        .read_to_string(&mut body)
        .ok()?;
    serde_json::from_str(&body).ok()
}

fn download_image_bytes(agent: &ureq::Agent, url: &str) -> Option<Vec<u8>> {
    let resp = agent.get(url).call().ok()?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take((MAX_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut buf)
        .ok()?;
    detect_image_extension(&buf)?;
    Some(buf)
}

fn lookup_itunes_artwork_by_id(agent: &ureq::Agent, adam_id: &str) -> Option<String> {
    for country in ["jp", "us"] {
        let url = format!("https://itunes.apple.com/lookup?id={adam_id}&country={country}");
        let Some(json) = fetch_json(agent, &url) else {
            continue;
        };
        let Some(results) = json.get("results").and_then(|v| v.as_array()) else {
            continue;
        };
        for item in results {
            if let Some(art) = item.get("artworkUrl100").and_then(|v| v.as_str()) {
                return Some(upscale_itunes_artwork_url(art));
            }
        }
    }
    None
}

fn strip_track_decorations(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut depth = 0usize;
    for ch in title.chars() {
        match ch {
            '(' | '[' | '（' | '【' => depth += 1,
            ')' | ']' | '）' | '】' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out.split(" - ")
        .next()
        .unwrap_or(out.as_str())
        .trim()
        .to_string()
}

pub fn normalize_match_text(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn text_match_bonus(candidate: &str, target: &str, exact_pts: i32, contains_pts: i32) -> i32 {
    let c = normalize_match_text(candidate);
    let t = normalize_match_text(target);
    if c.is_empty() || t.is_empty() {
        return 0;
    }
    if c == t {
        exact_pts
    } else if c.contains(&t) || t.contains(&c) {
        contains_pts
    } else {
        0
    }
}

pub fn score_itunes_candidate(
    track_name: &str,
    artist_name: &str,
    collection_name: &str,
    event: &MusicTrackEvent,
) -> i32 {
    let album_score = text_match_bonus(collection_name, &event.album, 70, 55);
    let artist_score = text_match_bonus(artist_name, &event.artist, 55, 40);
    let track_score = text_match_bonus(track_name, &event.name, 45, 30);
    album_score + artist_score + track_score
}

fn search_itunes_artwork(agent: &ureq::Agent, event: &MusicTrackEvent) -> Option<String> {
    let mut terms = Vec::new();
    if !event.album.trim().is_empty() {
        terms.push(format!("{} {}", event.artist.trim(), event.album.trim()));
    }
    let clean_name = strip_track_decorations(&event.name);
    if !clean_name.is_empty() {
        let q = format!("{} {}", event.artist.trim(), clean_name);
        if !terms.contains(&q) {
            terms.push(q);
        }
    }
    if terms.is_empty() {
        return None;
    }

    let mut best: Option<(i32, String)> = None;
    for country in ["jp", "us"] {
        for term in &terms {
            let encoded = percent_encode_query(term.trim());
            let url = format!(
                "https://itunes.apple.com/search?term={encoded}&entity=song&limit=20&country={country}"
            );
            let Some(json) = fetch_json(agent, &url) else {
                continue;
            };
            let Some(results) = json.get("results").and_then(|v| v.as_array()) else {
                continue;
            };
            for item in results {
                let Some(art) = item.get("artworkUrl100").and_then(|v| v.as_str()) else {
                    continue;
                };
                let tn = item.get("trackName").and_then(|v| v.as_str()).unwrap_or("");
                let an = item
                    .get("artistName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let cn = item
                    .get("collectionName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let score = score_itunes_candidate(tn, an, cn, event);
                if score >= MIN_ITUNES_SEARCH_SCORE
                    && best.as_ref().is_none_or(|(best_s, _)| score > *best_s)
                {
                    best = Some((score, upscale_itunes_artwork_url(art)));
                }
            }
            if best.as_ref().is_some_and(|(s, _)| *s >= 100) {
                return best.map(|(_, url)| url);
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map(|(_, url)| url)
}

fn query_current_track_if_running() -> Option<MusicTrackEvent> {
    #[cfg(target_os = "macos")]
    {
        if !macos_bridge::is_music_app_running() {
            return None;
        }
        let script = r#"
const app = Application("Music");
const state = String(app.playerState());
if (state !== "playing") {
  JSON.stringify({ is_playing: false, name: "", artist: "", album: "", persistent_id: null });
} else {
  const t = app.currentTrack;
  JSON.stringify({
    is_playing: true,
    name: String(t.name() || ""),
    artist: String(t.artist() || ""),
    album: String(t.album() || ""),
    persistent_id: t.persistentID ? String(t.persistentID() || "") : null
  });
}
"#;
        let output = std::process::Command::new("osascript")
            .args(["-l", "JavaScript", "-e", script])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        parse_jxa_track_json(text.trim())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn parse_jxa_track_json(raw: &str) -> Option<MusicTrackEvent> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let is_playing = v
        .get("is_playing")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let artist = v
        .get("artist")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let album = v
        .get("album")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let persistent_id = v
        .get("persistent_id")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(MusicTrackEvent {
        is_playing,
        name,
        artist,
        album,
        store_url: None,
        persistent_id,
    })
}

fn fetch_embedded_music_artwork(expected_track_name: &str) -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        if !macos_bridge::is_music_app_running() {
            return None;
        }
        let tmp_path =
            std::env::temp_dir().join(format!("afterlife-music-art-{}.bin", std::process::id()));
        let _ = fs::remove_file(&tmp_path);
        let script = r#"
on run argv
    set expectedName to item 1 of argv
    set outPath to item 2 of argv
    tell application "Music"
        if player state is not playing then return "none"
        set t to current track
        if expectedName is not "" and (name of t as string) is not expectedName then return "mismatch"
        if (count of artworks of t) < 1 then return "none"
        set rawBytes to raw data of artwork 1 of t
    end tell
    set f to open for access (POSIX file outPath) with write permission
    set eof f to 0
    write rawBytes to f
    close access f
    return "ok"
end run
"#;
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                script,
                expected_track_name,
                tmp_path.to_string_lossy().as_ref(),
            ])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let bytes = if output.status.success() && stdout.trim() == "ok" {
            fs::read(&tmp_path).ok()
        } else {
            None
        };
        let _ = fs::remove_file(&tmp_path);
        bytes.filter(|b| detect_image_extension(b).is_some())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = expected_track_name;
        None
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod macos_bridge {
    use std::{ffi::CStr, os::raw::c_char, sync::Once};

    use cocoa::{
        base::{BOOL, NO, id, nil},
        foundation::{NSAutoreleasePool, NSDictionary, NSString},
    };
    use objc::{
        class,
        declare::ClassDecl,
        msg_send,
        runtime::{Class, Object, Sel},
        sel, sel_impl,
    };

    use super::{AppleMusicWatcher, MusicTrackEvent, handle_notification_event};

    const OBSERVER_CLASS_NAME: &str = "AfterlifeMusicNotificationObserver";
    const MUSIC_NOTIFICATION_NAME: &str = "com.apple.Music.playerInfo";
    const MUSIC_BUNDLE_ID: &str = "com.apple.Music";

    static REGISTER_CLASS: Once = Once::new();

    fn observer_class() -> &'static Class {
        REGISTER_CLASS.call_once(|| {
            let superclass = class!(NSObject);
            if let Some(mut decl) = ClassDecl::new(OBSERVER_CLASS_NAME, superclass) {
                unsafe {
                    decl.add_method(
                        sel!(onMusicPlayerInfo:),
                        on_music_player_info as extern "C" fn(&Object, Sel, id),
                    );
                }
                decl.register();
            }
        });
        Class::get(OBSERVER_CLASS_NAME).expect("registered observer class")
    }

    unsafe fn ns_dict_string(dict: id, key: &str) -> Option<String> {
        if dict == nil {
            return None;
        }
        let ns_key: id = unsafe { NSString::alloc(nil).init_str(key).autorelease() };
        let val: id = unsafe { dict.objectForKey_(ns_key) };
        if val == nil {
            return None;
        }
        let is_str: BOOL = unsafe { msg_send![val, isKindOfClass: class!(NSString)] };
        let str_obj: id = if is_str != NO {
            val
        } else {
            let responds: BOOL = unsafe { msg_send![val, respondsToSelector: sel!(stringValue)] };
            if responds != NO {
                unsafe { msg_send![val, stringValue] }
            } else {
                nil
            }
        };
        if str_obj == nil {
            return None;
        }
        let ptr: *const c_char = unsafe { msg_send![str_obj, UTF8String] };
        if ptr.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .ok()
            .map(|s| s.to_string())
    }

    extern "C" fn on_music_player_info(_this: &Object, _cmd: Sel, notification: id) {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            if notification != nil {
                let user_info: id = msg_send![notification, userInfo];
                if user_info != nil {
                    let player_state =
                        ns_dict_string(user_info, "Player State").unwrap_or_default();
                    let is_playing = player_state.eq_ignore_ascii_case("Playing");
                    let event = MusicTrackEvent {
                        is_playing,
                        name: ns_dict_string(user_info, "Name").unwrap_or_default(),
                        artist: ns_dict_string(user_info, "Artist").unwrap_or_default(),
                        album: ns_dict_string(user_info, "Album").unwrap_or_default(),
                        store_url: ns_dict_string(user_info, "Store URL")
                            .filter(|s| !s.trim().is_empty()),
                        persistent_id: ns_dict_string(user_info, "PersistentID")
                            .filter(|s| !s.trim().is_empty()),
                    };
                    handle_notification_event(event);
                }
            }
            pool.drain();
        }
    }

    pub fn is_music_app_running() -> bool {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let bundle_id: id = NSString::alloc(nil).init_str(MUSIC_BUNDLE_ID).autorelease();
            let apps: id = msg_send![
                class!(NSRunningApplication),
                runningApplicationsWithBundleIdentifier: bundle_id
            ];
            let count: usize = if apps != nil {
                msg_send![apps, count]
            } else {
                0
            };
            pool.drain();
            count > 0
        }
    }

    pub fn register_observer(w: &AppleMusicWatcher) {
        let mut guard = w.observer_ptr.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_some() {
            return;
        }
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let cls = observer_class();
            let observer: id = msg_send![cls, new];
            if observer != nil {
                let center: id = msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
                let name: id = NSString::alloc(nil)
                    .init_str(MUSIC_NOTIFICATION_NAME)
                    .autorelease();
                let _: () = msg_send![
                    center,
                    addObserver: observer
                    selector: sel!(onMusicPlayerInfo:)
                    name: name
                    object: nil
                ];
                *guard = Some(observer as usize);
            }
            pool.drain();
        }
    }

    pub fn unregister_observer(w: &AppleMusicWatcher) {
        let mut guard = w.observer_ptr.lock().unwrap_or_else(|e| e.into_inner());
        let Some(ptr) = guard.take() else {
            return;
        };
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let observer = ptr as id;
            let center: id = msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
            let _: () = msg_send![center, removeObserver: observer];
            let _: () = msg_send![observer, release];
            pool.drain();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_cache_key_matches_between_jxa_initial_sync_and_distributed_notification() {
        let from_jxa = MusicTrackEvent {
            is_playing: true,
            name: "夜に駆ける".into(),
            artist: "YOASOBI".into(),
            album: "THE BOOK".into(),
            store_url: None,
            persistent_id: Some("A1B2C3D4E5F60718".into()),
        };
        let from_notification = MusicTrackEvent {
            is_playing: true,
            name: "夜に駆ける".into(),
            artist: "YOASOBI".into(),
            album: "THE BOOK".into(),
            store_url: Some("itms://itunes.com/album?p=1544545937&i=1544545938".into()),
            persistent_id: Some("-6795154822414858472".into()),
        };
        assert_eq!(
            track_cache_key(&from_jxa),
            track_cache_key(&from_notification)
        );
    }

    #[test]
    fn extract_adam_id_handles_itms_and_web_urls() {
        assert_eq!(
            extract_adam_id("itms://itunes.com/album?p=1440857781&i=1440857795"),
            Some("1440857795".to_string())
        );
        assert_eq!(
            extract_adam_id("https://music.apple.com/jp/album/example/1677006637?i=1677006641"),
            Some("1677006641".to_string())
        );
        assert_eq!(
            extract_adam_id("https://music.apple.com/jp/album/example/1677006637"),
            Some("1677006637".to_string())
        );
        assert_eq!(extract_adam_id("not-a-url"), None);
    }

    #[test]
    fn upscale_itunes_artwork_replaces_resolution_suffix() {
        assert_eq!(
            upscale_itunes_artwork_url(
                "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/aa/bb/cc/100x100bb.jpg"
            ),
            "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/aa/bb/cc/1200x1200bb.jpg"
        );
    }

    #[test]
    fn scoring_prefers_matching_album_and_artist_over_unrelated_song() {
        let event = MusicTrackEvent {
            is_playing: true,
            name: "夜に駆ける".into(),
            artist: "YOASOBI".into(),
            album: "THE BOOK".into(),
            store_url: None,
            persistent_id: None,
        };
        let exact = score_itunes_candidate("夜に駆ける", "YOASOBI", "THE BOOK", &event);
        let wrong_album =
            score_itunes_candidate("夜に駆ける", "YOASOBI", "Other Compilation", &event);
        let unrelated =
            score_itunes_candidate("Another Song", "Other Artist", "Other Album", &event);
        assert!(exact > wrong_album);
        assert!(exact >= 150);
        assert!(unrelated < MIN_ITUNES_SEARCH_SCORE);
    }

    #[test]
    fn parse_jxa_track_json_reads_playing_and_paused_states() {
        let playing = parse_jxa_track_json(
            r#"{"is_playing":true,"name":"Song A","artist":"Artist B","album":"Album C","persistent_id":"ABCD1234"}"#,
        )
        .expect("playing json");
        assert!(playing.is_playing);
        assert_eq!(playing.name, "Song A");
        assert_eq!(playing.persistent_id.as_deref(), Some("ABCD1234"));

        let paused = parse_jxa_track_json(
            r#"{"is_playing":false,"name":"","artist":"","album":"","persistent_id":null}"#,
        )
        .expect("paused json");
        assert!(!paused.is_playing);
        assert!(paused.persistent_id.is_none());
    }

    #[test]
    fn detect_image_extension_validates_magic_bytes_and_size() {
        let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        jpg.resize(128, 0);
        assert_eq!(detect_image_extension(&jpg), Some("jpg"));

        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.resize(128, 0);
        assert_eq!(detect_image_extension(&png), Some("png"));

        let mut webp = b"RIFF1234WEBP".to_vec();
        webp.resize(128, 0);
        assert_eq!(detect_image_extension(&webp), Some("webp"));

        assert_eq!(detect_image_extension(&[0xFF, 0xD8, 0xFF]), None);
        assert_eq!(detect_image_extension(&[0u8; 128]), None);
    }

    #[test]
    fn prune_cache_dir_removes_oldest_entries_beyond_limit() {
        let dir =
            std::env::temp_dir().join(format!("afterlife-music-cache-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for idx in 0..5 {
            let p = dir.join(format!("item_{idx}.jpg"));
            fs::write(&p, vec![0xFFu8; 128]).unwrap();
            std::thread::sleep(Duration::from_millis(12));
        }
        prune_cache_dir(&dir, 3);
        let remaining: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(remaining.len(), 3);
        assert!(remaining.contains(&"item_4.jpg".to_string()));
        assert!(remaining.contains(&"item_3.jpg".to_string()));
        assert!(remaining.contains(&"item_2.jpg".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }
}
