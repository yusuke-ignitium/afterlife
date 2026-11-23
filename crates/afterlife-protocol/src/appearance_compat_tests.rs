use std::fs;

use crate::{BackdropConfig, BackdropGlassAppearance, BackdropStore};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "afterlife_appearance_compat_{tag}_{}_{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn legacy_backdrop_json_without_notification_flag_loads_cleanly() {
    let dir = temp_dir("legacy_notif");
    let legacy_json = serde_json::json!({
        "images": [],
        "tuning": {
            "image_enabled": false,
            "apple_music_artwork_enabled": true,
            "youtube_mirror_enabled": true,
            "tile_size_px": 6.0,
            "image_hold_secs": 10.0,
            "image_brightness": 0.4,
            "image_peak_luminance": 0.55,
            "image_saturation": 0.64,
            "master_brightness": 1.5
        },
        "glass": {
            "disp_scale": 193.0,
            "rim_thickness": 0.06,
            "refract_reach": 50.0,
            "chroma": 0.0,
            "highlight_alpha": 0.24,
            "rim_width": 1.3,
            "blur_std_dev": 0.0,
            "bg_alpha": 0.04,
            "radius": 6.6,
            "tint_rgb": [7, 9, 14],
            "pause_animation_when_inactive": true,
            "fade_ui_after_secs": 12.0
        }
    });
    fs::write(
        dir.join("backdrop.json"),
        serde_json::to_string_pretty(&legacy_json).unwrap(),
    )
    .unwrap();

    let store = BackdropStore::open(&dir);
    let loaded: BackdropConfig = store.load().unwrap();
    let glass = loaded.glass.expect("glass section should load");
    assert!(glass.pause_animation_when_inactive);
    assert_eq!(glass.fade_ui_after_secs, 12.0);
    assert_eq!(glass.fade_ui_transparency_pct, 76.0);
    assert!(glass.approval_notifications_enabled);
    assert_eq!(glass.disp_scale, BackdropGlassAppearance::default().disp_scale);

    let mut updated_glass = glass;
    updated_glass.approval_notifications_enabled = false;
    updated_glass.fade_ui_transparency_pct = 42.6;
    store
        .save_appearance_with_displays(loaded.tuning, updated_glass, std::collections::BTreeMap::new())
        .unwrap();
    let reloaded = store.load().unwrap();
    let reloaded_glass = reloaded.glass.unwrap();
    assert!(!reloaded_glass.approval_notifications_enabled);
    assert_eq!(reloaded_glass.fade_ui_transparency_pct, 43.0);

    let clamped_hi = BackdropGlassAppearance {
        fade_ui_transparency_pct: 120.0,
        ..BackdropGlassAppearance::default()
    }
    .clamped();
    assert_eq!(clamped_hi.fade_ui_transparency_pct, 90.0);
    let clamped_lo = BackdropGlassAppearance {
        fade_ui_transparency_pct: -10.0,
        ..BackdropGlassAppearance::default()
    }
    .clamped();
    assert_eq!(clamped_lo.fade_ui_transparency_pct, 0.0);
    let clamped_nan = BackdropGlassAppearance {
        fade_ui_transparency_pct: f32::NAN,
        ..BackdropGlassAppearance::default()
    }
    .clamped();
    assert_eq!(clamped_nan.fade_ui_transparency_pct, 76.0);

    let _ = fs::remove_dir_all(&dir);
}
