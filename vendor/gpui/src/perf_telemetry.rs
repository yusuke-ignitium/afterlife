//! Lightweight runtime performance telemetry hooks for GPUI's window & Metal renderer.
//!
//! When both benchmark recording (`set_enabled`) and production live telemetry
//! (`set_live_enabled`) are disabled, all recording hooks short-circuit on a single
//! relaxed atomic load so normal application rendering incurs no overhead.

use parking_lot::Mutex;
use std::{
    collections::VecDeque,
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

static BENCH_ENABLED: AtomicBool = AtomicBool::new(false);
static LIVE_ENABLED: AtomicBool = AtomicBool::new(false);
static FIRST_FRAME_RECORDED: AtomicBool = AtomicBool::new(false);
const LIVE_RING_CAP: usize = 120;

/// Per-frame telemetry captured inside GPUI's macOS Metal renderer.
#[derive(Clone, Debug, Default)]
pub struct MetalFrameSample {
    /// Monotonic frame sequence number within the current telemetry session.
    pub seq: u64,
    /// Viewport width in device pixels.
    pub viewport_w: u32,
    /// Viewport height in device pixels.
    pub viewport_h: u32,
    /// Total CPU time spent in `MetalRenderer::draw` (microseconds), including `next_drawable` wait.
    pub draw_total_us: u64,
    /// Time spent waiting in `CAMetalLayer::next_drawable()` (microseconds).
    pub next_drawable_us: u64,
    /// CPU time spent encoding commands in `draw_primitives` (microseconds).
    pub encode_us: u64,
    /// Wall-clock interval since the previous Metal frame submission (microseconds).
    pub present_interval_us: u64,
    /// Resident GPU memory reported by `MTLDevice.currentAllocatedSize` (bytes).
    pub gpu_allocated_bytes: u64,
    /// Number of backdrop PNG textures currently resident on the GPU.
    pub backdrop_images_loaded: u32,
    /// Whether the particle tile-grid compute scatter pass ran on this frame.
    pub compute_scatter_dispatched: bool,
    /// Number of Metal render command encoders created in this frame.
    pub encoder_switches: u32,
    /// Number of backdrop texture blit copies performed in this frame.
    pub blit_count: u32,
    /// Total pixels copied into the backdrop texture in this frame.
    pub blit_pixels: u64,
    /// Count of standard non-glass quads drawn.
    pub quads_regular: u32,
    /// Count of Liquid Glass quads drawn.
    pub quads_glass: u32,
    /// Count of Particle Wave background quads drawn.
    pub quads_wave: u32,
    /// Count of shadow primitives drawn.
    pub shadows: u32,
    /// Count of monochrome sprite primitives (glyphs/icons) drawn.
    pub monochrome_sprites: u32,
    /// Count of polychrome sprite primitives (images) drawn.
    pub polychrome_sprites: u32,
    /// Count of vector path primitives drawn.
    pub paths: u32,
    /// Total instance buffer bytes consumed by this frame.
    pub instance_bytes: u64,
}

/// Per-frame GPUI layout, prepaint, and paint breakdown from `Window::draw`.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowDrawSample {
    /// Time spent in Taffy layout, prepaint, and text shaping (microseconds).
    pub prepaint_us: u64,
    /// Time spent painting elements into the scene (microseconds).
    pub paint_us: u64,
    /// Time spent finalizing text system, frame swap, and focus tracking (microseconds).
    pub finish_us: u64,
    /// Total CPU duration of `Window::draw` (microseconds).
    pub total_us: u64,
}

/// Summary percentiles for a rolling metric.
#[derive(Clone, Copy, Debug, Default)]
pub struct MetricSummary {
    /// Sample count included in the summary.
    pub count: usize,
    /// Arithmetic mean of samples.
    pub avg: f64,
    /// Median (50th percentile) value.
    pub p50: f64,
    /// 95th percentile value.
    pub p95: f64,
    /// 99th percentile value.
    pub p99: f64,
    /// Maximum observed value.
    pub max: f64,
}

