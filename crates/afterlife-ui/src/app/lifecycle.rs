use std::{sync::Arc, time::Duration};

use afterlife_core::JobId;
use afterlife_daemon::AppDaemonState;
use afterlife_design::UiStyle;
use gpui::{
    App, AppContext, Application, Entity, KeyBinding, Menu, MenuItem, TitlebarOptions,
    WindowBackgroundAppearance, WindowOptions, actions, point, px,
};

use crate::{
    app::{
        state::{AppState, SharedAppState},
        window_state::{
            WindowStateStore, capture_window_state, collect_displays, resolve_initial_window,
            sanitize_detail_height,
        },
    },
    perf::{PerfConfig, PerfScenario, PerfSession},
    platform,
    views::root::AfterlifeApp,
};

actions!(afterlife, [Quit, CloseWindow]);

fn spawn_embedded_daemon(daemon: Arc<AppDaemonState>) {
    if std::env::var("AFTERLIFE_DISABLE_EMBEDDED_DAEMON").is_ok() {
        return;
    }
    let bind_addr = daemon.config.server.bind_addr.clone();
    let public_url = daemon.config.server.public_base_url.clone();
    let _ = std::thread::Builder::new()
        .name("afterlifed-embedded".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[afterlife-ui] failed to create tokio runtime: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!(
                            "[afterlife-ui] embedded afterlifed skipped ({bind_addr} already in use or unavailable: {e})"
                        );
                        return;
                    }
                };
                eprintln!(
                    "[afterlife-ui] embedded afterlifed listening on {bind_addr} (MCP: {public_url}/mcp | Metrics: {public_url}/v1/metrics)"
                );
                let router = afterlife_daemon::build_router(daemon);
                if let Err(e) = axum::serve(listener, router).await {
                    eprintln!("[afterlife-ui] embedded afterlifed stopped: {e}");
                }
            });
        });
}

pub fn open_main_window(state: &Entity<AppState>, perf: Option<PerfSession>, cx: &mut App) {
    let state_handle = state.clone();
    let is_power_save = perf
        .as_ref()
        .is_some_and(|p| p.scenario() == PerfScenario::InactivePowerSave);
    let window_store = WindowStateStore::from_env(perf.is_some());
    let saved = window_store.as_ref().and_then(WindowStateStore::load).or_else(|| {
        std::env::var("AFTERLIFE_PERF_USE_LIVE_DATA")
            .ok()
            .and_then(|_| {
                WindowStateStore::new(afterlife_protocol::default_backdrop_data_dir()).load()
            })
    });
    if let Some(ref saved_state) = saved {
        let restored_detail_h = sanitize_detail_height(saved_state.detail_height);
        state.update(cx, |st, _| {
            st.detail_height = restored_detail_h;
        });
    }
    let (displays, primary_id) = collect_displays(cx);
    let (display_id, window_bounds) = resolve_initial_window(saved.as_ref(), &displays, primary_id);
    let store_for_close = window_store.clone();
    let state_for_close = state.clone();
    cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            display_id,
            focus: !is_power_save,
            titlebar: Some(TitlebarOptions {
                title: Some("Afterlife".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.0), px(14.0))),
            }),
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        move |window, cx| {
            if let Some(store) = store_for_close {
                window.on_window_should_close(cx, move |window, cx| {
                    let detail_h = Some(state_for_close.read(cx).detail_height);
                    if let Some(snapshot) = capture_window_state(window, cx, detail_h)
                        && let Err(err) = store.save(&snapshot)
                    {
                        eprintln!("[afterlife-ui] failed to persist window state: {err}");
                    }
                    true
                });
            }
            cx.new(|cx| AfterlifeApp::new(state_handle, perf, window_store, window, cx))
        },
    )
    .unwrap();
    if !is_power_save {
        cx.activate(true);
    }
}

