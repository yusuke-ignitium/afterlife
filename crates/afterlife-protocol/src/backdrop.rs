use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

pub const DEFAULT_IMAGE_ENABLED: bool = false;
pub const DEFAULT_APPLE_MUSIC_ARTWORK_ENABLED: bool = true;
pub const DEFAULT_YOUTUBE_MIRROR_ENABLED: bool = true;
pub const DEFAULT_TILE_SIZE_PX: f32 = 6.0;
pub const DEFAULT_IMAGE_HOLD_SECS: f32 = 10.0;
pub const DEFAULT_IMAGE_BRIGHTNESS: f32 = 0.40;
pub const DEFAULT_IMAGE_PEAK_LUMINANCE: f32 = 0.55;
pub const DEFAULT_IMAGE_SATURATION: f32 = 0.64;
pub const DEFAULT_MASTER_BRIGHTNESS: f32 = 1.50;

const MAX_BACKDROP_DIM: &str = "1600";
static TMP_FILE_SEQ: AtomicU64 = AtomicU64::new(1);

fn finite_clamp(val: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if val.is_finite() {
        val.clamp(min, max)
    } else {
        fallback
    }
}

fn unique_tmp_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let seq = TMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("{stem}.tmp.{}.{seq}.{ext}", std::process::id()))
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackdropTuning {
    pub image_enabled: bool,
    pub apple_music_artwork_enabled: bool,
    pub youtube_mirror_enabled: bool,
    pub tile_size_px: f32,
    pub image_hold_secs: f32,
    pub image_brightness: f32,
    pub image_peak_luminance: f32,
    pub image_saturation: f32,
    pub master_brightness: f32,
}

impl Default for BackdropTuning {
    fn default() -> Self {
        Self {
            image_enabled: DEFAULT_IMAGE_ENABLED,
            apple_music_artwork_enabled: DEFAULT_APPLE_MUSIC_ARTWORK_ENABLED,
            youtube_mirror_enabled: DEFAULT_YOUTUBE_MIRROR_ENABLED,
            tile_size_px: DEFAULT_TILE_SIZE_PX,
            image_hold_secs: DEFAULT_IMAGE_HOLD_SECS,
            image_brightness: DEFAULT_IMAGE_BRIGHTNESS,
            image_peak_luminance: DEFAULT_IMAGE_PEAK_LUMINANCE,
            image_saturation: DEFAULT_IMAGE_SATURATION,
            master_brightness: DEFAULT_MASTER_BRIGHTNESS,
        }
    }
}

