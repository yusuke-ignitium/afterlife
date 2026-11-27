mod spec;

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use afterlife_design::{
    EdgeAccent, FadePx, FontPx, GlassSettings, GlassSurface, Palette, RowHeightPx, ScrollFadeExt,
    SpacePx, UiStyle, with_alpha,
};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, BoxShadow, Context, Image, ImageFormat,
    ImageSource, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img, particle_wave_with, point,
    prelude::FluentBuilder, px, size,
};

use crate::spec::{
    ARM_TRANSITION_MS, AUTO_DISARM_MS, ActionTone, ConfirmVariant, DETAIL_TOGGLE_WIDTH_PX,
    DISARM_TRANSITION_MS, DemoContextKind, EXEC_FLASH_MS, ScalarTransition, lerp_glass_rgba,
    lerp_rgba,
};

const APP_TITLE_LOGO_PNG: &[u8] =
    include_bytes!("../../../../../crates/afterlife-ui/assets/afterlife-logo.png");

#[derive(Clone, Debug)]
struct ArmedSlot {
    variant_idx: usize,
    full_key: String,
    armed_at: Instant,
    epoch: u64,
}

struct ConfirmButtonDemoApp {
    logo: Arc<Image>,
    glass: GlassSettings,
    palette: Palette,
    ctx_kind: DemoContextKind,
    auto_disarm: bool,
    stopped: [bool; 3],
    armed: Option<ArmedSlot>,
    epoch: u64,
    arm_transitions: HashMap<String, ScalarTransition>,
    exec_flashes: HashMap<String, Instant>,
    last_event: [Option<(String, bool)>; 3],
}