pub fn run_app() {
    gpui::perf_telemetry::mark_app_start();
    let perf_session = PerfConfig::from_env().map(PerfSession::new);
    if perf_session.is_none() {
        gpui::perf_telemetry::set_live_enabled(true);
    }
    let app = Application::new();
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            let shared = cx.global::<SharedAppState>();
            let state = shared.state.clone();
            let perf = shared.perf.clone();
            open_main_window(&state, perf, cx);
        } else {
            cx.activate(true);
        }
    });

    app.run(move |cx: &mut App| {
        platform::init_platform();

        let state = cx.new(|_| {
            let mut st = AppState::new();
            if let Some(ref perf) = perf_session {
                match perf.scenario() {
                    PerfScenario::IdleLiquidGlass => {}
                    PerfScenario::IdleLiquidGlassImagesOn => {
                        st.glass.bg_image_enabled = true;
                        st.refresh_palette();
                    }
                    PerfScenario::IdleAppearanceDrawer => {
                        st.settings_open = true;
                    }
                    PerfScenario::LiveDetailSheet => {
                        st.detail_open = true;
                    }
                    PerfScenario::IdleWireframe => {
                        st.style = UiStyle::Wireframe;
                        st.refresh_palette();
                    }
                    PerfScenario::InteractiveChurn => {
                        st.detail_open = true;
                        st.settings_open = true;
                    }
                    PerfScenario::InactivePowerSave => {
                        st.pause_animation_when_inactive = true;
                    }
                }
            }
            st
        });
        cx.set_global(SharedAppState {
            state: state.clone(),
            perf: perf_session.clone(),
        });

        let window_store_for_close = WindowStateStore::from_env(perf_session.is_some());
        let window_store_for_quit = window_store_for_close.clone();
        let state_for_quit = state.clone();
        let state_for_close_action = state.clone();
        cx.on_app_quit(move |cx| {
            platform::apple_music::sync_watcher_enabled(false);
            if let Some(ref store) = window_store_for_quit {
                let detail_h = Some(state_for_quit.read(cx).detail_height);
                for handle in cx.windows() {
                    let _ = handle.update(cx, |_, window, cx| {
                        if let Some(snapshot) = capture_window_state(window, cx, detail_h)
                            && let Err(err) = store.save(&snapshot)
                        {
                            eprintln!("[afterlife-ui] failed to persist window state: {err}");
                        }
                    });
                }
            }
            async {}
        })
        .detach();

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(move |_: &CloseWindow, cx| {
            if let Some(w) = cx.active_window() {
                let detail_h = Some(state_for_close_action.read(cx).detail_height);
                let _ = w.update(cx, |_, window, cx| {
                    if let Some(ref store) = window_store_for_close
                        && let Some(snapshot) = capture_window_state(window, cx, detail_h)
                        && let Err(err) = store.save(&snapshot)
                    {
                        eprintln!("[afterlife-ui] failed to persist window state: {err}");
                    }
                    window.remove_window();
                });
            }
        });
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-w", CloseWindow, None),
        ]);
        cx.set_menus(vec![
            Menu {
                name: "Afterlife".into(),
                items: vec![MenuItem::action("Quit Afterlife", Quit)],
            },
            Menu {
                name: "Window".into(),
                items: vec![MenuItem::action("Close Window", CloseWindow)],
            },
        ]);

        open_main_window(&state, perf_session.clone(), cx);

        if perf_session.is_none() {
            platform::apple_music::activate_watcher(
                state.read(cx).glass.bg_apple_music_artwork_enabled,
            );
            let daemon_opt = state.read(cx).daemon.clone();
            if let Some(daemon) = daemon_opt {
                daemon
                    .metrics
                    .set_runtime_provider(Arc::new(crate::perf::live_runtime_metrics_json));
                spawn_embedded_daemon(daemon.clone());
                let mut rx = daemon.ports.event_bus.subscribe();
                let state_for_sync = state.clone();
                cx.spawn(async move |cx| {
                    while let Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) =
                        rx.recv().await
                    {
                        cx.background_executor()
                            .timer(Duration::from_millis(33))
                            .await;
                        while rx.try_recv().is_ok() {}
                        let _ = cx.update(|cx| {
                            state_for_sync.update(cx, |st, _| {
                                st.reload_from_daemon();
                            });
                            cx.refresh_windows();
                        });
                    }
                })
                .detach();
            }

            if let Some(store) = state.read(cx).backdrop_store.clone() {
                let state_for_backdrop = state.clone();
                cx.spawn(async move |cx| {
                    let mut last_mtime = store.mtime();
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(500))
                            .await;
                        let mtime = store.mtime();
                        if mtime != last_mtime {
                            last_mtime = mtime;
                            let _ = cx.update(|cx| {
                                let changed = state_for_backdrop
                                    .update(cx, |st, _| st.reload_backdrop_tuning());
                                if changed {
                                    cx.refresh_windows();
                                }
                            });
                        }
                    }
                })
                .detach();
            }
        }

        if let Some(perf) = perf_session {
            let state_for_perf = state.clone();
            cx.spawn(async move |cx| {
                let mut step: usize = 0;
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(50))
                        .await;
                    if perf.tick() {
                        let _ = cx.update(|cx| cx.quit());
                        std::process::exit(0);
                    }
                    if perf.scenario() == PerfScenario::InteractiveChurn && step.is_multiple_of(2) {
                        let s = step / 2;
                        let _ = cx.update(|cx| {
                            state_for_perf.update(cx, |st, _| {
                                let ids = [
                                    "JOB-041", "JOB-042", "JOB-043", "JOB-044", "JOB-045",
                                    "JOB-046", "JOB-047",
                                ];
                                st.selected_job_id = JobId::from(ids[s % ids.len()]);
                                st.detail_open = !s.is_multiple_of(5);
                                st.glass.bg_brightness = 1.2 + ((s % 8) as f32) * 0.1;
                                st.refresh_palette();
                            });
                            cx.refresh_windows();
                        });
                    }
                    step = step.wrapping_add(1);
                }
            })
            .detach();
        }
    });
}