/// Rolling live performance snapshot suitable for exposing via `/v1/metrics`.
#[derive(Clone, Debug, Default)]
pub struct LivePerfSnapshot {
    /// Whether always-on rolling live telemetry is enabled.
    pub live_enabled: bool,
    /// Monotonic count of Metal frames presented since launch.
    pub total_frames_presented: u64,
    /// Time from application start to first submitted Metal frame (milliseconds).
    pub ttff_ms: Option<f64>,
    /// Current drawable viewport dimensions in device pixels.
    pub viewport_px: [u32; 2],
    /// Rolling frames per second estimated from recent present intervals.
    pub rolling_fps: f64,
    /// Rolling Metal frame-to-frame present interval summary (milliseconds).
    pub present_interval_ms: MetricSummary,
    /// Cumulative count of frame intervals exceeding 18 ms.
    pub dropped_frames_18ms: u64,
    /// Cumulative count of frame intervals exceeding 33.3 ms.
    pub dropped_frames_33ms: u64,
    /// Rolling total CPU duration of `MetalRenderer::draw` including drawable wait (microseconds).
    pub draw_total_us: MetricSummary,
    /// Rolling `CAMetalLayer::next_drawable()` wait duration (microseconds).
    pub next_drawable_us: MetricSummary,
    /// Rolling CPU draw duration excluding `next_drawable()` wait (microseconds).
    pub draw_cpu_us: MetricSummary,
    /// Rolling Metal command encoding duration (microseconds).
    pub encode_us: MetricSummary,
    /// Rolling hardware GPU command buffer execution duration (microseconds).
    pub gpu_us: MetricSummary,
    /// Rolling GPUI layout and prepaint duration (microseconds).
    pub gpui_prepaint_us: MetricSummary,
    /// Rolling GPUI element paint duration (microseconds).
    pub gpui_paint_us: MetricSummary,
    /// Rolling total GPUI `Window::draw` duration (microseconds).
    pub gpui_draw_total_us: MetricSummary,
    /// Latest resident Metal GPU memory reported by `currentAllocatedSize` (MiB).
    pub gpu_allocated_mb: f64,
    /// Peak resident Metal GPU memory observed while live telemetry was enabled (MiB).
    pub gpu_allocated_mb_peak: f64,
    /// Number of backdrop PNG textures currently resident on the GPU.
    pub backdrop_images_loaded: u32,
    /// Whether the most recent frame dispatched the particle tile-grid compute scatter pass.
    pub compute_scatter_active: bool,
    /// Rolling average Metal render encoder switches per frame.
    pub encoder_switches_avg: f64,
    /// Rolling average backdrop texture blits per frame.
    pub blit_count_avg: f64,
    /// Rolling average ratio of blitted pixels to full viewport pixels.
    pub blit_full_frame_ratio_avg: f64,
    /// Rolling average instance buffer bytes uploaded per frame.
    pub instance_bytes_avg: f64,
}

struct PerfCollector {
    app_started_at: Instant,
    ttff_us: Option<u64>,
    next_seq: u64,
    total_frames_presented: u64,
    last_bench_frame_at: Option<Instant>,
    last_live_frame_at: Option<Instant>,
    metal_frames: Vec<MetalFrameSample>,
    gpu_timings_us: Vec<u64>,
    window_draws: Vec<WindowDrawSample>,
    live_metal: VecDeque<MetalFrameSample>,
    live_gpu_us: VecDeque<u64>,
    live_window_draws: VecDeque<WindowDrawSample>,
    live_gpu_allocated_peak_bytes: u64,
    live_dropped_18ms: u64,
    live_dropped_33ms: u64,
}

impl Default for PerfCollector {
    fn default() -> Self {
        Self {
            app_started_at: Instant::now(),
            ttff_us: None,
            next_seq: 0,
            total_frames_presented: 0,
            last_bench_frame_at: None,
            last_live_frame_at: None,
            metal_frames: Vec::new(),
            gpu_timings_us: Vec::new(),
            window_draws: Vec::new(),
            live_metal: VecDeque::with_capacity(LIVE_RING_CAP),
            live_gpu_us: VecDeque::with_capacity(LIVE_RING_CAP),
            live_window_draws: VecDeque::with_capacity(LIVE_RING_CAP),
            live_gpu_allocated_peak_bytes: 0,
            live_dropped_18ms: 0,
            live_dropped_33ms: 0,
        }
    }
}

