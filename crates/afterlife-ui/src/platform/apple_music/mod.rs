mod itunes;
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod macos_bridge;
#[cfg(test)]
mod tests;

use itunes::{
    download_image_bytes, fetch_embedded_music_artwork, lookup_itunes_artwork_by_id,
    normalize_match_text, query_current_track_if_running, search_itunes_artwork,
};

use std::{
    fs,
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

pub(super) struct AppleMusicWatcher {
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
pub(super) fn handle_notification_event(event: MusicTrackEvent) {
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

