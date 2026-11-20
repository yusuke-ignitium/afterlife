use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use afterlife_design::{SpacePx, UiStyle};
use gpui::{
    App, Background, Bounds, ClickEvent, Context, CursorStyle, Entity, InteractiveElement,
    IntoElement, MouseButton, MouseMoveEvent, ParentElement, Pixels, Render,
    StatefulInteractiveElement, Styled, Subscription, Task, Window, div, particle_wave_with,
    prelude::FluentBuilder, px,
};

use crate::{
    app::{
        lifecycle::CloseWindow,
        motion::UiMotion,
        state::{AppState, GlassParamKey, TabId},
        window_state::{WindowStateStore, capture_window_state, clamp_detail_height},
    },
    components::top_bar::render_top_bar,
    perf::PerfSession,
    views::{
        appearance_drawer::render_appearance_drawer,
        guard_view::render_guard_tab,
        templates_view::render_templates_tab,
        workspace::{districts_pane::render_right_districts_pane, left_pane::render_left_pane},
    },
};

#[derive(Clone, Copy)]
pub struct DetailResizeDrag {
    pub start_mouse_y: f32,
    pub start_height: f32,
}

const LEFT_PANE_CHROME_HEIGHT_PX: f32 = 112.0;

pub struct AfterlifeApp {
    pub state: Entity<AppState>,
    pub perf: Option<PerfSession>,
    pub motion: UiMotion,
    pub active_slider: Option<(GlassParamKey, Rc<Cell<Bounds<Pixels>>>)>,
    pub slider_bounds: Vec<(GlassParamKey, Rc<Cell<Bounds<Pixels>>>)>,
    pub detail_resize: Option<DetailResizeDrag>,
    window_store: Option<WindowStateStore>,
    _bounds_subscription: Option<Subscription>,
    window_save_task: Option<Task<()>>,
}

impl AfterlifeApp {
    pub fn new(
        state: Entity<AppState>,
        perf: Option<PerfSession>,
        window_store: Option<WindowStateStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let slider_bounds = GlassParamKey::ALL
            .iter()
            .map(|&k| (k, Rc::new(Cell::new(Bounds::default()))))
            .collect();
        let bounds_subscription = window_store.clone().map(|store| {
            cx.observe_window_bounds(window, move |this, window, cx| {
                let detail_h = Some(this.state.read(cx).detail_height);
                let Some(snapshot) = capture_window_state(window, cx, detail_h) else {
                    return;
                };
                let store_for_task = store.clone();
                this.window_save_task = Some(cx.spawn(async move |_, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    if let Err(err) = store_for_task.save(&snapshot) {
                        eprintln!("[afterlife-ui] failed to persist window state: {err}");
                    }
                }));
            })
        });
        Self {
            state,
            perf,
            motion: UiMotion::new(),
            active_slider: None,
            slider_bounds,
            detail_resize: None,
            window_store,
            _bounds_subscription: bounds_subscription,
            window_save_task: None,
        }
    }

    pub fn persist_window_state(&self, window: &Window, cx: &App) {
        let Some(ref store) = self.window_store else {
            return;
        };
        let detail_h = Some(self.state.read(cx).detail_height);
        if let Some(snapshot) = capture_window_state(window, cx, detail_h)
            && let Err(err) = store.save(&snapshot)
        {
            eprintln!("[afterlife-ui] failed to persist window state: {err}");
        }
    }

    pub fn apply_slider_at(
        &mut self,
        key: GlassParamKey,
        bounds: &Bounds<Pixels>,
        mouse_x: Pixels,
        cx: &mut Context<Self>,
    ) {
        let w = f32::from(bounds.size.width).max(1.0);
        let rel_x = (f32::from(mouse_x) - f32::from(bounds.origin.x)).clamp(0.0, w);
        let t = (rel_x / w).clamp(0.0, 1.0);
        let (_, min, max, _) = key.spec();
        self.state.update(cx, |st, _| {
            key.set(&mut st.glass, min + t * (max - min));
            st.refresh_palette();
        });
    }

    fn finish_active_slider(&mut self, cx: &mut Context<Self>) {
        if self.active_slider.take().is_some() {
            self.state.read(cx).persist_appearance();
            cx.notify();
        }
    }

    fn finish_detail_resize(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.detail_resize.take().is_some() {
            self.persist_window_state(window, cx);
            cx.notify();
        }
    }
}

