use super::metal_atlas::MetalAtlas;
use crate::{
    AtlasTextureId, Background, BackgroundTag, Bounds, ContentMask, DevicePixels, MonochromeSprite,
    PaintSurface, Path, Point, PolychromeSprite, PrimitiveBatch, Quad, ScaledPixels, Scene, Shadow,
    Size, Surface, Underline, point, size,
};
use anyhow::Result;
use block::ConcreteBlock;
use cocoa::{
    base::{NO, YES},
    foundation::{NSSize, NSUInteger},
    quartzcore::AutoresizingMask,
};

use core_foundation::base::TCFType;
use core_video::{
    metal_texture::CVMetalTextureGetTexture, metal_texture_cache::CVMetalTextureCache,
    pixel_buffer::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
};
use foreign_types::{ForeignType, ForeignTypeRef};
use metal::{
    CAMetalLayer, CommandQueue, MTLPixelFormat, MTLResourceOptions, NSRange,
    RenderPassColorAttachmentDescriptorRef,
};
use objc::{self, class, msg_send, sel, sel_impl};
use parking_lot::Mutex;

use std::{
    cell::Cell,
    ffi::c_void,
    mem, ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

// Exported to metal
pub(crate) type PointF = crate::Point<f32>;

#[cfg(not(feature = "runtime_shaders"))]
const SHADERS_METALLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shaders.metallib"));
#[cfg(feature = "runtime_shaders")]
const SHADERS_SOURCE_FILE: &str = include_str!(concat!(env!("OUT_DIR"), "/stitched_shaders.metal"));
// Use 4x MSAA, all devices support it.
// https://developer.apple.com/documentation/metal/mtldevice/1433355-supportstexturesamplecount
const PATH_SAMPLE_COUNT: u32 = 4;

pub type Context = Arc<Mutex<InstanceBufferPool>>;
pub type Renderer = MetalRenderer;

pub unsafe fn new_renderer(
    context: self::Context,
    _native_window: *mut c_void,
    _native_view: *mut c_void,
    _bounds: crate::Size<f32>,
    _transparent: bool,
) -> Renderer {
    MetalRenderer::new(context)
}

pub(crate) struct InstanceBufferPool {
    buffer_size: usize,
    buffers: Vec<metal::Buffer>,
}

impl Default for InstanceBufferPool {
    fn default() -> Self {
        Self {
            buffer_size: 2 * 1024 * 1024,
            buffers: Vec::new(),
        }
    }
}

pub(crate) struct InstanceBuffer {
    metal_buffer: metal::Buffer,
    size: usize,
}

impl InstanceBufferPool {
    pub(crate) fn reset(&mut self, buffer_size: usize) {
        self.buffer_size = buffer_size;
        self.buffers.clear();
    }

    pub(crate) fn acquire(&mut self, device: &metal::Device) -> InstanceBuffer {
        let storage_mode = if device.has_unified_memory() {
            MTLResourceOptions::StorageModeShared
        } else {
            MTLResourceOptions::StorageModeManaged
        };
        let buffer = self
            .buffers
            .pop()
            .unwrap_or_else(|| device.new_buffer(self.buffer_size as u64, storage_mode));
        InstanceBuffer {
            metal_buffer: buffer,
            size: self.buffer_size,
        }
    }

    pub(crate) fn release(&mut self, buffer: InstanceBuffer) {
        if buffer.size == self.buffer_size {
            self.buffers.push(buffer.metal_buffer)
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ParticleWaveUniforms {
    time_sec: f32,
    has_image: f32,
    image_w: f32,
    image_h: f32,
    phase_mode: f32,
    phase_progress: f32,
    grid_w: u32,
    grid_h: u32,
    viewport_w: f32,
    viewport_h: f32,
    tile_size: f32,
    fit_cover: f32,
    image_brightness: f32,
    image_peak_luminance: f32,
    image_saturation: f32,
    _pad0: f32,
}

impl Default for ParticleWaveUniforms {
    fn default() -> Self {
        let tuning = crate::particle_wave_tuning();
        Self {
            time_sec: 0.0,
            has_image: 0.0,
            image_w: 1.0,
            image_h: 1.0,
            phase_mode: 0.0,
            phase_progress: 0.0,
            grid_w: 1,
            grid_h: 1,
            viewport_w: 1.0,
            viewport_h: 1.0,
            tile_size: tuning.tile_size_px,
            fit_cover: 0.0,
            image_brightness: tuning.image_brightness,
            image_peak_luminance: tuning.image_peak_luminance,
            image_saturation: tuning.image_saturation,
            _pad0: 0.0,
        }
    }
}

fn create_rgba8_texture(
    device: &metal::Device,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> metal::Texture {
    let desc = metal::TextureDescriptor::new();
    desc.set_texture_type(metal::MTLTextureType::D2);
    desc.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
    desc.set_width(width.max(1) as u64);
    desc.set_height(height.max(1) as u64);
    desc.set_usage(metal::MTLTextureUsage::ShaderRead);
    desc.set_storage_mode(metal::MTLStorageMode::Shared);
    let tex = device.new_texture(&desc);
    let region = metal::MTLRegion {
        origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
        size: metal::MTLSize {
            width: width.max(1) as u64,
            height: height.max(1) as u64,
            depth: 1,
        },
    };
    tex.replace_region(
        region,
        0,
        rgba.as_ptr() as *const c_void,
        (width.max(1) as u64) * 4,
    );
    tex
}

fn resolve_backdrop_config_path() -> Option<std::path::PathBuf> {
    if std::env::var("AFTERLIFE_BACKDROP_IMAGES")
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        return None;
    }
    if let Ok(custom) = std::env::var("AFTERLIFE_DATA_DIR") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Some(std::path::PathBuf::from(trimmed).join("backdrop.json"));
        }
    }
    std::env::var("HOME")
        .ok()
        .map(|home| std::path::PathBuf::from(home).join(".afterlife/backdrop.json"))
}

fn read_backdrop_paths_from_config(path: &std::path::Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    val.get("images")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    item.get("path")
                        .and_then(|p| p.as_str())
                        .map(str::to_string)
                })
                .filter(|s| !s.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

type BackdropImageEntryKey = (String, Option<std::time::SystemTime>);

fn snapshot_backdrop_entry_keys(paths: &[String]) -> Vec<BackdropImageEntryKey> {
    paths
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mtime = std::fs::metadata(p).ok().and_then(|m| m.modified().ok());
            (p.to_string(), mtime)
        })
        .collect()
}

fn load_single_backdrop_image(
    device: &metal::Device,
    path: &str,
) -> Option<(metal::Texture, [f32; 3])> {
    let img = image::open(path).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let corners_opaque = [
        rgba.get_pixel(0, 0)[3],
        rgba.get_pixel(w - 1, 0)[3],
        rgba.get_pixel(0, h - 1)[3],
        rgba.get_pixel(w - 1, h - 1)[3],
    ]
    .into_iter()
    .all(|a| a >= 250);
    let fit_cover = if corners_opaque { 1.0 } else { 0.0 };
    Some((
        create_rgba8_texture(device, w, h, rgba.as_raw()),
        [w as f32, h as f32, fit_cover],
    ))
}

fn load_backdrop_images_from_entries(
    device: &metal::Device,
    entries: &[BackdropImageEntryKey],
) -> (Vec<metal::Texture>, Vec<[f32; 3]>) {
    let mut textures = Vec::new();
    let mut sizes = Vec::new();
    for (entry, _) in entries {
        if let Some((tex, dims)) = load_single_backdrop_image(device, entry) {
            textures.push(tex);
            sizes.push(dims);
        }
    }
    (textures, sizes)
}

fn load_backdrop_images(
    device: &metal::Device,
) -> (
    Vec<metal::Texture>,
    Vec<[f32; 3]>,
    Vec<BackdropImageEntryKey>,
    Option<std::path::PathBuf>,
    Option<std::time::SystemTime>,
) {
    if let Ok(raw) = std::env::var("AFTERLIFE_BACKDROP_IMAGES") {
        let paths: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if !paths.is_empty() {
            let entries = snapshot_backdrop_entry_keys(&paths);
            let (textures, sizes) = load_backdrop_images_from_entries(device, &entries);
            return (textures, sizes, entries, None, None);
        }
    }
    let config_path = resolve_backdrop_config_path();
    let mtime = config_path
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok()?.modified().ok());
    let paths = config_path
        .as_deref()
        .map(read_backdrop_paths_from_config)
        .unwrap_or_default();
    let entries = snapshot_backdrop_entry_keys(&paths);
    let (textures, sizes) = load_backdrop_images_from_entries(device, &entries);
    (textures, sizes, entries, config_path, mtime)
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct FacetLutUniforms {
    grid_w: u32,
    grid_h: u32,
    _pad0: [f32; 2],
    col_blue: [f32; 4],
    col_violet: [f32; 4],
    col_cyan: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FacetLutCacheKey {
    grid_w: u32,
    grid_h: u32,
    col_blue_bits: [u32; 3],
    col_violet_bits: [u32; 3],
    col_cyan_bits: [u32; 3],
}

fn inactive_particle_uniforms(
    time_sec: f32,
    viewport_w: f32,
    viewport_h: f32,
    tuning: crate::ParticleWaveTuning,
) -> ParticleWaveUniforms {
    let tile_size = tuning.tile_size_px.round().max(2.0);
    let grid_w = (viewport_w / tile_size).ceil().max(1.0) as u32;
    let grid_h = (viewport_h / tile_size).ceil().max(1.0) as u32;
    ParticleWaveUniforms {
        time_sec,
        has_image: 0.0,
        image_w: 1.0,
        image_h: 1.0,
        phase_mode: 0.0,
        phase_progress: 0.0,
        grid_w,
        grid_h,
        viewport_w,
        viewport_h,
        tile_size: tuning.tile_size_px,
        fit_cover: 0.0,
        image_brightness: tuning.image_brightness,
        image_peak_luminance: tuning.image_peak_luminance,
        image_saturation: tuning.image_saturation,
        _pad0: 0.0,
    }
}

const GPU_TS_MAX_SAMPLES: usize = 64;
const MTL_COUNTER_DONT_SAMPLE: u64 = u64::MAX;

static DUMP_PASSES_DONE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug)]
enum GpuPassKind {
    BlitFill,
    ComputeScatter,
    BlitBackdrop {
        pixels: u64,
        mipmaps: bool,
        regions: usize,
        reason: String,
    },
    RenderPass {
        index: u32,
        summary: String,
    },
}

#[derive(Clone, Debug)]
struct GpuPassSpan {
    kind: GpuPassKind,
    start_slot: u64,
    end_slot: u64,
}

struct GpuTimestampPool {
    counter_set_ptr: usize,
    buffers: Mutex<Vec<usize>>,
}

impl GpuTimestampPool {
    fn new(device: &metal::Device) -> Option<Arc<Self>> {
        unsafe {
            let dev = device.as_ptr() as *mut objc::runtime::Object;
            let supports: cocoa::base::BOOL = msg_send![dev, supportsCounterSampling: 0u64];
            if supports == NO {
                return None;
            }
            let sets: *mut objc::runtime::Object = msg_send![dev, counterSets];
            if sets.is_null() {
                return None;
            }
            let count: NSUInteger = msg_send![sets, count];
            let mut found: *mut objc::runtime::Object = ptr::null_mut();
            for i in 0..count {
                let cs: *mut objc::runtime::Object = msg_send![sets, objectAtIndex: i];
                let name: *mut objc::runtime::Object = msg_send![cs, name];
                if !name.is_null() {
                    let utf8: *const std::os::raw::c_char = msg_send![name, UTF8String];
                    if !utf8.is_null()
                        && let Ok(s) = std::ffi::CStr::from_ptr(utf8).to_str()
                        && s == "timestamp"
                    {
                        let _: *mut objc::runtime::Object = msg_send![cs, retain];
                        found = cs;
                        break;
                    }
                }
            }
            if found.is_null() {
                return None;
            }
            Some(Arc::new(Self {
                counter_set_ptr: found as usize,
                buffers: Mutex::new(Vec::with_capacity(4)),
            }))
        }
    }

    fn acquire(&self, device: &metal::Device) -> Option<usize> {
        if let Some(ptr) = self.buffers.lock().pop() {
            return Some(ptr);
        }
        unsafe {
            let dev = device.as_ptr() as *mut objc::runtime::Object;
            let desc: *mut objc::runtime::Object =
                msg_send![class!(MTLCounterSampleBufferDescriptor), new];
            if desc.is_null() {
                return None;
            }
            let cs = self.counter_set_ptr as *mut objc::runtime::Object;
            let _: () = msg_send![desc, setCounterSet: cs];
            let _: () = msg_send![desc, setStorageMode: 0u64];
            let _: () = msg_send![desc, setSampleCount: GPU_TS_MAX_SAMPLES as NSUInteger];
            let mut err: *mut objc::runtime::Object = ptr::null_mut();
            let sb: *mut objc::runtime::Object = msg_send![
                dev,
                newCounterSampleBufferWithDescriptor: desc
                error: &mut err
            ];
            let _: () = msg_send![desc, release];
            if sb.is_null() {
                return None;
            }
            Some(sb as usize)
        }
    }

    fn release(&self, buf_ptr: usize) {
        let mut guard = self.buffers.lock();
        if guard.len() < 8 {
            guard.push(buf_ptr);
        } else {
            unsafe {
                let obj = buf_ptr as *mut objc::runtime::Object;
                let _: () = msg_send![obj, release];
            }
        }
    }
}

struct FrameGpuProfiler {
    pool: Arc<GpuTimestampPool>,
    sample_buffer_ptr: usize,
    next_slot: u64,
    spans: Vec<GpuPassSpan>,
    dump_detail: bool,
}

impl FrameGpuProfiler {
    fn alloc_pair(&mut self) -> Option<(u64, u64)> {
        if (self.next_slot as usize) + 2 > GPU_TS_MAX_SAMPLES {
            return None;
        }
        let s = self.next_slot;
        let e = s + 1;
        self.next_slot += 2;
        Some((s, e))
    }

    fn record_span(&mut self, kind: GpuPassKind, start_slot: u64, end_slot: u64) {
        self.spans.push(GpuPassSpan {
            kind,
            start_slot,
            end_slot,
        });
    }

    fn update_last_render_summary(&mut self, summary: String) {
        if let Some(span) = self.spans.last_mut()
            && let GpuPassKind::RenderPass {
                summary: ref mut s, ..
            } = span.kind
        {
            *s = summary;
        }
    }

    fn resolve_and_report(self, total_gpu_us: u64, viewport_w: u32, viewport_h: u32) {
        let used = self.next_slot as usize;
        if used == 0 {
            self.pool.release(self.sample_buffer_ptr);
            return;
        }
        let timestamps: Vec<u64> = unsafe {
            let sb = self.sample_buffer_ptr as *mut objc::runtime::Object;
            let range = NSRange {
                location: 0,
                length: used as NSUInteger,
            };
            let data: *mut objc::runtime::Object = msg_send![sb, resolveCounterRange: range];
            if data.is_null() {
                Vec::new()
            } else {
                let len: NSUInteger = msg_send![data, length];
                let bytes: *const u64 = msg_send![data, bytes];
                let count = (len as usize / mem::size_of::<u64>()).min(used);
                if bytes.is_null() || count == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(bytes, count).to_vec()
                }
            }
        };
        self.pool.release(self.sample_buffer_ptr);

        if timestamps.is_empty() {
            return;
        }

        let mut raw_durs: Vec<u64> = Vec::with_capacity(self.spans.len());
        let mut raw_sum = 0u64;
        for span in &self.spans {
            let s = timestamps
                .get(span.start_slot as usize)
                .copied()
                .unwrap_or(0);
            let e = timestamps.get(span.end_slot as usize).copied().unwrap_or(0);
            let dt = if s != 0 && e != 0 && e >= s && s != u64::MAX && e != u64::MAX {
                e - s
            } else {
                0
            };
            raw_durs.push(dt);
            raw_sum = raw_sum.saturating_add(dt);
        }

        let mut breakdown = crate::perf_telemetry::GpuPassBreakdown {
            total_gpu_us,
            pass_count: self.spans.len() as u32,
            ..Default::default()
        };

        for (span, &dt_ns) in self.spans.iter().zip(raw_durs.iter()) {
            let dt_us = (dt_ns + 500) / 1000;
            match &span.kind {
                GpuPassKind::BlitFill => breakdown.blit_fill_gpu_us += dt_us,
                GpuPassKind::ComputeScatter => breakdown.compute_gpu_us += dt_us,
                GpuPassKind::BlitBackdrop { .. } => breakdown.blit_backdrop_gpu_us += dt_us,
                GpuPassKind::RenderPass { index, .. } => {
                    if *index == 0 {
                        breakdown.render_pass_0_gpu_us += dt_us;
                    } else {
                        breakdown.render_pass_rest_gpu_us += dt_us;
                    }
                }
            }
        }

        if self.dump_detail && !DUMP_PASSES_DONE.swap(true, Ordering::AcqRel) {
            let t0 = timestamps
                .iter()
                .copied()
                .filter(|&v| v != 0 && v != u64::MAX)
                .min()
                .unwrap_or(0);
            let mut lines = Vec::new();
            lines.push(format!(
                "=== GPU PASS BREAKDOWN ({viewport_w}x{viewport_h}, cb_gpu_us={total_gpu_us}us, sum_passes_us={}us) ===",
                (raw_sum + 500) / 1000
            ));
            for (idx, (span, &dt_ns)) in self.spans.iter().zip(raw_durs.iter()).enumerate() {
                let s_ns = timestamps
                    .get(span.start_slot as usize)
                    .copied()
                    .unwrap_or(t0);
                let e_ns = timestamps
                    .get(span.end_slot as usize)
                    .copied()
                    .unwrap_or(t0);
                let rel_s = s_ns.saturating_sub(t0) as f64 / 1000.0;
                let rel_e = e_ns.saturating_sub(t0) as f64 / 1000.0;
                let us = dt_ns as f64 / 1000.0;
                match &span.kind {
                    GpuPassKind::BlitFill => {
                        lines.push(format!("  [{idx:02}] [{rel_s:6.0}..{rel_e:6.0}] BlitFill       {us:7.1} us"));
                    }
                    GpuPassKind::ComputeScatter => {
                        lines.push(format!("  [{idx:02}] [{rel_s:6.0}..{rel_e:6.0}] ComputeScatter {us:7.1} us"));
                    }
                    GpuPassKind::BlitBackdrop {
                        pixels,
                        mipmaps,
                        regions,
                        reason,
                    } => {
                        let ratio = *pixels as f64 / ((viewport_w as f64 * viewport_h as f64).max(1.0));
                        lines.push(format!(
                            "  [{idx:02}] [{rel_s:6.0}..{rel_e:6.0}] BlitBackdrop   {us:7.1} us | px={pixels} ({ratio:.2}x) regions={regions} mips={mipmaps} | {reason}"
                        ));
                    }
                    GpuPassKind::RenderPass { index, summary } => {
                        lines.push(format!(
                            "  [{idx:02}] [{rel_s:6.0}..{rel_e:6.0}] RenderPass#{index:<2}  {us:7.1} us | {summary}"
                        ));
                    }
                }
            }
            lines.push("==================================================================".into());
            let body = lines.join("\n") + "\n";
            if let Ok(path) = std::env::var("AFTERLIFE_PERF_DUMP_PATH") {
                let _ = std::fs::write(&path, &body);
            } else {
                eprint!("{body}");
            }
        }

        crate::perf_telemetry::record_gpu_pass_breakdown(breakdown);
    }
}

unsafe fn attach_blit_timestamps<'a>(
    command_buffer: &'a metal::CommandBufferRef,
    profiler: Option<&mut FrameGpuProfiler>,
    kind: GpuPassKind,
) -> &'a metal::BlitCommandEncoderRef {
    if let Some(prof) = profiler
        && let Some((start_slot, end_slot)) = prof.alloc_pair()
    {
        let bpd: *mut objc::runtime::Object =
            msg_send![class!(MTLBlitPassDescriptor), blitPassDescriptor];
        if !bpd.is_null() {
            let atts: *mut objc::runtime::Object = msg_send![bpd, sampleBufferAttachments];
            let att0: *mut objc::runtime::Object = msg_send![atts, objectAtIndexedSubscript: 0usize];
            let sb = prof.sample_buffer_ptr as *mut objc::runtime::Object;
            let _: () = msg_send![att0, setSampleBuffer: sb];
            let _: () = msg_send![att0, setStartOfEncoderSampleIndex: start_slot];
            let _: () = msg_send![att0, setEndOfEncoderSampleIndex: end_slot];
            let cb = command_buffer.as_ptr() as *mut objc::runtime::Object;
            let enc_ptr: *mut objc::runtime::Object =
                msg_send![cb, blitCommandEncoderWithDescriptor: bpd];
            if !enc_ptr.is_null() {
                prof.record_span(kind, start_slot, end_slot);
                return unsafe { metal::BlitCommandEncoderRef::from_ptr(enc_ptr as *mut _) };
            }
        }
    }
    command_buffer.new_blit_command_encoder()
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MipRectUniforms {
    dst_ox: u32,
    dst_oy: u32,
    dst_w: u32,
    dst_h: u32,
    inv_src_w: f32,
    inv_src_h: f32,
}