static COLLECTOR: LazyLock<Mutex<PerfCollector>> =
    LazyLock::new(|| Mutex::new(PerfCollector::default()));

/// Record the application boot timestamp for Time-to-First-Frame (TTFF) calculation.
pub fn mark_app_start() {
    FIRST_FRAME_RECORDED.store(false, Ordering::Release);
    let mut guard = COLLECTOR.lock();
    guard.app_started_at = Instant::now();
    guard.ttff_us = None;
}

/// Record Time-to-First-Frame once when the very first Metal frame is submitted.
#[inline]
pub fn mark_first_frame() {
    if !FIRST_FRAME_RECORDED.load(Ordering::Relaxed)
        && !FIRST_FRAME_RECORDED.swap(true, Ordering::AcqRel)
    {
        let now = Instant::now();
        let mut guard = COLLECTOR.lock();
        if guard.ttff_us.is_none() {
            guard.ttff_us = Some(now.duration_since(guard.app_started_at).as_micros() as u64);
        }
    }
}

/// Returns Time-to-First-Frame in microseconds once the first Metal frame has been submitted.
pub fn ttff_us() -> Option<u64> {
    COLLECTOR.lock().ttff_us
}

/// Enable or disable full benchmark sample recording.
pub fn set_enabled(enabled: bool) {
    BENCH_ENABLED.store(enabled, Ordering::Release);
}

/// Enable or disable always-on rolling live telemetry for production metrics endpoints.
pub fn set_live_enabled(enabled: bool) {
    LIVE_ENABLED.store(enabled, Ordering::Release);
}

/// Returns `true` if benchmark sample recording is currently enabled.
#[inline]
pub fn is_enabled() -> bool {
    BENCH_ENABLED.load(Ordering::Relaxed)
}

/// Returns `true` if either benchmark recording or live ring telemetry is enabled.
#[inline]
pub fn is_any_enabled() -> bool {
    BENCH_ENABLED.load(Ordering::Relaxed) || LIVE_ENABLED.load(Ordering::Relaxed)
}

/// Clear accumulated benchmark samples and reset the sequence counter.
pub fn reset() {
    let mut guard = COLLECTOR.lock();
    guard.next_seq = 0;
    guard.last_bench_frame_at = None;
    guard.metal_frames.clear();
    guard.gpu_timings_us.clear();
    guard.window_draws.clear();
}

/// Record a completed CPU-side Metal frame submission sample.
pub fn record_metal_frame(mut sample: MetalFrameSample) {
    let bench = BENCH_ENABLED.load(Ordering::Relaxed);
    let live = LIVE_ENABLED.load(Ordering::Relaxed);
    let now = Instant::now();
    let mut guard = COLLECTOR.lock();
    if guard.ttff_us.is_none() {
        guard.ttff_us = Some(now.duration_since(guard.app_started_at).as_micros() as u64);
    }
    guard.total_frames_presented = guard.total_frames_presented.saturating_add(1);
    if !bench && !live {
        return;
    }

    if live {
        let mut live_sample = sample.clone();
        live_sample.present_interval_us = guard
            .last_live_frame_at
            .map(|prev| now.duration_since(prev).as_micros() as u64)
            .unwrap_or(0);
        guard.last_live_frame_at = Some(now);
        if live_sample.present_interval_us > 18_000 {
            guard.live_dropped_18ms = guard.live_dropped_18ms.saturating_add(1);
        }
        if live_sample.present_interval_us > 33_333 {
            guard.live_dropped_33ms = guard.live_dropped_33ms.saturating_add(1);
        }
        if live_sample.gpu_allocated_bytes > guard.live_gpu_allocated_peak_bytes {
            guard.live_gpu_allocated_peak_bytes = live_sample.gpu_allocated_bytes;
        }
        if guard.live_metal.len() >= LIVE_RING_CAP {
            guard.live_metal.pop_front();
        }
        guard.live_metal.push_back(live_sample);
    }

    if bench {
        sample.seq = guard.next_seq;
        guard.next_seq += 1;
        sample.present_interval_us = guard
            .last_bench_frame_at
            .map(|prev| now.duration_since(prev).as_micros() as u64)
            .unwrap_or(0);
        guard.last_bench_frame_at = Some(now);
        if guard.metal_frames.len() >= 16_384 {
            guard.metal_frames.drain(0..4_096);
        }
        guard.metal_frames.push(sample);
    }
}

