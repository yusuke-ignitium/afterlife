use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use gpui::{App, Bounds, DisplayId, Pixels, Window, WindowBounds, point, px, size};
use serde::{Deserialize, Serialize};

use crate::platform;

pub const WINDOW_STATE_FILE_NAME: &str = "window.json";
pub const DEFAULT_DETAIL_HEIGHT_PX: f32 = 360.0;
pub const MIN_DETAIL_HEIGHT_PX: f32 = 160.0;
pub const MAX_DETAIL_HEIGHT_PX: f32 = 900.0;
const MIN_QUEUE_PANE_HEIGHT_PX: f32 = 140.0;
const MIN_WINDOW_WIDTH: f32 = 480.0;
const MIN_WINDOW_HEIGHT: f32 = 320.0;
const FALLBACK_WINDOW_WIDTH: f32 = 1480.0;
const FALLBACK_WINDOW_HEIGHT: f32 = 920.0;

static TEMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedWindowState {
    #[serde(default)]
    pub display_uuid: Option<String>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default)]
    pub detail_height: Option<f32>,
}

pub fn sanitize_detail_height(height: Option<f32>) -> f32 {
    height
        .filter(|h| h.is_finite())
        .map(|h| h.clamp(MIN_DETAIL_HEIGHT_PX, MAX_DETAIL_HEIGHT_PX))
        .unwrap_or(DEFAULT_DETAIL_HEIGHT_PX)
}

pub fn clamp_detail_height(height: f32, pane_height: f32) -> f32 {
    let max_h =
        (pane_height - MIN_QUEUE_PANE_HEIGHT_PX).clamp(MIN_DETAIL_HEIGHT_PX, MAX_DETAIL_HEIGHT_PX);
    sanitize_detail_height(Some(height)).clamp(MIN_DETAIL_HEIGHT_PX, max_h)
}

impl SavedWindowState {
    pub fn is_valid(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= MIN_WINDOW_WIDTH
            && self.height >= MIN_WINDOW_HEIGHT
    }
}

#[derive(Clone, Debug)]
pub struct DisplaySnapshot<Id = DisplayId> {
    pub id: Id,
    pub uuid: Option<String>,
    pub visible_bounds: Bounds<Pixels>,
}

#[derive(Clone, Debug)]
pub struct WindowStateStore {
    path: PathBuf,
}

impl WindowStateStore {
    pub fn from_env(perf_active: bool) -> Option<Self> {
        let ephemeral = std::env::var("AFTERLIFE_EPHEMERAL").is_ok()
            || std::env::var("AFTERLIFE_PERF_SCENARIO").is_ok()
            || perf_active;
        if ephemeral {
            None
        } else {
            Some(Self::new(afterlife_protocol::default_backdrop_data_dir()))
        }
    }

    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let dir = data_dir.into();
        Self {
            path: dir.join(WINDOW_STATE_FILE_NAME),
        }
    }

    pub fn load(&self) -> Option<SavedWindowState> {
        let raw = fs::read_to_string(&self.path).ok()?;
        let state = serde_json::from_str::<SavedWindowState>(&raw).ok()?;
        if state.is_valid() { Some(state) } else { None }
    }

    pub fn save(&self, state: &SavedWindowState) -> Result<(), String> {
        if !state.is_valid() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        let body = serde_json::to_string_pretty(state)
            .map_err(|e| format!("serialize window state: {e}"))?;
        let tmp = unique_temp_path(&self.path);
        if let Err(e) = fs::write(&tmp, body) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("write {}: {e}", tmp.display()));
        }
        if let Err(e) = fs::rename(&tmp, &self.path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("rename {}: {e}", self.path.display()));
        }
        Ok(())
    }
}

fn unique_temp_path(target: &Path) -> PathBuf {
    let seq = TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("window");
    target.with_file_name(format!(".{file_name}.{pid}.{seq}.tmp"))
}

pub fn collect_displays(cx: &App) -> (Vec<DisplaySnapshot>, Option<DisplayId>) {
    let primary_id = cx.primary_display().map(|d| d.id());
    let displays = cx
        .displays()
        .into_iter()
        .map(|d| {
            let id = d.id();
            let uuid = d.uuid().ok().map(|u| u.to_string());
            let visible_bounds = platform::display_visible_bounds(id, d.bounds());
            DisplaySnapshot {
                id,
                uuid,
                visible_bounds,
            }
        })
        .collect();
    (displays, primary_id)
}

