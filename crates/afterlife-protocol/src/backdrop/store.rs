use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

use super::{
    BackdropConfig, BackdropGlassAppearance, BackdropImageEntry, BackdropTuning,
    BackdropTuningPatch, sanitize_display_tile_sizes,
};

const MAX_BACKDROP_DIM: &str = "1600";
pub(super) static TMP_FILE_SEQ: AtomicU64 = AtomicU64::new(1);

fn unique_tmp_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let seq = TMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("{stem}.tmp.{}.{seq}.{ext}", std::process::id()))
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

