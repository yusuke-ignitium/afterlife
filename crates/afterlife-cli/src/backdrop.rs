use std::path::Path;

use afterlife_protocol::{BackdropStore, BackdropTuningPatch};
use serde_json::{Map, Value, json};

use crate::transport::CliError;

fn parse_toggle_pair(
    payload: &Map<String, Value>,
    enable_key: &str,
    disable_key: &str,
    enable_flag: &str,
    disable_flag: &str,
) -> Result<Option<bool>, CliError> {
    let enable = payload
        .get(enable_key)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let disable = payload
        .get(disable_key)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if enable && disable {
        return Err(CliError::usage(format!(
            "cannot pass both {enable_flag} and {disable_flag}"
        )));
    }
    Ok(if enable {
        Some(true)
    } else if disable {
        Some(false)
    } else {
        None
    })
}

fn opt_f32(payload: &Map<String, Value>, key: &str) -> Option<f32> {
    payload.get(key).and_then(Value::as_f64).map(|v| v as f32)
}

pub fn run_backdrop_command(sub: &str, payload: &Map<String, Value>) -> Result<Value, CliError> {
    let store = BackdropStore::open_default();
    let config_path = store.config_path().to_string_lossy().into_owned();
    match sub {
        "list" => {
            let cfg = store.load().map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "count": cfg.images.len(),
                "images": cfg.images,
                "tuning": cfg.tuning,
            }))
        }
        "add" => {
            let path = payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::usage("missing <path>"))?;
            let label = payload.get("label").and_then(Value::as_str);
            let (entry, cfg) = store
                .add_image(Path::new(path), label)
                .map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "added": entry,
                "count": cfg.images.len(),
                "images": cfg.images,
            }))
        }
        "update" => {
            let id = payload
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::usage("missing <id>"))?;
            let new_path = payload.get("path").and_then(Value::as_str).map(Path::new);
            let new_label = payload.get("label").and_then(Value::as_str);
            let new_index = payload
                .get("index")
                .and_then(Value::as_u64)
                .map(|n| n as usize);
            let (entry, cfg) = store
                .update_image(id, new_path, new_label, new_index)
                .map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "updated": entry,
                "count": cfg.images.len(),
                "images": cfg.images,
            }))
        }
        "remove" => {
            let id = payload
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::usage("missing <id>"))?;
            let (removed, cfg) = store.remove_image(id).map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "removed": removed,
                "count": cfg.images.len(),
                "images": cfg.images,
            }))
        }
        "clear" => {
            let (removed_count, cfg) = store.clear_images().map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "removed_count": removed_count,
                "count": cfg.images.len(),
                "images": cfg.images,
            }))
        }
        "tuning" => {
            let image_enabled = parse_toggle_pair(
                payload,
                "enable_images",
                "disable_images",
                "--enable-images",
                "--disable-images",
            )?;
            let apple_music_artwork_enabled = parse_toggle_pair(
                payload,
                "enable_apple_music",
                "disable_apple_music",
                "--enable-apple-music",
                "--disable-apple-music",
            )?;
            let youtube_mirror_enabled = parse_toggle_pair(
                payload,
                "enable_youtube_mirror",
                "disable_youtube_mirror",
                "--enable-youtube-mirror",
                "--disable-youtube-mirror",
            )?;
            let reset = payload
                .get("reset")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let cfg = store
                .update_tuning(BackdropTuningPatch {
                    image_enabled,
                    apple_music_artwork_enabled,
                    youtube_mirror_enabled,
                    tile_size_px: opt_f32(payload, "tile_size_px"),
                    image_hold_secs: opt_f32(payload, "image_hold_secs"),
                    image_brightness: opt_f32(payload, "image_brightness"),
                    image_peak_luminance: opt_f32(payload, "image_peak_luminance"),
                    image_saturation: opt_f32(payload, "image_saturation"),
                    master_brightness: opt_f32(payload, "master_brightness"),
                    reset,
                })
                .map_err(CliError::local)?;
            Ok(json!({
                "config_path": config_path,
                "tuning": cfg.tuning,
            }))
        }
        _ => Err(CliError::usage(format!(
            "unknown backdrop subcommand `{sub}`"
        ))),
    }
}
