use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

use super::{
    BackdropConfig, BackdropGlassAppearance, BackdropStore, BackdropTuning, BackdropTuningPatch,
    DEFAULT_IMAGE_HOLD_SECS, DEFAULT_IMAGE_SATURATION, DEFAULT_TILE_SIZE_PX,
    resolve_tile_size_for_display,
    store::TMP_FILE_SEQ,
};

const TINY_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
    0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
    31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn make_temp_dir(tag: &str) -> PathBuf {
    let seq = TMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "afterlife_backdrop_ut_{tag}_{}_{seq}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn clamped_sanitizes_nan_infinity_and_out_of_range_values() {
    let tuning = BackdropTuning {
        image_enabled: true,
        apple_music_artwork_enabled: false,
        youtube_mirror_enabled: false,
        tile_size_px: f32::NAN,
        image_hold_secs: f32::INFINITY,
        image_brightness: -5.0,
        image_peak_luminance: 99.0,
        image_saturation: f32::NEG_INFINITY,
        master_brightness: 0.0,
    }
    .clamped();
    assert!(tuning.image_enabled);
    assert!(!tuning.apple_music_artwork_enabled);
    assert!(!tuning.youtube_mirror_enabled);
    assert_eq!(tuning.tile_size_px, DEFAULT_TILE_SIZE_PX);
    assert_eq!(tuning.image_hold_secs, DEFAULT_IMAGE_HOLD_SECS);
    assert_eq!(tuning.image_brightness, 0.0);
    assert_eq!(tuning.image_peak_luminance, 1.0);
    assert_eq!(tuning.image_saturation, DEFAULT_IMAGE_SATURATION);
    assert_eq!(tuning.master_brightness, 0.2);

    let glass = BackdropGlassAppearance {
        disp_scale: f32::NAN,
        rim_thickness: 0.0,
        refract_reach: 9999.0,
        chroma: -1.0,
        highlight_alpha: 2.5,
        rim_width: f32::INFINITY,
        blur_std_dev: 500.0,
        bg_alpha: 1.0,
        radius: -3.0,
        tint_rgb: [12, 34, 56],
        pause_animation_when_inactive: true,
        fade_ui_after_secs: 999.0,
        fade_ui_transparency_pct: f32::NAN,
        approval_notifications_enabled: false,
        compact_districts: true,
    }
    .clamped();
    let def = BackdropGlassAppearance::default();
    assert_eq!(glass.disp_scale, def.disp_scale);
    assert_eq!(glass.rim_thickness, 0.01);
    assert_eq!(glass.refract_reach, 600.0);
    assert_eq!(glass.chroma, 0.0);
    assert_eq!(glass.highlight_alpha, 1.0);
    assert_eq!(glass.rim_width, def.rim_width);
    assert_eq!(glass.blur_std_dev, 20.0);
    assert_eq!(glass.bg_alpha, 0.85);
    assert_eq!(glass.radius, 0.0);
    assert_eq!(glass.tint_rgb, [12, 34, 56]);
    assert!(glass.pause_animation_when_inactive);
    assert_eq!(glass.fade_ui_after_secs, 120.0);
    assert!(glass.compact_districts);
}