unsafe fn attach_compute_timestamps_kind<'a>(
    command_buffer: &'a metal::CommandBufferRef,
    profiler: Option<&mut FrameGpuProfiler>,
    kind: GpuPassKind,
) -> &'a metal::ComputeCommandEncoderRef {
    if let Some(prof) = profiler
        && let Some((start_slot, end_slot)) = prof.alloc_pair()
    {
        let cpd: *mut objc::runtime::Object =
            msg_send![class!(MTLComputePassDescriptor), computePassDescriptor];
        if !cpd.is_null() {
            let atts: *mut objc::runtime::Object = msg_send![cpd, sampleBufferAttachments];
            let att0: *mut objc::runtime::Object = msg_send![atts, objectAtIndexedSubscript: 0usize];
            let sb = prof.sample_buffer_ptr as *mut objc::runtime::Object;
            let _: () = msg_send![att0, setSampleBuffer: sb];
            let _: () = msg_send![att0, setStartOfEncoderSampleIndex: start_slot];
            let _: () = msg_send![att0, setEndOfEncoderSampleIndex: end_slot];
            let cb = command_buffer.as_ptr() as *mut objc::runtime::Object;
            let enc_ptr: *mut objc::runtime::Object =
                msg_send![cb, computeCommandEncoderWithDescriptor: cpd];
            if !enc_ptr.is_null() {
                prof.record_span(kind, start_slot, end_slot);
                return unsafe { metal::ComputeCommandEncoderRef::from_ptr(enc_ptr as *mut _) };
            }
        }
    }
    command_buffer.new_compute_command_encoder()
}

