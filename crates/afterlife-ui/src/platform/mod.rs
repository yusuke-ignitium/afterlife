use gpui::{Bounds, DisplayId, Pixels};

pub mod apple_music;

#[cfg(target_os = "macos")]
pub mod macos;

pub fn init_platform() {
    #[cfg(target_os = "macos")]
    macos::set_dock_icon();
}

pub fn display_visible_bounds(display_id: DisplayId, fallback: Bounds<Pixels>) -> Bounds<Pixels> {
    #[cfg(target_os = "macos")]
    {
        macos::display_visible_bounds(display_id).unwrap_or(fallback)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = display_id;
        fallback
    }
}