pub fn capture_window_state(
    window: &Window,
    cx: &App,
    detail_height: Option<f32>,
) -> Option<SavedWindowState> {
    let (bounds, fullscreen) = match window.window_bounds() {
        WindowBounds::Fullscreen(b) => (b, true),
        WindowBounds::Maximized(b) | WindowBounds::Windowed(b) => (b, false),
    };
    let state = SavedWindowState {
        display_uuid: window
            .display(cx)
            .and_then(|d| d.uuid().ok())
            .map(|u| u.to_string()),
        x: f32::from(bounds.origin.x),
        y: f32::from(bounds.origin.y),
        width: f32::from(bounds.size.width),
        height: f32::from(bounds.size.height),
        fullscreen,
        detail_height: detail_height.map(|h| sanitize_detail_height(Some(h))),
    };
    if state.is_valid() { Some(state) } else { None }
}

pub fn resolve_initial_window<Id: Copy + Eq>(
    saved: Option<&SavedWindowState>,
    displays: &[DisplaySnapshot<Id>],
    primary_id: Option<Id>,
) -> (Option<Id>, WindowBounds) {
    let primary = primary_id
        .and_then(|pid| displays.iter().find(|d| d.id == pid))
        .or_else(|| displays.first());

    let Some(primary_display) = primary else {
        let fallback = Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(FALLBACK_WINDOW_WIDTH), px(FALLBACK_WINDOW_HEIGHT)),
        );
        return (None, WindowBounds::Windowed(fallback));
    };

    let Some(saved) = saved.filter(|s| s.is_valid()) else {
        return (
            Some(primary_display.id),
            WindowBounds::Windowed(primary_display.visible_bounds),
        );
    };

    let matched = saved
        .display_uuid
        .as_deref()
        .and_then(|uuid| displays.iter().find(|d| d.uuid.as_deref() == Some(uuid)));
    let (target, same_display) = match matched {
        Some(d) => (d, true),
        None => (primary_display, saved.display_uuid.is_none()),
    };

    let bounds = clamp_bounds_to_visible(saved, target.visible_bounds, same_display);
    let window_bounds = if saved.fullscreen {
        WindowBounds::Fullscreen(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    };
    (Some(target.id), window_bounds)
}

