use gpui::perf_telemetry::{self, MetalFrameSample, MetricSummary, WindowDrawSample};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerfScenario {
    IdleLiquidGlass,
    IdleLiquidGlassImagesOn,
    IdleAppearanceDrawer,
    IdleWireframe,
    InteractiveChurn,
    InactivePowerSave,
}

impl PerfScenario {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IdleLiquidGlass => "idle_liquid_glass",
            Self::IdleLiquidGlassImagesOn => "idle_liquid_glass_images_on",
            Self::IdleAppearanceDrawer => "idle_appearance_drawer",
            Self::IdleWireframe => "idle_wireframe",
            Self::InteractiveChurn => "interactive_churn",
            Self::InactivePowerSave => "inactive_power_save",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "idle_liquid_glass_images_on" => Self::IdleLiquidGlassImagesOn,
            "idle_appearance_drawer" => Self::IdleAppearanceDrawer,
            "idle_wireframe" => Self::IdleWireframe,
            "interactive_churn" => Self::InteractiveChurn,
            "inactive_power_save" => Self::InactivePowerSave,
            _ => Self::IdleLiquidGlass,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PerfConfig {
    pub out_path: PathBuf,
    pub scenario: PerfScenario,
    pub warmup_ms: u64,
    pub duration_ms: u64,
}

impl PerfConfig {
    pub fn from_env() -> Option<Self> {
        let out_path = env::var_os("AFTERLIFE_PERF_OUT")?;
        let scenario = env::var("AFTERLIFE_PERF_SCENARIO")
            .map(|s| PerfScenario::from_str(&s))
            .unwrap_or(PerfScenario::IdleLiquidGlass);
        let warmup_ms = env::var("AFTERLIFE_PERF_WARMUP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2_000);
        let duration_ms = env::var("AFTERLIFE_PERF_DURATION_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8_000);
        Some(Self {
            out_path: PathBuf::from(out_path),
            scenario,
            warmup_ms,
            duration_ms,
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ProcessSnapshot {
    user_us: u64,
    sys_us: u64,
    rss_bytes: u64,
    max_rss_bytes: u64,
    minor_faults: u64,
    major_faults: u64,
    voluntary_ctx_switches: u64,
    involuntary_ctx_switches: u64,
}

#[cfg(target_os = "macos")]
fn capture_process_snapshot() -> ProcessSnapshot {
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Timeval {
        tv_sec: i64,
        tv_usec: i32,
        _pad: i32,
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Rusage {
        ru_utime: Timeval,
        ru_stime: Timeval,
        ru_maxrss: i64,
        ru_ixrss: i64,
        ru_idrss: i64,
        ru_isrss: i64,
        ru_minflt: i64,
        ru_majflt: i64,
        ru_nswap: i64,
        ru_inblock: i64,
        ru_oublock: i64,
        ru_msgsnd: i64,
        ru_msgrcv: i64,
        ru_nsignals: i64,
        ru_nvcsw: i64,
        ru_nivcsw: i64,
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time_sec: i32,
        user_time_usec: i32,
        system_time_sec: i32,
        system_time_usec: i32,
        policy: i32,
        suspend_count: i32,
    }

    const RUSAGE_SELF: i32 = 0;
    const MACH_TASK_BASIC_INFO: u32 = 20;
    const MACH_TASK_BASIC_INFO_COUNT: u32 = 12;

    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
        fn mach_task_self() -> u32;
        fn task_info(
            target_task: u32,
            flavor: u32,
            task_info_out: *mut MachTaskBasicInfo,
            task_info_out_cnt: *mut u32,
        ) -> i32;
    }

    let mut ru = Rusage::default();
    let mut info = MachTaskBasicInfo::default();
    let mut count = MACH_TASK_BASIC_INFO_COUNT;

    unsafe {
        let _ = getrusage(RUSAGE_SELF, &mut ru);
        let _ = task_info(mach_task_self(), MACH_TASK_BASIC_INFO, &mut info, &mut count);
    }

    let user_us = (ru.ru_utime.tv_sec.max(0) as u64) * 1_000_000
        + (ru.ru_utime.tv_usec.max(0) as u64);
    let sys_us = (ru.ru_stime.tv_sec.max(0) as u64) * 1_000_000
        + (ru.ru_stime.tv_usec.max(0) as u64);

    ProcessSnapshot {
        user_us,
        sys_us,
        rss_bytes: info.resident_size,
        max_rss_bytes: ru.ru_maxrss.max(0) as u64,
        minor_faults: ru.ru_minflt.max(0) as u64,
        major_faults: ru.ru_majflt.max(0) as u64,
        voluntary_ctx_switches: ru.ru_nvcsw.max(0) as u64,
        involuntary_ctx_switches: ru.ru_nivcsw.max(0) as u64,
    }
}

#[cfg(not(target_os = "macos"))]
fn capture_process_snapshot() -> ProcessSnapshot {
    ProcessSnapshot::default()
}

fn round3(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

fn round2(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 { 0.0 } else { r }
}

fn to_mb(bytes: u64) -> f64 {
    round2(bytes as f64 / (1024.0 * 1024.0))
}

fn summary_value(s: MetricSummary) -> Value {
    json!({
        "count": s.count,
        "avg": round3(s.avg),
        "p50": round3(s.p50),
        "p95": round3(s.p95),
        "p99": round3(s.p99),
        "max": round3(s.max),
    })
}

pub fn live_runtime_metrics_json() -> Value {
    let proc_snap = capture_process_snapshot();
    let live = perf_telemetry::live_snapshot();
    json!({
        "live_enabled": live.live_enabled,
        "ttff_ms": live.ttff_ms.map(round2),
        "process": {
            "rss_mb": to_mb(proc_snap.rss_bytes),
            "max_rss_mb_os": to_mb(proc_snap.max_rss_bytes),
            "cpu_user_ms": round2(proc_snap.user_us as f64 / 1000.0),
            "cpu_sys_ms": round2(proc_snap.sys_us as f64 / 1000.0),
            "minor_faults": proc_snap.minor_faults,
            "major_faults": proc_snap.major_faults,
        },
        "gpui_draw": {
            "prepaint_us": summary_value(live.gpui_prepaint_us),
            "paint_us": summary_value(live.gpui_paint_us),
            "total_us": summary_value(live.gpui_draw_total_us),
        },
        "metal": {
            "total_frames_presented": live.total_frames_presented,
            "rolling_fps": round2(live.rolling_fps),
            "viewport_px": live.viewport_px,
            "present_interval_ms": summary_value(live.present_interval_ms),
            "dropped_frames_18ms": live.dropped_frames_18ms,
            "dropped_frames_33ms": live.dropped_frames_33ms,
            "draw_total_us": summary_value(live.draw_total_us),
            "next_drawable_us": summary_value(live.next_drawable_us),
            "draw_cpu_us": summary_value(live.draw_cpu_us),
            "encode_us": summary_value(live.encode_us),
            "gpu_us": summary_value(live.gpu_us),
            "gpu_allocated_mb": round2(live.gpu_allocated_mb),
            "gpu_allocated_mb_peak": round2(live.gpu_allocated_mb_peak),
            "backdrop_images_loaded": live.backdrop_images_loaded,
            "compute_scatter_active": live.compute_scatter_active,
            "encoder_switches_avg": round2(live.encoder_switches_avg),
            "blit_count_avg": round2(live.blit_count_avg),
            "blit_full_frame_ratio_avg": round3(live.blit_full_frame_ratio_avg),
            "instance_bytes_avg": live.instance_bytes_avg.round() as u64,
        },
    })
}

struct PerfState {
    config: PerfConfig,
    boot_at: Instant,
    measure_started_at: Option<Instant>,
    start_proc: ProcessSnapshot,
    peak_rss_bytes: u64,
    finished: bool,
    last_render_at: Option<Instant>,
    render_cpu_us: Vec<u64>,
    state_clone_us: Vec<u64>,
    frame_interval_us: Vec<u64>,
}

#[derive(Clone)]
pub struct PerfSession {
    inner: Arc<Mutex<PerfState>>,
}

impl PerfSession {
    pub fn new(config: PerfConfig) -> Self {
        perf_telemetry::set_enabled(false);
        perf_telemetry::set_live_enabled(false);
        perf_telemetry::reset();
        Self {
            inner: Arc::new(Mutex::new(PerfState {
                config,
                boot_at: Instant::now(),
                measure_started_at: None,
                start_proc: ProcessSnapshot::default(),
                peak_rss_bytes: 0,
                finished: false,
                last_render_at: None,
                render_cpu_us: Vec::with_capacity(2048),
                state_clone_us: Vec::with_capacity(2048),
                frame_interval_us: Vec::with_capacity(2048),
            })),
        }
    }

    pub fn scenario(&self) -> PerfScenario {
        self.inner.lock().unwrap().config.scenario
    }

    pub fn tick(&self) -> bool {
        let mut st = self.inner.lock().unwrap();
        if st.finished {
            return true;
        }
        let now = Instant::now();
        if st.measure_started_at.is_none() {
            if now.duration_since(st.boot_at).as_millis() as u64 >= st.config.warmup_ms {
                let proc_snap = capture_process_snapshot();
                st.start_proc = proc_snap;
                st.peak_rss_bytes = proc_snap.rss_bytes;
                st.measure_started_at = Some(now);
                st.last_render_at = None;
                st.render_cpu_us.clear();
                st.state_clone_us.clear();
                st.frame_interval_us.clear();
                perf_telemetry::reset();
                perf_telemetry::set_enabled(true);
            }
            return false;
        }

        let started_at = st.measure_started_at.unwrap();
        let current_proc = capture_process_snapshot();
        if current_proc.rss_bytes > st.peak_rss_bytes {
            st.peak_rss_bytes = current_proc.rss_bytes;
        }

        let elapsed_ms = now.duration_since(started_at).as_millis() as u64;
        if elapsed_ms >= st.config.duration_ms {
            st.finished = true;
            perf_telemetry::set_enabled(false);
            let wall_ms = now.duration_since(started_at).as_secs_f64() * 1000.0;
            let ttff_ms = perf_telemetry::ttff_us().map(|us| round2(us as f64 / 1000.0));
            let metal_samples = perf_telemetry::drain_metal_samples();
            let gpu_timings_us = perf_telemetry::drain_gpu_timings_us();
            let window_draws = perf_telemetry::drain_window_draw_samples();
            let report = build_report_value(
                &st.config,
                wall_ms,
                ttff_ms,
                st.start_proc,
                current_proc,
                st.peak_rss_bytes,
                &st.render_cpu_us,
                &st.state_clone_us,
                &st.frame_interval_us,
                &window_draws,
                &metal_samples,
                &gpu_timings_us,
            );
            let json_body = serde_json::to_string_pretty(&report).unwrap_or_default() + "\n";
            if let Some(parent) = st.config.out_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(err) = fs::write(&st.config.out_path, json_body) {
                eprintln!(
                    "[afterlife-perf] failed to write {}: {}",
                    st.config.out_path.display(),
                    err
                );
            }
            return true;
        }

        false
    }

    pub fn record_render(&self, clone_us: u64, render_us: u64) {
        let mut st = self.inner.lock().unwrap();
        if st.measure_started_at.is_none() || st.finished {
            return;
        }
        let now = Instant::now();
        if let Some(prev) = st.last_render_at {
            st.frame_interval_us
                .push(now.duration_since(prev).as_micros() as u64);
        }
        st.last_render_at = Some(now);
        st.state_clone_us.push(clone_us);
        st.render_cpu_us.push(render_us);
    }
}

fn compute_stats_u64(values: &[u64], scale: f64) -> MetricSummary {
    if values.is_empty() {
        return MetricSummary::default();
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    let sum: f64 = sorted.iter().map(|&v| v as f64).sum();
    let pct = |p: f64| -> f64 {
        let idx = (((count - 1) as f64) * p).round() as usize;
        sorted[idx.min(count - 1)] as f64 * scale
    };
    MetricSummary {
        count,
        avg: (sum / count as f64) * scale,
        p50: pct(0.50),
        p95: pct(0.95),
        p99: pct(0.99),
        max: sorted[count - 1] as f64 * scale,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_report_value(
    cfg: &PerfConfig,
    wall_ms: f64,
    ttff_ms: Option<f64>,
    start_proc: ProcessSnapshot,
    end_proc: ProcessSnapshot,
    peak_rss_bytes: u64,
    render_cpu_us: &[u64],
    state_clone_us: &[u64],
    frame_interval_us: &[u64],
    window_draws: &[WindowDrawSample],
    metal_samples: &[MetalFrameSample],
    gpu_timings_us: &[u64],
) -> Value {
    let wall_sec = (wall_ms / 1000.0).max(0.001);
    let user_ms = end_proc.user_us.saturating_sub(start_proc.user_us) as f64 / 1000.0;
    let sys_ms = end_proc.sys_us.saturating_sub(start_proc.sys_us) as f64 / 1000.0;
    let cpu_total_pct = ((user_ms + sys_ms) / wall_ms.max(1.0)) * 100.0;

    let render_stats = compute_stats_u64(render_cpu_us, 1.0);
    let clone_stats = compute_stats_u64(state_clone_us, 1.0);
    let interval_ms_stats = compute_stats_u64(frame_interval_us, 0.001);

    let wd_prepaint: Vec<u64> = window_draws.iter().map(|s| s.prepaint_us).collect();
    let wd_paint: Vec<u64> = window_draws.iter().map(|s| s.paint_us).collect();
    let wd_finish: Vec<u64> = window_draws.iter().map(|s| s.finish_us).collect();
    let wd_total: Vec<u64> = window_draws.iter().map(|s| s.total_us).collect();

    let draw_total_us: Vec<u64> = metal_samples.iter().map(|s| s.draw_total_us).collect();
    let next_drawable_us: Vec<u64> = metal_samples.iter().map(|s| s.next_drawable_us).collect();
    let draw_cpu_us: Vec<u64> = metal_samples
        .iter()
        .map(|s| s.draw_total_us.saturating_sub(s.next_drawable_us))
        .collect();
    let encode_us: Vec<u64> = metal_samples.iter().map(|s| s.encode_us).collect();
    let present_intervals_us: Vec<u64> = metal_samples
        .iter()
        .map(|s| s.present_interval_us)
        .filter(|&v| v > 0)
        .collect();
    let dropped_18ms = present_intervals_us.iter().filter(|&&v| v > 18_000).count();
    let dropped_33ms = present_intervals_us.iter().filter(|&&v| v > 33_333).count();

    let m_count = metal_samples.len();
    let mf = m_count.max(1) as f64;
    let avg_enc_switches =
        metal_samples.iter().map(|s| s.encoder_switches as f64).sum::<f64>() / mf;
    let avg_blit_count = metal_samples.iter().map(|s| s.blit_count as f64).sum::<f64>() / mf;
    let avg_blit_pixels = metal_samples.iter().map(|s| s.blit_pixels as f64).sum::<f64>() / mf;
    let avg_blit_full_frames = metal_samples
        .iter()
        .map(|s| {
            let vp = (s.viewport_w as f64 * s.viewport_h as f64).max(1.0);
            s.blit_pixels as f64 / vp
        })
        .sum::<f64>()
        / mf;
    let avg_gpu_alloc_bytes = metal_samples
        .iter()
        .map(|s| s.gpu_allocated_bytes as f64)
        .sum::<f64>()
        / mf;
    let peak_gpu_alloc_bytes = metal_samples
        .iter()
        .map(|s| s.gpu_allocated_bytes)
        .max()
        .unwrap_or(0);
    let backdrop_images_loaded = metal_samples
        .iter()
        .map(|s| s.backdrop_images_loaded)
        .max()
        .unwrap_or(0);
    let compute_scatter_ratio = metal_samples
        .iter()
        .filter(|s| s.compute_scatter_dispatched)
        .count() as f64
        / mf;
    let avg_q_reg = metal_samples.iter().map(|s| s.quads_regular as f64).sum::<f64>() / mf;
    let avg_q_glass = metal_samples.iter().map(|s| s.quads_glass as f64).sum::<f64>() / mf;
    let avg_q_wave = metal_samples.iter().map(|s| s.quads_wave as f64).sum::<f64>() / mf;
    let avg_shadows = metal_samples.iter().map(|s| s.shadows as f64).sum::<f64>() / mf;
    let avg_mono = metal_samples
        .iter()
        .map(|s| s.monochrome_sprites as f64)
        .sum::<f64>()
        / mf;
    let avg_poly = metal_samples
        .iter()
        .map(|s| s.polychrome_sprites as f64)
        .sum::<f64>()
        / mf;
    let avg_paths = metal_samples.iter().map(|s| s.paths as f64).sum::<f64>() / mf;
    let avg_inst_bytes = metal_samples
        .iter()
        .map(|s| s.instance_bytes as f64)
        .sum::<f64>()
        / mf;
    let (vp_w, vp_h) = metal_samples
        .last()
        .map(|s| (s.viewport_w, s.viewport_h))
        .unwrap_or((0, 0));

    json!({
        "scenario": cfg.scenario.as_str(),
        "warmup_ms": cfg.warmup_ms,
        "duration_ms": round2(wall_ms),
        "ttff_ms": ttff_ms,
        "process": {
            "cpu_user_ms": round3(user_ms),
            "cpu_sys_ms": round3(sys_ms),
            "cpu_total_pct": round2(cpu_total_pct),
            "rss_mb_start": to_mb(start_proc.rss_bytes),
            "rss_mb_end": to_mb(end_proc.rss_bytes),
            "rss_mb_peak": to_mb(peak_rss_bytes),
            "max_rss_mb_os": to_mb(end_proc.max_rss_bytes),
            "minor_faults": end_proc.minor_faults.saturating_sub(start_proc.minor_faults),
            "major_faults": end_proc.major_faults.saturating_sub(start_proc.major_faults),
            "voluntary_ctx_switches": end_proc.voluntary_ctx_switches.saturating_sub(start_proc.voluntary_ctx_switches),
            "involuntary_ctx_switches": end_proc.involuntary_ctx_switches.saturating_sub(start_proc.involuntary_ctx_switches),
        },
        "ui": {
            "render_calls": render_cpu_us.len(),
            "render_hz": round2(render_cpu_us.len() as f64 / wall_sec),
            "render_cpu_us": summary_value(render_stats),
            "state_clone_us": summary_value(clone_stats),
            "frame_interval_ms": summary_value(interval_ms_stats),
        },
        "gpui_draw": {
            "count": window_draws.len(),
            "prepaint_us": summary_value(compute_stats_u64(&wd_prepaint, 1.0)),
            "paint_us": summary_value(compute_stats_u64(&wd_paint, 1.0)),
            "finish_us": summary_value(compute_stats_u64(&wd_finish, 1.0)),
            "total_us": summary_value(compute_stats_u64(&wd_total, 1.0)),
        },
        "metal": {
            "frames": m_count,
            "fps": round2(m_count as f64 / wall_sec),
            "viewport_px": [vp_w, vp_h],
            "present_interval_ms": summary_value(compute_stats_u64(&present_intervals_us, 0.001)),
            "dropped_frames_18ms": dropped_18ms,
            "dropped_frames_33ms": dropped_33ms,
            "draw_total_us": summary_value(compute_stats_u64(&draw_total_us, 1.0)),
            "next_drawable_us": summary_value(compute_stats_u64(&next_drawable_us, 1.0)),
            "draw_cpu_us": summary_value(compute_stats_u64(&draw_cpu_us, 1.0)),
            "encode_us": summary_value(compute_stats_u64(&encode_us, 1.0)),
            "gpu_us": summary_value(compute_stats_u64(gpu_timings_us, 1.0)),
            "gpu_allocated_mb_avg": round2(avg_gpu_alloc_bytes / (1024.0 * 1024.0)),
            "gpu_allocated_mb_peak": to_mb(peak_gpu_alloc_bytes),
            "backdrop_images_loaded": backdrop_images_loaded,
            "compute_scatter_ratio": round3(compute_scatter_ratio),
            "encoder_switches_avg": round2(avg_enc_switches),
            "blit_count_avg": round2(avg_blit_count),
            "blit_pixels_avg": avg_blit_pixels.round() as u64,
            "blit_full_frame_ratio_avg": round3(avg_blit_full_frames),
            "primitives_avg": {
                "quads_regular": round2(avg_q_reg),
                "quads_glass": round2(avg_q_glass),
                "quads_wave": round2(avg_q_wave),
                "shadows": round2(avg_shadows),
                "monochrome_sprites": round2(avg_mono),
                "polychrome_sprites": round2(avg_poly),
                "paths": round2(avg_paths),
                "instance_bytes": avg_inst_bytes.round() as u64,
            },
        },
    })
}
