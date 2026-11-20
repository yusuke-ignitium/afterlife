use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use afterlife_design::{
    FontPx, GlassSettings, GlassSurface, Palette, RadiusPx, RowHeightPx, SpacePx, single_line,
    with_alpha,
};
use gpui::{
    App, AppContext, Application, Bounds, Context, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, ParticleWaveTuning, Pixels,
    Render, SharedString, StatefulInteractiveElement, Styled, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, canvas, div, particle_wave_with,
    point, prelude::FluentBuilder, px, rgb, set_particle_wave_tuning, size,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum DemoParam {
    TileSizePx,
    ImageHoldSecs,
    ImageBrightness,
    ImagePeakLuminance,
    ImageSaturation,
    MasterBrightness,
}

impl DemoParam {
    const ALL: [DemoParam; 6] = [
        Self::TileSizePx,
        Self::ImageHoldSecs,
        Self::ImageBrightness,
        Self::ImagePeakLuminance,
        Self::ImageSaturation,
        Self::MasterBrightness,
    ];

    fn spec(self) -> (&'static str, f32, f32, usize) {
        match self {
            Self::TileSizePx => ("Tile Size (px)", 2.0, 24.0, 1),
            Self::ImageHoldSecs => ("Image Hold (s)", 0.5, 60.0, 1),
            Self::ImageBrightness => ("Image Brightness", 0.00, 2.00, 2),
            Self::ImagePeakLuminance => ("Image Peak", 0.00, 1.00, 2),
            Self::ImageSaturation => ("Image Saturation", 0.00, 1.50, 2),
            Self::MasterBrightness => ("Master Brightness", 0.20, 2.00, 2),
        }
    }

    fn get(self, tuning: &ParticleWaveTuning, glass: &GlassSettings) -> f32 {
        match self {
            Self::TileSizePx => tuning.tile_size_px,
            Self::ImageHoldSecs => tuning.image_hold_secs,
            Self::ImageBrightness => tuning.image_brightness,
            Self::ImagePeakLuminance => tuning.image_peak_luminance,
            Self::ImageSaturation => tuning.image_saturation,
            Self::MasterBrightness => glass.bg_brightness,
        }
    }

    fn set(self, tuning: &mut ParticleWaveTuning, glass: &mut GlassSettings, val: f32) {
        let (_, min, max, _) = self.spec();
        let v = val.clamp(min, max);
        match self {
            Self::TileSizePx => tuning.tile_size_px = v,
            Self::ImageHoldSecs => tuning.image_hold_secs = v,
            Self::ImageBrightness => tuning.image_brightness = v,
            Self::ImagePeakLuminance => tuning.image_peak_luminance = v,
            Self::ImageSaturation => tuning.image_saturation = v,
            Self::MasterBrightness => glass.bg_brightness = v,
        }
    }
}

struct MirrorTileDemo {
    started_at: Instant,
    glass: GlassSettings,
    tuning: ParticleWaveTuning,
    has_images: bool,
    show_glass_card: bool,
    active_slider: Option<(DemoParam, Rc<Cell<Bounds<Pixels>>>)>,
    slider_bounds: Vec<(DemoParam, Rc<Cell<Bounds<Pixels>>>)>,
}

impl MirrorTileDemo {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        let tuning = ParticleWaveTuning {
            image_enabled: true,
            ..ParticleWaveTuning::default()
        };
        set_particle_wave_tuning(tuning);
        let has_images = std::env::var("AFTERLIFE_BACKDROP_IMAGES")
            .ok()
            .is_some_and(|v| !v.trim().is_empty());
        let slider_bounds = DemoParam::ALL
            .iter()
            .copied()
            .map(|k| (k, Rc::new(Cell::new(Bounds::default()))))
            .collect();
        Self {
            started_at: Instant::now(),
            glass: GlassSettings::default(),
            tuning,
            has_images,
            show_glass_card: true,
            active_slider: None,
            slider_bounds,
        }
    }

    fn apply_slider_at(&mut self, param: DemoParam, bounds: &Bounds<Pixels>, x: Pixels) {
        let w = f32::from(bounds.size.width).max(1.0);
        let rel = (f32::from(x) - f32::from(bounds.origin.x)) / w;
        let (_, min, max, _) = param.spec();
        let val = min + rel.clamp(0.0, 1.0) * (max - min);
        param.set(&mut self.tuning, &mut self.glass, val);
        set_particle_wave_tuning(self.tuning);
    }

    fn phase_label(&self) -> &'static str {
        if !self.has_images {
            return "NO IMAGES (pass paths as args or AFTERLIFE_BACKDROP_IMAGES)";
        }
        let t = self.started_at.elapsed().as_secs_f32() % 16.7;
        if t < 3.6 {
            "HOLD"
        } else if t < 9.8 {
            "DISINTEGRATE (TL -> BR)"
        } else if t < 10.5 {
            "VOID"
        } else {
            "REASSEMBLE (BR -> TL)"
        }
    }
}