fn clamp_bounds_to_visible(
    saved: &SavedWindowState,
    visible: Bounds<Pixels>,
    preserve_origin: bool,
) -> Bounds<Pixels> {
    let vis_x = f32::from(visible.origin.x);
    let vis_y = f32::from(visible.origin.y);
    let vis_w = f32::from(visible.size.width).max(1.0);
    let vis_h = f32::from(visible.size.height).max(1.0);

    let min_w = MIN_WINDOW_WIDTH.min(vis_w);
    let min_h = MIN_WINDOW_HEIGHT.min(vis_h);
    let width = saved.width.clamp(min_w, vis_w);
    let height = saved.height.clamp(min_h, vis_h);

    let (x, y) = if preserve_origin {
        let max_x = vis_x + (vis_w - width).max(0.0);
        let max_y = vis_y + (vis_h - height).max(0.0);
        (saved.x.clamp(vis_x, max_x), saved.y.clamp(vis_y, max_y))
    } else {
        (
            vis_x + ((vis_w - width) * 0.5).max(0.0),
            vis_y + ((vis_h - height) * 0.5).max(0.0),
        )
    };

    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_display(id: u32, uuid: &str, x: f32, y: f32, w: f32, h: f32) -> DisplaySnapshot<u32> {
        DisplaySnapshot {
            id,
            uuid: Some(uuid.to_string()),
            visible_bounds: Bounds::new(point(px(x), px(y)), size(px(w), px(h))),
        }
    }

    #[test]
    fn default_window_fills_primary_visible_bounds_when_no_saved_state() {
        let primary = test_display(1, "uuid-primary", 0.0, 38.0, 1512.0, 900.0);
        let secondary = test_display(2, "uuid-secondary", 0.0, 25.0, 2560.0, 1415.0);
        let displays = vec![primary.clone(), secondary];

        let (display_id, bounds) = resolve_initial_window(None, &displays, Some(primary.id));
        assert_eq!(display_id, Some(primary.id));
        assert_eq!(bounds, WindowBounds::Windowed(primary.visible_bounds));
    }

    #[test]
    fn restores_exact_secondary_monitor_position_size_and_fullscreen() {
        let primary = test_display(1, "uuid-primary", 0.0, 38.0, 1512.0, 900.0);
        let secondary = test_display(2, "uuid-secondary", 0.0, 25.0, 2560.0, 1415.0);
        let displays = vec![primary.clone(), secondary.clone()];
        let saved = SavedWindowState {
            display_uuid: Some("uuid-secondary".to_string()),
            x: 120.0,
            y: 90.0,
            width: 1800.0,
            height: 1050.0,
            fullscreen: true,
            detail_height: None,
        };

        let (display_id, bounds) =
            resolve_initial_window(Some(&saved), &displays, Some(primary.id));
        assert_eq!(display_id, Some(secondary.id));
        assert_eq!(
            bounds,
            WindowBounds::Fullscreen(Bounds::new(
                point(px(120.0), px(90.0)),
                size(px(1800.0), px(1050.0)),
            ))
        );
    }

    #[test]
    fn falls_back_to_primary_and_clamps_when_saved_monitor_disconnected() {
        let primary = test_display(1, "uuid-primary", 0.0, 38.0, 1512.0, 900.0);
        let displays = vec![primary.clone()];
        let saved = SavedWindowState {
            display_uuid: Some("uuid-unplugged".to_string()),
            x: 400.0,
            y: 200.0,
            width: 2400.0,
            height: 1300.0,
            fullscreen: false,
            detail_height: None,
        };

        let (display_id, bounds) =
            resolve_initial_window(Some(&saved), &displays, Some(primary.id));
        assert_eq!(display_id, Some(primary.id));
        assert_eq!(bounds, WindowBounds::Windowed(primary.visible_bounds));
    }

    #[test]
    fn clamps_offscreen_origin_on_same_monitor_and_rejects_nan() {
        let primary = test_display(1, "uuid-primary", 60.0, 38.0, 1452.0, 900.0);
        let displays = vec![primary.clone()];

        let offscreen = SavedWindowState {
            display_uuid: Some("uuid-primary".to_string()),
            x: 5000.0,
            y: -200.0,
            width: 1000.0,
            height: 700.0,
            fullscreen: false,
            detail_height: None,
        };
        let (display_id, bounds) =
            resolve_initial_window(Some(&offscreen), &displays, Some(primary.id));
        assert_eq!(display_id, Some(primary.id));
        assert_eq!(
            bounds,
            WindowBounds::Windowed(Bounds::new(
                point(px(512.0), px(38.0)),
                size(px(1000.0), px(700.0)),
            ))
        );

        let corrupt = SavedWindowState {
            display_uuid: Some("uuid-primary".to_string()),
            x: f32::NAN,
            y: 40.0,
            width: 1000.0,
            height: 700.0,
            fullscreen: false,
            detail_height: None,
        };
        let (_, fallback_bounds) =
            resolve_initial_window(Some(&corrupt), &displays, Some(primary.id));
        assert_eq!(
            fallback_bounds,
            WindowBounds::Windowed(primary.visible_bounds)
        );
    }

    #[test]
    fn detail_height_sanitizes_clamps_and_persists_with_backward_compat() {
        assert_eq!(sanitize_detail_height(None), DEFAULT_DETAIL_HEIGHT_PX);
        assert_eq!(
            sanitize_detail_height(Some(f32::NAN)),
            DEFAULT_DETAIL_HEIGHT_PX
        );
        assert_eq!(sanitize_detail_height(Some(40.0)), MIN_DETAIL_HEIGHT_PX);
        assert_eq!(sanitize_detail_height(Some(2000.0)), MAX_DETAIL_HEIGHT_PX);
        assert_eq!(clamp_detail_height(700.0, 600.0), 460.0);
        assert_eq!(clamp_detail_height(100.0, 600.0), MIN_DETAIL_HEIGHT_PX);

        let legacy_json = r#"{"x":10.0,"y":20.0,"width":1200.0,"height":800.0,"fullscreen":false}"#;
        let parsed: SavedWindowState = serde_json::from_str(legacy_json).expect("legacy json");
        assert_eq!(parsed.detail_height, None);
        assert_eq!(
            sanitize_detail_height(parsed.detail_height),
            DEFAULT_DETAIL_HEIGHT_PX
        );
    }

    #[test]
    fn store_round_trips_and_handles_concurrent_saves() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "afterlife-window-state-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp_dir);
        let store = WindowStateStore::new(&tmp_dir);
        assert_eq!(store.load(), None);

        let sample = SavedWindowState {
            display_uuid: Some("uuid-monitor-2".to_string()),
            x: 80.0,
            y: 44.0,
            width: 1920.0,
            height: 1080.0,
            fullscreen: false,
            detail_height: Some(442.0),
        };
        store.save(&sample).expect("save sample");
        assert_eq!(store.load(), Some(sample));

        let mut handles = Vec::new();
        for i in 0..8 {
            let store_clone = store.clone();
            handles.push(std::thread::spawn(move || {
                for j in 0..10 {
                    let s = SavedWindowState {
                        display_uuid: Some(format!("uuid-{i}")),
                        x: (i * 10 + j) as f32,
                        y: 38.0,
                        width: 1200.0 + j as f32,
                        height: 800.0,
                        fullscreen: false,
                        detail_height: Some(300.0 + j as f32),
                    };
                    store_clone.save(&s).expect("concurrent save");
                }
            }));
        }
        for h in handles {
            h.join().expect("thread join");
        }
        assert!(store.load().is_some());
        let _ = fs::remove_dir_all(&tmp_dir);
    }
}