impl Render for AfterlifeApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let render_start = self.perf.as_ref().map(|_| Instant::now());
        let st = self.state.read(cx).clone();
        let clone_us = render_start
            .map(|s| s.elapsed().as_micros() as u64)
            .unwrap_or(0);
        let p = st.palette;

        let motion = self.motion.sync(&st, Instant::now());
        if motion.animating {
            window.request_animation_frame();
        }

        if st.style == UiStyle::LiquidGlass {
            let is_power_save = self
                .perf
                .as_ref()
                .is_some_and(|p| p.scenario() == crate::perf::PerfScenario::InactivePowerSave);
            if is_power_save {
                window.set_continuous_present(None, false);
            } else {
                let present_when_inactive =
                    self.perf.is_some() || !st.pause_animation_when_inactive;
                window
                    .set_continuous_present(Some(Duration::from_millis(14)), present_when_inactive);
            }
        } else {
            window.set_continuous_present(None, false);
        }

        let root_bg: Background = match st.style {
            UiStyle::LiquidGlass => particle_wave_with(
                p.window_backdrop,
                p.wave_a,
                p.wave_b,
                st.glass.bg_brightness,
            ),
            UiStyle::Wireframe => p.window_backdrop.into(),
        };
        let viewport_size = window.viewport_size();
        let window_width = f32::from(viewport_size.width);
        let window_height = f32::from(viewport_size.height);
        let left_pane_height = (window_height - LEFT_PANE_CHROME_HEIGHT_PX).max(300.0);
        let is_resizing_detail = self.detail_resize.is_some();
        let tab_opacity = 0.55 + 0.45 * motion.tab_progress;
        let tab_offset = (1.0 - motion.tab_progress) * 4.0;

        let root = div()
            .id("afterlife-root")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(root_bg)
            .text_color(p.text_primary)
            .pt(px(40.0))
            .px(SpacePx::S16.px())
            .pb(SpacePx::S16.px())
            .gap(SpacePx::S12.px())
            .when(is_resizing_detail, |d| d.cursor(CursorStyle::ResizeUpDown))
            .on_action(|_: &CloseWindow, window, _| {
                window.remove_window();
            })
            .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, window, cx| {
                if ev.dragging() {
                    if let Some((key, cell)) = this.active_slider.clone() {
                        let b = cell.get();
                        this.apply_slider_at(key, &b, ev.position.x, cx);
                        cx.notify();
                    }
                    if let Some(drag) = this.detail_resize {
                        let delta = drag.start_mouse_y - f32::from(ev.position.y);
                        let next_h =
                            clamp_detail_height(drag.start_height + delta, left_pane_height);
                        this.state.update(cx, |st, _| {
                            st.detail_height = next_h;
                        });
                        cx.notify();
                    }
                } else {
                    this.finish_active_slider(cx);
                    this.finish_detail_resize(window, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.finish_active_slider(cx);
                    this.finish_detail_resize(window, cx);
                }),
            )
            .child(
                div()
                    .id("window-titlebar-hitbox")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(40.0))
                    .on_click(|ev: &ClickEvent, window, _| {
                        if ev.click_count() == 2 {
                            window.zoom_window();
                        }
                    }),
            )
            .child(render_top_bar(&st, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .pt(px(tab_offset))
                    .opacity(tab_opacity)
                    .child(match st.active_tab {
                        TabId::Workspace => div()
                            .flex()
                            .flex_row()
                            .flex_1()
                            .min_h(px(0.0))
                            .items_start()
                            .gap(SpacePx::S24.px())
                            .child(render_left_pane(
                                &st,
                                &motion,
                                left_pane_height,
                                is_resizing_detail,
                                cx,
                            ))
                            .child(render_right_districts_pane(&st, &motion, window_width, cx))
                            .into_any_element(),
                        TabId::Templates => div()
                            .id("templates-scroll")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .restrict_scroll_to_axis()
                            .child(render_templates_tab(&st))
                            .into_any_element(),
                        TabId::Guard => div()
                            .id("guard-scroll")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .restrict_scroll_to_axis()
                            .child(render_guard_tab(&st))
                            .into_any_element(),
                    }),
            )
            .when(motion.drawer_progress > 0.005, |d| {
                let drawer_t = motion.drawer_progress;
                d.when(st.settings_open, |d| {
                    d.child(
                        div()
                            .id("appearance-drawer-backdrop")
                            .absolute()
                            .inset_0()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.state.update(cx, |st, _| {
                                        st.settings_open = false;
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(|this, _, _, cx| {
                                    this.state.update(cx, |st, _| {
                                        st.settings_open = false;
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .top(px(96.0 - (1.0 - drawer_t) * 8.0))
                        .right(SpacePx::S16.px())
                        .opacity(drawer_t)
                        .child(render_appearance_drawer(
                            self,
                            &st,
                            &motion,
                            window_height,
                            cx,
                        )),
                )
            });

        if let (Some(perf), Some(start)) = (&self.perf, render_start) {
            perf.record_render(clone_us, start.elapsed().as_micros() as u64);
        }

        root
    }
}
