use std::{
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::Value;

static DIR_SEQ: AtomicU64 = AtomicU64::new(1);

fn run_cli_with_data_dir(data_dir: &std::path::Path, args: &[&str]) -> (bool, Value) {
    let bin = env!("CARGO_BIN_EXE_afterlife");
    let out = Command::new(bin)
        .env("AFTERLIFE_DATA_DIR", data_dir)
        .args(args)
        .output()
        .expect("spawn afterlife CLI");
    let stream = if out.status.success() {
        &out.stdout
    } else {
        &out.stderr
    };
    let text = String::from_utf8_lossy(stream);
    let parsed: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("CLI output for {args:?} not JSON ({e}): {text}"));
    (out.status.success(), parsed)
}

#[test]
fn e2e_cli_backdrop_crud_and_tuning() {
    let seq = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_dir = std::env::temp_dir().join(format!(
        "afterlife_backdrop_cli_{}_{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let tiny_png: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
        31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let img_a = tmp_dir.join("alpha.png");
    let img_b = tmp_dir.join("beta.png");
    let img_c = tmp_dir.join("gamma.png");
    let fake_txt = tmp_dir.join("notes.txt");
    std::fs::write(&img_a, tiny_png).unwrap();
    std::fs::write(&img_b, tiny_png).unwrap();
    std::fs::write(&img_c, tiny_png).unwrap();
    std::fs::write(&fake_txt, "definitely not an image").unwrap();

    let (ok, list0) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "list"]);
    assert!(ok, "backdrop list failed: {list0}");
    assert_eq!(list0["count"], 0);
    assert_eq!(list0["tuning"]["image_enabled"], false);
    assert_eq!(list0["tuning"]["image_shuffle"], true);
    assert_eq!(list0["tuning"]["apple_music_artwork_enabled"], true);
    assert!((list0["tuning"]["tile_size_px"].as_f64().unwrap() - 6.0).abs() < 1e-4);
    assert!((list0["tuning"]["image_hold_secs"].as_f64().unwrap() - 10.0).abs() < 1e-4);
    assert!((list0["tuning"]["image_brightness"].as_f64().unwrap() - 0.40).abs() < 1e-4);
    assert!((list0["tuning"]["image_peak_luminance"].as_f64().unwrap() - 0.55).abs() < 1e-4);
    assert!((list0["tuning"]["image_saturation"].as_f64().unwrap() - 0.64).abs() < 1e-4);
    assert!((list0["tuning"]["master_brightness"].as_f64().unwrap() - 1.50).abs() < 1e-4);

    let (ok, rejected) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "add",
            fake_txt.to_str().unwrap(),
            "--label",
            "bad",
        ],
    );
    assert!(!ok, "non-image file must be rejected: {rejected}");

    let store = afterlife_protocol::BackdropStore::open(&tmp_dir);
    let saved_glass = afterlife_protocol::BackdropGlassAppearance {
        blur_std_dev: 5.5,
        radius: 9.0,
        tint_rgb: [27, 45, 56],
        pause_animation_when_inactive: true,
        ..afterlife_protocol::BackdropGlassAppearance::default()
    };
    store
        .save_appearance(afterlife_protocol::BackdropTuning::default(), saved_glass)
        .unwrap();

    let (ok, added1) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "add",
            img_a.to_str().unwrap(),
            "--label",
            "first",
        ],
    );
    assert!(ok, "backdrop add 1 failed: {added1}");
    assert_eq!(added1["added"]["id"], "img-1");
    assert_eq!(added1["added"]["label"], "first");
    assert_eq!(added1["count"], 1);
    assert!(std::path::Path::new(added1["added"]["path"].as_str().unwrap()).is_file());
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, added2) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "add",
            img_b.to_str().unwrap(),
            "--label",
            "second",
        ],
    );
    assert!(ok, "backdrop add 2 failed: {added2}");
    assert_eq!(added2["added"]["id"], "img-2");
    assert_eq!(added2["count"], 2);

    let (ok, noop_update) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "update", "img-2"]);
    assert!(
        !ok,
        "update with no mutation flags must fail: {noop_update}"
    );

    let (ok, missing_update) = run_cli_with_data_dir(
        &tmp_dir,
        &["backdrop", "update", "img-999", "--label", "ghost"],
    );
    assert!(!ok, "update on unknown id must fail: {missing_update}");

    let (ok, updated) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "update",
            "img-2",
            "--path",
            img_c.to_str().unwrap(),
            "--label",
            "promoted",
            "--index",
            "0",
        ],
    );
    assert!(ok, "backdrop update failed: {updated}");
    assert_eq!(updated["updated"]["label"], "promoted");
    assert_eq!(updated["images"][0]["id"], "img-2");
    assert_eq!(updated["images"][1]["id"], "img-1");
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, tuned) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "tuning",
            "--enable-images",
            "--tile-size",
            "10.5",
            "--hold-secs",
            "15.0",
            "--image-brightness",
            "0.65",
            "--image-peak",
            "0.70",
            "--image-saturation",
            "0.80",
            "--master-brightness",
            "1.40",
        ],
    );
    assert!(ok, "backdrop tuning failed: {tuned}");
    assert_eq!(tuned["tuning"]["image_enabled"], true);
    assert!((tuned["tuning"]["tile_size_px"].as_f64().unwrap() - 11.0).abs() < 1e-4);
    assert!((tuned["tuning"]["image_hold_secs"].as_f64().unwrap() - 15.0).abs() < 1e-4);
    assert!((tuned["tuning"]["image_brightness"].as_f64().unwrap() - 0.65).abs() < 1e-4);
    assert!((tuned["tuning"]["image_peak_luminance"].as_f64().unwrap() - 0.70).abs() < 1e-4);
    assert!((tuned["tuning"]["image_saturation"].as_f64().unwrap() - 0.80).abs() < 1e-4);
    assert!((tuned["tuning"]["master_brightness"].as_f64().unwrap() - 1.40).abs() < 1e-4);
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, conflict) = run_cli_with_data_dir(
        &tmp_dir,
        &["backdrop", "tuning", "--enable-images", "--disable-images"],
    );
    assert!(
        !ok,
        "conflicting enable/disable flags should fail: {conflict}"
    );

    let (ok, disabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-images"]);
    assert!(ok, "backdrop tuning disable failed: {disabled}");
    assert_eq!(disabled["tuning"]["image_enabled"], false);

    let (ok, reenabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--enable-images"]);
    assert!(ok, "backdrop tuning re-enable failed: {reenabled}");
    assert_eq!(reenabled["tuning"]["image_enabled"], true);

    let (ok, sh_conflict) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "tuning",
            "--enable-shuffle",
            "--disable-shuffle",
        ],
    );
    assert!(
        !ok,
        "conflicting shuffle enable/disable flags should fail: {sh_conflict}"
    );

    let (ok, sh_disabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-shuffle"]);
    assert!(ok, "backdrop tuning disable shuffle failed: {sh_disabled}");
    assert_eq!(sh_disabled["tuning"]["image_shuffle"], false);

    let (ok, sh_reenabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--enable-shuffle"]);
    assert!(ok, "backdrop tuning enable shuffle failed: {sh_reenabled}");
    assert_eq!(sh_reenabled["tuning"]["image_shuffle"], true);

    let (ok, sh_disabled_before_reset) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-shuffle"]);
    assert!(
        ok,
        "backdrop tuning disable shuffle before reset failed: {sh_disabled_before_reset}"
    );

    let (ok, am_conflict) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "tuning",
            "--enable-apple-music",
            "--disable-apple-music",
        ],
    );
    assert!(
        !ok,
        "conflicting apple music enable/disable flags should fail: {am_conflict}"
    );

    let (ok, am_disabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-apple-music"]);
    assert!(
        ok,
        "backdrop tuning disable apple music failed: {am_disabled}"
    );
    assert_eq!(am_disabled["tuning"]["apple_music_artwork_enabled"], false);

    let (ok, am_reenabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--enable-apple-music"]);
    assert!(
        ok,
        "backdrop tuning enable apple music failed: {am_reenabled}"
    );
    assert_eq!(am_reenabled["tuning"]["apple_music_artwork_enabled"], true);

    let (ok, am_disabled_before_reset) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-apple-music"]);
    assert!(
        ok,
        "backdrop tuning disable before reset failed: {am_disabled_before_reset}"
    );

    let (ok, yt_conflict) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "tuning",
            "--enable-youtube-mirror",
            "--disable-youtube-mirror",
        ],
    );
    assert!(
        !ok,
        "conflicting youtube mirror enable/disable flags should fail: {yt_conflict}"
    );

    let (ok, yt_disabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-youtube-mirror"]);
    assert!(
        ok,
        "backdrop tuning disable youtube mirror failed: {yt_disabled}"
    );
    assert_eq!(yt_disabled["tuning"]["youtube_mirror_enabled"], false);

    let (ok, yt_reenabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--enable-youtube-mirror"]);
    assert!(
        ok,
        "backdrop tuning enable youtube mirror failed: {yt_reenabled}"
    );
    assert_eq!(yt_reenabled["tuning"]["youtube_mirror_enabled"], true);

    let (ok, yt_disabled_before_reset) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-youtube-mirror"]);
    assert!(
        ok,
        "backdrop tuning disable youtube before reset failed: {yt_disabled_before_reset}"
    );

    let (ok, reset_tuned) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--reset"]);
    assert!(ok, "backdrop tuning reset failed: {reset_tuned}");
    assert_eq!(reset_tuned["tuning"]["image_enabled"], false);
    assert_eq!(reset_tuned["tuning"]["image_shuffle"], true);
    assert_eq!(reset_tuned["tuning"]["apple_music_artwork_enabled"], true);
    assert_eq!(reset_tuned["tuning"]["youtube_mirror_enabled"], true);
    assert!((reset_tuned["tuning"]["tile_size_px"].as_f64().unwrap() - 6.0).abs() < 1e-4);
    assert!((reset_tuned["tuning"]["image_hold_secs"].as_f64().unwrap() - 10.0).abs() < 1e-4);
    assert!((reset_tuned["tuning"]["image_brightness"].as_f64().unwrap() - 0.40).abs() < 1e-4);
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, missing_remove) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "remove", "img-999"]);
    assert!(!ok, "remove on unknown id must fail: {missing_remove}");

    let removed_file = added1["added"]["path"].as_str().unwrap().to_string();
    let (ok, removed) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "remove", "img-1"]);
    assert!(ok, "backdrop remove failed: {removed}");
    assert_eq!(removed["count"], 1);
    assert!(!std::path::Path::new(&removed_file).exists());
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, cleared) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "clear"]);
    assert!(ok, "backdrop clear failed: {cleared}");
    assert_eq!(cleared["removed_count"], 1);
    assert_eq!(cleared["count"], 0);
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}
