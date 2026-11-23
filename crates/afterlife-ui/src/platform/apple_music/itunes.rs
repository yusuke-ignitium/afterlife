use std::{fs, io::Read};

#[cfg(target_os = "macos")]
use super::macos_bridge;
use super::{
    MAX_IMAGE_BYTES, MIN_ITUNES_SEARCH_SCORE, MusicTrackEvent, detect_image_extension,
    upscale_itunes_artwork_url,
};

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

pub(super) fn download_image_bytes(agent: &ureq::Agent, url: &str) -> Option<Vec<u8>> {
    let resp = agent.get(url).call().ok()?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take((MAX_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut buf)
        .ok()?;
    detect_image_extension(&buf)?;
    Some(buf)
}

pub(super) fn lookup_itunes_artwork_by_id(agent: &ureq::Agent, adam_id: &str) -> Option<String> {
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

pub(super) fn search_itunes_artwork(agent: &ureq::Agent, event: &MusicTrackEvent) -> Option<String> {
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

pub(super) fn query_current_track_if_running() -> Option<MusicTrackEvent> {
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

pub(super) fn fetch_embedded_music_artwork(expected_track_name: &str) -> Option<Vec<u8>> {
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

