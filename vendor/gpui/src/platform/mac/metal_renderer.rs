use super::metal_atlas::MetalAtlas;
use crate::{
    AtlasTextureId, Background, BackgroundTag, Bounds, ContentMask, DevicePixels,
    MonochromeSprite, PaintSurface, Path, Point, PolychromeSprite, PrimitiveBatch, Quad,
    ScaledPixels, Scene, Shadow, Size, Surface, Underline, point, size,
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
use objc::{self, msg_send, sel, sel_impl};
use parking_lot::Mutex;

use std::{cell::Cell, ffi::c_void, mem, ptr, sync::Arc, time::Instant};

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
        let buffer = self.buffers.pop().unwrap_or_else(|| {
            device.new_buffer(
                self.buffer_size as u64,
                MTLResourceOptions::StorageModeManaged,
            )
        });
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
                .filter_map(|item| item.get("path").and_then(|p| p.as_str()).map(str::to_string))
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

fn load_backdrop_images_from_entries(
    device: &metal::Device,
    entries: &[BackdropImageEntryKey],
) -> (Vec<metal::Texture>, Vec<[f32; 3]>) {
    let mut textures = Vec::new();
    let mut sizes = Vec::new();
    for (entry, _) in entries {
        if let Ok(img) = image::open(entry) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            if w > 0 && h > 0 {
                let corners_opaque = [
                    rgba.get_pixel(0, 0)[3],
                    rgba.get_pixel(w - 1, 0)[3],
                    rgba.get_pixel(0, h - 1)[3],
                    rgba.get_pixel(w - 1, h - 1)[3],
                ]
                .into_iter()
                .all(|a| a >= 250);
                let fit_cover = if corners_opaque { 1.0 } else { 0.0 };
                textures.push(create_rgba8_texture(device, w, h, rgba.as_raw()));
                sizes.push([w as f32, h as f32, fit_cover]);
            }
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

fn inactive_particle_uniforms(
    time_sec: f32,
    viewport_w: f32,
    viewport_h: f32,
    tuning: crate::ParticleWaveTuning,
) -> ParticleWaveUniforms {
    ParticleWaveUniforms {
        time_sec,
        has_image: 0.0,
        image_w: 1.0,
        image_h: 1.0,
        phase_mode: 0.0,
        phase_progress: 0.0,
        grid_w: 1,
        grid_h: 1,
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

pub(crate) struct MetalRenderer {
    device: metal::Device,
    layer: metal::MetalLayer,
    presents_with_transaction: bool,
    command_queue: CommandQueue,
    paths_rasterization_pipeline_state: metal::RenderPipelineState,
    path_sprites_pipeline_state: metal::RenderPipelineState,
    shadows_pipeline_state: metal::RenderPipelineState,
    quads_pipeline_state: metal::RenderPipelineState,
    liquid_glass_pipeline_state: metal::RenderPipelineState,
    liquid_glass_synced_pipeline_state: metal::RenderPipelineState,
    particle_wave_pipeline_state: metal::RenderPipelineState,
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
    backdrop_sampler: metal::SamplerState,
    backdrop_image_textures: Vec<metal::Texture>,
    backdrop_image_sizes: Vec<[f32; 3]>,
    backdrop_loaded_entries: Vec<BackdropImageEntryKey>,
    backdrop_images_loaded: bool,
    backdrop_config_path: Option<std::path::PathBuf>,
    backdrop_config_mtime: Option<std::time::SystemTime>,
    backdrop_last_poll: Instant,
    backdrop_phase_stage: u8,
    backdrop_phase_elapsed: f32,
    backdrop_phase_last_tick: Instant,
    backdrop_image_index: usize,
    particle_tile_scatter_pipeline: metal::ComputePipelineState,
    particle_tile_grid_buffer: metal::Buffer,
    particle_tile_grid_capacity: usize,
    current_particle_uniforms: ParticleWaveUniforms,
    start_time: Instant,
    path_sample_count: u32,
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
        let unit_vertices = device.new_buffer_with_data(
            unit_vertices.as_ptr() as *const c_void,
            mem::size_of_val(&unit_vertices) as u64,
            MTLResourceOptions::StorageModeManaged,
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
        let liquid_glass_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "liquid_glass",
            "quad_vertex",
            "liquid_glass_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let liquid_glass_synced_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "liquid_glass_synced",
            "quad_vertex",
            "liquid_glass_synced_fragment",
            MTLPixelFormat::BGRA8Unorm,
        );
        let particle_wave_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "particle_wave",
            "quad_vertex",
            "particle_wave_fragment",
            MTLPixelFormat::BGRA8Unorm,
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
        let particle_tile_grid_capacity = 1usize;
        let particle_tile_grid_buffer = device.new_buffer(4, MTLResourceOptions::StorageModePrivate);

        Self {
            device,
            layer,
            presents_with_transaction: false,
            command_queue,
            paths_rasterization_pipeline_state,
            path_sprites_pipeline_state,
            shadows_pipeline_state,
            quads_pipeline_state,
            liquid_glass_pipeline_state,
            liquid_glass_synced_pipeline_state,
            particle_wave_pipeline_state,
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
            backdrop_sampler,
            backdrop_image_textures,
            backdrop_image_sizes,
            backdrop_loaded_entries,
            backdrop_images_loaded,
            backdrop_config_path,
            backdrop_config_mtime,
            backdrop_last_poll: Instant::now(),
            backdrop_phase_stage: 0,
            backdrop_phase_elapsed: 0.0,
            backdrop_phase_last_tick: Instant::now(),
            backdrop_image_index: 0,
            particle_tile_scatter_pipeline,
            particle_tile_grid_buffer,
            particle_tile_grid_capacity,
            current_particle_uniforms: ParticleWaveUniforms::default(),
            start_time: Instant::now(),
            path_sample_count: PATH_SAMPLE_COUNT,
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
        // We are uncertain when this happens, but sometimes size can be 0 here. Most likely before
        // the layout pass on window creation. Zero-sized texture creation causes SIGABRT.
        // https://github.com/zed-industries/zed/issues/36229
        if size.width.0 <= 0 || size.height.0 <= 0 {
            self.path_intermediate_texture = None;
            self.path_intermediate_msaa_texture = None;
            self.backdrop_texture = None;
            return;
        }

        let texture_descriptor = metal::TextureDescriptor::new();
        texture_descriptor.set_width(size.width.0 as u64);
        texture_descriptor.set_height(size.height.0 as u64);
        texture_descriptor.set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
        texture_descriptor
            .set_usage(metal::MTLTextureUsage::RenderTarget | metal::MTLTextureUsage::ShaderRead);
        self.path_intermediate_texture = Some(self.device.new_texture(&texture_descriptor));

        let min_dim = (size.width.0.max(1) as u64).min(size.height.0.max(1) as u64);
        let mip_levels = (64 - min_dim.leading_zeros()).clamp(1, 6) as u64;
        let backdrop_desc = metal::TextureDescriptor::new();
        backdrop_desc.set_width(size.width.0 as u64);
        backdrop_desc.set_height(size.height.0 as u64);
        backdrop_desc.set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
        backdrop_desc.set_mipmap_level_count(mip_levels);
        backdrop_desc.set_storage_mode(metal::MTLStorageMode::Private);
        backdrop_desc.set_usage(metal::MTLTextureUsage::ShaderRead);
        self.backdrop_texture = Some(self.device.new_texture(&backdrop_desc));

        if self.path_sample_count > 1 {
            let mut msaa_descriptor = texture_descriptor;
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

            let command_buffer =
                self.draw_primitives(scene, &mut instance_buffer, drawable, viewport_size, perf_on);

            match command_buffer {
                Ok((command_buffer, perf_sample)) => {
                    let instance_buffer_pool = self.instance_buffer_pool.clone();
                    let instance_buffer = Cell::new(Some(instance_buffer));
                    let block = ConcreteBlock::new(move |cb: &metal::CommandBufferRef| {
                        if perf_on && crate::perf_telemetry::is_any_enabled() {
                            unsafe {
                                let ptr = cb.as_ptr() as *mut objc::runtime::Object;
                                let gpu_start: f64 = msg_send![ptr, GPUStartTime];
                                let gpu_end: f64 = msg_send![ptr, GPUEndTime];
                                if gpu_start > 0.0 && gpu_end >= gpu_start {
                                    let dur_us = ((gpu_end - gpu_start) * 1_000_000.0).round() as u64;
                                    crate::perf_telemetry::record_gpu_timing_us(dur_us);
                                }
                            }
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
        drawable: &metal::MetalDrawableRef,
        viewport_size: Size<DevicePixels>,
        perf_on: bool,
    ) -> Result<(metal::CommandBuffer, Option<crate::perf_telemetry::MetalFrameSample>)> {
        let encode_start = perf_on.then(Instant::now);
        let mut perf = perf_on.then(|| crate::perf_telemetry::MetalFrameSample {
            viewport_w: viewport_size.width.0.max(0) as u32,
            viewport_h: viewport_size.height.0.max(0) as u32,
            encoder_switches: 1,
            ..Default::default()
        });
        let command_queue = self.command_queue.clone();
        let command_buffer = command_queue.new_command_buffer();
        let alpha = if self.layer.is_opaque() { 1. } else { 0. };
        let mut instance_offset = 0;
        let mut backdrop_seeded = false;
        let mut backdrop_mips_valid = false;
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

        let compute_dispatched = self.dispatch_particle_tile_scatter(command_buffer, viewport_size);
        if let Some(p) = perf.as_mut() {
            p.compute_scatter_dispatched = compute_dispatched;
            p.backdrop_images_loaded = self.backdrop_image_textures.len() as u32;
        }

        let mut command_encoder = new_command_encoder(
            command_buffer,
            drawable,
            viewport_size,
            |color_attachment| {
                color_attachment.set_load_action(metal::MTLLoadAction::Clear);
                color_attachment.set_clear_color(metal::MTLClearColor::new(0., 0., 0., alpha));
            },
        );

        let quad_mode = |q: &Quad| -> u8 {
            match q.background.tag {
                BackgroundTag::LiquidGlass => {
                    let edge_mode = q.background.colors[1].color.h;
                    if (1.62..1.88).contains(&edge_mode) {
                        3u8 // Production Backdrop-Synced fast path
                    } else {
                        1u8 // Generic demo liquid glass
                    }
                }
                BackgroundTag::ParticleWave => 2u8,
                _ => 0u8,
            }
        };

        for batch in scene.batches() {
            let ok = match batch {
                PrimitiveBatch::Shadows(shadows) => {
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
                    self.draw_shadows(
                        shadows,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_encoder,
                    )
                }
                PrimitiveBatch::Quads(quads) => {
                    let mut ok = true;
                    let mut i = 0;
                    while i < quads.len() {
                        let mode = quad_mode(&quads[i]);
                        let start = i;
                        while i < quads.len() && quad_mode(&quads[i]) == mode {
                            i += 1;
                        }
                        let chunk = &quads[start..i];
                        if mode == 1 || mode == 3 {
                            if let Some(p) = perf.as_mut() {
                                p.quads_glass += chunk.len() as u32;
                            }
                            let chunk_needs_blur = chunk
                                .iter()
                                .any(|q| q.background.colors[1].percentage * 2.0 > 0.5);

                            if !backdrop_seeded {
                                command_encoder.end_encoding();
                                let copied_px = self.copy_drawable_to_backdrop(
                                    command_buffer,
                                    drawable.texture(),
                                    viewport_size,
                                    None,
                                    chunk_needs_blur,
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
                                    drawable,
                                    viewport_size,
                                    |color_attachment| {
                                        color_attachment
                                            .set_load_action(metal::MTLLoadAction::Load);
                                    },
                                );
                                backdrop_seeded = true;
                                backdrop_mips_valid = chunk_needs_blur;
                                dirty_rects.clear();
                            } else {
                                let mut needs_blit = false;
                                if !dirty_rects.is_empty() {
                                    // Check whether any glass quad in this chunk overlaps any dirty rect.
                                    // Refraction bends strictly inward (-gradN), so only blurPx > 0.5
                                    // samples outside quad bounds (<= blurPx + 2px).
                                    'check_hit: for q in chunk {
                                        let blur_px = q.background.colors[1].percentage * 2.0;
                                        let margin = if blur_px > 0.5 { blur_px + 2.0 } else { 2.0 };
                                        let padded = Bounds {
                                            origin: point(
                                                ScaledPixels((q.bounds.origin.x.0 - margin).max(0.0)),
                                                ScaledPixels((q.bounds.origin.y.0 - margin).max(0.0)),
                                            ),
                                            size: size(
                                                ScaledPixels(q.bounds.size.width.0 + margin * 2.0),
                                                ScaledPixels(q.bounds.size.height.0 + margin * 2.0),
                                            ),
                                        };
                                        for dirty in &dirty_rects {
                                            if dirty.intersects(&padded) {
                                                needs_blit = true;
                                                break 'check_hit;
                                            }
                                        }
                                    }
                                }

                                if needs_blit {
                                    // Coalesce nearby/overlapping dirty rects (within 16px) into a
                                    // small set (<= 12) of compact copy regions so we avoid both
                                    // full-screen AABB bloat across panes and tiny glyph-level blits.
                                    const CLUSTER_PAD: f32 = 16.0;
                                    let mut clusters: Vec<Bounds<ScaledPixels>> =
                                        Vec::with_capacity(12);
                                    for &d in &dirty_rects {
                                        let mut merged = false;
                                        for c in &mut clusters {
                                            let exp = Bounds {
                                                origin: point(
                                                    ScaledPixels(c.origin.x.0 - CLUSTER_PAD),
                                                    ScaledPixels(c.origin.y.0 - CLUSTER_PAD),
                                                ),
                                                size: size(
                                                    ScaledPixels(c.size.width.0 + CLUSTER_PAD * 2.0),
                                                    ScaledPixels(c.size.height.0 + CLUSTER_PAD * 2.0),
                                                ),
                                            };
                                            if exp.intersects(&d) {
                                                *c = c.union(&d);
                                                merged = true;
                                                break;
                                            }
                                        }
                                        if !merged {
                                            if clusters.len() >= 12 {
                                                let mut best_idx = 0usize;
                                                let mut best_cost = f32::INFINITY;
                                                for (idx, c) in clusters.iter().enumerate() {
                                                    let u = c.union(&d);
                                                    let cost = u.size.width.0 * u.size.height.0
                                                        - c.size.width.0 * c.size.height.0;
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
                                    command_encoder.end_encoding();
                                    let copied_px = self.copy_regions_to_backdrop(
                                        command_buffer,
                                        drawable.texture(),
                                        viewport_size,
                                        &clusters,
                                        chunk_needs_blur,
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
                                        drawable,
                                        viewport_size,
                                        |color_attachment| {
                                            color_attachment
                                                .set_load_action(metal::MTLLoadAction::Load);
                                        },
                                    );
                                    backdrop_mips_valid = chunk_needs_blur;
                                    dirty_rects.clear();
                                } else if chunk_needs_blur && !backdrop_mips_valid {
                                    command_encoder.end_encoding();
                                    self.generate_backdrop_mipmaps(command_buffer);
                                    if let Some(p) = perf.as_mut() {
                                        p.encoder_switches += 1;
                                    }
                                    command_encoder = new_command_encoder(
                                        command_buffer,
                                        drawable,
                                        viewport_size,
                                        |color_attachment| {
                                            color_attachment
                                                .set_load_action(metal::MTLLoadAction::Load);
                                        },
                                    );
                                    backdrop_mips_valid = true;
                                }
                            }

                            ok = self.draw_liquid_glass_quads(
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
                        } else if mode == 2 {
                            if let Some(p) = perf.as_mut() {
                                p.quads_wave += chunk.len() as u32;
                            }
                            for q in chunk {
                                push_dirty(&mut dirty_rects, q.bounds, 0.0);
                            }
                            ok = self.draw_particle_wave_quads(
                                chunk,
                                instance_buffer,
                                &mut instance_offset,
                                viewport_size,
                                command_encoder,
                            );
                        } else {
                            if let Some(p) = perf.as_mut() {
                                p.quads_regular += chunk.len() as u32;
                            }
                            for q in chunk {
                                push_dirty(&mut dirty_rects, q.bounds, 0.0);
                            }
                            ok = self.draw_quads(
                                chunk,
                                instance_buffer,
                                &mut instance_offset,
                                viewport_size,
                                command_encoder,
                            );
                        }
                        if !ok {
                            break;
                        }
                    }
                    ok
                }
                PrimitiveBatch::Paths(paths) => {
                    if let Some(p) = perf.as_mut() {
                        p.paths += paths.len() as u32;
                        p.encoder_switches += 1;
                    }
                    for p in paths {
                        push_dirty(&mut dirty_rects, p.bounds, 0.0);
                    }
                    command_encoder.end_encoding();

                    let did_draw = self.draw_paths_to_intermediate(
                        paths,
                        instance_buffer,
                        &mut instance_offset,
                        viewport_size,
                        command_buffer,
                    );

                    command_encoder = new_command_encoder(
                        command_buffer,
                        drawable,
                        viewport_size,
                        |color_attachment| {
                            color_attachment.set_load_action(metal::MTLLoadAction::Load);
                        },
                    );

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
                PrimitiveBatch::Underlines(underlines) => {
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
                PrimitiveBatch::MonochromeSprites {
                    texture_id,
                    sprites,
                } => {
                    if let Some(p) = perf.as_mut() {
                        p.monochrome_sprites += sprites.len() as u32;
                    }
                    for s in sprites {
                        push_dirty(&mut dirty_rects, s.bounds, 4.0);
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
                PrimitiveBatch::PolychromeSprites {
                    texture_id,
                    sprites,
                } => {
                    if let Some(p) = perf.as_mut() {
                        p.polychrome_sprites += sprites.len() as u32;
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
                PrimitiveBatch::Surfaces(surfaces) => {
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

        command_encoder.end_encoding();

        instance_buffer.metal_buffer.did_modify_range(NSRange {
            location: 0,
            length: instance_offset as NSUInteger,
        });
        if let (Some(p), Some(start)) = (perf.as_mut(), encode_start) {
            p.encode_us = start.elapsed().as_micros() as u64;
            p.instance_bytes = instance_offset as u64;
        }
        Ok((command_buffer.to_owned(), perf))
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
        instance_buffer: &mut InstanceBuffer,
        instance_offset: &mut usize,
        viewport_size: Size<DevicePixels>,
        command_encoder: &metal::RenderCommandEncoderRef,
    ) -> bool {
        if quads.is_empty() {
            return true;
        }
        align_offset(instance_offset);

        command_encoder.set_render_pipeline_state(&self.quads_pipeline_state);
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
        command_encoder.set_fragment_buffer(
            QuadInputIndex::Quads as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );

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

    fn copy_drawable_to_backdrop(
        &self,
        command_buffer: &metal::CommandBufferRef,
        drawable_tex: &metal::TextureRef,
        viewport_size: Size<DevicePixels>,
        region: Option<Bounds<ScaledPixels>>,
        generate_mipmaps: bool,
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

        let (ox, oy, cw, ch) = if let Some(r) = region {
            let x0 = (r.origin.x.0.floor().max(0.0) as u64).min(max_w);
            let y0 = (r.origin.y.0.floor().max(0.0) as u64).min(max_h);
            let x1 = (r.bottom_right().x.0.ceil().max(0.0) as u64).min(max_w);
            let y1 = (r.bottom_right().y.0.ceil().max(0.0) as u64).min(max_h);
            if x1 <= x0 || y1 <= y0 {
                return 0;
            }
            (x0, y0, x1 - x0, y1 - y0)
        } else {
            (0, 0, max_w, max_h)
        };

        let blit = command_buffer.new_blit_command_encoder();
        blit.copy_from_texture(
            drawable_tex,
            0,
            0,
            metal::MTLOrigin {
                x: ox,
                y: oy,
                z: 0,
            },
            metal::MTLSize {
                width: cw,
                height: ch,
                depth: 1,
            },
            backdrop_tex,
            0,
            0,
            metal::MTLOrigin {
                x: ox,
                y: oy,
                z: 0,
            },
        );
        if generate_mipmaps && backdrop_tex.mipmap_level_count() > 1 {
            blit.generate_mipmaps(backdrop_tex);
        }
        blit.end_encoding();
        cw * ch
    }

    fn copy_regions_to_backdrop(
        &self,
        command_buffer: &metal::CommandBufferRef,
        drawable_tex: &metal::TextureRef,
        viewport_size: Size<DevicePixels>,
        regions: &[Bounds<ScaledPixels>],
        generate_mipmaps: bool,
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

        let mut blit: Option<&metal::BlitCommandEncoderRef> = None;
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
            let enc = blit.get_or_insert_with(|| command_buffer.new_blit_command_encoder());
            enc.copy_from_texture(
                drawable_tex,
                0,
                0,
                metal::MTLOrigin {
                    x: x0,
                    y: y0,
                    z: 0,
                },
                metal::MTLSize {
                    width: cw,
                    height: ch,
                    depth: 1,
                },
                backdrop_tex,
                0,
                0,
                metal::MTLOrigin {
                    x: x0,
                    y: y0,
                    z: 0,
                },
            );
            total_px += cw * ch;
        }
        if generate_mipmaps && backdrop_tex.mipmap_level_count() > 1 {
            let enc = blit.get_or_insert_with(|| command_buffer.new_blit_command_encoder());
            enc.generate_mipmaps(backdrop_tex);
        }
        if let Some(enc) = blit {
            enc.end_encoding();
        }
        total_px
    }

    fn generate_backdrop_mipmaps(&self, command_buffer: &metal::CommandBufferRef) {
        let Some(backdrop_tex) = &self.backdrop_texture else {
            return;
        };
        if backdrop_tex.mipmap_level_count() <= 1 {
            return;
        }
        let blit = command_buffer.new_blit_command_encoder();
        blit.generate_mipmaps(backdrop_tex);
        blit.end_encoding();
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
            &self.liquid_glass_synced_pipeline_state
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
        command_encoder.set_fragment_buffer(
            QuadInputIndex::Quads as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_vertex_bytes(
            QuadInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_fragment_bytes(
            QuadInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );
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

    fn release_backdrop_image_resources(&mut self) {
        if !self.backdrop_image_textures.is_empty() {
            self.backdrop_image_textures.clear();
            self.backdrop_image_sizes.clear();
        }
        if !self.backdrop_loaded_entries.is_empty() {
            self.backdrop_loaded_entries.clear();
        }
        if self.particle_tile_grid_capacity > 1 {
            self.particle_tile_grid_capacity = 1;
            self.particle_tile_grid_buffer =
                self.device.new_buffer(4, MTLResourceOptions::StorageModePrivate);
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
    }

    fn dispatch_particle_tile_scatter(
        &mut self,
        command_buffer: &metal::CommandBufferRef,
        viewport_size: Size<DevicePixels>,
    ) -> bool {
        let time_sec = self.start_time.elapsed().as_secs_f32();
        let vw = viewport_size.width.0.max(1) as f32;
        let vh = viewport_size.height.0.max(1) as f32;
        let tuning = crate::particle_wave_tuning().clamped();
        let tile_size = tuning.tile_size_px;

        if !tuning.image_enabled {
            if self.backdrop_images_loaded || self.particle_tile_grid_capacity > 1 {
                self.release_backdrop_image_resources();
                self.backdrop_images_loaded = false;
                self.backdrop_config_mtime = None;
            }
            self.current_particle_uniforms = inactive_particle_uniforms(time_sec, vw, vh, tuning);
            return false;
        }

        self.sync_backdrop_images();

        if self.backdrop_image_textures.is_empty() {
            if self.particle_tile_grid_capacity > 1 {
                self.release_backdrop_image_resources();
            }
            self.current_particle_uniforms = inactive_particle_uniforms(time_sec, vw, vh, tuning);
            return false;
        }

        let grid_w = (vw / tile_size).ceil().max(1.0) as u32;
        let grid_h = (vh / tile_size).ceil().max(1.0) as u32;
        let needed = (grid_w as usize) * (grid_h as usize);
        if needed > self.particle_tile_grid_capacity {
            self.particle_tile_grid_capacity = needed;
            self.particle_tile_grid_buffer = self.device.new_buffer(
                (needed * 4) as u64,
                MTLResourceOptions::StorageModePrivate,
            );
        }

        let now = Instant::now();
        let dt = now
            .saturating_duration_since(self.backdrop_phase_last_tick)
            .as_secs_f32()
            .min(0.25);
        self.backdrop_phase_last_tick = now;

        let hold_sec = tuning.image_hold_secs;
        let disintegrate_sec = 6.2_f32;
        let void_sec = 0.7_f32;
        let reassemble_sec = 6.2_f32;

        let img_count = self.backdrop_image_textures.len();
        self.backdrop_image_index %= img_count;
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
            self.backdrop_phase_stage = (self.backdrop_phase_stage + 1) % 4;
            if self.backdrop_phase_stage == 2 {
                self.backdrop_image_index = (self.backdrop_image_index + 1) % img_count;
            }
        }

        let stage_dur = match self.backdrop_phase_stage {
            0 => hold_sec,
            1 => disintegrate_sec,
            2 => void_sec,
            _ => reassemble_sec,
        };
        let phase_mode = self.backdrop_phase_stage as f32;
        let phase_progress = (self.backdrop_phase_elapsed / stage_dur.max(0.001)).clamp(0.0, 1.0);
        let tex_idx = self.backdrop_image_index;
        let dims = self.backdrop_image_sizes[tex_idx];
        let uniforms = ParticleWaveUniforms {
            time_sec,
            has_image: 1.0,
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

        let blit = command_buffer.new_blit_command_encoder();
        blit.fill_buffer(
            &self.particle_tile_grid_buffer,
            NSRange::new(0, (needed * 4) as u64),
            0,
        );
        blit.end_encoding();

        if phase_mode > 1.5 && phase_mode < 2.5 {
            return false;
        }

        let compute = command_buffer.new_compute_command_encoder();
        compute.set_compute_pipeline_state(&self.particle_tile_scatter_pipeline);
        compute.set_buffer(0, Some(&self.particle_tile_grid_buffer), 0);
        compute.set_bytes(
            1,
            mem::size_of::<ParticleWaveUniforms>() as u64,
            &uniforms as *const ParticleWaveUniforms as *const _,
        );
        compute.set_texture(0, Some(&self.backdrop_image_textures[tex_idx]));
        compute.set_sampler_state(0, Some(&self.backdrop_sampler));
        let tg = metal::MTLSize::new(16, 16, 1);
        let grid = metal::MTLSize::new(grid_w as u64, grid_h as u64, 1);
        compute.dispatch_threads(grid, tg);
        compute.end_encoding();
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

        command_encoder.set_render_pipeline_state(&self.particle_wave_pipeline_state);
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
        command_encoder.set_fragment_buffer(
            QuadInputIndex::Quads as u64,
            Some(&instance_buffer.metal_buffer),
            *instance_offset as u64,
        );
        command_encoder.set_vertex_bytes(
            QuadInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_fragment_bytes(
            QuadInputIndex::ViewportSize as u64,
            mem::size_of_val(&viewport_size) as u64,
            &viewport_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_fragment_bytes(
            3,
            mem::size_of::<ParticleWaveUniforms>() as u64,
            &self.current_particle_uniforms as *const ParticleWaveUniforms as *const _,
        );
        command_encoder.set_fragment_buffer(4, Some(&self.particle_tile_grid_buffer), 0);

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

fn new_command_encoder<'a>(
    command_buffer: &'a metal::CommandBufferRef,
    drawable: &'a metal::MetalDrawableRef,
    viewport_size: Size<DevicePixels>,
    configure_color_attachment: impl Fn(&RenderPassColorAttachmentDescriptorRef),
) -> &'a metal::RenderCommandEncoderRef {
    let render_pass_descriptor = metal::RenderPassDescriptor::new();
    let color_attachment = render_pass_descriptor
        .color_attachments()
        .object_at(0)
        .unwrap();
    color_attachment.set_texture(Some(drawable.texture()));
    color_attachment.set_store_action(metal::MTLStoreAction::Store);
    configure_color_attachment(color_attachment);

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
