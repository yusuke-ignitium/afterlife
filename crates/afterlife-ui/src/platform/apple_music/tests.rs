use super::itunes::{parse_jxa_track_json, score_itunes_candidate};
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