impl Render for MirrorTileDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::abyssal_glass(&self.glass);
        let phase = self.phase_label();
        let show_card = self.show_glass_card;
        let track_w: f32 = 220.0;

        div()
            .id("mirror-tile-demo-root")
            .relative()
            .w_full()
            .h_full()
            .bg(particle_wave_with(
                p.window_backdrop,
                p.wave_a,
                p.wave_b,
                self.glass.bg_brightness,
            ))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if ev.dragging() {
                    if let Some((param, cell)) = this.active_slider.clone() {
                        let b = cell.get();
                        this.apply_slider_at(param, &b, ev.position.x);
                        cx.notify();
                    }
                } else if this.active_slider.is_some() {
                    this.active_slider = None;
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    this.active_slider = None;
                }),
            )
            .child(
                div()
                    .absolute()
                    .bottom(SpacePx::S24.px())
                    .left(SpacePx::S24.px())
                    .right(SpacePx::S24.px())
                    .flex()
                    .flex_row()
                    .items_end()
                    .justify_between()
                    .child(
                        GlassSurface::panel(&p, &self.glass)
                            .pad(SpacePx::S12)
                            .gap(SpacePx::S12)
                            .build()
                            .flex_row()
                            .items_center()
                            .child(single_line(
                                RowHeightPx::H24,
                                SpacePx::S0,
                                SpacePx::S4,
                                FontPx::F12,
                                RadiusPx::R4,
                                None,
                                None,
                                p.text_primary,
                                false,
                                p.centerline,
                                [SharedString::from("Mirror-Ball Tile Dissolve Demo")
                                    .into_any_element()],
                            ))
                            .child(single_line(
                                RowHeightPx::H24,
                                SpacePx::S0,
                                SpacePx::S4,
                                FontPx::F11,
                                RadiusPx::R4,
                                None,
                                None,
                                p.accent,
                                false,
                                p.centerline,
                                [SharedString::from(phase).into_any_element()],
                            )),
                    )
                    .child(
                        GlassSurface::panel(&p, &self.glass)
                            .pad(SpacePx::S12)
                            .gap(SpacePx::S8)
                            .build()
                            .w(px(248.0))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(single_line(
                                        RowHeightPx::H22,
                                        SpacePx::S0,
                                        SpacePx::S0,
                                        FontPx::F12,
                                        RadiusPx::R4,
                                        None,
                                        None,
                                        p.text_primary,
                                        false,
                                        p.centerline,
                                        [SharedString::from("Backdrop Tuning").into_any_element()],
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap(SpacePx::S4.px())
                                            .child(
                                                div()
                                                    .id("reset-tuning")
                                                    .px(SpacePx::S8.px())
                                                    .py(px(2.0))
                                                    .rounded(RadiusPx::R4.px())
                                                    .bg(with_alpha(p.window_backdrop, 0.55))
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.glass = GlassSettings::default();
                                                        this.tuning = ParticleWaveTuning {
                                                            image_enabled: true,
                                                            ..ParticleWaveTuning::default()
                                                        };
                                                        set_particle_wave_tuning(this.tuning);
                                                        cx.notify();
                                                    }))
                                                    .text_size(FontPx::F11.px())
                                                    .text_color(p.text_secondary)
                                                    .child("Reset"),
                                            )
                                            .child(
                                                div()
                                                    .id("toggle-glass-sample")
                                                    .px(SpacePx::S8.px())
                                                    .py(px(2.0))
                                                    .rounded(RadiusPx::R4.px())
                                                    .bg(with_alpha(p.window_backdrop, 0.55))
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.show_glass_card =
                                                            !this.show_glass_card;
                                                        cx.notify();
                                                    }))
                                                    .text_size(FontPx::F11.px())
                                                    .text_color(p.text_secondary)
                                                    .child(if show_card {
                                                        "Card Off"
                                                    } else {
                                                        "Card On"
                                                    }),
                                            ),
                                    ),
                            )
                            .children(self.slider_bounds.iter().enumerate().map(
                                |(idx, (param, cell))| {
                                    let param = *param;
                                    let cell_for_canvas = cell.clone();
                                    let cell_for_down = cell.clone();
                                    let (label, min, max, dp) = param.spec();
                                    let val = param.get(&self.tuning, &self.glass);
                                    let frac = ((val - min) / (max - min)).clamp(0.0, 1.0);
                                    let fill_w = (track_w * frac).max(2.0);
                                    let val_text = match dp {
                                        1 => format!("{:.1}", val),
                                        _ => format!("{:.2}", val),
                                    };

                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(SpacePx::S4.px())
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(p.text_secondary)
                                                        .child(label),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(p.accent)
                                                        .child(val_text),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "demo-slider-{}",
                                                    idx
                                                )))
                                                .relative()
                                                .w(px(track_w))
                                                .h(px(14.0))
                                                .rounded(RadiusPx::R4.px())
                                                .bg(with_alpha(p.window_backdrop, 0.65))
                                                .cursor_pointer()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(
                                                        move |this, ev: &MouseDownEvent, _, cx| {
                                                            let b = cell_for_down.get();
                                                            this.apply_slider_at(
                                                                param,
                                                                &b,
                                                                ev.position.x,
                                                            );
                                                            this.active_slider = Some((
                                                                param,
                                                                cell_for_down.clone(),
                                                            ));
                                                            cx.stop_propagation();
                                                            cx.notify();
                                                        },
                                                    ),
                                                )
                                                .child(
                                                    div()
                                                        .absolute()
                                                        .left_0()
                                                        .top_0()
                                                        .bottom_0()
                                                        .w(px(fill_w))
                                                        .rounded(RadiusPx::R4.px())
                                                        .bg(p.accent),
                                                )
                                                .child(
                                                    canvas(
                                                        move |bounds, _, _| {
                                                            cell_for_canvas.set(bounds);
                                                        },
                                                        |_, _, _, _| {},
                                                    )
                                                    .absolute()
                                                    .inset_0(),
                                                ),
                                        )
                                },
                            )),
                    ),
            )
            .when(show_card, |root| {
                root.child(
                    div()
                        .absolute()
                        .top(SpacePx::S24.px())
                        .left(SpacePx::S24.px())
                        .w(px(280.0))
                        .child(
                            GlassSurface::card(&p, &self.glass)
                                .pad(SpacePx::S16)
                                .gap(SpacePx::S8)
                                .build()
                                .child(single_line(
                                    RowHeightPx::H24,
                                    SpacePx::S0,
                                    SpacePx::S4,
                                    FontPx::F13,
                                    RadiusPx::R4,
                                    None,
                                    None,
                                    p.text_primary,
                                    false,
                                    p.centerline,
                                    [SharedString::from("Liquid Glass Frost Check")
                                        .into_any_element()],
                                ))
                                .child(
                                    div()
                                        .text_size(FontPx::F11.px())
                                        .text_color(rgb(0xb8c0cc))
                                        .child("Backdrop-synced glass floats cleanly over mirror-ball tile particles."),
                                ),
                        ),
                )
            })
    }
}

fn main() {
    let cli_paths: Vec<String> = std::env::args().skip(1).collect();
    if !cli_paths.is_empty() {
        let joined = cli_paths.join(",");
        unsafe {
            std::env::set_var("AFTERLIFE_BACKDROP_IMAGES", joined);
        }
    }

    Application::new().run(|cx: &mut App| {
        let bounds = cx
            .primary_display()
            .map(|d| d.bounds())
            .unwrap_or_else(|| Bounds::centered(None, size(px(1440.0), px(900.0)), cx));
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Maximized(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Afterlife — Mirror-Ball Tile Dissolve Demo".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.0), px(14.0))),
            }),
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        };
        cx.open_window(opts, |_, cx| cx.new(MirrorTileDemo::new))
            .unwrap();
        cx.activate(true);
    });
}