impl BackdropTuning {
    pub fn clamped(self) -> Self {
        let def = Self::default();
        Self {
            image_enabled: self.image_enabled,
            apple_music_artwork_enabled: self.apple_music_artwork_enabled,
            youtube_mirror_enabled: self.youtube_mirror_enabled,
            tile_size_px: finite_clamp(self.tile_size_px, 2.0, 24.0, def.tile_size_px).round(),
            image_hold_secs: finite_clamp(self.image_hold_secs, 0.5, 600.0, def.image_hold_secs),
            image_brightness: finite_clamp(self.image_brightness, 0.0, 2.0, def.image_brightness),
            image_peak_luminance: finite_clamp(
                self.image_peak_luminance,
                0.0,
                1.0,
                def.image_peak_luminance,
            ),
            image_saturation: finite_clamp(self.image_saturation, 0.0, 1.5, def.image_saturation),
            master_brightness: finite_clamp(
                self.master_brightness,
                0.2,
                2.0,
                def.master_brightness,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackdropTuningPatch {
    pub image_enabled: Option<bool>,
    pub apple_music_artwork_enabled: Option<bool>,
    pub youtube_mirror_enabled: Option<bool>,
    pub tile_size_px: Option<f32>,
    pub image_hold_secs: Option<f32>,
    pub image_brightness: Option<f32>,
    pub image_peak_luminance: Option<f32>,
    pub image_saturation: Option<f32>,
    pub master_brightness: Option<f32>,
    pub reset: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackdropGlassAppearance {
    pub disp_scale: f32,
    pub rim_thickness: f32,
    pub refract_reach: f32,
    pub chroma: f32,
    pub highlight_alpha: f32,
    pub rim_width: f32,
    pub blur_std_dev: f32,
    pub bg_alpha: f32,
    pub radius: f32,
    pub tint_rgb: [u8; 3],
    pub pause_animation_when_inactive: bool,
    pub fade_ui_after_secs: f32,
    pub approval_notifications_enabled: bool,
}

impl Default for BackdropGlassAppearance {
    fn default() -> Self {
        Self {
            disp_scale: 193.0,
            rim_thickness: 0.06,
            refract_reach: 50.0,
            chroma: 0.0,
            highlight_alpha: 0.24,
            rim_width: 1.3,
            blur_std_dev: 0.0,
            bg_alpha: 0.04,
            radius: 6.6,
            tint_rgb: [7, 9, 14],
            pause_animation_when_inactive: false,
            fade_ui_after_secs: 0.0,
            approval_notifications_enabled: true,
        }
    }
}

impl BackdropGlassAppearance {
    pub fn clamped(self) -> Self {
        let def = Self::default();
        Self {
            disp_scale: finite_clamp(self.disp_scale, 0.0, 800.0, def.disp_scale),
            rim_thickness: finite_clamp(self.rim_thickness, 0.01, 0.5, def.rim_thickness),
            refract_reach: finite_clamp(self.refract_reach, 1.0, 600.0, def.refract_reach),
            chroma: finite_clamp(self.chroma, 0.0, 1.0, def.chroma),
            highlight_alpha: finite_clamp(self.highlight_alpha, 0.0, 1.0, def.highlight_alpha),
            rim_width: finite_clamp(self.rim_width, 0.2, 8.0, def.rim_width),
            blur_std_dev: finite_clamp(self.blur_std_dev, 0.0, 20.0, def.blur_std_dev),
            bg_alpha: finite_clamp(self.bg_alpha, 0.0, 0.85, def.bg_alpha),
            radius: finite_clamp(self.radius, 0.0, 20.0, def.radius),
            tint_rgb: self.tint_rgb,
            pause_animation_when_inactive: self.pause_animation_when_inactive,
            fade_ui_after_secs: finite_clamp(self.fade_ui_after_secs, 0.0, 60.0, def.fade_ui_after_secs).round(),
            approval_notifications_enabled: self.approval_notifications_enabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackdropImageEntry {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackdropConfig {
    pub images: Vec<BackdropImageEntry>,
    pub tuning: BackdropTuning,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass: Option<BackdropGlassAppearance>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub display_tile_sizes: BTreeMap<String, f32>,
}

pub fn sanitize_display_tile_sizes(map: BTreeMap<String, f32>) -> BTreeMap<String, f32> {
    map.into_iter()
        .filter_map(|(k, v)| {
            let trimmed = k.trim();
            if trimmed.is_empty() || !v.is_finite() {
                return None;
            }
            Some((trimmed.to_string(), v.clamp(2.0, 24.0).round()))
        })
        .collect()
}

pub fn resolve_tile_size_for_display(
    base_tile_size_px: f32,
    display_uuid: Option<&str>,
    scale_factor: f32,
    overrides: &BTreeMap<String, f32>,
) -> f32 {
    if let Some(uuid) = display_uuid.map(str::trim).filter(|s| !s.is_empty())
        && let Some(&saved) = overrides.get(uuid)
        && saved.is_finite()
    {
        return saved.clamp(2.0, 24.0).round();
    }
    let base = finite_clamp(base_tile_size_px, 2.0, 24.0, DEFAULT_TILE_SIZE_PX).round();
    let safe_scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        2.0
    };
    (base * (safe_scale / 2.0)).round().clamp(2.0, 24.0)
}

impl BackdropConfig {
    pub fn sanitized(mut self) -> Self {
        self.tuning = self.tuning.clamped();
        self.glass = self.glass.map(BackdropGlassAppearance::clamped);
        self.display_tile_sizes = sanitize_display_tile_sizes(self.display_tile_sizes);
        self
    }
}

#[derive(Clone, Debug)]
pub struct BackdropStore {
    data_dir: PathBuf,
}

pub fn default_backdrop_data_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("AFTERLIFE_DATA_DIR") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".afterlife");
    }
    PathBuf::from(".afterlife")
}

impl BackdropStore {
    pub fn open(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn open_default() -> Self {
        Self::open(default_backdrop_data_dir())
    }

    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join("backdrop.json")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.data_dir.join("backdrops")
    }

    pub fn mtime(&self) -> Option<SystemTime> {
        fs::metadata(self.config_path()).ok()?.modified().ok()
    }

    pub fn load(&self) -> Result<BackdropConfig, String> {
        let path = self.config_path();
        if !path.exists() {
            return Ok(BackdropConfig::default());
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let cfg: BackdropConfig = serde_json::from_str(&raw)
            .map_err(|e| format!("invalid backdrop config {}: {e}", path.display()))?;
        Ok(cfg.sanitized())
    }

    pub fn save(&self, cfg: &BackdropConfig) -> Result<(), String> {
        fs::create_dir_all(&self.data_dir)
            .map_err(|e| format!("failed to create {}: {e}", self.data_dir.display()))?;
        let path = self.config_path();
        let tmp = unique_tmp_path(&self.data_dir, "backdrop", "json");
        let sanitized = cfg.clone().sanitized();
        let json = serde_json::to_string_pretty(&sanitized)
            .map_err(|e| format!("failed to serialize backdrop config: {e}"))?;
        if let Err(e) = fs::write(&tmp, format!("{json}\n")) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("failed to write {}: {e}", tmp.display()));
        }
        if let Err(e) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("failed to replace {}: {e}", path.display()));
        }
        Ok(())
    }

    pub fn add_image(
        &self,
        source_path: &Path,
        label: Option<&str>,
    ) -> Result<(BackdropImageEntry, BackdropConfig), String> {
        let canonical_src = canonicalize_source(source_path)?;
        let mut cfg = self.load()?;
        let id = next_image_id(&cfg.images);
        let stored_path = self.import_image(&canonical_src, &id)?;
        let resolved_label = match label.map(str::trim).filter(|s| !s.is_empty()) {
            Some(l) => l.to_string(),
            None => default_label_from_path(&canonical_src, &id),
        };
        let entry = BackdropImageEntry {
            id,
            path: stored_path.to_string_lossy().into_owned(),
            source: canonical_src.to_string_lossy().into_owned(),
            label: resolved_label,
        };
        cfg.images.push(entry.clone());
        self.save(&cfg)?;
        Ok((entry, cfg))
    }

    pub fn update_image(
        &self,
        target: &str,
        new_source: Option<&Path>,
        new_label: Option<&str>,
        new_index: Option<usize>,
    ) -> Result<(BackdropImageEntry, BackdropConfig), String> {
        if new_source.is_none() && new_label.is_none() && new_index.is_none() {
            return Err("at least one of --path, --label, or --index must be provided".to_string());
        }
        let mut cfg = self.load()?;
        let pos = find_entry_index(&cfg.images, target)
            .ok_or_else(|| format!("backdrop image `{target}` not found"))?;

        if let Some(src) = new_source {
            let canonical_src = canonicalize_source(src)?;
            let id = cfg.images[pos].id.clone();
            let old_path = PathBuf::from(&cfg.images[pos].path);
            let stored_path = self.import_image(&canonical_src, &id)?;
            if old_path != stored_path {
                self.remove_managed_file(&old_path);
            }
            cfg.images[pos].path = stored_path.to_string_lossy().into_owned();
            cfg.images[pos].source = canonical_src.to_string_lossy().into_owned();
        }
        if let Some(lbl) = new_label {
            let trimmed = lbl.trim();
            if trimmed.is_empty() {
                return Err("--label cannot be empty".to_string());
            }
            cfg.images[pos].label = trimmed.to_string();
        }

        let entry = cfg.images.remove(pos);
        let insert_at = match new_index {
            Some(idx) => idx.min(cfg.images.len()),
            None => pos.min(cfg.images.len()),
        };
        cfg.images.insert(insert_at, entry);
        let updated = cfg.images[insert_at].clone();
        self.save(&cfg)?;
        Ok((updated, cfg))
    }

    pub fn remove_image(
        &self,
        target: &str,
    ) -> Result<(BackdropImageEntry, BackdropConfig), String> {
        let mut cfg = self.load()?;
        let pos = find_entry_index(&cfg.images, target)
            .ok_or_else(|| format!("backdrop image `{target}` not found"))?;
        let removed = cfg.images.remove(pos);
        self.remove_managed_file(Path::new(&removed.path));
        self.save(&cfg)?;
        Ok((removed, cfg))
    }

    pub fn clear_images(&self) -> Result<(usize, BackdropConfig), String> {
        let mut cfg = self.load()?;
        let count = cfg.images.len();
        for item in cfg.images.drain(..) {
            self.remove_managed_file(Path::new(&item.path));
        }
        self.save(&cfg)?;
        Ok((count, cfg))
    }

    pub fn update_tuning(&self, patch: BackdropTuningPatch) -> Result<BackdropConfig, String> {
        let mut cfg = self.load()?;
        if patch.reset {
            cfg.tuning = BackdropTuning::default();
            cfg.display_tile_sizes.clear();
        }
        if let Some(v) = patch.image_enabled {
            cfg.tuning.image_enabled = v;
        }
        if let Some(v) = patch.apple_music_artwork_enabled {
            cfg.tuning.apple_music_artwork_enabled = v;
        }
        if let Some(v) = patch.youtube_mirror_enabled {
            cfg.tuning.youtube_mirror_enabled = v;
        }
        if let Some(v) = patch.tile_size_px {
            cfg.tuning.tile_size_px = v;
            cfg.display_tile_sizes.clear();
        }
        if let Some(v) = patch.image_hold_secs {
            cfg.tuning.image_hold_secs = v;
        }
        if let Some(v) = patch.image_brightness {
            cfg.tuning.image_brightness = v;
        }
        if let Some(v) = patch.image_peak_luminance {
            cfg.tuning.image_peak_luminance = v;
        }
        if let Some(v) = patch.image_saturation {
            cfg.tuning.image_saturation = v;
        }
        if let Some(v) = patch.master_brightness {
            cfg.tuning.master_brightness = v;
        }
        cfg = cfg.sanitized();
        self.save(&cfg)?;
        Ok(cfg)
    }

    pub fn save_appearance(
        &self,
        tuning: BackdropTuning,
        glass: BackdropGlassAppearance,
    ) -> Result<BackdropConfig, String> {
        let mut cfg = self.load()?;
        cfg.tuning = tuning.clamped();
        cfg.glass = Some(glass.clamped());
        self.save(&cfg)?;
        Ok(cfg)
    }

    pub fn save_appearance_with_displays(
        &self,
        tuning: BackdropTuning,
        glass: BackdropGlassAppearance,
        display_tile_sizes: BTreeMap<String, f32>,
    ) -> Result<BackdropConfig, String> {
        let mut cfg = self.load()?;
        cfg.tuning = tuning.clamped();
        cfg.glass = Some(glass.clamped());
        cfg.display_tile_sizes = sanitize_display_tile_sizes(display_tile_sizes);
        self.save(&cfg)?;
        Ok(cfg)
    }

    fn import_image(&self, canonical_src: &Path, id: &str) -> Result<PathBuf, String> {
        let dir = self.images_dir();
        fs::create_dir_all(&dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
        let dst_png = dir.join(format!("{id}.png"));
        if convert_with_sips_atomic(&dir, id, canonical_src, &dst_png)? {
            return Ok(dst_png);
        }
        let ext = detect_supported_image_ext(canonical_src).ok_or_else(|| {
            format!(
                "unsupported or unreadable image file: {}",
                canonical_src.display()
            )
        })?;
        let fallback_dst = dir.join(format!("{id}.{ext}"));
        let tmp_dst = unique_tmp_path(&dir, id, ext);
        if let Err(e) = fs::copy(canonical_src, &tmp_dst) {
            let _ = fs::remove_file(&tmp_dst);
            return Err(format!(
                "failed to copy {} to {}: {e}",
                canonical_src.display(),
                tmp_dst.display()
            ));
        }
        if let Err(e) = fs::rename(&tmp_dst, &fallback_dst) {
            let _ = fs::remove_file(&tmp_dst);
            return Err(format!("failed to replace {}: {e}", fallback_dst.display()));
        }
        Ok(fallback_dst)
    }

    fn remove_managed_file(&self, path: &Path) {
        let managed = self.images_dir();
        if path.starts_with(&managed) {
            let _ = fs::remove_file(path);
        }
    }
}

fn canonicalize_source(source_path: &Path) -> Result<PathBuf, String> {
    let expanded = expand_tilde(source_path);
    if !expanded.is_file() {
        return Err(format!("image file not found: {}", expanded.display()));
    }
    fs::canonicalize(&expanded)
        .map_err(|e| format!("failed to resolve {}: {e}", expanded.display()))
}

fn expand_tilde(path: &Path) -> PathBuf {
    let Some(s) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = s.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    path.to_path_buf()
}

fn next_image_id(existing: &[BackdropImageEntry]) -> String {
    let max_num = existing
        .iter()
        .filter_map(|e| e.id.strip_prefix("img-")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("img-{}", max_num + 1)
}

fn default_label_from_path(path: &Path, fallback: &str) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn find_entry_index(entries: &[BackdropImageEntry], target: &str) -> Option<usize> {
    entries
        .iter()
        .position(|e| e.id == target)
        .or_else(|| entries.iter().position(|e| e.label == target))
}

fn detect_supported_image_ext(path: &Path) -> Option<&'static str> {
    let mut file = fs::File::open(path).ok()?;
    let mut head = [0u8; 16];
    let n = file.read(&mut head).ok()?;
    let bytes = &head[..n];
    if bytes.len() >= 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some("png");
    }
    if bytes.len() >= 3 && bytes[..3] == [0xFF, 0xD8, 0xFF] {
        return Some("jpg");
    }
    if bytes.len() >= 6 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        return Some("gif");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    None
}

fn convert_with_sips_atomic(
    dir: &Path,
    id: &str,
    src: &Path,
    dst_png: &Path,
) -> Result<bool, String> {
    let sips = Path::new("/usr/bin/sips");
    if !sips.is_file() {
        return Ok(false);
    }
    let tmp_png = unique_tmp_path(dir, id, "png");
    let converted = Command::new(sips)
        .args(["-s", "format", "png"])
        .arg(src)
        .arg("--out")
        .arg(&tmp_png)
        .output();
    let Ok(out) = converted else {
        let _ = fs::remove_file(&tmp_png);
        return Ok(false);
    };
    if !out.status.success() || !tmp_png.is_file() {
        let _ = fs::remove_file(&tmp_png);
        return Ok(false);
    }
    let _ = Command::new(sips)
        .args(["-Z", MAX_BACKDROP_DIM])
        .arg(&tmp_png)
        .arg("--out")
        .arg(&tmp_png)
        .output();
    if detect_supported_image_ext(&tmp_png) != Some("png") {
        let _ = fs::remove_file(&tmp_png);
        return Ok(false);
    }
    if let Err(e) = fs::rename(&tmp_png, dst_png) {
        let _ = fs::remove_file(&tmp_png);
        return Err(format!("failed to replace {}: {e}", dst_png.display()));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
            approval_notifications_enabled: false,
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
        assert_eq!(glass.fade_ui_after_secs, 60.0);
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
        assert_eq!(
            resolve_tile_size_for_display(3.0, Some("retina-uuid"), 2.0, &overrides),
            3.0
        );
        assert_eq!(
            resolve_tile_size_for_display(3.0, Some("lg-tv-uuid"), 1.0, &overrides),
            2.0
        );
        assert_eq!(
            resolve_tile_size_for_display(6.0, Some("fhd-24-uuid"), 1.0, &overrides),
            3.0
        );

        overrides.insert("lg-tv-uuid".to_string(), 3.0289884);
        assert_eq!(
            resolve_tile_size_for_display(3.0, Some("lg-tv-uuid"), 1.0, &overrides),
            3.0
        );
        assert_eq!(
            resolve_tile_size_for_display(5.0, Some("fhd-24-uuid"), 1.0, &overrides),
            3.0
        );
        assert_eq!(
            resolve_tile_size_for_display(3.0, Some("retina-uuid"), 2.0, &overrides),
            3.0
        );
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
}