unsafe fn attach_compute_timestamps<'a>(
    command_buffer: &'a metal::CommandBufferRef,
    profiler: Option<&mut FrameGpuProfiler>,
) -> &'a metal::ComputeCommandEncoderRef {
    unsafe { attach_compute_timestamps_kind(command_buffer, profiler, GpuPassKind::ComputeScatter) }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveBackdropSource {
    None,
    Override(u64),
    LiveVideo(u64),
    Custom(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ParticleTileGridCacheKey {
    source: ActiveBackdropSource,
    custom_textures_version: u64,
    grid_w: u32,
    grid_h: u32,
    viewport_w_bits: u32,
    viewport_h_bits: u32,
    tile_size_bits: u32,
    image_w_bits: u32,
    image_h_bits: u32,
    fit_cover_bits: u32,
    image_brightness_bits: u32,
    image_peak_luminance_bits: u32,
    image_saturation_bits: u32,
    phase_stage: u8,
}

pub(crate) struct MetalRenderer {
    device: metal::Device,
    layer: metal::MetalLayer,
    presents_with_transaction: bool,
    command_queue: CommandQueue,
    paths_rasterization_pipeline_state: metal::RenderPipelineState,
    path_sprites_pipeline_state: metal::RenderPipelineState,
    shadows_pipeline_state: metal::RenderPipelineState,
    quads_pipeline_state: metal::RenderPipelineState,
    solid_quads_pipeline_state: metal::RenderPipelineState,
    liquid_glass_pipeline_state: metal::RenderPipelineState,
    liquid_glass_synced_sharp_pipeline_state: metal::RenderPipelineState,
    liquid_glass_synced_blur_pipeline_state: metal::RenderPipelineState,
    particle_wave_pipeline_state: metal::RenderPipelineState,
    particle_wave_no_image_pipeline_state: metal::RenderPipelineState,
    underlines_pipeline_state: metal::RenderPipelineState,
    monochrome_sprites_pipeline_state: metal::RenderPipelineState,
    polychrome_sprites_pipeline_state: metal::RenderPipelineState,
    surfaces_pipeline_state: metal::RenderPipelineState,
    unit_vertices: metal::Buffer,
    #[allow(clippy::arc_with_non_send_sync)]
    instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
    sprite_atlas: Arc<MetalAtlas>,
    core_video_texture_cache: core_video::metal_texture_cache::CVMetalTextureCache,
    path_intermediate_texture: Option<metal::Texture>,
    path_intermediate_msaa_texture: Option<metal::Texture>,
    backdrop_texture: Option<metal::Texture>,
    backdrop_mip_views: Vec<metal::Texture>,
    backdrop_mip_enabled: bool,
    backdrop_downsample_pipeline: metal::ComputePipelineState,
    backdrop_sampler: metal::SamplerState,
    backdrop_image_textures: Vec<metal::Texture>,
    backdrop_image_sizes: Vec<[f32; 3]>,
    backdrop_loaded_entries: Vec<BackdropImageEntryKey>,
    backdrop_images_loaded: bool,
    backdrop_custom_version: u64,
    backdrop_config_path: Option<std::path::PathBuf>,
    backdrop_config_mtime: Option<std::time::SystemTime>,
    backdrop_last_poll: Instant,
    backdrop_override_seen_gen: u64,
    backdrop_pending_override: Option<(u64, metal::Texture, [f32; 3])>,
    backdrop_active_override: Option<(u64, metal::Texture, [f32; 3])>,
    backdrop_live_video_seen_gen: u64,
    backdrop_live_video: Option<(u64, metal::Texture, [f32; 3], Instant)>,
    backdrop_active_source: ActiveBackdropSource,
    backdrop_phase_stage: u8,
    backdrop_phase_elapsed: f32,
    backdrop_phase_last_tick: Instant,
    backdrop_image_index: usize,
    particle_tile_scatter_pipeline: metal::ComputePipelineState,
    particle_tile_hold_pipeline: metal::ComputePipelineState,
    particle_tile_unpack_pipeline: metal::ComputePipelineState,
    particle_facet_lut_pipeline: metal::ComputePipelineState,
    particle_tile_grid_buffer: metal::Buffer,
    particle_tile_grid_capacity: usize,
    particle_tile_grid_needs_clear: bool,
    particle_tile_img_texture: Option<metal::Texture>,
    particle_tile_grid_cache_key: Option<ParticleTileGridCacheKey>,
    particle_facet_lut_texture: Option<metal::Texture>,
    particle_facet_lut_cache_key: Option<FacetLutCacheKey>,
    current_particle_uniforms: ParticleWaveUniforms,
    start_time: Instant,
    path_sample_count: u32,
    gpu_timestamp_pool: Option<Arc<GpuTimestampPool>>,
    prof_frame_counter: u64,
}

#[repr(C)]
pub struct PathRasterizationVertex {
    pub xy_position: Point<ScaledPixels>,
    pub st_position: Point<f32>,
    pub color: Background,
    pub bounds: Bounds<ScaledPixels>,
}

impl MetalRenderer {
    pub fn new(instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>) -> Self {
        // Prefer low‐power integrated GPUs on Intel Mac. On Apple
        // Silicon, there is only ever one GPU, so this is equivalent to
        // `metal::Device::system_default()`.
        let mut devices = metal::Device::all();
        devices.sort_by_key(|device| (device.is_removable(), device.is_low_power()));
        let Some(device) = devices.pop() else {
            log::error!("unable to access a compatible graphics device");
            std::process::exit(1);
        };

        let layer = metal::MetalLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        layer.set_opaque(false);
        layer.set_framebuffer_only(false);
        layer.set_maximum_drawable_count(3);
        unsafe {
            let _: () = msg_send![&*layer, setAllowsNextDrawableTimeout: NO];
            let _: () = msg_send![&*layer, setNeedsDisplayOnBoundsChange: YES];
            let _: () = msg_send![
                &*layer,
                setAutoresizingMask: AutoresizingMask::WIDTH_SIZABLE
                    | AutoresizingMask::HEIGHT_SIZABLE
            ];
        }
        #[cfg(feature = "runtime_shaders")]
        let library = device
            .new_library_with_source(&SHADERS_SOURCE_FILE, &metal::CompileOptions::new())
            .expect("error building metal library");
        #[cfg(not(feature = "runtime_shaders"))]
        let library = device
            .new_library_with_data(SHADERS_METALLIB)
            .expect("error building metal library");

        fn to_float2_bits(point: PointF) -> u64 {
            let mut output = point.y.to_bits() as u64;
            output <<= 32;
            output |= point.x.to_bits() as u64;
            output
        }

        let unit_vertices = [
            to_float2_bits(point(0., 0.)),
            to_float2_bits(point(1., 0.)),
            to_float2_bits(point(0., 1.)),
            to_float2_bits(point(0., 1.)),
            to_float2_bits(point(1., 0.)),
            to_float2_bits(point(1., 1.)),
        ];
        let unit_vertices_storage = if device.has_unified_memory() {
            MTLResourceOptions::StorageModeShared
        } else {
            MTLResourceOptions::StorageModeManaged
        };
        let unit_vertices = device.new_buffer_with_data(
            unit_vertices.as_ptr() as *const c_void,
            mem::size_of_val(&unit_vertices) as u64,
            unit_vertices_storage,
        );

        let paths_rasterization_pipeline_state = build_path_rasterization_pipeline_state(
            &device,
            &library,
            "paths_rasterization",
            "path_rasterization_vertex",
            "path_rasterization_fragment",
            MTLPixelFormat::BGRA8Unorm,
            PATH_SAMPLE_COUNT,
        );
        let path_sprites_pipeline_state = build_path_sprite_pipeline_state(
            &device,
            &library,
            "path_sprites",
            "path_sprite_vertex",
            "path_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let shadows_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "shadows",
            "shadow_vertex",
            "shadow_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let quads_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "quads",
            "quad_vertex",
            "quad_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let solid_quads_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "solid_quads",
            "solid_quad_vertex",
            "solid_quad_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let liquid_glass_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "liquid_glass",
            "quad_vertex",
            "liquid_glass_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let liquid_glass_synced_sharp_pipeline_state = build_pipeline_state_with_bool_constant(
            &device,
            &library,
            "liquid_glass_synced_sharp",
            "liquid_glass_synced_vertex",
            "liquid_glass_synced_fragment",
            MTLPixelFormat::BGRA8Unorm,
            1,
            false,
        );
        let liquid_glass_synced_blur_pipeline_state = build_pipeline_state_with_bool_constant(
            &device,
            &library,
            "liquid_glass_synced_blur",
            "liquid_glass_synced_vertex",
            "liquid_glass_synced_fragment",
            MTLPixelFormat::BGRA8Unorm,
            1,
            true,
        );
        let particle_wave_pipeline_state = build_opaque_pipeline_state_with_bool_constant(
            &device,
            &library,
            "particle_wave",
            "particle_wave_vertex",
            "particle_wave_fragment",
            MTLPixelFormat::BGRA8Unorm,
            0,
            true,
        );
        let particle_wave_no_image_pipeline_state = build_opaque_pipeline_state_with_bool_constant(
            &device,
            &library,
            "particle_wave_no_image",
            "particle_wave_vertex",
            "particle_wave_fragment",
            MTLPixelFormat::BGRA8Unorm,
            0,
            false,
        );
        let sampler_desc = metal::SamplerDescriptor::new();
        sampler_desc.set_min_filter(metal::MTLSamplerMinMagFilter::Linear);
        sampler_desc.set_mag_filter(metal::MTLSamplerMinMagFilter::Linear);
        sampler_desc.set_mip_filter(metal::MTLSamplerMipFilter::Linear);
        sampler_desc.set_address_mode_s(metal::MTLSamplerAddressMode::ClampToEdge);
        sampler_desc.set_address_mode_t(metal::MTLSamplerAddressMode::ClampToEdge);
        let backdrop_sampler = device.new_sampler(&sampler_desc);
        let underlines_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "underlines",
            "underline_vertex",
            "underline_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let monochrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "monochrome_sprites",
            "monochrome_sprite_vertex",
            "monochrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let polychrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "polychrome_sprites",
            "polychrome_sprite_vertex",
            "polychrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let surfaces_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "surfaces",
            "surface_vertex",
            "surface_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );

        let command_queue = device.new_command_queue();
        let sprite_atlas = Arc::new(MetalAtlas::new(device.clone()));
        let core_video_texture_cache =
            CVMetalTextureCache::new(None, device.clone(), None).unwrap();
        let image_enabled = crate::particle_wave_tuning().image_enabled;
        let (
            backdrop_image_textures,
            backdrop_image_sizes,
            backdrop_loaded_entries,
            backdrop_config_path,
            backdrop_config_mtime,
            backdrop_images_loaded,
        ) = if image_enabled {
            let (textures, sizes, entries, cfg_path, mtime) = load_backdrop_images(&device);
            (textures, sizes, entries, cfg_path, mtime, true)
        } else {
            (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                resolve_backdrop_config_path(),
                None,
                false,
            )
        };
        let scatter_fn = library
            .get_function("particle_tile_scatter_kernel", None)
            .expect("could not find particle_tile_scatter_kernel function");
        let particle_tile_scatter_pipeline = device
            .new_compute_pipeline_state_with_function(&scatter_fn)
            .expect("could not create particle_tile_scatter_pipeline");
        let hold_fn = library
            .get_function("particle_tile_hold_kernel", None)
            .expect("could not find particle_tile_hold_kernel function");
        let particle_tile_hold_pipeline = device
            .new_compute_pipeline_state_with_function(&hold_fn)
            .expect("could not create particle_tile_hold_pipeline");
        let unpack_fn = library
            .get_function("particle_tile_unpack_to_tex_kernel", None)
            .expect("could not find particle_tile_unpack_to_tex_kernel function");
        let particle_tile_unpack_pipeline = device
            .new_compute_pipeline_state_with_function(&unpack_fn)
            .expect("could not create particle_tile_unpack_pipeline");
        let facet_lut_fn = library
            .get_function("particle_facet_lut_kernel", None)
            .expect("could not find particle_facet_lut_kernel function");
        let particle_facet_lut_pipeline = device
            .new_compute_pipeline_state_with_function(&facet_lut_fn)
            .expect("could not create particle_facet_lut_pipeline");
        let downsample_fn = library
            .get_function("backdrop_downsample_rect_kernel", None)
            .expect("could not find backdrop_downsample_rect_kernel function");
        let backdrop_downsample_pipeline = device
            .new_compute_pipeline_state_with_function(&downsample_fn)
            .expect("could not create backdrop_downsample_pipeline");
        let particle_tile_grid_capacity = 1usize;
        let particle_tile_grid_buffer =
            device.new_buffer(4, MTLResourceOptions::StorageModePrivate);

        let gpu_timestamp_pool = GpuTimestampPool::new(&device);
        Self {
            device,
            layer,
            presents_with_transaction: false,
            command_queue,
            paths_rasterization_pipeline_state,
            path_sprites_pipeline_state,
            shadows_pipeline_state,
            quads_pipeline_state,
            solid_quads_pipeline_state,
            liquid_glass_pipeline_state,
            liquid_glass_synced_sharp_pipeline_state,
            liquid_glass_synced_blur_pipeline_state,
            particle_wave_pipeline_state,
            particle_wave_no_image_pipeline_state,
            underlines_pipeline_state,
            monochrome_sprites_pipeline_state,
            polychrome_sprites_pipeline_state,
            surfaces_pipeline_state,
            unit_vertices,
            instance_buffer_pool,
            sprite_atlas,
            core_video_texture_cache,
            path_intermediate_texture: None,
            path_intermediate_msaa_texture: None,
            backdrop_texture: None,
            backdrop_mip_views: Vec::new(),
            backdrop_mip_enabled: false,
            backdrop_downsample_pipeline,
            backdrop_sampler,
            backdrop_image_textures,
            backdrop_image_sizes,
            backdrop_loaded_entries,
            backdrop_images_loaded,
            backdrop_custom_version: 0,
            backdrop_config_path,
            backdrop_config_mtime,
            backdrop_last_poll: Instant::now(),
            backdrop_override_seen_gen: 0,
            backdrop_pending_override: None,
            backdrop_active_override: None,
            backdrop_live_video_seen_gen: 0,
            backdrop_live_video: None,
            backdrop_active_source: ActiveBackdropSource::None,
            backdrop_phase_stage: 0,
            backdrop_phase_elapsed: 0.0,
            backdrop_phase_last_tick: Instant::now(),
            backdrop_image_index: 0,
            particle_tile_scatter_pipeline,
            particle_tile_hold_pipeline,
            particle_tile_unpack_pipeline,
            particle_facet_lut_pipeline,
            particle_tile_grid_buffer,
            particle_tile_grid_capacity,
            particle_tile_grid_needs_clear: true,
            particle_tile_img_texture: None,
            particle_tile_grid_cache_key: None,
            particle_facet_lut_texture: None,
            particle_facet_lut_cache_key: None,
            current_particle_uniforms: ParticleWaveUniforms::default(),
            start_time: Instant::now(),
            path_sample_count: PATH_SAMPLE_COUNT,
            gpu_timestamp_pool,
            prof_frame_counter: 0,
        }
    }

    pub fn layer(&self) -> &metal::MetalLayerRef {
        &self.layer
    }

    pub fn layer_ptr(&self) -> *mut CAMetalLayer {
        self.layer.as_ptr()
    }

    pub fn sprite_atlas(&self) -> &Arc<MetalAtlas> {
        &self.sprite_atlas
    }

    pub fn set_presents_with_transaction(&mut self, presents_with_transaction: bool) {
        self.presents_with_transaction = presents_with_transaction;
        self.layer
            .set_presents_with_transaction(presents_with_transaction);
    }

    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        let size = NSSize {
            width: size.width.0 as f64,
            height: size.height.0 as f64,
        };
        unsafe {
            let _: () = msg_send![
                self.layer(),
                setDrawableSize: size
            ];
        }
        let device_pixels_size = Size {
            width: DevicePixels(size.width as i32),
            height: DevicePixels(size.height as i32),
        };
        self.update_path_intermediate_textures(device_pixels_size);
    }

    fn update_path_intermediate_textures(&mut self, size: Size<DevicePixels>) {
        self.path_intermediate_texture = None;
        self.path_intermediate_msaa_texture = None;
        if size.width.0 <= 0 || size.height.0 <= 0 {
            self.backdrop_texture = None;
            self.backdrop_mip_views.clear();
            return;
        }
        self.allocate_backdrop_texture(size, self.backdrop_mip_enabled);
    }

    fn allocate_backdrop_texture(&mut self, size: Size<DevicePixels>, with_mipmaps: bool) {
        let w = size.width.0.max(1) as u64;
        let h = size.height.0.max(1) as u64;
        let mip_levels = if with_mipmaps {
            let min_dim = w.min(h);
            (64 - min_dim.leading_zeros()).clamp(1, 6) as u64
        } else {
            1
        };
        let backdrop_desc = metal::TextureDescriptor::new();
        backdrop_desc.set_width(w);
        backdrop_desc.set_height(h);
        backdrop_desc.set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
        backdrop_desc.set_mipmap_level_count(mip_levels);
        backdrop_desc.set_storage_mode(metal::MTLStorageMode::Private);
        backdrop_desc.set_usage(if mip_levels > 1 {
            metal::MTLTextureUsage::ShaderRead | metal::MTLTextureUsage::ShaderWrite
        } else {
            metal::MTLTextureUsage::ShaderRead
        });
        let tex = self.device.new_texture(&backdrop_desc);
        let mut views = Vec::with_capacity(mip_levels as usize);
        if mip_levels > 1 {
            for lvl in 0..mip_levels {
                views.push(tex.new_texture_view_from_slice(
                    metal::MTLPixelFormat::BGRA8Unorm,
                    metal::MTLTextureType::D2,
                    NSRange::new(lvl, 1),
                    NSRange::new(0, 1),
                ));
            }
        }
        self.backdrop_texture = Some(tex);
        self.backdrop_mip_views = views;
        self.backdrop_mip_enabled = with_mipmaps;
    }

    fn ensure_backdrop_mipmaps_allocated(&mut self, size: Size<DevicePixels>) {
        let needs_alloc = match &self.backdrop_texture {
            Some(tex) => {
                tex.mipmap_level_count() <= 1
                    || tex.width() != size.width.0.max(1) as u64
                    || tex.height() != size.height.0.max(1) as u64
            }
            None => true,
        };
        if needs_alloc && size.width.0 > 0 && size.height.0 > 0 {
            self.allocate_backdrop_texture(size, true);
        }
    }

    fn ensure_path_intermediate_textures(&mut self, size: Size<DevicePixels>) {
        if size.width.0 <= 0 || size.height.0 <= 0 {
            return;
        }
        let w = size.width.0 as u64;
        let h = size.height.0 as u64;
        if let Some(ref tex) = self.path_intermediate_texture
            && tex.width() == w
            && tex.height() == h
        {
            return;
        }

        let texture_descriptor = metal::TextureDescriptor::new();
        texture_descriptor.set_width(w);
        texture_descriptor.set_height(h);
        texture_descriptor.set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
        texture_descriptor.set_storage_mode(metal::MTLStorageMode::Private);
        texture_descriptor
            .set_usage(metal::MTLTextureUsage::RenderTarget | metal::MTLTextureUsage::ShaderRead);
        self.path_intermediate_texture = Some(self.device.new_texture(&texture_descriptor));

        if self.path_sample_count > 1 {
            let msaa_descriptor = texture_descriptor;
            msaa_descriptor.set_texture_type(metal::MTLTextureType::D2Multisample);
            msaa_descriptor.set_storage_mode(metal::MTLStorageMode::Private);
            msaa_descriptor.set_sample_count(self.path_sample_count as _);
            self.path_intermediate_msaa_texture = Some(self.device.new_texture(&msaa_descriptor));
        } else {
            self.path_intermediate_msaa_texture = None;
        }
    }

    pub fn update_transparency(&self, _transparent: bool) {
        // todo(mac)?
    }

    pub fn destroy(&self) {
        // nothing to do
    }

    pub fn draw(&mut self, scene: &Scene) {
        let perf_on = crate::perf_telemetry::is_any_enabled();
        let draw_start = perf_on.then(Instant::now);
        let layer = self.layer.clone();
        let viewport_size = layer.drawable_size();
        let viewport_size: Size<DevicePixels> = size(
            (viewport_size.width.ceil() as i32).into(),
            (viewport_size.height.ceil() as i32).into(),
        );
        let drawable_start = perf_on.then(Instant::now);
        let drawable = if let Some(drawable) = layer.next_drawable() {
            drawable
        } else {
            log::error!(
                "failed to retrieve next drawable, drawable size: {:?}",
                viewport_size
            );
            return;
        };
        let next_drawable_us = drawable_start
            .map(|s| s.elapsed().as_micros() as u64)
            .unwrap_or(0);

        loop {
            let mut instance_buffer = self.instance_buffer_pool.lock().acquire(&self.device);

            let command_buffer = self.draw_primitives(
                scene,
                &mut instance_buffer,
                drawable.texture(),
                viewport_size,
                perf_on,
            );

            match command_buffer {
                Ok((command_buffer, perf_sample, gpu_profiler)) => {
                    let instance_buffer_pool = self.instance_buffer_pool.clone();
                    let instance_buffer = Cell::new(Some(instance_buffer));
                    let gpu_profiler = Cell::new(gpu_profiler);
                    let vp_w = viewport_size.width.0.max(0) as u32;
                    let vp_h = viewport_size.height.0.max(0) as u32;
                    let block = ConcreteBlock::new(move |cb: &metal::CommandBufferRef| {
                        let mut cb_gpu_us = 0u64;
                        if perf_on && crate::perf_telemetry::is_any_enabled() {
                            unsafe {
                                let ptr = cb.as_ptr() as *mut objc::runtime::Object;
                                let gpu_start: f64 = msg_send![ptr, GPUStartTime];
                                let gpu_end: f64 = msg_send![ptr, GPUEndTime];
                                if gpu_start > 0.0 && gpu_end >= gpu_start {
                                    cb_gpu_us =
                                        ((gpu_end - gpu_start) * 1_000_000.0).round() as u64;
                                    crate::perf_telemetry::record_gpu_timing_us(cb_gpu_us);
                                }
                            }
                        }
                        if let Some(prof) = gpu_profiler.take() {
                            prof.resolve_and_report(cb_gpu_us, vp_w, vp_h);
                        }
                        if let Some(instance_buffer) = instance_buffer.take() {
                            instance_buffer_pool.lock().release(instance_buffer);
                        }
                    });
                    let block = block.copy();
                    command_buffer.add_completed_handler(&block);

                    if self.presents_with_transaction {
                        command_buffer.commit();
                        command_buffer.wait_until_scheduled();
                        drawable.present();
                    } else {
                        command_buffer.present_drawable(drawable);
                        command_buffer.commit();
                    }
                    crate::perf_telemetry::mark_first_frame();
                    if let (Some(mut sample), Some(start)) = (perf_sample, draw_start) {
                        sample.draw_total_us = start.elapsed().as_micros() as u64;
                        sample.next_drawable_us = next_drawable_us;
                        sample.gpu_allocated_bytes = unsafe {
                            let ptr = self.device.as_ptr() as *mut objc::runtime::Object;
                            let bytes: u64 = msg_send![ptr, currentAllocatedSize];
                            bytes
                        };
                        crate::perf_telemetry::record_metal_frame(sample);
                    }
                    return;
                }
                Err(err) => {
                    log::error!(
                        "failed to render: {}. retrying with larger instance buffer size",
                        err
                    );
                    let mut instance_buffer_pool = self.instance_buffer_pool.lock();
                    let buffer_size = instance_buffer_pool.buffer_size;
                    if buffer_size >= 256 * 1024 * 1024 {
                        log::error!("instance buffer size grew too large: {}", buffer_size);
                        break;
                    }
                    instance_buffer_pool.reset(buffer_size * 2);
                    log::info!(
                        "increased instance buffer size to {}",
                        instance_buffer_pool.buffer_size
                    );
                }
            }
        }
    }

    fn draw_primitives(
        &mut self,
        scene: &Scene,
        instance_buffer: &mut InstanceBuffer,
        target_texture: &metal::TextureRef,
        viewport_size: Size<DevicePixels>,
        perf_on: bool,
    ) -> Result<(
        metal::CommandBuffer,
        Option<crate::perf_telemetry::MetalFrameSample>,
        Option<FrameGpuProfiler>,
    )> {
        let encode_start = perf_on.then(Instant::now);
        let mut perf = perf_on.then(|| crate::perf_telemetry::MetalFrameSample {
            viewport_w: viewport_size.width.0.max(0) as u32,
            viewport_h: viewport_size.height.0.max(0) as u32,
            encoder_switches: 1,
            ..Default::default()
        });
        let dump_detail = if perf_on {
            self.prof_frame_counter = self.prof_frame_counter.wrapping_add(1);
            self.prof_frame_counter >= 15
                && !DUMP_PASSES_DONE.load(Ordering::Relaxed)
                && std::env::var_os("AFTERLIFE_PERF_DUMP_PASSES").is_some()
        } else {
            false
        };
        let mut gpu_prof = if perf_on
            && let Some(ref pool) = self.gpu_timestamp_pool
            && let Some(sb_ptr) = pool.acquire(&self.device)
        {
            Some(FrameGpuProfiler {
                pool: pool.clone(),
                sample_buffer_ptr: sb_ptr,
                next_slot: 0,
                spans: Vec::with_capacity(16),
                dump_detail,
            })
        } else {
            None
        };
        let mut render_pass_idx = 0u32;
        let mut rp_log = String::new();
        let command_queue = self.command_queue.clone();
        let command_buffer = command_queue.new_command_buffer();
        let alpha = if self.layer.is_opaque() { 1. } else { 0. };
        let mut instance_offset = 0;
        let mut backdrop_seeded = false;
        let mut backdrop_mips_valid = false;
        let vw_f = i32::from(viewport_size.width).max(0) as f32;
        let vh_f = i32::from(viewport_size.height).max(0) as f32;
        let mut dirty_rects: Vec<Bounds<ScaledPixels>> = Vec::with_capacity(32);

        let push_dirty = |dirty: &mut Vec<Bounds<ScaledPixels>>,
                          b: Bounds<ScaledPixels>,
                          merge_pad: f32| {
            if b.size.width.0 <= 0.0 || b.size.height.0 <= 0.0 {
                return;
            }
            for r in dirty.iter_mut() {
                if r.origin.x.0 <= b.origin.x.0
                    && r.origin.y.0 <= b.origin.y.0
                    && r.bottom_right().x.0 >= b.bottom_right().x.0
                    && r.bottom_right().y.0 >= b.bottom_right().y.0
                {
                    return;
                }
                if merge_pad > 0.0 {
                    let expanded = Bounds {
                        origin: point(
                            ScaledPixels(r.origin.x.0 - merge_pad),
                            ScaledPixels(r.origin.y.0 - merge_pad),
                        ),
                        size: size(
                            ScaledPixels(r.size.width.0 + merge_pad * 2.0),
                            ScaledPixels(r.size.height.0 + merge_pad * 2.0),
                        ),
                    };
                    if expanded.intersects(&b) {
                        *r = r.union(&b);
                        return;
                    }
                } else if r.intersects(&b) {
                    *r = r.union(&b);
                    return;
                }
            }
            if dirty.len() >= 48 {
                let mut best_idx = 0usize;
                let mut best_cost = f32::INFINITY;
                for (idx, r) in dirty.iter().enumerate() {
                    let u = r.union(&b);
                    let cost = u.size.width.0 * u.size.height.0 - r.size.width.0 * r.size.height.0;
                    if cost < best_cost {
                        best_cost = cost;
                        best_idx = idx;
                    }
                }
                dirty[best_idx] = dirty[best_idx].union(&b);
            } else {
                dirty.push(b);
            }
        };

        let batches: Vec<PrimitiveBatch<'_>> = scene.batches().collect();
        let compute_dispatched = self.dispatch_particle_tile_scatter(
            command_buffer,
            viewport_size,
            gpu_prof.as_mut(),
        );
        self.ensure_particle_facet_lut(
            &batches,
            command_buffer,
            gpu_prof.as_mut(),
        );
        if let Some(p) = perf.as_mut() {
            p.compute_scatter_dispatched = compute_dispatched;
            p.backdrop_images_loaded = (self.backdrop_image_textures.len()
                + usize::from(self.backdrop_active_override.is_some()))
                as u32;
        }

        let scene_has_blur = batches.iter().any(|batch| {
            if let PrimitiveBatch::Quads(quads) = batch {
                quads.iter().any(|q| {
                    q.background.tag == BackgroundTag::LiquidGlass
                        && q.background.colors[1].percentage * 2.0 > 0.5
                })
            } else {
                false
            }
        });
        if scene_has_blur {
            self.ensure_backdrop_mipmaps_allocated(viewport_size);
        }
        let first_covers_viewport = batches.first().is_some_and(|batch| {
            if let PrimitiveBatch::Quads(quads) = batch
                && let Some(first) = quads.first()
                && first.background.tag == BackgroundTag::ParticleWave
            {
                let vw = i32::from(viewport_size.width) as f32;
                let vh = i32::from(viewport_size.height) as f32;
                let b = first.bounds;
                let m = first.content_mask.bounds;
                let no_corners = first.corner_radii.top_left.0 <= 0.0
                    && first.corner_radii.top_right.0 <= 0.0
                    && first.corner_radii.bottom_right.0 <= 0.0
                    && first.corner_radii.bottom_left.0 <= 0.0;
                no_corners
                    && b.origin.x.0 <= 0.0
                    && b.origin.y.0 <= 0.0
                    && b.bottom_right().x.0 >= vw
                    && b.bottom_right().y.0 >= vh
                    && m.origin.x.0 <= 0.0
                    && m.origin.y.0 <= 0.0
                    && m.bottom_right().x.0 >= vw
                    && m.bottom_right().y.0 >= vh
            } else {
                false
            }
        });

        let mut command_encoder = new_command_encoder(
            command_buffer,
            target_texture,
            viewport_size,
            gpu_prof.as_mut(),
            render_pass_idx,
            |color_attachment| {
                if first_covers_viewport {
                    color_attachment.set_load_action(metal::MTLLoadAction::DontCare);
                } else {
                    color_attachment.set_load_action(metal::MTLLoadAction::Clear);
                    color_attachment.set_clear_color(metal::MTLClearColor::new(0., 0., 0., alpha));
                }
            },
        );
        render_pass_idx += 1;

        let items = schedule_draw_items(&batches, vw_f, vh_f);

        for (item_idx, item) in items.iter().enumerate() {
            let ok = match *item {
                DrawItem::Shadows(shadows) => {
                    if let Some(p) = perf.as_mut() {
                        p.shadows += shadows.len() as u32;
                    }
                    for s in shadows {
                        // Match `shadow_vertex` which expands `shadow.bounds` by `3 * blur_radius`
                        // clipped to `content_mask.bounds`, so backdrop blits capture drop shadows
                        // around card rims and borders without leaving seams.
                        let margin = 3.0 * s.blur_radius.0;
                        let expanded = Bounds {
                            origin: point(
                                ScaledPixels(s.bounds.origin.x.0 - margin),
                                ScaledPixels(s.bounds.origin.y.0 - margin),
                            ),
                            size: size(
                                ScaledPixels(s.bounds.size.width.0 + margin * 2.0),
                                ScaledPixels(s.bounds.size.height.0 + margin * 2.0),
                            ),
                        }
                        .intersect(&s.content_mask.bounds);
                        push_dirty(&mut dirty_rects, expanded, 0.0);
                    }
                    if dump_detail {
                        let px_sum: f64 = shadows
                            .iter()
                            .map(|s| {
                                let m = 3.0 * s.blur_radius.0;
                                ((s.bounds.size.width.0 + m * 2.0) * (s.bounds.size.height.0 + m * 2.0))
                                    as f64
                            })
                            .sum();
                        rp_log.push_str(&format!(
                            "shadows={}({:.2}Mpx) ",
                            shadows.len(),
                            px_sum / 1e6
                        ));
                    }
                    self.draw_shadows(
                        shadows,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_encoder,
                    )
                }
                DrawItem::Quads { quads: chunk, mode } => {
                    if mode == 1 || mode == 3 {
                        if let Some(p) = perf.as_mut() {
                            p.quads_glass += chunk.len() as u32;
                        }
                        let chunk_blur_rects: Vec<Bounds<ScaledPixels>> = chunk
                            .iter()
                            .filter(|q| q.background.colors[1].percentage * 2.0 > 0.5)
                            .map(|q| glass_sample_rect(q, vw_f, vh_f))
                            .filter(rect_non_empty)
                            .collect();
                        let chunk_needs_blur = !chunk_blur_rects.is_empty();

                        if !backdrop_seeded {
                            let mut all_glass_rects: Vec<Bounds<ScaledPixels>> =
                                Vec::with_capacity(32);
                            for it in &items[item_idx..] {
                                if let DrawItem::Quads { quads: qs, mode: m } = *it
                                    && (m == 1 || m == 3)
                                {
                                    for q in qs {
                                        let sr = glass_sample_rect(q, vw_f, vh_f);
                                        if rect_non_empty(&sr) {
                                            all_glass_rects.push(sr);
                                        }
                                    }
                                }
                            }
                            let clusters = coalesce_copy_regions(&all_glass_rects, 48.0, 6);
                            if !clusters.is_empty() {
                                if dump_detail && let Some(prof) = gpu_prof.as_mut() {
                                    prof.update_last_render_summary(std::mem::take(&mut rp_log));
                                }
                                command_encoder.end_encoding();
                                let copied_px = self.copy_regions_to_backdrop(
                                    command_buffer,
                                    target_texture,
                                    viewport_size,
                                    &clusters,
                                    &chunk_blur_rects,
                                    gpu_prof.as_mut(),
                                    if dump_detail {
                                        format!(
                                            "initial_seed_clipped before glass_chunk(n={})",
                                            chunk.len()
                                        )
                                    } else {
                                        String::new()
                                    },
                                );
                                if let Some(p) = perf.as_mut() {
                                    if copied_px > 0 {
                                        p.blit_count += 1;
                                        p.blit_pixels += copied_px;
                                    }
                                    p.encoder_switches += 1;
                                }
                                command_encoder = new_command_encoder(
                                    command_buffer,
                                    target_texture,
                                    viewport_size,
                                    gpu_prof.as_mut(),
                                    render_pass_idx,
                                    |color_attachment| {
                                        color_attachment
                                            .set_load_action(metal::MTLLoadAction::Load);
                                    },
                                );
                                render_pass_idx += 1;
                            }
                            backdrop_seeded = true;
                            backdrop_mips_valid = chunk_needs_blur;
                            dirty_rects.clear();
                        } else {
                            let mut needs_blit = false;
                            let mut hit_reason = String::new();
                            if !dirty_rects.is_empty() {
                                'check_hit: for q in chunk {
                                    let padded = glass_sample_rect(q, vw_f, vh_f);
                                    if !rect_non_empty(&padded) {
                                        continue;
                                    }
                                    for dirty in &dirty_rects {
                                        if dirty.intersects(&padded) {
                                            needs_blit = true;
                                            if dump_detail {
                                                let blur_px =
                                                    q.background.colors[1].percentage * 2.0;
                                                hit_reason = format!(
                                                    "glass({:.0},{:.0} {:.0}x{:.0} blur={:.1}) hit dirty({:.0},{:.0} {:.0}x{:.0}) total_dirty={}",
                                                    q.bounds.origin.x.0,
                                                    q.bounds.origin.y.0,
                                                    q.bounds.size.width.0,
                                                    q.bounds.size.height.0,
                                                    blur_px,
                                                    dirty.origin.x.0,
                                                    dirty.origin.y.0,
                                                    dirty.size.width.0,
                                                    dirty.size.height.0,
                                                    dirty_rects.len()
                                                );
                                            }
                                            break 'check_hit;
                                        }
                                    }
                                }
                            }

                            if needs_blit {
                                let (need_rects, blur_rects) = collect_lookahead_needs_items(
                                    &items,
                                    item_idx,
                                    chunk,
                                    &dirty_rects,
                                    vw_f,
                                    vh_f,
                                );
                                let mut intersections: Vec<Bounds<ScaledPixels>> =
                                    Vec::with_capacity(need_rects.len() * 2);
                                for n in &need_rects {
                                    for d in &dirty_rects {
                                        let isect = d.intersect(n);
                                        if rect_non_empty(&isect) {
                                            intersections.push(isect);
                                        }
                                    }
                                }
                                let clusters = coalesce_copy_regions(&intersections, 24.0, 8);
                                if !clusters.is_empty() {
                                    if dump_detail && let Some(prof) = gpu_prof.as_mut() {
                                        prof.update_last_render_summary(std::mem::take(
                                            &mut rp_log,
                                        ));
                                    }
                                    command_encoder.end_encoding();
                                    let copied_px = self.copy_regions_to_backdrop(
                                        command_buffer,
                                        target_texture,
                                        viewport_size,
                                        &clusters,
                                        &blur_rects,
                                        gpu_prof.as_mut(),
                                        hit_reason,
                                    );
                                    if let Some(p) = perf.as_mut() {
                                        if copied_px > 0 {
                                            p.blit_count += 1;
                                            p.blit_pixels += copied_px;
                                        }
                                        p.encoder_switches += 1;
                                    }
                                    command_encoder = new_command_encoder(
                                        command_buffer,
                                        target_texture,
                                        viewport_size,
                                        gpu_prof.as_mut(),
                                        render_pass_idx,
                                        |color_attachment| {
                                            color_attachment
                                                .set_load_action(metal::MTLLoadAction::Load);
                                        },
                                    );
                                    render_pass_idx += 1;
                                    backdrop_mips_valid = !blur_rects.is_empty();
                                    subtract_copied_from_dirty(&mut dirty_rects, &clusters);
                                } else if chunk_needs_blur && !backdrop_mips_valid {
                                    if dump_detail && let Some(prof) = gpu_prof.as_mut() {
                                        prof.update_last_render_summary(std::mem::take(
                                            &mut rp_log,
                                        ));
                                    }
                                    command_encoder.end_encoding();
                                    self.generate_backdrop_mipmaps(
                                        command_buffer,
                                        viewport_size,
                                        &chunk_blur_rects,
                                        gpu_prof.as_mut(),
                                    );
                                    if let Some(p) = perf.as_mut() {
                                        p.encoder_switches += 1;
                                    }
                                    command_encoder = new_command_encoder(
                                        command_buffer,
                                        target_texture,
                                        viewport_size,
                                        gpu_prof.as_mut(),
                                        render_pass_idx,
                                        |color_attachment| {
                                            color_attachment
                                                .set_load_action(metal::MTLLoadAction::Load);
                                        },
                                    );
                                    render_pass_idx += 1;
                                    backdrop_mips_valid = true;
                                }
                            } else if chunk_needs_blur && !backdrop_mips_valid {
                                if dump_detail && let Some(prof) = gpu_prof.as_mut() {
                                    prof.update_last_render_summary(std::mem::take(&mut rp_log));
                                }
                                command_encoder.end_encoding();
                                self.generate_backdrop_mipmaps(
                                    command_buffer,
                                    viewport_size,
                                    &chunk_blur_rects,
                                    gpu_prof.as_mut(),
                                );
                                if let Some(p) = perf.as_mut() {
                                    p.encoder_switches += 1;
                                }
                                command_encoder = new_command_encoder(
                                    command_buffer,
                                    target_texture,
                                    viewport_size,
                                    gpu_prof.as_mut(),
                                    render_pass_idx,
                                    |color_attachment| {
                                        color_attachment
                                            .set_load_action(metal::MTLLoadAction::Load);
                                    },
                                );
                                render_pass_idx += 1;
                                backdrop_mips_valid = true;
                            }
                        }

                        if dump_detail {
                            let px_sum: f64 = chunk
                                .iter()
                                .map(|q| (q.bounds.size.width.0 * q.bounds.size.height.0) as f64)
                                .sum();
                            rp_log.push_str(&format!(
                                "glass{}={}({:.2}Mpx) ",
                                if mode == 3 { "_sync" } else { "" },
                                chunk.len(),
                                px_sum / 1e6
                            ));
                        }
                        let ok = self.draw_liquid_glass_quads(
                            chunk,
                            mode == 3,
                            instance_buffer,
                            &mut instance_offset,
                            viewport_size,
                            command_encoder,
                        );
                        for q in chunk {
                            push_dirty(&mut dirty_rects, q.bounds, 0.0);
                        }
                        ok
                    } else if mode == 2 {
                        if let Some(p) = perf.as_mut() {
                            p.quads_wave += chunk.len() as u32;
                        }
                        if dump_detail {
                            let px_sum: f64 = chunk
                                .iter()
                                .map(|q| (q.bounds.size.width.0 * q.bounds.size.height.0) as f64)
                                .sum();
                            rp_log.push_str(&format!(
                                "wave={}({:.2}Mpx) ",
                                chunk.len(),
                                px_sum / 1e6
                            ));
                        }
                        for q in chunk {
                            push_dirty(&mut dirty_rects, q.bounds, 0.0);
                        }
                        self.draw_particle_wave_quads(
                            chunk,
                            instance_buffer,
                            &mut instance_offset,
                            viewport_size,
                            command_encoder,
                        )
                    } else {
                        if let Some(p) = perf.as_mut() {
                            p.quads_regular += chunk.len() as u32;
                        }
                        if dump_detail {
                            let px_sum: f64 = chunk
                                .iter()
                                .map(|q| (q.bounds.size.width.0 * q.bounds.size.height.0) as f64)
                                .sum();
                            rp_log.push_str(&format!(
                                "reg={}({:.2}Mpx",
                                chunk.len(),
                                px_sum / 1e6
                            ));
                            if px_sum > 500_000.0 {
                                rp_log.push('[');
                                for q in chunk {
                                    let area = q.bounds.size.width.0 * q.bounds.size.height.0;
                                    if area > 100_000.0 {
                                        rp_log.push_str(&format!(
                                            "{:.0}x{:.0}(bgA={:.2},bdA={:.2},bw={:.1}),",
                                            q.bounds.size.width.0,
                                            q.bounds.size.height.0,
                                            q.background.solid.a,
                                            q.border_color.a,
                                            q.border_widths.top.0,
                                        ));
                                    }
                                }
                                rp_log.push(']');
                            }
                            rp_log.push_str(") ");
                        }
                        for q in chunk {
                            push_dirty(&mut dirty_rects, q.bounds, 0.0);
                        }
                        self.draw_quads(
                            chunk,
                            mode == 4,
                            instance_buffer,
                            &mut instance_offset,
                            viewport_size,
                            command_encoder,
                        )
                    }
                }
                DrawItem::Paths(paths) => {
                    if let Some(p) = perf.as_mut() {
                        p.paths += paths.len() as u32;
                        p.encoder_switches += 1;
                    }
                    for p in paths {
                        push_dirty(&mut dirty_rects, p.bounds, 0.0);
                    }
                    if dump_detail && let Some(prof) = gpu_prof.as_mut() {
                        prof.update_last_render_summary(std::mem::take(&mut rp_log));
                    }
                    command_encoder.end_encoding();
                    self.ensure_path_intermediate_textures(viewport_size);

                    let did_draw = self.draw_paths_to_intermediate(
                        paths,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_buffer,
                    );

                    command_encoder = new_command_encoder(
                        command_buffer,
                        target_texture,
                        viewport_size,
                        gpu_prof.as_mut(),
                        render_pass_idx,
                        |color_attachment| {
                            color_attachment.set_load_action(metal::MTLLoadAction::Load);
                        },
                    );
                    render_pass_idx += 1;
                    if dump_detail {
                        rp_log.push_str(&format!("paths={} ", paths.len()));
                    }

                    if did_draw {
                        self.draw_paths_from_intermediate(
                            paths,
                            instance_buffer,
                            &mut instance_offset,
                            viewport_size,
                            command_encoder,
                        )
                    } else {
                        false
                    }
                }
                DrawItem::Underlines(underlines) => {
                    for u in underlines {
                        push_dirty(&mut dirty_rects, u.bounds, 4.0);
                    }
                    self.draw_underlines(
                        underlines,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_encoder,
                    )
                }
                DrawItem::MonochromeSprites {
                    texture_id,
                    sprites,
                } => {
                    if let Some(p) = perf.as_mut() {
                        p.monochrome_sprites += sprites.len() as u32;
                    }
                    if dump_detail {
                        rp_log.push_str(&format!("mono={} ", sprites.len()));
                    }
                    let mut line_rects: Vec<Bounds<ScaledPixels>> = Vec::with_capacity(32);
                    coalesce_text_lines(sprites, &mut line_rects);
                    for r in line_rects {
                        push_dirty(&mut dirty_rects, r, 0.0);
                    }
                    self.draw_monochrome_sprites(
                        texture_id,
                        sprites,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_encoder,
                    )
                }
                DrawItem::PolychromeSprites {
                    texture_id,
                    sprites,
                } => {
                    if let Some(p) = perf.as_mut() {
                        p.polychrome_sprites += sprites.len() as u32;
                    }
                    if dump_detail {
                        rp_log.push_str(&format!("poly={} ", sprites.len()));
                    }
                    for s in sprites {
                        push_dirty(&mut dirty_rects, s.bounds, 0.0);
                    }
                    self.draw_polychrome_sprites(
                        texture_id,
                        sprites,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_encoder,
                    )
                }
                DrawItem::Surfaces(surfaces) => {
                    for s in surfaces {
                        push_dirty(&mut dirty_rects, s.bounds, 0.0);
                    }
                    self.draw_surfaces(
                        surfaces,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_encoder,
                    )
                }
            };
            if !ok {
                command_encoder.end_encoding();
                anyhow::bail!(
                    "scene too large: {} paths, {} shadows, {} quads, {} underlines, {} mono, {} poly, {} surfaces",
                    scene.paths.len(),
                    scene.shadows.len(),
                    scene.quads.len(),
                    scene.underlines.len(),
                    scene.monochrome_sprites.len(),
                    scene.polychrome_sprites.len(),
                    scene.surfaces.len(),
                );
            }
        }

        if dump_detail && let Some(prof) = gpu_prof.as_mut() {
            prof.update_last_render_summary(std::mem::take(&mut rp_log));
        }
        command_encoder.end_encoding();

        if !self.device.has_unified_memory() {
            instance_buffer.metal_buffer.did_modify_range(NSRange {
                location: 0,
                length: instance_offset as NSUInteger,
            });
        }
        if let (Some(p), Some(start)) = (perf.as_mut(), encode_start) {
            p.encode_us = start.elapsed().as_micros() as u64;
            p.instance_bytes = instance_offset as u64;
        }
        Ok((command_buffer.to_owned(), perf, gpu_prof))
    }

    fn draw_paths_to_intermediate(
        &self,
        paths: &[Path<ScaledPixels>],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_buffer: &metal::CommandBufferRef,
    ) -> bool {
        if paths.is_empty() {
            return true;
        }
        let Some(intermediate_texture) = &self.path_intermediate_texture else {
            return false;
        };

        let render_pass_descriptor = metal::RenderPassDescriptor::new();
        let color_attachment = render_pass_descriptor
            .color_attachments()
            .object_at(0)
            .unwrap();
        color_attachment.set_load_action(metal::MTLLoadAction::Clear);
        color_attachment.set_clear_color(metal::MTLClearColor::new(0., 0., 0., 0.));

        if let Some(msaa_texture) = &self.path_intermediate_msaa_texture {
            color_attachment.set_texture(Some(msaa_texture));
            color_attachment.set_resolve_texture(Some(intermediate_texture));
            color_attachment.set_store_action(metal::MTLStoreAction::MultisampleResolve);
        } else {
            color_attachment.set_texture(Some(intermediate_texture));
            color_attachment.set_store_action(metal::MTLStoreAction::Store);
        }

        let command_encoder = command_buffer.new_render_command_encoder(render_pass_descriptor);
        command_encoder.set_render_pipeline_state(&self.paths_rasterization_pipeline_state);

        align_offset(instance_offset);
        let mut vertices = Vec::new();
        for path in paths {
            vertices.extend(path.vertices.iter().map(|v| PathRasterizationVertex {
                xy_position: v.xy_position,
                st_position: v.st_position,
                color: path.color,
                bounds: path.bounds.intersect(&path.content_mask.bounds),
            }));
        }
        let vertices_bytes_len = mem::size_of_val(vertices.as_slice());
        let next_offset = *instance_offset + vertices_bytes_len;
        if next_offset > instance_buffer.size {
            command_encoder.end_encoding();
            return false;
        }
        command_encoder.set_vertex_buffer(
            PathRasterizationInputIndex::Vertices as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_vertex_bytes(
            PathRasterizationInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_fragment_buffer(
            PathRasterizationInputIndex::Vertices as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        let buffer_contents =
            unsafe { (instance_buffer.metal_buffer.contents() as *mut u8).add(*instance_offset) };
        unsafe {
            ptr::copy_nonoverlapping(
                vertices.as_ptr() as *const u8,
                buffer_contents,
                vertices_bytes_len,
            );
        }
        command_encoder.draw_primitives(
            metal::MTLPrimitiveType::Triangle,
            0,
            vertices.len() as u64,
        );
        *instance_offset = next_offset;

        command_encoder.end_encoding();
        true
    }

    fn draw_shadows(
        &self,
        shadows: &[Shadow],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        if shadows.is_empty() {
            return true;
        }
        align_offset(instance_offset);

        command_encoder.set_render_pipeline_state(&self.shadows_pipeline_state);
        command_encoder.set_vertex_buffer(
            ShadowInputIndex::Vertices as u64,
            Some(&self.unit_vertices),
            0,
        );
        command_encoder.set_vertex_buffer(
            ShadowInputIndex::Shadows as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_fragment_buffer(
            ShadowInputIndex::Shadows as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );

        command_encoder.set_vertex_bytes(
            ShadowInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );

        let shadow_bytes_len = mem::size_of_val(shadows);
        let buffer_contents =
            unsafe { (instance_buffer.metal_buffer.contents() as *mut u8).add(*instance_offset) };

        let next_offset = *instance_offset + shadow_bytes_len;
        if next_offset > instance_buffer.size {
            return false;
        }

        unsafe {
            ptr::copy_nonoverlapping(
                shadows.as_ptr() as *const u8,
                buffer_contents,
                shadow_bytes_len,
            );
        }

        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            shadows.len() as u64,
        );
        *instance_offset = next_offset;
        true
    }

    fn draw_quads(
        &self,
        quads: &[Quad],
        use_solid_pipeline: bool,
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        if quads.is_empty() {
            return true;
        }
        align_offset(instance_offset);

        let pipeline = if use_solid_pipeline {
            &self.solid_quads_pipeline_state
        } else {
            &self.quads_pipeline_state
        };
        command_encoder.set_render_pipeline_state(pipeline);
        command_encoder.set_vertex_buffer(
            QuadInputIndex::Vertices as u64,
            Some(&self.unit_vertices),
            0,
        );
        command_encoder.set_vertex_buffer(
            QuadInputIndex::Quads as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        if !use_solid_pipeline {
            command_encoder.set_fragment_buffer(
                QuadInputIndex::Quads as u64,
                Some(&instance_buffer.metal_buffer),
                *instance_offset as u64,
            );
        }

        command_encoder.set_vertex_bytes(
            QuadInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );

        let quad_bytes_len = mem::size_of_val(quads);
        let buffer_contents =
            unsafe { (instance_buffer.metal_buffer.contents() as *mut u8).add(*instance_offset) };

        let next_offset = *instance_offset + quad_bytes_len;
        if next_offset > instance_buffer.size {
            return false;
        }

        unsafe {
            ptr::copy_nonoverlapping(quads.as_ptr() as *const u8, buffer_contents, quad_bytes_len);
        }

        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            quads.len() as u64,
        );
        *instance_offset = next_offset;
        true
    }

    fn copy_regions_to_backdrop(
        &self,
        command_buffer: &metal::CommandBufferRef,
        drawable_tex: &metal::TextureRef,
        viewport_size: Size<DevicePixels>,
        regions: &[Bounds<ScaledPixels>],
        mip_regions: &[Bounds<ScaledPixels>],
        profiler: Option<&mut FrameGpuProfiler>,
        reason: String,
    ) -> u64 {
        let Some(backdrop_tex) = &self.backdrop_texture else {
            return 0;
        };
        let max_w = (viewport_size.width.0.max(0) as u64)
            .min(drawable_tex.width())
            .min(backdrop_tex.width());
        let max_h = (viewport_size.height.0.max(0) as u64)
            .min(drawable_tex.height())
            .min(backdrop_tex.height());
        if max_w == 0 || max_h == 0 {
            return 0;
        }

        let mut rects: Vec<(u64, u64, u64, u64)> = Vec::with_capacity(regions.len());
        let mut total_px = 0u64;
        for r in regions {
            let x0 = (r.origin.x.0.floor().max(0.0) as u64).min(max_w);
            let y0 = (r.origin.y.0.floor().max(0.0) as u64).min(max_h);
            let x1 = (r.bottom_right().x.0.ceil().max(0.0) as u64).min(max_w);
            let y1 = (r.bottom_right().y.0.ceil().max(0.0) as u64).min(max_h);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let cw = x1 - x0;
            let ch = y1 - y0;
            rects.push((x0, y0, cw, ch));
            total_px += cw * ch;
        }
        let do_mips = !mip_regions.is_empty() && backdrop_tex.mipmap_level_count() > 1;
        if rects.is_empty() && !do_mips {
            return 0;
        }
        let mut profiler = profiler;
        if !rects.is_empty() {
            let enc = unsafe {
                attach_blit_timestamps(
                    command_buffer,
                    profiler.as_deref_mut(),
                    GpuPassKind::BlitBackdrop {
                        pixels: total_px,
                        mipmaps: false,
                        regions: rects.len(),
                        reason,
                    },
                )
            };
            for &(x0, y0, cw, ch) in &rects {
                enc.copy_from_texture(
                    drawable_tex,
                    0,
                    0,
                    metal::MTLOrigin { x: x0, y: y0, z: 0 },
                    metal::MTLSize {
                        width: cw,
                        height: ch,
                        depth: 1,
                    },
                    backdrop_tex,
                    0,
                    0,
                    metal::MTLOrigin { x: x0, y: y0, z: 0 },
                );
            }
            enc.end_encoding();
        }
        if do_mips {
            self.generate_backdrop_mipmaps_for_rects(
                command_buffer,
                viewport_size,
                mip_regions,
                profiler,
            );
        }
        total_px
    }

    fn generate_backdrop_mipmaps_for_rects(
        &self,
        command_buffer: &metal::CommandBufferRef,
        viewport_size: Size<DevicePixels>,
        regions: &[Bounds<ScaledPixels>],
        profiler: Option<&mut FrameGpuProfiler>,
    ) {
        let Some(backdrop_tex) = &self.backdrop_texture else {
            return;
        };
        let mip_count = backdrop_tex.mipmap_level_count() as usize;
        if mip_count <= 1 || self.backdrop_mip_views.len() < mip_count {
            return;
        }
        let base_w = (viewport_size.width.0.max(0) as u64).min(backdrop_tex.width());
        let base_h = (viewport_size.height.0.max(0) as u64).min(backdrop_tex.height());
        if base_w == 0 || base_h == 0 {
            return;
        }

        let coalesced_regions = if regions.len() > 1 {
            coalesce_copy_regions(regions, 16.0, 6)
        } else {
            regions.to_vec()
        };
        let mut base_rects: Vec<(u64, u64, u64, u64)> =
            Vec::with_capacity(coalesced_regions.len().max(1));
        if coalesced_regions.is_empty() {
            base_rects.push((0, 0, base_w, base_h));
        } else {
            for r in &coalesced_regions {
                let x0 = (r.origin.x.0.floor().max(0.0) as u64).min(base_w);
                let y0 = (r.origin.y.0.floor().max(0.0) as u64).min(base_h);
                let x1 = (r.bottom_right().x.0.ceil().max(0.0) as u64).min(base_w);
                let y1 = (r.bottom_right().y.0.ceil().max(0.0) as u64).min(base_h);
                if x1 > x0 && y1 > y0 {
                    base_rects.push((x0, y0, x1, y1));
                }
            }
        }
        if base_rects.is_empty() {
            return;
        }

        let total_l0_px: u64 = base_rects.iter().map(|(x0, y0, x1, y1)| (x1 - x0) * (y1 - y0)).sum();
        let compute = unsafe {
            attach_compute_timestamps_kind(
                command_buffer,
                profiler,
                GpuPassKind::BlitBackdrop {
                    pixels: total_l0_px / 3,
                    mipmaps: true,
                    regions: base_rects.len(),
                    reason: format!("compute_mip_rects(levels=1..{})", mip_count - 1),
                },
            )
        };
        compute.set_compute_pipeline_state(&self.backdrop_downsample_pipeline);
        compute.set_sampler_state(0, Some(&self.backdrop_sampler));
        let tg = metal::MTLSize::new(16, 16, 1);

        for lvl in 1..mip_count {
            if lvl > 1 {
                compute.memory_barrier_with_resources(&[backdrop_tex.as_ref()]);
            }
            let src_w = (base_w >> (lvl - 1)).max(1);
            let src_h = (base_h >> (lvl - 1)).max(1);
            let dst_w_max = (base_w >> lvl).max(1);
            let dst_h_max = (base_h >> lvl).max(1);
            let inv_src_w = 1.0 / (src_w as f32);
            let inv_src_h = 1.0 / (src_h as f32);
            compute.set_texture(0, Some(&self.backdrop_mip_views[lvl - 1]));
            compute.set_texture(1, Some(&self.backdrop_mip_views[lvl]));
            let shift = lvl as u64;
            let round = (1u64 << shift) - 1;
            for &(x0, y0, x1, y1) in &base_rects {
                let dx0 = (x0 >> shift).min(dst_w_max.saturating_sub(1));
                let dy0 = (y0 >> shift).min(dst_h_max.saturating_sub(1));
                let dx1 = ((x1 + round) >> shift).min(dst_w_max).max(dx0 + 1);
                let dy1 = ((y1 + round) >> shift).min(dst_h_max).max(dy0 + 1);
                let u = MipRectUniforms {
                    dst_ox: dx0 as u32,
                    dst_oy: dy0 as u32,
                    dst_w: (dx1 - dx0) as u32,
                    dst_h: (dy1 - dy0) as u32,
                    inv_src_w,
                    inv_src_h,
                };
                compute.set_bytes(
                    0,
                    mem::size_of::<MipRectUniforms>() as u64,
                    &u as *const MipRectUniforms as *const _,
                );
                compute.dispatch_threads(metal::MTLSize::new(dx1 - dx0, dy1 - dy0, 1), tg);
            }
        }
        compute.end_encoding();
    }

    fn generate_backdrop_mipmaps(
        &self,
        command_buffer: &metal::CommandBufferRef,
        viewport_size: Size<DevicePixels>,
        regions: &[Bounds<ScaledPixels>],
        profiler: Option<&mut FrameGpuProfiler>,
    ) {
        self.generate_backdrop_mipmaps_for_rects(command_buffer, viewport_size, regions, profiler);
    }

    fn draw_liquid_glass_quads(
        &self,
        quads: &[Quad],
        use_synced_pipeline: bool,
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        if quads.is_empty() {
            return true;
        }
        align_offset(instance_offset);

        let pipeline = if use_synced_pipeline {
            let chunk_has_blur = quads
                .iter()
                .any(|q| q.background.colors[1].percentage * 2.0 > 0.5);
            if chunk_has_blur {
                &self.liquid_glass_synced_blur_pipeline_state
            } else {
                &self.liquid_glass_synced_sharp_pipeline_state
            }
        } else {
            &self.liquid_glass_pipeline_state
        };
        command_encoder.set_render_pipeline_state(pipeline);
        command_encoder.set_vertex_buffer(
            QuadInputIndex::Vertices as u64,
            Some(&self.unit_vertices),
            0,
        );
        command_encoder.set_vertex_buffer(
            QuadInputIndex::Quads as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_vertex_bytes(
            QuadInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );
        if !use_synced_pipeline {
            command_encoder.set_fragment_buffer(
                QuadInputIndex::Quads as u64,
                Some(&instance_buffer.metal_buffer),
                *instance_offset as u64,
            );
            command_encoder.set_fragment_bytes(
                QuadInputIndex::ViewportSize as u64,
                mem::size_of_val(&viewport_size) as u64,
                &viewport_size as *const Size<DevicePixels> as *const _,
            );
        }
        let time_sec = self.start_time.elapsed().as_secs_f32();
        command_encoder.set_fragment_bytes(
            3,
            mem::size_of::<f32>() as u64,
            &time_sec as *const f32 as *const _,
        );
        if let Some(backdrop_tex) = &self.backdrop_texture {
            command_encoder.set_fragment_texture(0, Some(backdrop_tex));
        }
        command_encoder.set_fragment_sampler_state(0, Some(&self.backdrop_sampler));

        let quad_bytes_len = mem::size_of_val(quads);
        let buffer_contents =
            unsafe { (instance_buffer.metal_buffer.contents() as *mut u8).add(*instance_offset) };

        let next_offset = *instance_offset + quad_bytes_len;
        if next_offset > instance_buffer.size {
            return false;
        }

        unsafe {
            ptr::copy_nonoverlapping(quads.as_ptr() as *const u8, buffer_contents, quad_bytes_len);
        }

        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            quads.len() as u64,
        );
        *instance_offset = next_offset;
        true
    }

    fn release_particle_grid_buffer(&mut self) {
        self.particle_tile_grid_cache_key = None;
        self.particle_tile_img_texture = None;
        self.particle_tile_grid_needs_clear = true;
        if self.particle_tile_grid_capacity > 1 {
            self.particle_tile_grid_capacity = 1;
            self.particle_tile_grid_buffer = self
                .device
                .new_buffer(4, MTLResourceOptions::StorageModePrivate);
        }
    }

    fn ensure_particle_2d_texture(
        device: &metal::DeviceRef,
        slot: &mut Option<metal::Texture>,
        w: u32,
        h: u32,
    ) -> bool {
        let needs_alloc = match slot {
            Some(tex) => tex.width() != w as u64 || tex.height() != h as u64,
            None => true,
        };
        if needs_alloc {
            let desc = metal::TextureDescriptor::new();
            desc.set_texture_type(metal::MTLTextureType::D2);
            desc.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
            desc.set_width(w.max(1) as u64);
            desc.set_height(h.max(1) as u64);
            desc.set_usage(
                metal::MTLTextureUsage::ShaderRead | metal::MTLTextureUsage::ShaderWrite,
            );
            desc.set_storage_mode(metal::MTLStorageMode::Private);
            *slot = Some(device.new_texture(&desc));
            true
        } else {
            false
        }
    }

    fn ensure_particle_facet_lut(
        &mut self,
        batches: &[PrimitiveBatch<'_>],
        command_buffer: &metal::CommandBufferRef,
        profiler: Option<&mut FrameGpuProfiler>,
    ) {
        let wave_quad = batches.iter().find_map(|b| {
            if let PrimitiveBatch::Quads(qs) = b {
                qs.iter().find(|q| q.background.tag == BackgroundTag::ParticleWave)
            } else {
                None
            }
        });
        let Some(quad) = wave_quad else {
            return;
        };

        let grid_w = self.current_particle_uniforms.grid_w.max(1);
        let grid_h = self.current_particle_uniforms.grid_h.max(1);
        let (col_blue, col_violet, col_cyan) = if quad.background.colors[0].color.a > 0.01 {
            let c0: crate::Rgba = quad.background.colors[0].color.into();
            let c1: crate::Rgba = quad.background.colors[1].color.into();
            let cb = [c0.r, c0.g, c0.b, 1.0];
            let cv = [c1.r, c1.g, c1.b, 1.0];
            let cc = [
                cb[0] + (0.85 - cb[0]) * 0.35,
                cb[1] + (0.92 - cb[1]) * 0.35,
                cb[2] + (1.0 - cb[2]) * 0.35,
                1.0,
            ];
            (cb, cv, cc)
        } else {
            (
                [0.28f32, 0.50, 0.92, 1.0],
                [0.55f32, 0.42, 0.92, 1.0],
                [0.32f32, 0.76, 0.88, 1.0],
            )
        };

        let key = FacetLutCacheKey {
            grid_w,
            grid_h,
            col_blue_bits: [col_blue[0].to_bits(), col_blue[1].to_bits(), col_blue[2].to_bits()],
            col_violet_bits: [
                col_violet[0].to_bits(),
                col_violet[1].to_bits(),
                col_violet[2].to_bits(),
            ],
            col_cyan_bits: [col_cyan[0].to_bits(), col_cyan[1].to_bits(), col_cyan[2].to_bits()],
        };

        let reallocated = Self::ensure_particle_2d_texture(
            &self.device,
            &mut self.particle_facet_lut_texture,
            grid_w,
            grid_h,
        );
        if !reallocated && self.particle_facet_lut_cache_key == Some(key) {
            return;
        }

        let Some(ref lut_tex) = self.particle_facet_lut_texture else {
            return;
        };
        let uniforms = FacetLutUniforms {
            grid_w,
            grid_h,
            _pad0: [0.0; 2],
            col_blue,
            col_violet,
            col_cyan,
        };
        let compute = unsafe { attach_compute_timestamps(command_buffer, profiler) };
        compute.set_compute_pipeline_state(&self.particle_facet_lut_pipeline);
        compute.set_texture(0, Some(lut_tex));
        compute.set_bytes(
            0,
            mem::size_of::<FacetLutUniforms>() as u64,
            &uniforms as *const FacetLutUniforms as *const _,
        );
        let tg = metal::MTLSize::new(16, 16, 1);
        let grid = metal::MTLSize::new(grid_w as u64, grid_h as u64, 1);
        compute.dispatch_threads(grid, tg);
        compute.end_encoding();
        self.particle_facet_lut_cache_key = Some(key);
    }

    fn release_custom_backdrop_images(&mut self) {
        if !self.backdrop_image_textures.is_empty() {
            self.backdrop_image_textures.clear();
            self.backdrop_image_sizes.clear();
            self.backdrop_custom_version = self.backdrop_custom_version.wrapping_add(1);
            self.particle_tile_grid_cache_key = None;
        }
        if !self.backdrop_loaded_entries.is_empty() {
            self.backdrop_loaded_entries.clear();
        }
        self.backdrop_images_loaded = false;
        self.backdrop_config_mtime = None;
    }

    fn sync_override_image(&mut self, enabled: bool) {
        if !enabled {
            self.backdrop_pending_override = None;
            self.backdrop_override_seen_gen = 0;
            return;
        }
        let current_gen = crate::particle_wave_override_image_gen();
        if current_gen == self.backdrop_override_seen_gen {
            return;
        }
        let (locked_gen, path_opt) = crate::particle_wave_override_image();
        self.backdrop_override_seen_gen = locked_gen;
        self.backdrop_pending_override = path_opt.and_then(|path| {
            load_single_backdrop_image(&self.device, &path)
                .map(|(tex, dims)| (locked_gen, tex, dims))
        });
    }

    fn sync_live_video(&mut self, enabled: bool) {
        const LIVE_VIDEO_STALE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
        if !enabled {
            self.backdrop_live_video = None;
            self.backdrop_live_video_seen_gen = 0;
            return;
        }
        let current_gen = crate::particle_wave_live_video_gen();
        if current_gen != self.backdrop_live_video_seen_gen {
            let (locked_gen, frame_opt) = crate::particle_wave_live_video_frame();
            self.backdrop_live_video_seen_gen = locked_gen;
            match frame_opt {
                Some(frame) if frame.updated_at.elapsed() <= LIVE_VIDEO_STALE_TIMEOUT => {
                    let w = frame.width as u64;
                    let h = frame.height as u64;
                    let can_reuse = self.backdrop_live_video.as_ref().is_some_and(|(_, tex, _, _)| {
                        tex.width() == w && tex.height() == h
                    });
                    if can_reuse
                        && let Some((ref mut gen_slot, ref tex, ref mut dims, ref mut ts)) =
                            self.backdrop_live_video
                    {
                        tex.replace_region(
                            metal::MTLRegion::new_2d(0, 0, w, h),
                            0,
                            frame.rgba.as_ptr() as *const _,
                            w * 4,
                        );
                        *gen_slot = locked_gen;
                        *dims = [frame.width as f32, frame.height as f32, 1.0];
                        *ts = frame.updated_at;
                    } else {
                        let desc = metal::TextureDescriptor::new();
                        desc.set_texture_type(metal::MTLTextureType::D2);
                        desc.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
                        desc.set_width(w);
                        desc.set_height(h);
                        desc.set_usage(metal::MTLTextureUsage::ShaderRead);
                        desc.set_storage_mode(if self.device.has_unified_memory() {
                            metal::MTLStorageMode::Shared
                        } else {
                            metal::MTLStorageMode::Managed
                        });
                        let tex = self.device.new_texture(&desc);
                        tex.replace_region(
                            metal::MTLRegion::new_2d(0, 0, w, h),
                            0,
                            frame.rgba.as_ptr() as *const _,
                            w * 4,
                        );
                        self.backdrop_live_video = Some((
                            locked_gen,
                            tex,
                            [frame.width as f32, frame.height as f32, 1.0],
                            frame.updated_at,
                        ));
                    }
                }
                _ => {
                    self.backdrop_live_video = None;
                }
            }
        } else if let Some((_, _, _, updated_at)) = self.backdrop_live_video
            && updated_at.elapsed() > LIVE_VIDEO_STALE_TIMEOUT
        {
            self.backdrop_live_video = None;
        }
    }

    fn sync_backdrop_images(&mut self) {
        if !self.backdrop_images_loaded {
            let (textures, sizes, entries, cfg_path, mtime) = load_backdrop_images(&self.device);
            self.backdrop_image_textures = textures;
            self.backdrop_image_sizes = sizes;
            self.backdrop_loaded_entries = entries;
            self.backdrop_config_path = cfg_path;
            self.backdrop_config_mtime = mtime;
            self.backdrop_images_loaded = true;
            self.backdrop_last_poll = Instant::now();
            self.backdrop_phase_last_tick = Instant::now();
            return;
        }
        if self.backdrop_last_poll.elapsed() < std::time::Duration::from_millis(400) {
            return;
        }
        self.backdrop_last_poll = Instant::now();
        let Some(ref path) = self.backdrop_config_path else {
            return;
        };
        let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        if mtime == self.backdrop_config_mtime {
            return;
        }
        self.backdrop_config_mtime = mtime;
        let paths = read_backdrop_paths_from_config(path);
        let next_entries = snapshot_backdrop_entry_keys(&paths);
        if next_entries == self.backdrop_loaded_entries {
            return;
        }
        let (textures, sizes) = load_backdrop_images_from_entries(&self.device, &next_entries);
        self.backdrop_image_textures = textures;
        self.backdrop_image_sizes = sizes;
        self.backdrop_loaded_entries = next_entries;
        self.backdrop_custom_version = self.backdrop_custom_version.wrapping_add(1);
        self.particle_tile_grid_cache_key = None;
    }

    fn dispatch_particle_tile_scatter(
        &mut self,
        command_buffer: &metal::CommandBufferRef,
        viewport_size: Size<DevicePixels>,
        mut profiler: Option<&mut FrameGpuProfiler>,
    ) -> bool {
        let time_sec = self.start_time.elapsed().as_secs_f32();
        let vw = viewport_size.width.0.max(1) as f32;
        let vh = viewport_size.height.0.max(1) as f32;
        let tuning = crate::particle_wave_tuning().clamped();
        let tile_size = tuning.tile_size_px.round().max(2.0);
        let grid_w = (vw / tile_size).ceil().max(1.0) as u32;
        let grid_h = (vh / tile_size).ceil().max(1.0) as u32;
        crate::set_particle_wave_grid_dimensions(grid_w, grid_h);

        self.sync_override_image(tuning.apple_music_artwork_enabled);
        self.sync_live_video(tuning.youtube_mirror_enabled);

        if tuning.image_enabled {
            self.sync_backdrop_images();
        } else {
            if self.backdrop_images_loaded {
                self.release_custom_backdrop_images();
            }
            if matches!(self.backdrop_active_source, ActiveBackdropSource::Custom(_)) {
                self.backdrop_active_source = ActiveBackdropSource::None;
                self.backdrop_phase_stage = 0;
                self.backdrop_phase_elapsed = 0.0;
            }
        }

        let custom_count = if tuning.image_enabled {
            self.backdrop_image_textures.len()
        } else {
            0
        };
        if custom_count == 0
            && matches!(self.backdrop_active_source, ActiveBackdropSource::Custom(_))
        {
            self.backdrop_active_source = ActiveBackdropSource::None;
            self.backdrop_phase_stage = 0;
            self.backdrop_phase_elapsed = 0.0;
        }

        let desired_source = if let Some((pending_gen, _, _)) = self.backdrop_pending_override {
            ActiveBackdropSource::Override(pending_gen)
        } else if let Some((live_gen, _, _, _)) = self.backdrop_live_video {
            ActiveBackdropSource::LiveVideo(live_gen)
        } else if custom_count > 0 {
            self.backdrop_image_index %= custom_count;
            ActiveBackdropSource::Custom(self.backdrop_image_index)
        } else {
            ActiveBackdropSource::None
        };

        if matches!(desired_source, ActiveBackdropSource::LiveVideo(_))
            && !matches!(self.backdrop_active_source, ActiveBackdropSource::Override(_))
        {
            self.backdrop_active_override = None;
            self.backdrop_active_source = desired_source;
            self.backdrop_phase_stage = 0;
            self.backdrop_phase_elapsed = 0.0;
            self.backdrop_phase_last_tick = Instant::now();
        } else if matches!(self.backdrop_active_source, ActiveBackdropSource::LiveVideo(_))
            && !matches!(desired_source, ActiveBackdropSource::LiveVideo(_))
        {
            match desired_source {
                ActiveBackdropSource::Override(_) => {
                    self.backdrop_active_override = self.backdrop_pending_override.clone();
                    self.backdrop_active_source = desired_source;
                    self.backdrop_phase_stage = 3;
                    self.backdrop_phase_elapsed = 0.0;
                    self.backdrop_phase_last_tick = Instant::now();
                }
                ActiveBackdropSource::Custom(idx) => {
                    self.backdrop_active_override = None;
                    self.backdrop_active_source = ActiveBackdropSource::Custom(idx);
                    self.backdrop_phase_stage = 3;
                    self.backdrop_phase_elapsed = 0.0;
                    self.backdrop_phase_last_tick = Instant::now();
                }
                ActiveBackdropSource::None => {
                    self.backdrop_active_override = None;
                    self.backdrop_active_source = ActiveBackdropSource::None;
                    self.backdrop_phase_stage = 0;
                    self.backdrop_phase_elapsed = 0.0;
                    self.release_particle_grid_buffer();
                    self.current_particle_uniforms =
                        inactive_particle_uniforms(time_sec, vw, vh, tuning);
                    return false;
                }
                ActiveBackdropSource::LiveVideo(_) => {}
            }
        }

        if self.backdrop_active_source == ActiveBackdropSource::None {
            if desired_source == ActiveBackdropSource::None {
                self.backdrop_active_override = None;
                self.release_particle_grid_buffer();
                self.current_particle_uniforms =
                    inactive_particle_uniforms(time_sec, vw, vh, tuning);
                return false;
            }
            match desired_source {
                ActiveBackdropSource::Override(_) => {
                    self.backdrop_active_override = self.backdrop_pending_override.clone();
                    self.backdrop_active_source = desired_source;
                    self.backdrop_phase_stage = 3;
                }
                ActiveBackdropSource::LiveVideo(_) => {
                    self.backdrop_active_override = None;
                    self.backdrop_active_source = desired_source;
                    self.backdrop_phase_stage = 0;
                }
                ActiveBackdropSource::Custom(idx) => {
                    self.backdrop_active_override = None;
                    self.backdrop_active_source = ActiveBackdropSource::Custom(idx);
                    self.backdrop_phase_stage = 3;
                }
                ActiveBackdropSource::None => {}
            }
            self.backdrop_phase_elapsed = 0.0;
            self.backdrop_phase_last_tick = Instant::now();
        }

        let hold_sec = tuning.image_hold_secs;
        let disintegrate_sec = 6.2_f32;
        let void_sec = 0.7_f32;
        let reassemble_sec = 6.2_f32;

        let needs_switch = match (self.backdrop_active_source, desired_source) {
            (ActiveBackdropSource::Override(cur), ActiveBackdropSource::Override(want)) => {
                cur != want
            }
            (ActiveBackdropSource::Override(_), _) => true,
            (ActiveBackdropSource::Custom(_), ActiveBackdropSource::Override(_)) => true,
            (ActiveBackdropSource::Custom(_), ActiveBackdropSource::None) => true,
            _ => false,
        };

        if needs_switch {
            if self.backdrop_phase_stage == 0 {
                self.backdrop_phase_stage = 1;
                self.backdrop_phase_elapsed = 0.0;
            } else if self.backdrop_phase_stage == 3 {
                let p = (self.backdrop_phase_elapsed / reassemble_sec).clamp(0.0, 1.0);
                self.backdrop_phase_stage = 1;
                self.backdrop_phase_elapsed = (1.0 - p) * disintegrate_sec;
            }
        }

        let now = Instant::now();
        let dt = now
            .saturating_duration_since(self.backdrop_phase_last_tick)
            .as_secs_f32()
            .min(0.25);
        self.backdrop_phase_last_tick = now;

        let stay_in_hold = matches!(
            self.backdrop_active_source,
            ActiveBackdropSource::Override(_) | ActiveBackdropSource::LiveVideo(_)
        ) && !needs_switch;

        if !(stay_in_hold && self.backdrop_phase_stage == 0) {
            self.backdrop_phase_elapsed += dt;
            loop {
                let stage_dur = match self.backdrop_phase_stage {
                    0 => hold_sec,
                    1 => disintegrate_sec,
                    2 => void_sec,
                    _ => reassemble_sec,
                };
                if self.backdrop_phase_elapsed < stage_dur {
                    break;
                }
                self.backdrop_phase_elapsed -= stage_dur;
                let prev_stage = self.backdrop_phase_stage;
                self.backdrop_phase_stage = (self.backdrop_phase_stage + 1) % 4;

                if prev_stage == 1 {
                    let was_custom =
                        matches!(self.backdrop_active_source, ActiveBackdropSource::Custom(_));
                    match desired_source {
                        ActiveBackdropSource::Override(_) => {
                            self.backdrop_active_override = self.backdrop_pending_override.clone();
                            self.backdrop_active_source = desired_source;
                        }
                        ActiveBackdropSource::LiveVideo(_) => {
                            self.backdrop_active_override = None;
                            self.backdrop_active_source = desired_source;
                            self.backdrop_phase_stage = 0;
                            self.backdrop_phase_elapsed = 0.0;
                            break;
                        }
                        ActiveBackdropSource::Custom(_) if custom_count > 0 => {
                            if was_custom {
                                self.backdrop_image_index =
                                    (self.backdrop_image_index + 1) % custom_count;
                            } else {
                                self.backdrop_image_index %= custom_count;
                            }
                            self.backdrop_active_override = None;
                            self.backdrop_active_source =
                                ActiveBackdropSource::Custom(self.backdrop_image_index);
                        }
                        _ => {
                            self.backdrop_active_override = None;
                            self.backdrop_active_source = ActiveBackdropSource::None;
                            self.backdrop_phase_stage = 0;
                            self.backdrop_phase_elapsed = 0.0;
                            self.release_particle_grid_buffer();
                            self.current_particle_uniforms =
                                inactive_particle_uniforms(time_sec, vw, vh, tuning);
                            return false;
                        }
                    }
                } else if prev_stage == 2 {
                    match desired_source {
                        ActiveBackdropSource::Override(want_gen) => {
                            if self.backdrop_active_source
                                != ActiveBackdropSource::Override(want_gen)
                            {
                                self.backdrop_active_override =
                                    self.backdrop_pending_override.clone();
                                self.backdrop_active_source =
                                    ActiveBackdropSource::Override(want_gen);
                            }
                        }
                        ActiveBackdropSource::LiveVideo(_) => {
                            self.backdrop_active_override = None;
                            self.backdrop_active_source = desired_source;
                            self.backdrop_phase_stage = 0;
                            self.backdrop_phase_elapsed = 0.0;
                            break;
                        }
                        ActiveBackdropSource::Custom(_) if custom_count > 0 => {
                            self.backdrop_image_index %= custom_count;
                            self.backdrop_active_override = None;
                            self.backdrop_active_source =
                                ActiveBackdropSource::Custom(self.backdrop_image_index);
                        }
                        _ => {
                            self.backdrop_active_override = None;
                            self.backdrop_active_source = ActiveBackdropSource::None;
                            self.backdrop_phase_stage = 0;
                            self.backdrop_phase_elapsed = 0.0;
                            self.release_particle_grid_buffer();
                            self.current_particle_uniforms =
                                inactive_particle_uniforms(time_sec, vw, vh, tuning);
                            return false;
                        }
                    }
                } else if prev_stage == 3 && stay_in_hold {
                    self.backdrop_phase_elapsed = 0.0;
                    break;
                }
            }
        }

        let active_tex_and_dims: Option<(&metal::Texture, [f32; 3])> = match self
            .backdrop_active_source
        {
            ActiveBackdropSource::Override(_) => self
                .backdrop_active_override
                .as_ref()
                .map(|(_, tex, dims)| (tex, *dims)),
            ActiveBackdropSource::LiveVideo(_) => self
                .backdrop_live_video
                .as_ref()
                .map(|(_, tex, dims, _)| (tex, *dims)),
            ActiveBackdropSource::Custom(idx) if idx < self.backdrop_image_textures.len() => {
                Some((
                    &self.backdrop_image_textures[idx],
                    self.backdrop_image_sizes[idx],
                ))
            }
            _ => None,
        };

        let Some((active_tex, dims)) = active_tex_and_dims else {
            self.backdrop_active_source = ActiveBackdropSource::None;
            self.backdrop_active_override = None;
            self.release_particle_grid_buffer();
            self.current_particle_uniforms = inactive_particle_uniforms(time_sec, vw, vh, tuning);
            return false;
        };

        let needed = (grid_w as usize) * (grid_h as usize);
        let tex_reallocated = Self::ensure_particle_2d_texture(
            &self.device,
            &mut self.particle_tile_img_texture,
            grid_w,
            grid_h,
        );
        if tex_reallocated {
            self.particle_tile_grid_cache_key = None;
        }

        let stage_dur = match self.backdrop_phase_stage {
            0 => hold_sec,
            1 => disintegrate_sec,
            2 => void_sec,
            _ => reassemble_sec,
        };
        let phase_mode = self.backdrop_phase_stage as f32;
        let phase_progress = if stay_in_hold && self.backdrop_phase_stage == 0 {
            0.0
        } else {
            (self.backdrop_phase_elapsed / stage_dur.max(0.001)).clamp(0.0, 1.0)
        };
        let is_void_phase = phase_mode > 1.5 && phase_mode < 2.5;
        let uniforms = ParticleWaveUniforms {
            time_sec,
            has_image: if is_void_phase { 0.0 } else { 1.0 },
            image_w: dims[0],
            image_h: dims[1],
            phase_mode,
            phase_progress,
            grid_w,
            grid_h,
            viewport_w: vw,
            viewport_h: vh,
            tile_size,
            fit_cover: dims[2],
            image_brightness: tuning.image_brightness,
            image_peak_luminance: tuning.image_peak_luminance,
            image_saturation: tuning.image_saturation,
            _pad0: 0.0,
        };
        self.current_particle_uniforms = uniforms;

        let is_static_phase = self.backdrop_phase_stage == 0 || self.backdrop_phase_stage == 2;
        let cache_key = is_static_phase.then_some(ParticleTileGridCacheKey {
            source: self.backdrop_active_source,
            custom_textures_version: self.backdrop_custom_version,
            grid_w,
            grid_h,
            viewport_w_bits: vw.to_bits(),
            viewport_h_bits: vh.to_bits(),
            tile_size_bits: tile_size.to_bits(),
            image_w_bits: dims[0].to_bits(),
            image_h_bits: dims[1].to_bits(),
            fit_cover_bits: dims[2].to_bits(),
            image_brightness_bits: tuning.image_brightness.to_bits(),
            image_peak_luminance_bits: tuning.image_peak_luminance.to_bits(),
            image_saturation_bits: tuning.image_saturation.to_bits(),
            phase_stage: self.backdrop_phase_stage,
        });
        if cache_key.is_some() && self.particle_tile_grid_cache_key == cache_key {
            return false;
        }

        if is_void_phase {
            self.particle_tile_grid_cache_key = cache_key;
            return false;
        }

        let Some(ref tile_img_tex) = self.particle_tile_img_texture else {
            return false;
        };
        let tg = metal::MTLSize::new(16, 16, 1);
        let grid = metal::MTLSize::new(grid_w as u64, grid_h as u64, 1);

        if self.backdrop_phase_stage == 0 {
            let compute = unsafe { attach_compute_timestamps(command_buffer, profiler) };
            compute.set_compute_pipeline_state(&self.particle_tile_hold_pipeline);
            compute.set_texture(0, Some(tile_img_tex));
            compute.set_texture(1, Some(active_tex));
            compute.set_sampler_state(0, Some(&self.backdrop_sampler));
            compute.set_bytes(
                0,
                mem::size_of::<ParticleWaveUniforms>() as u64,
                &uniforms as *const ParticleWaveUniforms as *const _,
            );
            compute.dispatch_threads(grid, tg);
            compute.end_encoding();
            self.particle_tile_grid_cache_key = cache_key;
            return true;
        }

        if needed > self.particle_tile_grid_capacity {
            self.particle_tile_grid_capacity = needed;
            self.particle_tile_grid_buffer = self
                .device
                .new_buffer((needed * 4) as u64, MTLResourceOptions::StorageModePrivate);
            self.particle_tile_grid_needs_clear = true;
        }

        if self.particle_tile_grid_needs_clear {
            let blit = unsafe {
                attach_blit_timestamps(command_buffer, profiler.as_deref_mut(), GpuPassKind::BlitFill)
            };
            blit.fill_buffer(
                &self.particle_tile_grid_buffer,
                NSRange::new(0, (needed * 4) as u64),
                0,
            );
            blit.end_encoding();
            self.particle_tile_grid_needs_clear = false;
        }

        let compute = unsafe { attach_compute_timestamps(command_buffer, profiler) };
        compute.set_compute_pipeline_state(&self.particle_tile_scatter_pipeline);
        compute.set_buffer(0, Some(&self.particle_tile_grid_buffer), 0);
        compute.set_bytes(
            1,
            mem::size_of::<ParticleWaveUniforms>() as u64,
            &uniforms as *const ParticleWaveUniforms as *const _,
        );
        compute.set_texture(0, Some(active_tex));
        compute.set_sampler_state(0, Some(&self.backdrop_sampler));
        compute.dispatch_threads(grid, tg);

        compute.memory_barrier_with_resources(&[&self.particle_tile_grid_buffer]);

        compute.set_compute_pipeline_state(&self.particle_tile_unpack_pipeline);
        compute.set_buffer(0, Some(&self.particle_tile_grid_buffer), 0);
        compute.set_texture(0, Some(tile_img_tex));
        compute.set_bytes(
            1,
            mem::size_of::<ParticleWaveUniforms>() as u64,
            &uniforms as *const ParticleWaveUniforms as *const _,
        );
        compute.dispatch_threads(grid, tg);
        compute.end_encoding();
        self.particle_tile_grid_cache_key = cache_key;
        true
    }

    fn draw_particle_wave_quads(
        &self,
        quads: &[Quad],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        if quads.is_empty() {
            return true;
        }
        align_offset(instance_offset);

        let has_image = self.current_particle_uniforms.has_image > 0.5
            && self.particle_tile_img_texture.is_some();
        if has_image {
            command_encoder.set_render_pipeline_state(&self.particle_wave_pipeline_state);
        } else {
            command_encoder.set_render_pipeline_state(&self.particle_wave_no_image_pipeline_state);
        }
        command_encoder.set_vertex_buffer(
            QuadInputIndex::Vertices as u64,
            Some(&self.unit_vertices),
            0,
        );
        command_encoder.set_vertex_buffer(
            QuadInputIndex::Quads as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_vertex_bytes(
            QuadInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_vertex_bytes(
            3,
            mem::size_of::<ParticleWaveUniforms>() as u64,
            &self.current_particle_uniforms as *const ParticleWaveUniforms as *const _,
        );
        if let Some(ref lut_tex) = self.particle_facet_lut_texture {
            command_encoder.set_fragment_texture(0, Some(lut_tex));
        }
        if has_image && let Some(ref img_tex) = self.particle_tile_img_texture {
            command_encoder.set_fragment_texture(1, Some(img_tex));
        }

        let quad_bytes_len = mem::size_of_val(quads);
        let buffer_contents =
            unsafe { (instance_buffer.metal_buffer.contents() as *mut u8).add(*instance_offset) };

        let next_offset = *instance_offset + quad_bytes_len;
        if next_offset > instance_buffer.size {
            return false;
        }

        unsafe {
            ptr::copy_nonoverlapping(quads.as_ptr() as *const u8, buffer_contents, quad_bytes_len);
        }

        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            quads.len() as u64,
        );
        *instance_offset = next_offset;
        true
    }

    fn draw_paths_from_intermediate(
        &self,
        paths: &[Path<ScaledPixels>],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        let Some(first_path) = paths.first() else {
            return true;
        };

        let Some(ref intermediate_texture) = self.path_intermediate_texture else {
            return false;
        };

        command_encoder.set_render_pipeline_state(&self.path_sprites_pipeline_state);
        command_encoder.set_vertex_buffer(
            SpriteInputIndex::Vertices as u64,
            Some(&self.unit_vertices),
            0,
        );
        command_encoder.set_vertex_bytes(
            SpriteInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );

        command_encoder.set_fragment_texture(
            SpriteInputIndex::AtlasTexture as u64,
            Some(intermediate_texture),
        );

        // When copying paths from the intermediate texture to the drawable,
        // each pixel must only be copied once, in case of transparent paths.
        //
        // If all paths have the same draw order, then their bounds are all
        // disjoint, so we can copy each path's bounds individually. If this
        // batch combines different draw orders, we perform a single copy
        // for a minimal spanning rect.
        let sprites;
        if paths.last().unwrap().order == first_path.order {
            sprites = paths
                .iter()
                .map(|path| PathSprite {
                    bounds: path.clipped_bounds(),
                })
                .collect();
        } else {
            let mut bounds = first_path.clipped_bounds();
            for path in paths.iter().skip(1) {
                bounds = bounds.union(&path.clipped_bounds());
            }
            sprites = vec![PathSprite { bounds }];
        }

        align_offset(instance_offset);
        let sprite_bytes_len = mem::size_of_val(sprites.as_slice());
        let next_offset = *instance_offset + sprite_bytes_len;
        if next_offset > instance_buffer.size {
            return false;
        }

        command_encoder.set_vertex_buffer(
            SpriteInputIndex::Sprites as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );

        let buffer_contents =
            unsafe { (instance_buffer.metal_buffer.contents() as *mut u8).add(*instance_offset) };
        unsafe {
            ptr::copy_nonoverlapping(
                sprites.as_ptr() as *const u8,
                buffer_contents,
                sprite_bytes_len,
            );
        }

        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            sprites.len() as u64,
        );
        *instance_offset = next_offset;

        true
    }

    fn draw_underlines(
        &self,
        underlines: &[Underline],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        if underlines.is_empty() {
            return true;
        }
        align_offset(instance_offset);

        command_encoder.set_render_pipeline_state(&self.underlines_pipeline_state);
        command_encoder.set_vertex_buffer(
            UnderlineInputIndex::Vertices as u64,
            Some(&self.unit_vertices),
            0,
        );
        command_encoder.set_vertex_buffer(
            UnderlineInputIndex::Underlines as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_fragment_buffer(
            UnderlineInputIndex::Underlines as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );

        command_encoder.set_vertex_bytes(
            UnderlineInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );

        let underline_bytes_len = mem::size_of_val(underlines);
        let buffer_contents =
            unsafe { (instance_buffer.metal_buffer.contents() as *mut u8).add(*instance_offset) };

        let next_offset = *instance_offset + underline_bytes_len;
        if next_offset > instance_buffer.size {
            return false;
        }

        unsafe {
            ptr::copy_nonoverlapping(
                underlines.as_ptr() as *const u8,
                buffer_contents,
                underline_bytes_len,
            );
        }

        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            underlines.len() as u64,
        );
        *instance_offset = next_offset;
        true
    }

    fn draw_monochrome_sprites(
        &self,
        texture_id: AtlasTextureId,
        sprites: &[MonochromeSprite],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        if sprites.is_empty() {
            return true;
        }
        align_offset(instance_offset);

        let sprite_bytes_len = mem::size_of_val(sprites);
        let buffer_contents =
            unsafe { (instance_buffer.metal_buffer.contents() as *mut u8).add(*instance_offset) };

        let next_offset = *instance_offset + sprite_bytes_len;
        if next_offset > instance_buffer.size {
            return false;
        }

        let texture = self.sprite_atlas.metal_texture(texture_id);
        let texture_size = size(
            DevicePixels(texture.width() as i32),
            DevicePixels(texture.height() as i32),
        );
        command_encoder.set_render_pipeline_state(&self.monochrome_sprites_pipeline_state);
        command_encoder.set_vertex_buffer(
            SpriteInputIndex::Vertices as u64,
            Some(&self.unit_vertices),
            0,
        );
        command_encoder.set_vertex_buffer(
            SpriteInputIndex::Sprites as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_vertex_bytes(
            SpriteInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_vertex_bytes(
            SpriteInputIndex::AtlasTextureSize as u64,
            mem::size_of_val(&texture_size) as u64,
            &texture_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_fragment_buffer(
            SpriteInputIndex::Sprites as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_fragment_texture(SpriteInputIndex::AtlasTexture as u64, Some(&texture));

        unsafe {
            ptr::copy_nonoverlapping(
                sprites.as_ptr() as *const u8,
                buffer_contents,
                sprite_bytes_len,
            );
        }

        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            sprites.len() as u64,
        );
        *instance_offset = next_offset;
        true
    }

    fn draw_polychrome_sprites(
        &self,
        texture_id: AtlasTextureId,
        sprites: &[PolychromeSprite],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        if sprites.is_empty() {
            return true;
        }
        align_offset(instance_offset);

        let texture = self.sprite_atlas.metal_texture(texture_id);
        let texture_size = size(
            DevicePixels(texture.width() as i32),
            DevicePixels(texture.height() as i32),
        );
        command_encoder.set_render_pipeline_state(&self.polychrome_sprites_pipeline_state);
        command_encoder.set_vertex_buffer(
            SpriteInputIndex::Vertices as u64,
            Some(&self.unit_vertices),
            0,
        );
        command_encoder.set_vertex_buffer(
            SpriteInputIndex::Sprites as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_vertex_bytes(
            SpriteInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_vertex_bytes(
            SpriteInputIndex::AtlasTextureSize as u64,
            mem::size_of_val(&texture_size) as u64,
            &texture_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_fragment_buffer(
            SpriteInputIndex::Sprites as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_fragment_texture(SpriteInputIndex::AtlasTexture as u64, Some(&texture));

        let sprite_bytes_len = mem::size_of_val(sprites);
        let buffer_contents =
            unsafe { (instance_buffer.metal_buffer.contents() as *mut u8).add(*instance_offset) };

        let next_offset = *instance_offset + sprite_bytes_len;
        if next_offset > instance_buffer.size {
            return false;
        }

        unsafe {
            ptr::copy_nonoverlapping(
                sprites.as_ptr() as *const u8,
                buffer_contents,
                sprite_bytes_len,
            );
        }

        command_encoder.draw_primitives_instanced(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            sprites.len() as u64,
        );
        *instance_offset = next_offset;
        true
    }

    fn draw_surfaces(
        &mut self,
        surfaces: &[PaintSurface],
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        command_encoder.set_render_pipeline_state(&self.surfaces_pipeline_state);
        command_encoder.set_vertex_buffer(
            SurfaceInputIndex::Vertices as u64,
            Some(&self.unit_vertices),
            0,
        );
        command_encoder.set_vertex_bytes(
            SurfaceInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );

        for surface in surfaces {
            let texture_size = size(
                DevicePixels::from(surface.image_buffer.get_width() as i32),
                DevicePixels::from(surface.image_buffer.get_height() as i32),
            );

            assert_eq!(
                surface.image_buffer.get_pixel_format(),
                kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
            );

            let y_texture = self
                .core_video_texture_cache
                .create_texture_from_image(
                    surface.image_buffer.as_concrete_TypeRef(),
                    None,
                    MTLPixelFormat::R8Unorm,
                    surface.image_buffer.get_width_of_plane(0),
                    surface.image_buffer.get_height_of_plane(0),
                    0,
                )
                .unwrap();
            let cb_cr_texture = self
                .core_video_texture_cache
                .create_texture_from_image(
                    surface.image_buffer.as_concrete_TypeRef(),
                    None,
                    MTLPixelFormat::RG8Unorm,
                    surface.image_buffer.get_width_of_plane(1),
                    surface.image_buffer.get_height_of_plane(1),
                    1,
                )
                .unwrap();

            align_offset(instance_offset);
            let next_offset = *instance_offset + mem::size_of::<Surface>();
            if next_offset > instance_buffer.size {
                return false;
            }

            command_encoder.set_vertex_buffer(
                SurfaceInputIndex::Surfaces as u64,
                Some(&instance_buffer.metal_buffer),
                *instance_offset as u64,
            );
            command_encoder.set_vertex_bytes(
                SurfaceInputIndex::TextureSize as u64,
                mem::size_of_val(&texture_size) as u64,
                &texture_size as *const Size<DevicePixels> as *const _,
            );
            // let y_texture = y_texture.get_texture().unwrap().
            command_encoder.set_fragment_texture(SurfaceInputIndex::YTexture as u64, unsafe {
                let texture = CVMetalTextureGetTexture(y_texture.as_concrete_TypeRef());
                Some(metal::TextureRef::from_ptr(texture as *mut _))
            });
            command_encoder.set_fragment_texture(SurfaceInputIndex::CbCrTexture as u64, unsafe {
                let texture = CVMetalTextureGetTexture(cb_cr_texture.as_concrete_TypeRef());
                Some(metal::TextureRef::from_ptr(texture as *mut _))
            });

            unsafe {
                let buffer_contents = (instance_buffer.metal_buffer.contents() as *mut u8)
                    .add(*instance_offset)
                    as *mut SurfaceBounds;
                ptr::write(
                    buffer_contents,
                    SurfaceBounds {
                        bounds: surface.bounds,
                        content_mask: surface.content_mask.clone(),
                    },
                );
            }

            command_encoder.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, 6);
            *instance_offset = next_offset;
        }
        true
    }
}

fn glass_sample_rect(q: &Quad, vw: f32, vh: f32) -> Bounds<ScaledPixels> {
    let blur_px = q.background.colors[1].percentage * 2.0;
    let margin = if blur_px > 0.5 {
        (blur_px * 2.0 + 16.0).ceil()
    } else {
        2.0
    };
    let x0 = (q.bounds.origin.x.0 - margin).max(0.0).min(vw);
    let y0 = (q.bounds.origin.y.0 - margin).max(0.0).min(vh);
    let x1 = (q.bounds.bottom_right().x.0 + margin).max(x0).min(vw);
    let y1 = (q.bounds.bottom_right().y.0 + margin).max(y0).min(vh);
    Bounds {
        origin: point(ScaledPixels(x0), ScaledPixels(y0)),
        size: size(ScaledPixels(x1 - x0), ScaledPixels(y1 - y0)),
    }
}

fn rect_area(b: &Bounds<ScaledPixels>) -> f32 {
    b.size.width.0.max(0.0) * b.size.height.0.max(0.0)
}

fn rect_non_empty(b: &Bounds<ScaledPixels>) -> bool {
    b.size.width.0 > 0.0 && b.size.height.0 > 0.0
}

fn rect_contains(outer: &Bounds<ScaledPixels>, inner: &Bounds<ScaledPixels>) -> bool {
    outer.origin.x.0 <= inner.origin.x.0
        && outer.origin.y.0 <= inner.origin.y.0
        && outer.bottom_right().x.0 >= inner.bottom_right().x.0
        && outer.bottom_right().y.0 >= inner.bottom_right().y.0
}

fn subtract_single_rect(
    a: Bounds<ScaledPixels>,
    b: Bounds<ScaledPixels>,
    out: &mut Vec<Bounds<ScaledPixels>>,
) {
    let ax0 = a.origin.x.0;
    let ay0 = a.origin.y.0;
    let ax1 = a.bottom_right().x.0;
    let ay1 = a.bottom_right().y.0;
    let ix0 = ax0.max(b.origin.x.0);
    let iy0 = ay0.max(b.origin.y.0);
    let ix1 = ax1.min(b.bottom_right().x.0);
    let iy1 = ay1.min(b.bottom_right().y.0);
    if ix0 >= ix1 || iy0 >= iy1 {
        out.push(a);
        return;
    }
    if iy0 > ay0 {
        out.push(Bounds {
            origin: point(ScaledPixels(ax0), ScaledPixels(ay0)),
            size: size(ScaledPixels(ax1 - ax0), ScaledPixels(iy0 - ay0)),
        });
    }
    if ay1 > iy1 {
        out.push(Bounds {
            origin: point(ScaledPixels(ax0), ScaledPixels(iy1)),
            size: size(ScaledPixels(ax1 - ax0), ScaledPixels(ay1 - iy1)),
        });
    }
    if ix0 > ax0 {
        out.push(Bounds {
            origin: point(ScaledPixels(ax0), ScaledPixels(iy0)),
            size: size(ScaledPixels(ix0 - ax0), ScaledPixels(iy1 - iy0)),
        });
    }
    if ax1 > ix1 {
        out.push(Bounds {
            origin: point(ScaledPixels(ix1), ScaledPixels(iy0)),
            size: size(ScaledPixels(ax1 - ix1), ScaledPixels(iy1 - iy0)),
        });
    }
}

fn subtract_copied_from_dirty(
    dirty: &mut Vec<Bounds<ScaledPixels>>,
    copied: &[Bounds<ScaledPixels>],
) {
    let mut next: Vec<Bounds<ScaledPixels>> = Vec::with_capacity(dirty.len() + 8);
    for &c in copied {
        next.clear();
        for &d in dirty.iter() {
            subtract_single_rect(d, c, &mut next);
        }
        std::mem::swap(dirty, &mut next);
    }
    while dirty.len() > 64 {
        let mut best_i = 0usize;
        let mut best_j = 1usize;
        let mut best_cost = f32::INFINITY;
        for i in 0..dirty.len() {
            let ai = rect_area(&dirty[i]);
            for j in (i + 1)..dirty.len() {
                let u = dirty[i].union(&dirty[j]);
                let cost = rect_area(&u) - ai - rect_area(&dirty[j]);
                if cost < best_cost {
                    best_cost = cost;
                    best_i = i;
                    best_j = j;
                }
            }
        }
        dirty[best_i] = dirty[best_i].union(&dirty[best_j]);
        dirty.swap_remove(best_j);
    }
}

fn coalesce_copy_regions(
    rects: &[Bounds<ScaledPixels>],
    cluster_pad: f32,
    max_regions: usize,
) -> Vec<Bounds<ScaledPixels>> {
    let mut clusters: Vec<Bounds<ScaledPixels>> = Vec::with_capacity(max_regions.min(rects.len()));
    for &d in rects {
        if !rect_non_empty(&d) {
            continue;
        }
        let mut merged = false;
        for c in &mut clusters {
            if rect_contains(c, &d) {
                merged = true;
                break;
            }
            let exp = Bounds {
                origin: point(
                    ScaledPixels(c.origin.x.0 - cluster_pad),
                    ScaledPixels(c.origin.y.0 - cluster_pad),
                ),
                size: size(
                    ScaledPixels(c.size.width.0 + cluster_pad * 2.0),
                    ScaledPixels(c.size.height.0 + cluster_pad * 2.0),
                ),
            };
            if exp.intersects(&d) {
                *c = c.union(&d);
                merged = true;
                break;
            }
        }
        if !merged {
            if clusters.len() >= max_regions {
                let mut best_idx = 0usize;
                let mut best_cost = f32::INFINITY;
                for (idx, c) in clusters.iter().enumerate() {
                    let u = c.union(&d);
                    let cost = rect_area(&u) - rect_area(c);
                    if cost < best_cost {
                        best_cost = cost;
                        best_idx = idx;
                    }
                }
                clusters[best_idx] = clusters[best_idx].union(&d);
            } else {
                clusters.push(d);
            }
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..clusters.len() {
            for j in (i + 1)..clusters.len() {
                if clusters[i].intersects(&clusters[j]) {
                    clusters[i] = clusters[i].union(&clusters[j]);
                    clusters.swap_remove(j);
                    changed = true;
                    break 'outer;
                }
            }
        }
    }
    clusters
}

#[derive(Clone, Copy)]
enum DrawItem<'a> {
    Shadows(&'a [Shadow]),
    Quads {
        quads: &'a [Quad],
        mode: u8,
    },
    Paths(&'a [Path<ScaledPixels>]),
    Underlines(&'a [Underline]),
    MonochromeSprites {
        texture_id: AtlasTextureId,
        sprites: &'a [MonochromeSprite],
    },
    PolychromeSprites {
        texture_id: AtlasTextureId,
        sprites: &'a [PolychromeSprite],
    },
    Surfaces(&'a [PaintSurface]),
}

impl<'a> DrawItem<'a> {
    fn is_glass(&self) -> bool {
        matches!(self, DrawItem::Quads { mode: 1 | 3, .. })
    }
}

fn classify_quad_mode(q: &Quad) -> u8 {
    match q.background.tag {
        BackgroundTag::LiquidGlass => {
            let edge_mode = q.background.colors[1].color.h;
            if (1.62..1.88).contains(&edge_mode) {
                3u8
            } else {
                1u8
            }
        }
        BackgroundTag::ParticleWave => 2u8,
        BackgroundTag::Solid
            if q.border_widths.top.0 == 0.0
                && q.border_widths.right.0 == 0.0
                && q.border_widths.bottom.0 == 0.0
                && q.border_widths.left.0 == 0.0 =>
        {
            4u8
        }
        _ => 0u8,
    }
}

fn coalesce_text_lines(
    sprites: &[MonochromeSprite],
    out: &mut Vec<Bounds<ScaledPixels>>,
) {
    let mut cur: Option<Bounds<ScaledPixels>> = None;
    for s in sprites {
        let b = s.bounds;
        if !rect_non_empty(&b) {
            continue;
        }
        if let Some(ref mut c) = cur {
            let y_overlap = c.origin.y.0 <= b.bottom_right().y.0 + 2.0
                && b.origin.y.0 <= c.bottom_right().y.0 + 2.0;
            let x_near = c.origin.x.0 - 16.0 <= b.bottom_right().x.0
                && b.origin.x.0 <= c.bottom_right().x.0 + 16.0;
            if y_overlap && x_near {
                *c = c.union(&b);
                continue;
            }
            out.push(*c);
        }
        cur = Some(b);
    }
    if let Some(c) = cur {
        out.push(c);
    }
}

#[derive(Clone, Copy)]
struct SpatialMask([u64; 64]);

impl SpatialMask {
    #[inline]
    fn empty() -> Self {
        Self([0u64; 64])
    }

    #[inline]
    fn rect_cell_range(
        b: &Bounds<ScaledPixels>,
        inv_cell_w: f32,
        inv_cell_h: f32,
    ) -> Option<(u64, usize, usize)> {
        if !rect_non_empty(b) {
            return None;
        }
        let x0 = (b.origin.x.0 * inv_cell_w).floor().clamp(0.0, 63.0) as u32;
        let x1_excl = (b.bottom_right().x.0 * inv_cell_w).ceil().clamp(1.0, 64.0) as u32;
        let x1 = x1_excl.saturating_sub(1).max(x0).min(63);
        let y0 = (b.origin.y.0 * inv_cell_h).floor().clamp(0.0, 63.0) as usize;
        let y1_excl = (b.bottom_right().y.0 * inv_cell_h).ceil().clamp(1.0, 64.0) as usize;
        let y1 = y1_excl.saturating_sub(1).max(y0).min(63);
        let w = x1 - x0 + 1;
        let row_bits = if w >= 64 {
            u64::MAX
        } else {
            ((1u64 << w) - 1) << x0
        };
        Some((row_bits, y0, y1))
    }

    #[inline]
    fn add_rect(&mut self, b: &Bounds<ScaledPixels>, inv_cell_w: f32, inv_cell_h: f32) {
        if let Some((row_bits, y0, y1)) = Self::rect_cell_range(b, inv_cell_w, inv_cell_h) {
            for y in y0..=y1 {
                self.0[y] |= row_bits;
            }
        }
    }

    #[inline]
    fn intersects(&self, other: &Self) -> bool {
        for y in 0..64 {
            if (self.0[y] & other.0[y]) != 0 {
                return true;
            }
        }
        false
    }

    #[inline]
    fn intersects_rect(
        &self,
        b: &Bounds<ScaledPixels>,
        inv_cell_w: f32,
        inv_cell_h: f32,
    ) -> bool {
        if let Some((row_bits, y0, y1)) = Self::rect_cell_range(b, inv_cell_w, inv_cell_h) {
            for y in y0..=y1 {
                if (self.0[y] & row_bits) != 0 {
                    return true;
                }
            }
        }
        false
    }
}

fn build_item_spatial_mask(
    item: &DrawItem<'_>,
    vw: f32,
    vh: f32,
    inv_cell_w: f32,
    inv_cell_h: f32,
) -> (Option<Bounds<ScaledPixels>>, SpatialMask) {
    let mut mask = SpatialMask::empty();
    let mut aabb: Option<Bounds<ScaledPixels>> = None;
    let mut push = |b: Bounds<ScaledPixels>| {
        if !rect_non_empty(&b) {
            return;
        }
        mask.add_rect(&b, inv_cell_w, inv_cell_h);
        aabb = Some(match aabb {
            Some(cur) => cur.union(&b),
            None => b,
        });
    };
    match item {
        DrawItem::Shadows(shadows) => {
            for s in *shadows {
                let m = 3.0 * s.blur_radius.0;
                let exp = Bounds {
                    origin: point(
                        ScaledPixels(s.bounds.origin.x.0 - m),
                        ScaledPixels(s.bounds.origin.y.0 - m),
                    ),
                    size: size(
                        ScaledPixels(s.bounds.size.width.0 + m * 2.0),
                        ScaledPixels(s.bounds.size.height.0 + m * 2.0),
                    ),
                }
                .intersect(&s.content_mask.bounds);
                push(exp);
            }
        }
        DrawItem::Quads { quads, mode } => {
            if *mode == 1 || *mode == 3 {
                for q in *quads {
                    push(glass_sample_rect(q, vw, vh));
                }
            } else {
                for q in *quads {
                    push(q.bounds.intersect(&q.content_mask.bounds));
                }
            }
        }
        DrawItem::Paths(paths) => {
            for p in *paths {
                push(p.bounds);
            }
        }
        DrawItem::Underlines(underlines) => {
            for u in *underlines {
                push(u.bounds);
            }
        }
        DrawItem::MonochromeSprites { sprites, .. } => {
            for s in *sprites {
                push(s.bounds);
            }
        }
        DrawItem::PolychromeSprites { sprites, .. } => {
            for s in *sprites {
                push(s.bounds);
            }
        }
        DrawItem::Surfaces(surfaces) => {
            for s in *surfaces {
                push(s.bounds);
            }
        }
    }
    (aabb, mask)
}

fn item_overlaps_glass_exact(
    other: &DrawItem<'_>,
    glass_quads: &[Quad],
    vw: f32,
    vh: f32,
) -> bool {
    let mut g_rects: [Bounds<ScaledPixels>; 32] = [Bounds {
        origin: point(ScaledPixels(0.0), ScaledPixels(0.0)),
        size: size(ScaledPixels(0.0), ScaledPixels(0.0)),
    }; 32];
    let mut g_vec: Vec<Bounds<ScaledPixels>> = Vec::new();
    let g_slice: &[Bounds<ScaledPixels>] = if glass_quads.len() <= 32 {
        let mut count = 0usize;
        for q in glass_quads {
            let sr = glass_sample_rect(q, vw, vh);
            if rect_non_empty(&sr) {
                g_rects[count] = sr;
                count += 1;
            }
        }
        &g_rects[..count]
    } else {
        g_vec.reserve(glass_quads.len());
        for q in glass_quads {
            let sr = glass_sample_rect(q, vw, vh);
            if rect_non_empty(&sr) {
                g_vec.push(sr);
            }
        }
        &g_vec
    };
    if g_slice.is_empty() {
        return false;
    }
    let hits = |b: &Bounds<ScaledPixels>| -> bool {
        rect_non_empty(b) && g_slice.iter().any(|g| g.intersects(b))
    };
    match other {
        DrawItem::Shadows(shadows) => shadows.iter().any(|s| {
            let m = 3.0 * s.blur_radius.0;
            let exp = Bounds {
                origin: point(
                    ScaledPixels(s.bounds.origin.x.0 - m),
                    ScaledPixels(s.bounds.origin.y.0 - m),
                ),
                size: size(
                    ScaledPixels(s.bounds.size.width.0 + m * 2.0),
                    ScaledPixels(s.bounds.size.height.0 + m * 2.0),
                ),
            }
            .intersect(&s.content_mask.bounds);
            hits(&exp)
        }),
        DrawItem::Quads { quads, mode } => {
            if *mode == 1 || *mode == 3 {
                quads.iter().any(|q| hits(&glass_sample_rect(q, vw, vh)))
            } else {
                quads
                    .iter()
                    .any(|q| hits(&q.bounds.intersect(&q.content_mask.bounds)))
            }
        }
        DrawItem::Paths(paths) => paths.iter().any(|p| hits(&p.bounds)),
        DrawItem::Underlines(underlines) => underlines.iter().any(|u| hits(&u.bounds)),
        DrawItem::MonochromeSprites { sprites, .. } => sprites.iter().any(|s| hits(&s.bounds)),
        DrawItem::PolychromeSprites { sprites, .. } => sprites.iter().any(|s| hits(&s.bounds)),
        DrawItem::Surfaces(surfaces) => surfaces.iter().any(|s| hits(&s.bounds)),
    }
}

fn schedule_draw_items<'a>(
    batches: &[PrimitiveBatch<'a>],
    vw: f32,
    vh: f32,
) -> Vec<DrawItem<'a>> {
    let mut items: Vec<DrawItem<'a>> = Vec::with_capacity(batches.len() * 2);
    let mut glass_item_count = 0usize;
    let mut has_paths = false;
    for batch in batches {
        match *batch {
            PrimitiveBatch::Shadows(s) => items.push(DrawItem::Shadows(s)),
            PrimitiveBatch::Quads(quads) => {
                let mut i = 0;
                while i < quads.len() {
                    let mode = classify_quad_mode(&quads[i]);
                    let start = i;
                    while i < quads.len() && classify_quad_mode(&quads[i]) == mode {
                        i += 1;
                    }
                    if mode == 1 || mode == 3 {
                        glass_item_count += 1;
                    }
                    items.push(DrawItem::Quads {
                        quads: &quads[start..i],
                        mode,
                    });
                }
            }
            PrimitiveBatch::Paths(p) => {
                has_paths = true;
                items.push(DrawItem::Paths(p));
            }
            PrimitiveBatch::Underlines(u) => items.push(DrawItem::Underlines(u)),
            PrimitiveBatch::MonochromeSprites {
                texture_id,
                sprites,
            } => items.push(DrawItem::MonochromeSprites {
                texture_id,
                sprites,
            }),
            PrimitiveBatch::PolychromeSprites {
                texture_id,
                sprites,
            } => items.push(DrawItem::PolychromeSprites {
                texture_id,
                sprites,
            }),
            PrimitiveBatch::Surfaces(s) => items.push(DrawItem::Surfaces(s)),
        }
    }

    if glass_item_count < 2 || has_paths || items.len() <= 2 || vw <= 0.0 || vh <= 0.0 {
        return items;
    }

    let n = items.len();
    let inv_cell_w = 64.0 / vw;
    let inv_cell_h = 64.0 / vh;
    let deps: Vec<(Option<Bounds<ScaledPixels>>, SpatialMask)> = items
        .iter()
        .map(|it| build_item_spatial_mask(it, vw, vh, inv_cell_w, inv_cell_h))
        .collect();

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_deg: Vec<usize> = vec![0; n];

    for i in 0..n {
        let Some(aabb_i) = deps[i].0 else {
            continue;
        };
        let mask_i = &deps[i].1;
        for j in (i + 1)..n {
            let Some(aabb_j) = deps[j].0 else {
                continue;
            };
            if !aabb_i.intersects(&aabb_j) || !mask_i.intersects(&deps[j].1) {
                continue;
            }
            let overlaps = match (items[i], items[j]) {
                (DrawItem::Quads { quads, mode: 1 | 3 }, other) => {
                    item_overlaps_glass_exact(&other, quads, vw, vh)
                }
                (other, DrawItem::Quads { quads, mode: 1 | 3 }) => {
                    item_overlaps_glass_exact(&other, quads, vw, vh)
                }
                _ => true,
            };
            if overlaps {
                adj[i].push(j);
                in_deg[j] += 1;
            }
        }
    }

    let mut ready_non_glass: Vec<usize> = Vec::with_capacity(n);
    let mut ready_glass: Vec<usize> = Vec::with_capacity(glass_item_count);
    for i in 0..n {
        if in_deg[i] == 0 {
            if items[i].is_glass() {
                ready_glass.push(i);
            } else {
                ready_non_glass.push(i);
            }
        }
    }

    let mut scheduled: Vec<DrawItem<'a>> = Vec::with_capacity(n);
    while scheduled.len() < n {
        while !ready_non_glass.is_empty() {
            let min_pos = ready_non_glass
                .iter()
                .enumerate()
                .min_by_key(|&(_, &idx)| idx)
                .map(|(p, _)| p)
                .unwrap();
            let u = ready_non_glass.swap_remove(min_pos);
            scheduled.push(items[u]);
            for &v in &adj[u] {
                in_deg[v] -= 1;
                if in_deg[v] == 0 {
                    if items[v].is_glass() {
                        ready_glass.push(v);
                    } else {
                        ready_non_glass.push(v);
                    }
                }
            }
        }

        if !ready_glass.is_empty() {
            ready_glass.sort_unstable();
            let wave_glass = std::mem::take(&mut ready_glass);
            for u in wave_glass {
                scheduled.push(items[u]);
                for &v in &adj[u] {
                    in_deg[v] -= 1;
                    if in_deg[v] == 0 {
                        if items[v].is_glass() {
                            ready_glass.push(v);
                        } else {
                            ready_non_glass.push(v);
                        }
                    }
                }
            }
        } else {
            break;
        }
    }

    debug_assert_eq!(scheduled.len(), n);
    if scheduled.len() == n {
        scheduled
    } else {
        items
    }
}

fn collect_lookahead_needs_items(
    items: &[DrawItem<'_>],
    cur_item_idx: usize,
    cur_chunk: &[Quad],
    dirty_rects: &[Bounds<ScaledPixels>],
    vw: f32,
    vh: f32,
) -> (Vec<Bounds<ScaledPixels>>, Vec<Bounds<ScaledPixels>>) {
    let mut needs: Vec<Bounds<ScaledPixels>> = Vec::with_capacity(cur_chunk.len() + 16);
    let mut blur_needs: Vec<Bounds<ScaledPixels>> = Vec::new();
    let inv_cell_w = 64.0 / vw.max(1.0);
    let inv_cell_h = 64.0 / vh.max(1.0);
    let mut future_dirty = SpatialMask::empty();

    for q in cur_chunk {
        let sr = glass_sample_rect(q, vw, vh);
        if rect_non_empty(&sr) {
            needs.push(sr);
            if q.background.colors[1].percentage * 2.0 > 0.5 {
                blur_needs.push(sr);
            }
        }
        future_dirty.add_rect(&q.bounds, inv_cell_w, inv_cell_h);
    }

    for item in &items[(cur_item_idx + 1)..] {
        match *item {
            DrawItem::Quads { quads, mode } => {
                if mode == 1 || mode == 3 {
                    for q in quads {
                        let sr = glass_sample_rect(q, vw, vh);
                        if rect_non_empty(&sr)
                            && !future_dirty.intersects_rect(&sr, inv_cell_w, inv_cell_h)
                            && dirty_rects.iter().any(|dr| dr.intersects(&sr))
                        {
                            needs.push(sr);
                            if q.background.colors[1].percentage * 2.0 > 0.5 {
                                blur_needs.push(sr);
                            }
                        }
                        future_dirty.add_rect(&q.bounds, inv_cell_w, inv_cell_h);
                    }
                } else {
                    for q in quads {
                        future_dirty.add_rect(&q.bounds, inv_cell_w, inv_cell_h);
                    }
                }
            }
            DrawItem::Shadows(shadows) => {
                for s in shadows {
                    let margin = 3.0 * s.blur_radius.0;
                    let expanded = Bounds {
                        origin: point(
                            ScaledPixels(s.bounds.origin.x.0 - margin),
                            ScaledPixels(s.bounds.origin.y.0 - margin),
                        ),
                        size: size(
                            ScaledPixels(s.bounds.size.width.0 + margin * 2.0),
                            ScaledPixels(s.bounds.size.height.0 + margin * 2.0),
                        ),
                    }
                    .intersect(&s.content_mask.bounds);
                    future_dirty.add_rect(&expanded, inv_cell_w, inv_cell_h);
                }
            }
            DrawItem::Paths(paths) => {
                for p in paths {
                    future_dirty.add_rect(&p.bounds, inv_cell_w, inv_cell_h);
                }
            }
            DrawItem::Underlines(underlines) => {
                for u in underlines {
                    future_dirty.add_rect(&u.bounds, inv_cell_w, inv_cell_h);
                }
            }
            DrawItem::MonochromeSprites { sprites, .. } => {
                for s in sprites {
                    future_dirty.add_rect(&s.bounds, inv_cell_w, inv_cell_h);
                }
            }
            DrawItem::PolychromeSprites { sprites, .. } => {
                for s in sprites {
                    future_dirty.add_rect(&s.bounds, inv_cell_w, inv_cell_h);
                }
            }
            DrawItem::Surfaces(surfaces) => {
                for s in surfaces {
                    future_dirty.add_rect(&s.bounds, inv_cell_w, inv_cell_h);
                }
            }
        }
    }

    (needs, blur_needs)
}

fn new_command_encoder<'a>(
    command_buffer: &'a metal::CommandBufferRef,
    target_texture: &'a metal::TextureRef,
    viewport_size: Size<DevicePixels>,
    profiler: Option<&mut FrameGpuProfiler>,
    pass_index: u32,
    configure_color_attachment: impl Fn(&RenderPassColorAttachmentDescriptorRef),
) -> &'a metal::RenderCommandEncoderRef {
    let render_pass_descriptor = metal::RenderPassDescriptor::new();
    let color_attachment = render_pass_descriptor
        .color_attachments()
        .object_at(0)
        .unwrap();
    color_attachment.set_texture(Some(target_texture));
    color_attachment.set_store_action(metal::MTLStoreAction::Store);
    configure_color_attachment(color_attachment);

    if let Some(prof) = profiler
        && let Some((start_slot, end_slot)) = prof.alloc_pair()
    {
        unsafe {
            let rpd_ptr = render_pass_descriptor.as_ptr() as *mut objc::runtime::Object;
            let atts: *mut objc::runtime::Object = msg_send![rpd_ptr, sampleBufferAttachments];
            let att0: *mut objc::runtime::Object =
                msg_send![atts, objectAtIndexedSubscript: 0usize];
            let sb = prof.sample_buffer_ptr as *mut objc::runtime::Object;
            let _: () = msg_send![att0, setSampleBuffer: sb];
            let _: () = msg_send![att0, setStartOfVertexSampleIndex: MTL_COUNTER_DONT_SAMPLE];
            let _: () = msg_send![att0, setEndOfVertexSampleIndex: MTL_COUNTER_DONT_SAMPLE];
            let _: () = msg_send![att0, setStartOfFragmentSampleIndex: start_slot];
            let _: () = msg_send![att0, setEndOfFragmentSampleIndex: end_slot];
        }
        prof.record_span(
            GpuPassKind::RenderPass {
                index: pass_index,
                summary: String::new(),
            },
            start_slot,
            end_slot,
        );
    }

    let command_encoder = command_buffer.new_render_command_encoder(render_pass_descriptor);
    command_encoder.set_viewport(metal::MTLViewport {
        originX: 0.0,
        originY: 0.0,
        width: i32::from(viewport_size.width) as f64,
        height: i32::from(viewport_size.height) as f64,
        znear: 0.0,
        zfar: 1.0,
    });
    command_encoder
}

fn build_opaque_pipeline_state_with_bool_constant(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: metal::MTLPixelFormat,
    constant_index: u64,
    value: bool,
) -> metal::RenderPipelineState {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .expect("error locating vertex function");
    let constants = metal::FunctionConstantValues::new();
    constants.set_constant_value_at_index(
        &value as *const bool as *const std::ffi::c_void,
        metal::MTLDataType::Bool,
        constant_index,
    );
    let fragment_fn = library
        .get_function(fragment_fn_name, Some(constants))
        .expect("error locating fragment function with constant");

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    let color_attachment = descriptor.color_attachments().object_at(0).unwrap();
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(false);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("could not create render pipeline state")
}

fn build_pipeline_state_with_bool_constant(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: metal::MTLPixelFormat,
    constant_index: u64,
    value: bool,
) -> metal::RenderPipelineState {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .expect("error locating vertex function");
    let constants = metal::FunctionConstantValues::new();
    constants.set_constant_value_at_index(
        &value as *const bool as *const std::ffi::c_void,
        metal::MTLDataType::Bool,
        constant_index,
    );
    let fragment_fn = library
        .get_function(fragment_fn_name, Some(constants))
        .expect("error locating fragment function with constant");

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    let color_attachment = descriptor.color_attachments().object_at(0).unwrap();
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::SourceAlpha);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::One);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("could not create render pipeline state")
}

fn build_pipeline_state(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: metal::MTLPixelFormat,
) -> metal::RenderPipelineState {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .expect("error locating vertex function");
    let fragment_fn = library
        .get_function(fragment_fn_name, None)
        .expect("error locating fragment function");

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    let color_attachment = descriptor.color_attachments().object_at(0).unwrap();
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::SourceAlpha);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::One);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("could not create render pipeline state")
}

fn build_path_sprite_pipeline_state(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: metal::MTLPixelFormat,
) -> metal::RenderPipelineState {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .expect("error locating vertex function");
    let fragment_fn = library
        .get_function(fragment_fn_name, None)
        .expect("error locating fragment function");

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    let color_attachment = descriptor.color_attachments().object_at(0).unwrap();
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::One);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("could not create render pipeline state")
}

fn build_path_rasterization_pipeline_state(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: metal::MTLPixelFormat,
    path_sample_count: u32,
) -> metal::RenderPipelineState {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .expect("error locating vertex function");
    let fragment_fn = library
        .get_function(fragment_fn_name, None)
        .expect("error locating fragment function");

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    if path_sample_count > 1 {
        descriptor.set_raster_sample_count(path_sample_count as _);
        descriptor.set_alpha_to_coverage_enabled(false);
    }
    let color_attachment = descriptor.color_attachments().object_at(0).unwrap();
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

    device
        .new_render_pipeline_state(&descriptor)
        .expect("could not create render pipeline state")
}

// Align to multiples of 256 make Metal happy.
fn align_offset(offset: &mut usize) {
    *offset = (*offset).div_ceil(256) * 256;
}

#[repr(C)]
enum ShadowInputIndex {
    Vertices = 0,
    Shadows = 1,
    ViewportSize = 2,
}

#[repr(C)]
enum QuadInputIndex {
    Vertices = 0,
    Quads = 1,
    ViewportSize = 2,
}

#[repr(C)]
enum UnderlineInputIndex {
    Vertices = 0,
    Underlines = 1,
    ViewportSize = 2,
}

#[repr(C)]
enum SpriteInputIndex {
    Vertices = 0,
    Sprites = 1,
    ViewportSize = 2,
    AtlasTextureSize = 3,
    AtlasTexture = 4,
}

#[repr(C)]
enum SurfaceInputIndex {
    Vertices = 0,
    Surfaces = 1,
    ViewportSize = 2,
    TextureSize = 3,
    YTexture = 4,
    CbCrTexture = 5,
}

#[repr(C)]
enum PathRasterizationInputIndex {
    Vertices = 0,
    ViewportSize = 1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct PathSprite {
    pub bounds: Bounds<ScaledPixels>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct SurfaceBounds {
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
}

#[cfg(test)]
mod offscreen_gpu_bench {
    use super::*;
    use crate::{
        BorderStyle, Corners, Edges, Hsla, ParticleWaveTuning, Quad, Shadow,
        liquid_glass_styled, particle_wave_with,
    };

    fn make_test_backdrop_texture(device: &metal::Device) -> (metal::Texture, [f32; 3]) {
        let w = 1024u32;
        let h = 1024u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let fx = x as f32 / w as f32;
                let fy = y as f32 / h as f32;
                let r = ((fx * 0.85 + 0.15 * (fx * 31.0).sin().abs()) * 255.0) as u8;
                let g = ((fy * 0.80 + 0.20 * ((fx + fy) * 19.0).cos().abs()) * 255.0) as u8;
                let b = (((1.0 - fx * 0.6) * 0.7 + 0.3 * (fy * 27.0).sin().abs()) * 255.0) as u8;
                let idx = ((y * w + x) * 4) as usize;
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;
            }
        }
        let desc = metal::TextureDescriptor::new();
        desc.set_texture_type(metal::MTLTextureType::D2);
        desc.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
        desc.set_width(w as u64);
        desc.set_height(h as u64);
        desc.set_usage(metal::MTLTextureUsage::ShaderRead);
        desc.set_storage_mode(metal::MTLStorageMode::Shared);
        let tex = device.new_texture(&desc);
        tex.replace_region(
            metal::MTLRegion::new_2d(0, 0, w as u64, h as u64),
            0,
            rgba.as_ptr() as *const _,
            (w * 4) as u64,
        );
        (tex, [w as f32, h as f32, 1.0])
    }

    fn make_offscreen_target(device: &metal::Device, w: u32, h: u32) -> metal::Texture {
        let desc = metal::TextureDescriptor::new();
        desc.set_texture_type(metal::MTLTextureType::D2);
        desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        desc.set_width(w as u64);
        desc.set_height(h as u64);
        desc.set_usage(
            metal::MTLTextureUsage::RenderTarget | metal::MTLTextureUsage::ShaderRead,
        );
        desc.set_storage_mode(metal::MTLStorageMode::Shared);
        device.new_texture(&desc)
    }

    fn build_realistic_scene(vw: f32, vh: f32, blur_px: f32, wave_only: bool) -> Scene {
        let mut scene = Scene::default();
        let full_mask = ContentMask {
            bounds: Bounds {
                origin: point(ScaledPixels(0.0), ScaledPixels(0.0)),
                size: size(ScaledPixels(vw), ScaledPixels(vh)),
            },
            bottom_fade: ScaledPixels(0.0),
        };
        let mut order = 0u32;
        scene.insert_primitive(Quad {
            order,
            border_style: BorderStyle::Solid,
            bounds: full_mask.bounds,
            content_mask: full_mask.clone(),
            background: particle_wave_with(
                Hsla { h: 0.62, s: 0.25, l: 0.05, a: 1.0 },
                Hsla { h: 0.60, s: 0.72, l: 0.56, a: 1.0 },
                Hsla { h: 0.74, s: 0.65, l: 0.58, a: 1.0 },
                1.0,
            ),
            border_color: Hsla::default(),
            corner_radii: Corners::default(),
            border_widths: Edges::default(),
        });
        order += 1;
        if wave_only {
            scene.finish();
            return scene;
        }

        let panel_specs: &[(f32, f32, f32, f32, f32, f32)] = &[
            (24.0, 24.0, 3552.0, 104.0, 28.0, 0.0),
            (24.0, 152.0, 520.0, 2084.0, 32.0, 0.0),
            (568.0, 152.0, 1480.0, 2084.0, 32.0, 0.5),
            (2072.0, 152.0, 1504.0, 2084.0, 32.0, 1.0),
        ];
        for &(x, y, w, h, r, tint_kind) in panel_specs {
            let b = Bounds {
                origin: point(ScaledPixels(x), ScaledPixels(y)),
                size: size(ScaledPixels(w), ScaledPixels(h)),
            };
            scene.insert_primitive(Shadow {
                order,
                blur_radius: ScaledPixels(28.0),
                bounds: b,
                corner_radii: Corners::all(ScaledPixels(r)),
                content_mask: full_mask.clone(),
                color: Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.32 },
            });
            order += 1;
            scene.insert_primitive(Quad {
                order,
                border_style: BorderStyle::Solid,
                bounds: b,
                content_mask: full_mask.clone(),
                background: liquid_glass_styled(
                    Hsla { h: 0.60, s: 0.16, l: 0.11, a: 0.34 },
                    14.0,
                    22.0,
                    0.18,
                    0.55,
                    1.0,
                    blur_px,
                    7,
                    tint_kind,
                ),
                border_color: Hsla::default(),
                corner_radii: Corners::all(ScaledPixels(r)),
                border_widths: Edges::default(),
            });
            order += 1;
        }

        for row in 0..8 {
            let y = 240.0 + row as f32 * 230.0;
            let card_b = Bounds {
                origin: point(ScaledPixels(600.0), ScaledPixels(y)),
                size: size(ScaledPixels(1416.0), ScaledPixels(196.0)),
            };
            scene.insert_primitive(Quad {
                order,
                border_style: BorderStyle::Solid,
                bounds: card_b,
                content_mask: full_mask.clone(),
                background: Background::from(Hsla { h: 0.60, s: 0.12, l: 0.16, a: 0.45 }),
                border_color: Hsla::default(),
                corner_radii: Corners::all(ScaledPixels(20.0)),
                border_widths: Edges::default(),
            });
            order += 1;
            let badge_b = Bounds {
                origin: point(ScaledPixels(628.0), ScaledPixels(y + 24.0)),
                size: size(ScaledPixels(220.0), ScaledPixels(48.0)),
            };
            scene.insert_primitive(Quad {
                order,
                border_style: BorderStyle::Solid,
                bounds: badge_b,
                content_mask: full_mask.clone(),
                background: liquid_glass_styled(
                    Hsla { h: 0.58, s: 0.25, l: 0.22, a: 0.42 },
                    10.0,
                    16.0,
                    0.12,
                    0.45,
                    1.0,
                    blur_px,
                    7,
                    0.5,
                ),
                border_color: Hsla::default(),
                corner_radii: Corners::all(ScaledPixels(24.0)),
                border_widths: Edges::default(),
            });
            order += 1;
        }
        scene.finish();
        scene
    }

    fn render_one_frame_sync(
        renderer: &mut MetalRenderer,
        scene: &Scene,
        target: &metal::TextureRef,
        viewport: Size<DevicePixels>,
        fixed_time_sec: Option<f32>,
    ) -> (u64, u64) {
        if let Some(t) = fixed_time_sec {
            renderer.start_time =
                Instant::now().checked_sub(std::time::Duration::from_secs_f32(t)).unwrap_or_else(Instant::now);
        }
        let mut instance_buffer = renderer.instance_buffer_pool.lock().acquire(&renderer.device);
        let (cb, sample, gpu_prof) = renderer
            .draw_primitives(scene, &mut instance_buffer, target, viewport, true)
            .expect("draw_primitives failed");
        cb.commit();
        cb.wait_until_completed();
        let cb_gpu_us = unsafe {
            let ptr = cb.as_ptr() as *mut objc::runtime::Object;
            let s: f64 = msg_send![ptr, GPUStartTime];
            let e: f64 = msg_send![ptr, GPUEndTime];
            if s > 0.0 && e >= s {
                ((e - s) * 1_000_000.0).round() as u64
            } else {
                0
            }
        };
        if let Some(prof) = gpu_prof {
            prof.resolve_and_report(
                cb_gpu_us,
                viewport.width.0 as u32,
                viewport.height.0 as u32,
            );
        }
        renderer.instance_buffer_pool.lock().release(instance_buffer);
        let encode_us = sample.map(|s| s.encode_us).unwrap_or(0);
        (encode_us, cb_gpu_us)
    }

    fn readback_bgra(target: &metal::TextureRef, w: u32, h: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (w * h * 4) as usize];
        target.get_bytes(
            buf.as_mut_ptr() as *mut _,
            (w * 4) as u64,
            metal::MTLRegion::new_2d(0, 0, w as u64, h as u64),
            0,
        );
        buf
    }

    #[test]
    #[ignore]
    fn bench_offscreen_3600x2260_gpu() {
        let vw = 3600u32;
        let vh = 2260u32;
        let viewport = size(DevicePixels(vw as i32), DevicePixels(vh as i32));
        crate::set_particle_wave_tuning(ParticleWaveTuning {
            image_enabled: true,
            apple_music_artwork_enabled: false,
            youtube_mirror_enabled: false,
            tile_size_px: 3.0,
            image_hold_secs: 60.0,
            image_brightness: 1.0,
            image_peak_luminance: 0.42,
            image_saturation: 1.0,
        });
        crate::perf_telemetry::set_enabled(true);
        crate::perf_telemetry::reset();

        let mut renderer = MetalRenderer::new(Arc::new(Mutex::new(InstanceBufferPool::default())));
        renderer.update_drawable_size(size(
            DevicePixels(vw as i32),
            DevicePixels(vh as i32),
        ));
        let (img_tex, img_dim) = make_test_backdrop_texture(&renderer.device);
        renderer.backdrop_image_textures = vec![img_tex];
        renderer.backdrop_image_sizes = vec![img_dim];
        renderer.backdrop_images_loaded = true;
        renderer.backdrop_active_source = ActiveBackdropSource::Custom(0);
        renderer.backdrop_last_poll = Instant::now() + std::time::Duration::from_secs(3600);
        let target = make_offscreen_target(&renderer.device, vw, vh);

        let save_dir = std::env::var("AFTERLIFE_GPU_SNAPSHOT_SAVE").ok();
        let check_dir = std::env::var("AFTERLIFE_GPU_SNAPSHOT_CHECK").ok();
        if let Some(ref d) = save_dir {
            let _ = std::fs::create_dir_all(d);
        }

        let cases: &[(&str, f32, bool, u8, f32)] = &[
            ("wave_only_hold", 0.0, true, 0, 5.0),
            ("wave_only_dissolve", 0.0, true, 1, 2.2),
            ("glass_sharp_hold", 0.0, false, 0, 5.0),
            ("glass_blur12_hold", 12.0, false, 0, 5.0),
        ];

        for &(name, blur_px, wave_only, stage, stage_elapsed) in cases {
            let scene = build_realistic_scene(vw as f32, vh as f32, blur_px, wave_only);
            renderer.backdrop_phase_stage = stage;
            renderer.backdrop_phase_elapsed = stage_elapsed;
            renderer.backdrop_phase_last_tick = Instant::now();
            renderer.particle_tile_grid_cache_key = None;

            for _ in 0..30 {
                renderer.backdrop_phase_stage = stage;
                renderer.backdrop_phase_elapsed = stage_elapsed;
                renderer.backdrop_phase_last_tick = Instant::now();
                let _ = render_one_frame_sync(&mut renderer, &scene, &target, viewport, Some(12.345));
            }
            crate::perf_telemetry::reset();

            let mut enc_us = Vec::with_capacity(50);
            let mut gpu_us = Vec::with_capacity(50);
            for i in 0..50 {
                renderer.backdrop_phase_stage = stage;
                renderer.backdrop_phase_elapsed = stage_elapsed;
                renderer.backdrop_phase_last_tick = Instant::now();
                let t = if i == 49 { Some(12.345) } else { Some(12.345 + i as f32 * 0.016) };
                let (e, g) = render_one_frame_sync(&mut renderer, &scene, &target, viewport, t);
                enc_us.push(e);
                gpu_us.push(g);
            }
            enc_us.sort_unstable();
            gpu_us.sort_unstable();
            let passes = crate::perf_telemetry::drain_gpu_pass_breakdowns();
            let mut comp: Vec<u64> = passes.iter().map(|p| p.compute_gpu_us + p.blit_fill_gpu_us).collect();
            let mut rp0: Vec<u64> = passes.iter().map(|p| p.render_pass_0_gpu_us).collect();
            let mut blit: Vec<u64> = passes.iter().map(|p| p.blit_backdrop_gpu_us).collect();
            let mut rp1: Vec<u64> = passes.iter().map(|p| p.render_pass_rest_gpu_us).collect();
            comp.sort_unstable();
            rp0.sort_unstable();
            blit.sort_unstable();
            rp1.sort_unstable();
            let p50 = |v: &[u64]| if v.is_empty() { 0 } else { v[v.len() / 2] };
            let p10 = |v: &[u64]| if v.is_empty() { 0 } else { v[v.len() / 10] };
            println!(
                "[OFFSCREEN {:<18}] enc_p50={:>4}u | gpu_p10={:>4}u gpu_p50={:>4}u | Comp={:>4}u RP0={:>4}u Blit={:>4}u RP1+={:>4}u",
                name,
                p50(&enc_us),
                p10(&gpu_us),
                p50(&gpu_us),
                p50(&comp),
                p50(&rp0),
                p50(&blit),
                p50(&rp1),
            );

            if save_dir.is_some() || check_dir.is_some() {
                renderer.backdrop_phase_stage = stage;
                renderer.backdrop_phase_elapsed = stage_elapsed;
                renderer.backdrop_phase_last_tick = Instant::now();
                let _ = render_one_frame_sync(&mut renderer, &scene, &target, viewport, Some(12.345));
                let pixels = readback_bgra(&target, vw, vh);
                if let Some(ref d) = save_dir {
                    std::fs::write(format!("{}/{}.bgra", d, name), &pixels)
                        .expect("failed to write snapshot");
                }
                if let Some(ref d) = check_dir {
                    let ref_pixels = std::fs::read(format!("{}/{}.bgra", d, name))
                        .expect("failed to read ref snapshot");
                    assert_eq!(pixels.len(), ref_pixels.len());
                    let mut max_diff = 0u8;
                    let mut diff_gt_1_px = 0u64;
                    let mut diff_gt_0_px = 0u64;
                    let mut sum_diff = 0u64;
                    for chunk in pixels.chunks_exact(4).zip(ref_pixels.chunks_exact(4)) {
                        let (a, b) = chunk;
                        let md = a[0].abs_diff(b[0])
                            .max(a[1].abs_diff(b[1]))
                            .max(a[2].abs_diff(b[2]))
                            .max(a[3].abs_diff(b[3]));
                        if md > max_diff {
                            max_diff = md;
                        }
                        if md > 0 {
                            diff_gt_0_px += 1;
                            sum_diff += md as u64;
                        }
                        if md > 1 {
                            diff_gt_1_px += 1;
                        }
                    }
                    let total_px = (vw as u64) * (vh as u64);
                    println!(
                        "  [PIXEL-DIFF {:<18}] max_diff={} diff>0={} ({:.4}%) diff>1={} ({:.5}%) mean_abs={:.6}",
                        name,
                        max_diff,
                        diff_gt_0_px,
                        diff_gt_0_px as f64 * 100.0 / total_px as f64,
                        diff_gt_1_px,
                        diff_gt_1_px as f64 * 100.0 / total_px as f64,
                        sum_diff as f64 / total_px as f64,
                    );
                    if max_diff > 2 && stage == 0 {
                        panic!(
                            "visual regression on {}: max_diff={} (diff>1 px={})",
                            name, max_diff, diff_gt_1_px
                        );
                    }
                }
            }
        }
    }
}