#[test]
fn partial_json_deserializes_with_defaults_and_clamping() {
    let dir = make_temp_dir("partial");
    let store = BackdropStore::open(&dir);
    fs::write(
        store.config_path(),
        r#"{"tuning":{"tile_size_px":999.0},"glass":{"blur_std_dev":500.0,"tint_rgb":[1,2,3]}}"#,
    )
    .unwrap();

    let cfg = store.load().unwrap();
    assert!(!cfg.tuning.image_enabled);
    assert!(cfg.tuning.apple_music_artwork_enabled);
    assert!(cfg.tuning.youtube_mirror_enabled);
    assert_eq!(cfg.tuning.tile_size_px, 24.0);
    assert_eq!(cfg.tuning.image_hold_secs, DEFAULT_IMAGE_HOLD_SECS);
    let g = cfg.glass.expect("glass section should deserialize");
    assert_eq!(g.blur_std_dev, 20.0);
    assert_eq!(g.tint_rgb, [1, 2, 3]);
    assert_eq!(g.disp_scale, BackdropGlassAppearance::default().disp_scale);
    assert_eq!(g.fade_ui_after_secs, 0.0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rejects_non_image_files_without_leaking_into_backdrops_dir() {
    let dir = make_temp_dir("non_image");
    let store = BackdropStore::open(&dir);
    let fake = dir.join("notes.txt");
    fs::write(&fake, "not a valid image payload").unwrap();

    let err = store.add_image(&fake, Some("bad")).unwrap_err();
    assert!(
        err.contains("unsupported or unreadable image file"),
        "{err}"
    );
    let entries: Vec<_> = fs::read_dir(store.images_dir())
        .map(|rd| rd.filter_map(Result::ok).collect())
        .unwrap_or_default();
    assert!(entries.is_empty(), "backdrops dir must remain empty");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn appearance_and_cli_mutations_preserve_each_others_fields() {
    let dir = make_temp_dir("preserve");
    let store = BackdropStore::open(&dir);
    let img = dir.join("sample.png");
    fs::write(&img, TINY_PNG).unwrap();

    let custom_glass = BackdropGlassAppearance {
        blur_std_dev: 6.5,
        radius: 10.0,
        tint_rgb: [27, 45, 56],
        pause_animation_when_inactive: true,
        ..BackdropGlassAppearance::default()
    };
    store
        .save_appearance(
            BackdropTuning {
                image_enabled: true,
                tile_size_px: 8.0,
                ..BackdropTuning::default()
            },
            custom_glass,
        )
        .unwrap();

    let (added, after_add) = store.add_image(&img, Some("hero")).unwrap();
    assert_eq!(after_add.glass, Some(custom_glass));
    assert_eq!(after_add.images.len(), 1);

    let after_tune = store
        .update_tuning(BackdropTuningPatch {
            image_hold_secs: Some(12.0),
            ..BackdropTuningPatch::default()
        })
        .unwrap();
    assert_eq!(after_tune.glass, Some(custom_glass));
    assert_eq!(after_tune.images.len(), 1);

    let after_save = store
        .save_appearance(
            after_tune.tuning,
            BackdropGlassAppearance {
                radius: 12.0,
                ..custom_glass
            },
        )
        .unwrap();
    assert_eq!(after_save.images.len(), 1);
    assert_eq!(after_save.images[0].id, added.id);

    let (_, after_remove) = store.remove_image(&added.id).unwrap();
    assert_eq!(after_remove.glass.unwrap().radius, 12.0);
    assert!(after_remove.images.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn concurrent_saves_use_unique_temp_files_without_corruption() {
    let dir = make_temp_dir("concurrent");
    let store = Arc::new(BackdropStore::open(&dir));
    let mut handles = Vec::new();
    for i in 0..8u8 {
        let s = store.clone();
        handles.push(std::thread::spawn(move || {
            for step in 0..15u8 {
                let cfg = BackdropConfig {
                    images: Vec::new(),
                    tuning: BackdropTuning {
                        tile_size_px: 3.0 + ((i + step) % 10) as f32,
                        ..BackdropTuning::default()
                    },
                    glass: Some(BackdropGlassAppearance {
                        radius: 4.0 + (step % 8) as f32,
                        ..BackdropGlassAppearance::default()
                    }),
                    display_tile_sizes: BTreeMap::new(),
                };
                s.save(&cfg).unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let loaded = store.load().unwrap();
    assert!(loaded.glass.is_some());
    let leftover_tmps: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(leftover_tmps.is_empty(), "no temp files should remain");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn resolve_tile_size_scales_with_backing_factor_and_prefers_per_display_override() {
    let mut overrides = BTreeMap::new();
    let res = |base, id, scale, ov: &BTreeMap<String, f32>| {
        resolve_tile_size_for_display(base, Some(id), scale, ov)
    };
    assert_eq!(res(3.0, "retina-uuid", 2.0, &overrides), 3.0);
    assert_eq!(res(3.0, "lg-tv-uuid", 1.0, &overrides), 2.0);
    assert_eq!(res(6.0, "fhd-24-uuid", 1.0, &overrides), 3.0);

    overrides.insert("lg-tv-uuid".to_string(), 3.0289884);
    assert_eq!(res(3.0, "lg-tv-uuid", 1.0, &overrides), 3.0);
    assert_eq!(res(5.0, "fhd-24-uuid", 1.0, &overrides), 3.0);
    assert_eq!(res(3.0, "retina-uuid", 2.0, &overrides), 3.0);
}

#[test]
fn display_tile_sizes_sanitize_and_round_trip_without_breaking_legacy_json() {
    let dir = make_temp_dir("display-tiles");
    let store = BackdropStore::open(&dir);

    let mut map = BTreeMap::new();
    map.insert("retina-1".to_string(), 3.0);
    map.insert("ext-1".to_string(), 1.0);
    map.insert("ext-huge".to_string(), 99.0);
    map.insert("   ".to_string(), 4.0);
    map.insert("bad-nan".to_string(), f32::NAN);

    let saved = store
        .save_appearance_with_displays(
            BackdropTuning {
                tile_size_px: 3.0,
                ..BackdropTuning::default()
            },
            BackdropGlassAppearance::default(),
            map,
        )
        .unwrap();
    assert_eq!(saved.display_tile_sizes.len(), 3);
    assert_eq!(saved.display_tile_sizes.get("retina-1"), Some(&3.0));
    assert_eq!(saved.display_tile_sizes.get("ext-1"), Some(&2.0));
    assert_eq!(saved.display_tile_sizes.get("ext-huge"), Some(&24.0));

    let reloaded = store.load().unwrap();
    assert_eq!(reloaded.display_tile_sizes, saved.display_tile_sizes);

    let after_cli_reset = store
        .update_tuning(BackdropTuningPatch {
            reset: true,
            ..BackdropTuningPatch::default()
        })
        .unwrap();
    assert!(after_cli_reset.display_tile_sizes.is_empty());

    let _ = fs::remove_dir_all(&dir);
}