impl ConfirmButtonDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let palette = Palette::from_style(UiStyle::LiquidGlass, &glass);
        let logo = Arc::new(Image::from_bytes(ImageFormat::Png, APP_TITLE_LOGO_PNG.to_vec()));
        Self {
            logo,
            glass,
            palette,
            ctx_kind: DemoContextKind::ApprovalGate,
            auto_disarm: true,
            stopped: [false, false, false],
            armed: None,
            epoch: 0,
            arm_transitions: HashMap::new(),
            exec_flashes: HashMap::new(),
            last_event: [None, None, None],
        }
    }

    fn disarm(&mut self, reason: Option<&str>, now: Instant) {
        if let Some(prev) = self.armed.take() {
            self.arm_transitions
                .entry(prev.full_key)
                .or_insert_with(|| ScalarTransition::settled(1.0, now))
                .set_target(0.0, Duration::from_millis(DISARM_TRANSITION_MS), now);
            if let Some(msg) = reason {
                self.last_event[prev.variant_idx] = Some((format!("Disarmed ({msg})"), false));
            }
        }
    }

    fn press_two_step(
        &mut self,
        variant: ConfirmVariant,
        key: &str,
        tone: ActionTone,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let idx = variant.index();
        let full_key = format!("{}-{key}", variant.id_prefix());

        if self.armed.as_ref().is_some_and(|a| a.full_key == full_key) {
            self.armed = None;
            self.arm_transitions
                .entry(full_key.clone())
                .or_insert_with(|| ScalarTransition::settled(1.0, now))
                .set_target(0.0, Duration::from_millis(DISARM_TRANSITION_MS), now);
            self.exec_flashes.insert(full_key, now);
            self.last_event[idx] = Some((format!("Executed {}", tone.label()), true));
            cx.notify();
            return;
        }

        self.disarm(None, now);
        self.epoch += 1;
        let epoch = self.epoch;
        self.arm_transitions
            .entry(full_key.clone())
            .or_insert_with(|| ScalarTransition::settled(0.0, now))
            .set_target(1.0, Duration::from_millis(ARM_TRANSITION_MS), now);
        self.armed = Some(ArmedSlot {
            variant_idx: idx,
            full_key,
            armed_at: now,
            epoch,
        });
        let hint = if self.auto_disarm {
            format!("Armed {} (3s)", tone.label())
        } else {
            format!("Armed {} · press again", tone.label())
        };
        self.last_event[idx] = Some((hint, false));

        if self.auto_disarm {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(AUTO_DISARM_MS))
                    .await;
                let _ = this.update(cx, |app, cx| {
                    if app.armed.as_ref().is_some_and(|a| a.epoch == epoch) {
                        app.disarm(Some("3s timeout"), Instant::now());
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn needs_animation_frame(&self, now: Instant) -> bool {
        if self.arm_transitions.values().any(|tr| tr.is_active(now)) {
            return true;
        }
        if self
            .exec_flashes
            .values()
            .any(|t| now.saturating_duration_since(*t) < Duration::from_millis(EXEC_FLASH_MS))
        {
            return true;
        }
        if self.auto_disarm
            && let Some(slot) = &self.armed
            && slot.variant_idx == ConfirmVariant::B3GlassBarCountdown.index()
            && now.saturating_duration_since(slot.armed_at) < Duration::from_millis(AUTO_DISARM_MS)
        {
            return true;
        }
        false
    }

    fn render_two_step_btn(
        &self,
        variant: ConfirmVariant,
        key: &'static str,
        tone: ActionTone,
        compact_ghost: bool,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let full_key = format!("{}-{key}", variant.id_prefix());
        let arm_t = self.arm_transitions.get(&full_key).map(|tr| tr.value(now)).unwrap_or(0.0);
        let flash_t = self
            .exec_flashes
            .get(&full_key)
            .map(|t| {
                let raw = now.saturating_duration_since(*t).as_secs_f32()
                    / (EXEC_FLASH_MS as f32 / 1000.0);
                (1.0 - raw.clamp(0.0, 1.0)).powi(2)
            })
            .unwrap_or(0.0);

        let (idle_bg, idle_fg) = tone.idle_colors(p);
        let (armed_bg, armed_fg) = tone.armed_colors(p, variant);
        let base_bg = lerp_glass_rgba(idle_bg, armed_bg, arm_t);
        let base_fg = lerp_rgba(idle_fg, armed_fg, arm_t);
        let bg = if flash_t > 1e-3 { lerp_glass_rgba(base_bg, with_alpha(p.green, 0.36), flash_t) } else { base_bg };
        let fg = if flash_t > 1e-3 { lerp_rgba(base_fg, p.green, flash_t) } else { base_fg };

        let accent = tone.accent(p);
        let radius = p.radius_f32(&self.glass, 0.75);
        let font_size = if compact_ghost { FontPx::F11.px() } else { FontPx::F12.px() };
        let px_pad = if compact_ghost { SpacePx::S8.px() } else { SpacePx::S12.px() };

        let countdown_frac = if variant == ConfirmVariant::B3GlassBarCountdown {
            match &self.armed {
                Some(slot) if slot.full_key == full_key && self.auto_disarm => {
                    let elapsed = now.saturating_duration_since(slot.armed_at).as_secs_f32();
                    (1.0 - elapsed / (AUTO_DISARM_MS as f32 / 1000.0)).clamp(0.0, 1.0)
                }
                _ => arm_t,
            }
        } else {
            0.0
        };

        let glow_alpha = match variant {
            ConfirmVariant::B1SolidInversion => 0.28 * arm_t,
            ConfirmVariant::B2LuminousGlass => 0.34 * arm_t,
            ConfirmVariant::B3GlassBarCountdown => 0.16 * arm_t,
        };
        let glow_shadows = (glow_alpha > 1e-3).then(|| {
            vec![BoxShadow {
                color: with_alpha(accent, glow_alpha).into(),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(12.0 * arm_t),
                spread_radius: px(0.0),
            }]
        });

        div()
            .id(SharedString::from(full_key))
            .relative().overflow_hidden().cursor_pointer()
            .h(RowHeightPx::H28.px()).px(px_pad)
            .flex().items_center().justify_center()
            .rounded(radius).bg(bg)
            .when(variant == ConfirmVariant::B2LuminousGlass, |d| {
                d.border_1().border_color(with_alpha(accent, 0.72 * arm_t))
            })
            .when_some(glow_shadows, |d, s| d.shadow(s))
            .when(compact_ghost && arm_t < 0.05, move |d| {
                d.hover(move |s| s.bg(with_alpha(p.text_primary, 0.08)).text_color(p.text_primary))
            })
            .when(!compact_ghost || arm_t >= 0.05, |d| d.hover(|s| s.opacity(0.90)))
            .text_size(font_size).text_color(fg)
            .child(tone.label())
            .when(variant == ConfirmVariant::B3GlassBarCountdown && arm_t > 1e-3, |d| {
                d.child(
                    div().absolute().left(px(0.0)).bottom(px(0.0)).h(px(2.0))
                        .w(gpui::relative(countdown_frac))
                        .bg(with_alpha(accent, 0.90 * arm_t)),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.press_two_step(variant, key, tone, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }

    fn render_top_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let kinds = [
            DemoContextKind::ApprovalGate,
            DemoContextKind::WorkStage,
            DemoContextKind::Queued,
            DemoContextKind::Proposal,
        ];

        div()
            .flex().flex_row().items_center().justify_between()
            .h(px(46.0)).pl(px(86.0)).pr(SpacePx::S24.px())
            .child(
                div().flex().flex_row().items_center().gap(SpacePx::S12.px())
                    .child(img(ImageSource::Image(self.logo.clone())).h(px(20.0)).w(px(78.0)).object_fit(ObjectFit::Contain))
                    .child(div().text_size(FontPx::F13.px()).text_color(p.text_primary).child("Color-Only Two-Step Confirm (Smooth 220ms)")),
            )
            .child(
                div().flex().flex_row().items_center().gap(SpacePx::S8.px())
                    .children(kinds.into_iter().map(|kind| {
                        let active = self.ctx_kind == kind;
                        div()
                            .id(SharedString::from(format!("ctx-{kind:?}")))
                            .cursor_pointer().h(RowHeightPx::H28.px()).px(SpacePx::S12.px())
                            .flex().items_center().rounded(p.radius_f32(&self.glass, 0.75))
                            .bg(if active { p.active_bg } else { p.btn_bg })
                            .border_1().border_color(if active { p.accent } else { p.border_subtle })
                            .text_size(FontPx::F12.px())
                            .text_color(if active { p.accent } else { p.text_secondary })
                            .child(kind.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ctx_kind = kind;
                                this.disarm(None, Instant::now());
                                this.last_event = [None, None, None];
                                cx.stop_propagation();
                                cx.notify();
                            }))
                    }))
                    .child(
                        div()
                            .id("toggle-auto-disarm")
                            .cursor_pointer().h(RowHeightPx::H28.px()).px(SpacePx::S12.px())
                            .flex().items_center().rounded(p.radius_f32(&self.glass, 0.75))
                            .bg(if self.auto_disarm { p.active_bg } else { p.btn_bg })
                            .border_1().border_color(if self.auto_disarm { p.accent } else { p.border_subtle })
                            .text_size(FontPx::F12.px())
                            .text_color(if self.auto_disarm { p.accent } else { p.text_secondary })
                            .child(if self.auto_disarm { "Auto-Disarm: 3s" } else { "Auto-Disarm: Off" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.auto_disarm = !this.auto_disarm;
                                this.disarm(None, Instant::now());
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_footer_actions(&self, variant: ConfirmVariant, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let idx = variant.index();
        let is_stopped = self.stopped[idx];
        let stop_bg = if is_stopped { p.active_bg } else { with_alpha(p.coral_inactive, 0.30) };
        let stop_fg = if is_stopped { p.accent } else { p.coral };

        let stop_btn = div()
            .id(SharedString::from(format!("{}-stop", variant.id_prefix())))
            .cursor_pointer().h(RowHeightPx::H28.px()).w(px(DETAIL_TOGGLE_WIDTH_PX))
            .flex().items_center().justify_center()
            .rounded(p.radius_f32(&self.glass, 0.75))
            .bg(stop_bg).hover(|s| s.opacity(0.88))
            .text_size(FontPx::F12.px()).text_color(stop_fg)
            .child(if is_stopped { "Resume" } else { "Stop" })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.disarm(None, Instant::now());
                this.stopped[idx] = !this.stopped[idx];
                let msg = if this.stopped[idx] { "1-click Stop" } else { "1-click Resume" };
                this.last_event[idx] = Some((msg.to_string(), true));
                cx.stop_propagation();
                cx.notify();
            }));

        match self.ctx_kind {
            DemoContextKind::Proposal => div()
                .flex().flex_row().items_center().justify_between().w_full()
                .child(self.render_two_step_btn(variant, "prop-approve", ActionTone::Approve, false, now, cx))
                .into_any_element(),
            DemoContextKind::Queued => div()
                .flex().flex_row().items_center().justify_end().w_full()
                .child(self.render_two_step_btn(variant, "detail-archive", ActionTone::Archive, true, now, cx))
                .into_any_element(),
            DemoContextKind::ApprovalGate | DemoContextKind::WorkStage => {
                let at_gate = self.ctx_kind == DemoContextKind::ApprovalGate;
                div()
                    .flex().flex_row().items_center().justify_between().w_full().gap(SpacePx::S8.px())
                    .child(
                        div().flex().flex_row().items_center().gap(SpacePx::S8.px())
                            .when(at_gate, |d| {
                                d.child(self.render_two_step_btn(variant, "detail-approve", ActionTone::Approve, false, now, cx))
                            })
                            .child(stop_btn)
                            .child(self.render_two_step_btn(variant, "detail-rework", ActionTone::Rework, false, now, cx)),
                    )
                    .child(self.render_two_step_btn(variant, "detail-archive", ActionTone::Archive, true, now, cx))
                    .into_any_element()
            }
        }
    }

    fn render_left_gate_card(&self, variant: ConfirmVariant, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        GlassSurface::card(&p, &self.glass)
            .pad(SpacePx::S12).gap(SpacePx::S8).bg(Some(p.glass_gate)).accent(EdgeAccent::Gate).build()
            .child(
                div().flex().flex_row().items_center().justify_between()
                    .child(div().text_size(FontPx::F11.px()).text_color(p.amber).child("LEFT PANE · APPROVAL GATE CARD"))
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child("JOB-055")),
            )
            .child(div().text_size(FontPx::F12.px()).text_color(p.text_primary).child("Approval Gate quick actions in left rail"))
            .child(
                div().flex().flex_row().items_center().gap(SpacePx::S8.px())
                    .child(self.render_two_step_btn(variant, "gate-approve", ActionTone::Approve, false, now, cx))
                    .child(self.render_two_step_btn(variant, "gate-rework", ActionTone::Rework, false, now, cx)),
            )
            .into_any_element()
    }

    fn render_column(&self, variant: ConfirmVariant, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let idx = variant.index();
        let is_stopped = self.stopped[idx];

        let pill = GlassSurface::popover(&p, &self.glass)
            .pad(SpacePx::S8).gap(SpacePx::S0).radius_factor(1.0)
            .border(None).bg(Some(p.glass_popover)).accent(EdgeAccent::Neutral).build()
            .child(self.render_footer_actions(variant, now, cx));

        let sheet = GlassSurface::sheet(&p, &self.glass)
            .pad(SpacePx::S0).gap(SpacePx::S0).build()
            .relative().w(px(372.0)).h(px(360.0)).overflow_hidden()
            .child(
                div().px(SpacePx::S12.px()).pt(SpacePx::S12.px()).pb(SpacePx::S4.px())
                    .flex().flex_col().gap(SpacePx::S8.px())
                    .child(
                        div().flex().flex_row().items_center().justify_between()
                            .child(div().text_size(FontPx::F13.px()).text_color(p.text_primary).child(self.ctx_kind.header_title()))
                            .child(div().text_size(FontPx::F11.px()).text_color(p.accent).child(self.ctx_kind.item_id())),
                    )
                    .child(div().text_size(FontPx::F12.px()).text_color(p.text_primary).child(self.ctx_kind.title_text()))
                    .child(div().text_size(FontPx::F11.px()).text_color(if is_stopped { p.coral } else { p.text_tertiary }).child(self.ctx_kind.stage_label(is_stopped))),
            )
            .child(
                div()
                    .id(SharedString::from(format!("{}-scroll", variant.id_prefix())))
                    .flex().flex_col().flex_1().min_h(px(0.0))
                    .overflow_y_scroll().restrict_scroll_to_axis().overflow_fade_bottom(FadePx::F24)
                    .px(SpacePx::S12.px()).pt(SpacePx::S8.px()).pb(px(64.0)).gap(SpacePx::S8.px())
                    .child(
                        GlassSurface::card(&p, &self.glass)
                            .pad(SpacePx::S12).gap(SpacePx::S4).bg(Some(p.glass_card)).accent(EdgeAccent::Neutral).build()
                            .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child("COLOR-ONLY · SMOOTH TRANSITION"))
                            .child(div().text_size(FontPx::F12.px()).text_color(p.text_secondary).child("ラベル不変・横ズレ0px。1回目で220msかけて色/光がチャージされ、2回目で実行。外側クリック・別ボタン・3秒で滑らかに解除。")),
                    ),
            )
            .child(div().absolute().left(SpacePx::S8.px()).right(SpacePx::S8.px()).bottom(SpacePx::S8.px()).child(pill));

        div()
            .flex().flex_col().gap(SpacePx::S12.px()).w(px(372.0))
            .child(
                div().flex().flex_col().gap(px(2.0)).px(SpacePx::S4.px())
                    .child(
                        div().flex().flex_row().items_center().justify_between()
                            .child(div().text_size(FontPx::F13.px()).text_color(p.text_primary).child(variant.title()))
                            .when_some(self.last_event[idx].clone(), |d, (msg, executed)| {
                                d.child(div().text_size(FontPx::F11.px()).text_color(if executed { p.green } else { p.amber }).child(msg))
                            }),
                    )
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(variant.subtitle())),
            )
            .child(sheet)
            .child(self.render_left_gate_card(variant, now, cx))
            .into_any_element()
    }
}

impl Render for ConfirmButtonDemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        if self.needs_animation_frame(now) {
            window.request_animation_frame();
        }
        let p = self.palette;
        div()
            .id("confirm-demo-backdrop")
            .relative().flex().flex_col().size_full()
            .bg(particle_wave_with(p.window_backdrop, p.wave_a, p.wave_b, self.glass.bg_brightness))
            .text_color(p.text_primary)
            .on_click(cx.listener(|this, _, _, cx| {
                this.disarm(Some("outside click"), Instant::now());
                cx.notify();
            }))
            .child(self.render_top_toolbar(cx))
            .child(
                div()
                    .flex().flex_row().items_start().justify_center().flex_1()
                    .px(SpacePx::S24.px()).pt(SpacePx::S12.px()).pb(SpacePx::S24.px()).gap(SpacePx::S24.px())
                    .child(self.render_column(ConfirmVariant::B1SolidInversion, now, cx))
                    .child(self.render_column(ConfirmVariant::B2LuminousGlass, now, cx))
                    .child(self.render_column(ConfirmVariant::B3GlassBarCountdown, now, cx)),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1240.0), px(660.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Afterlife · Two-Step Confirm Button Demo")),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(14.0))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| ConfirmButtonDemoApp::new()),
        )
        .expect("open confirm button demo window");
    });
}