/// Record a hardware GPU execution duration reported by Metal's completion handler.
pub fn record_gpu_timing_us(duration_us: u64) {
    let bench = BENCH_ENABLED.load(Ordering::Relaxed);
    let live = LIVE_ENABLED.load(Ordering::Relaxed);
    if !bench && !live {
        return;
    }
    let mut guard = COLLECTOR.lock();
    if live {
        if guard.live_gpu_us.len() >= LIVE_RING_CAP {
            guard.live_gpu_us.pop_front();
        }
        guard.live_gpu_us.push_back(duration_us);
    }
    if bench {
        if guard.gpu_timings_us.len() >= 16_384 {
            guard.gpu_timings_us.drain(0..4_096);
        }
        guard.gpu_timings_us.push(duration_us);
    }
}

/// Record a GPUI `Window::draw` layout/prepaint/paint breakdown sample.
pub fn record_window_draw(sample: WindowDrawSample) {
    let bench = BENCH_ENABLED.load(Ordering::Relaxed);
    let live = LIVE_ENABLED.load(Ordering::Relaxed);
    if !bench && !live {
        return;
    }
    let mut guard = COLLECTOR.lock();
    if live {
        if guard.live_window_draws.len() >= LIVE_RING_CAP {
            guard.live_window_draws.pop_front();
        }
        guard.live_window_draws.push_back(sample);
    }
    if bench {
        if guard.window_draws.len() >= 16_384 {
            guard.window_draws.drain(0..4_096);
        }
        guard.window_draws.push(sample);
    }
}

/// Drain and return all accumulated CPU-side Metal frame samples.
pub fn drain_metal_samples() -> Vec<MetalFrameSample> {
    let mut guard = COLLECTOR.lock();
    std::mem::take(&mut guard.metal_frames)
}

/// Drain and return all accumulated hardware GPU execution timings (microseconds).
pub fn drain_gpu_timings_us() -> Vec<u64> {
    let mut guard = COLLECTOR.lock();
    std::mem::take(&mut guard.gpu_timings_us)
}

/// Drain and return all accumulated GPUI `Window::draw` breakdown samples.
pub fn drain_window_draw_samples() -> Vec<WindowDrawSample> {
    let mut guard = COLLECTOR.lock();
    std::mem::take(&mut guard.window_draws)
}

fn summarize_u64(values: &[u64], scale: f64) -> MetricSummary {
    if values.is_empty() {
        return MetricSummary::default();
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    let sum: f64 = sorted.iter().map(|&v| v as f64).sum();
    let pick = |p: f64| -> f64 {
        let idx = (((count - 1) as f64) * p).round() as usize;
        sorted[idx.min(count - 1)] as f64 * scale
    };
    MetricSummary {
        count,
        avg: (sum / count as f64) * scale,
        p50: pick(0.50),
        p95: pick(0.95),
        p99: pick(0.99),
        max: sorted[count - 1] as f64 * scale,
    }
}

/// Build a rolling snapshot of live UI/GPUI/Metal telemetry.
pub fn live_snapshot() -> LivePerfSnapshot {
    let guard = COLLECTOR.lock();
    let live_enabled = LIVE_ENABLED.load(Ordering::Relaxed);
    let m_count = guard.live_metal.len();
    let mf = m_count.max(1) as f64;

    let intervals_us: Vec<u64> = guard
        .live_metal
        .iter()
        .map(|s| s.present_interval_us)
        .filter(|&v| v > 0)
        .collect();
    let present_interval_ms = summarize_u64(&intervals_us, 0.001);
    let rolling_fps = if present_interval_ms.avg > 0.01 {
        1000.0 / present_interval_ms.avg
    } else {
        0.0
    };

    let draw_total: Vec<u64> = guard.live_metal.iter().map(|s| s.draw_total_us).collect();
    let next_drawable: Vec<u64> = guard
        .live_metal
        .iter()
        .map(|s| s.next_drawable_us)
        .collect();
    let draw_cpu: Vec<u64> = guard
        .live_metal
        .iter()
        .map(|s| s.draw_total_us.saturating_sub(s.next_drawable_us))
        .collect();
    let encode: Vec<u64> = guard.live_metal.iter().map(|s| s.encode_us).collect();
    let gpu: Vec<u64> = guard.live_gpu_us.iter().copied().collect();

    let prepaint: Vec<u64> = guard
        .live_window_draws
        .iter()
        .map(|s| s.prepaint_us)
        .collect();
    let paint: Vec<u64> = guard.live_window_draws.iter().map(|s| s.paint_us).collect();
    let draw_full: Vec<u64> = guard.live_window_draws.iter().map(|s| s.total_us).collect();

    let last = guard.live_metal.back();
    let viewport_px = last.map(|s| [s.viewport_w, s.viewport_h]).unwrap_or([0, 0]);
    let gpu_allocated_mb = last
        .map(|s| s.gpu_allocated_bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);
    let gpu_allocated_mb_peak = guard.live_gpu_allocated_peak_bytes as f64 / (1024.0 * 1024.0);
    let backdrop_images_loaded = last.map(|s| s.backdrop_images_loaded).unwrap_or(0);
    let compute_scatter_active = last.map(|s| s.compute_scatter_dispatched).unwrap_or(false);

    let encoder_switches_avg = guard
        .live_metal
        .iter()
        .map(|s| s.encoder_switches as f64)
        .sum::<f64>()
        / mf;
    let blit_count_avg = guard
        .live_metal
        .iter()
        .map(|s| s.blit_count as f64)
        .sum::<f64>()
        / mf;
    let blit_full_frame_ratio_avg = guard
        .live_metal
        .iter()
        .map(|s| {
            let vp = (s.viewport_w as f64 * s.viewport_h as f64).max(1.0);
            s.blit_pixels as f64 / vp
        })
        .sum::<f64>()
        / mf;
    let instance_bytes_avg = guard
        .live_metal
        .iter()
        .map(|s| s.instance_bytes as f64)
        .sum::<f64>()
        / mf;

    LivePerfSnapshot {
        live_enabled,
        total_frames_presented: guard.total_frames_presented,
        ttff_ms: guard.ttff_us.map(|us| us as f64 / 1000.0),
        viewport_px,
        rolling_fps,
        present_interval_ms,
        dropped_frames_18ms: guard.live_dropped_18ms,
        dropped_frames_33ms: guard.live_dropped_33ms,
        draw_total_us: summarize_u64(&draw_total, 1.0),
        next_drawable_us: summarize_u64(&next_drawable, 1.0),
        draw_cpu_us: summarize_u64(&draw_cpu, 1.0),
        encode_us: summarize_u64(&encode, 1.0),
        gpu_us: summarize_u64(&gpu, 1.0),
        gpui_prepaint_us: summarize_u64(&prepaint, 1.0),
        gpui_paint_us: summarize_u64(&paint, 1.0),
        gpui_draw_total_us: summarize_u64(&draw_full, 1.0),
        gpu_allocated_mb,
        gpu_allocated_mb_peak,
        backdrop_images_loaded,
        compute_scatter_active,
        encoder_switches_avg,
        blit_count_avg,
        blit_full_frame_ratio_avg,
        instance_bytes_avg,
    }
}
