//! wgpu GPU-accelerated scene renderer.

use std::sync::Arc;

use wgpu::util::DeviceExt;

use neomacs_display_protocol::face::BoxVerticalEdges;
use neomacs_display_protocol::frame_glyphs::{FringeBitmapData, StipplePattern};
use neomacs_display_protocol::scene::{Scene, SceneCursorStyle};
use neomacs_display_protocol::types::Color;

use super::image_cache::ImageCache;
use super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, SubpixelGlyphVertex, Uniforms};
#[cfg(feature = "video")]
use super::video_cache::VideoCache;
#[cfg(all(feature = "webview", target_os = "linux"))]
use super::webview_cache::WgpuWebViewCache;

mod box_tessellation;
mod child_frames;
mod content;
mod cursor_effects;
mod cursor_presentation;
mod deform;
mod dynamic_buffer;
mod effect_common;
mod effects_state;
mod frame_pass;
mod fx_state;
mod glyphs;
mod layer_backgrounds;
mod layer_chrome;
mod layer_effects;
mod layer_media;
mod layer_text;
mod media;
mod pattern_effects;
mod pointer_override;
mod resources;
mod row_reuse;
mod scissor;
mod stats;
mod transitions;
mod ui_overlays;
#[cfg(test)]
mod ui_overlays_test;
mod window_effects;

pub use fx_state::RendererFrameEffects;
pub(crate) use fx_state::*;
pub(crate) use resources::*;
pub use row_reuse::{FrameRowDamage, RowDamageInfo, RowReuseStats, WindowRowDamage};
pub use stats::*;

#[cfg(feature = "video")]
fn create_bi_planar_video_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
    depth_stencil: Option<wgpu::DepthStencilState>,
    label: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(GlyphVertex::desc())],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        cache: None,
        multiview_mask: None,
    })
}

#[cfg(feature = "video")]
fn create_bi_planar_video_copy_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Bi-planar Video Shader-channel Copy Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_copy"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        cache: None,
        multiview_mask: None,
    })
}

/// GPU-accelerated renderer using wgpu.
pub struct WgpuRenderer {
    pub(crate) device: Arc<wgpu::Device>,
    pub(crate) queue: Arc<wgpu::Queue>,
    pub(super) surface: Option<wgpu::Surface<'static>>,
    pub(super) surface_config: Option<wgpu::SurfaceConfiguration>,
    pub(super) surface_format: wgpu::TextureFormat,
    /// Render pipelines (base + stencil-clipped variants)
    pub(super) pipelines: Pipelines,
    /// Stencil texture/view for child frame rounded-corner clipping
    pub(super) stencil: StencilTargets,
    pub(super) glyph_bind_group_layout: wgpu::BindGroupLayout,
    pub(super) uniform_buffer: wgpu::Buffer,
    pub(super) uniform_bind_group: wgpu::BindGroup,
    /// Texture/media caches
    pub(super) caches: RenderCaches,
    /// Bounded asynchronous timestamp queries for frame-content passes.
    #[cfg(feature = "video")]
    pub(super) gpu_frame_timer: crate::gpu_frame_timing::GpuFrameTimer,
    /// Per-frame reusable vertex upload arenas
    pub(super) arenas: VertexArenas,
    pub(super) width: u32,
    pub(super) height: u32,
    /// Display scale factor (physical pixels / logical pixels)
    pub(super) scale_factor: f32,
    /// Logical screen size last written into `uniform_buffer` by [`Self::resize`],
    /// so a resize that changes nothing does not re-upload it. `None` until the
    /// first resize. Recording the written value (rather than comparing inputs)
    /// keeps this correct across `set_scale_factor`, which only stores the scale
    /// — `resize` is what pushes it to the GPU.
    pub(super) uniform_screen_size: Option<[f32; 2]>,
    /// User full-frame post shader (docs/display-engine/SHADER_SURFACES.md).
    pub(super) frame_post: Option<crate::frame_post::FramePost>,
    /// Unified media memory accounting + surface eviction (media_budget.rs).
    pub(super) media_budget: crate::media_budget::MediaBudget,
    /// Which shader surfaces the eviction driver may free (declarative specs
    /// re-resolve on the next redisplay walk; imperative handles cannot).
    pub(super) surface_recreatable: std::collections::HashMap<u32, bool>,

    // All visual effect configurations
    pub effects: crate::effect_config::EffectsConfig,
    /// Grouped per-effect animation state (transferred by frame-effects swaps)
    pub(super) fx: EffectsState,
    /// Free-running animation clocks (transferred with preserve-if-unset semantics)
    pub(super) clocks: EffectClocks,
    /// Ambient clocks shared by every frame context (not transferred)
    pub(super) ambient: AmbientClocks,
    /// Cached per-row text vertex streams for RowDamage-driven reuse
    pub(super) row_reuse: row_reuse::RowReuseCache,
    pub glyph_stats: GlyphRenderStats,
    /// Absolute time this frame's animation samples target (the frame tick's
    /// target presentation time). Set by the runtime before each render so
    /// time-driven effects sample one consistent instant instead of reading
    /// the wall clock mid-draw.
    /// The one time sample every effect in this frame dates itself to.
    ///
    /// Set once per frame from the scheduler's tick, so two effects drawn in
    /// the same frame cannot disagree about what time it is.
    pub(super) frame_sample: neomacs_display_protocol::frame_time::FrameSample,
    /// Monotonic frame counter, advanced with [`Self::set_frame_sample`].
    ///
    /// Effects that need pseudo-random per-entity values used to reach for
    /// `Instant::now().elapsed().subsec_nanos()`, entropy disguised as a clock
    /// read. The frame's time sample cannot replace that — it is constant
    /// across a frame, so it would collapse a scatter to a single value. This
    /// counter is the entropy input instead: mixed with an entity index by
    /// `effect_common::effect_entity_seed`, it gives every entity in a frame a
    /// different seed and the same entity a different seed each frame, and
    /// unlike the clock it replays identically.
    pub(super) frame_seq: u64,
}

impl WgpuRenderer {
    /// Create a new WgpuRenderer with its own GPU device.
    ///
    /// Returns an error if GPU initialization fails.
    /// Prefer `with_device()` when you already have a device/queue.
    pub fn new(
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        pollster::block_on(Self::new_async(surface, width, height))
    }

    /// Create a new WgpuRenderer using an existing device and queue.
    ///
    /// This is useful when you need to share the wgpu device with other components,
    /// such as when surfaces are created with a specific device.
    ///
    /// The `surface_format` parameter specifies the texture format for render pipelines.
    /// This must match the format of the surface being rendered to.
    pub fn with_device(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        surface_format: wgpu::TextureFormat,
        scale_factor: f32,
    ) -> Self {
        Self::create_renderer_internal(
            device,
            queue,
            None,
            Some(surface_format),
            width,
            height,
            scale_factor,
            #[cfg(feature = "video")]
            neomacs_video::GpuGeneration::INITIAL,
            #[cfg(feature = "video")]
            neomacs_video::VideoWake::noop(),
        )
    }

    /// Create a renderer whose native video adapters share this render
    /// thread's wake source and device generation.
    #[cfg(feature = "video")]
    #[allow(clippy::too_many_arguments)]
    pub fn with_device_and_video_runtime(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        surface_format: wgpu::TextureFormat,
        scale_factor: f32,
        generation: neomacs_video::GpuGeneration,
        wake: neomacs_video::VideoWake,
    ) -> Self {
        Self::create_renderer_internal(
            device,
            queue,
            None,
            Some(surface_format),
            width,
            height,
            scale_factor,
            generation,
            wake,
        )
    }

    /// Internal helper that creates the renderer with the given device/queue.
    ///
    /// This handles pipeline and buffer creation, and is used by both `new_async`
    /// and `with_device`.
    ///
    /// The `surface_format` parameter specifies the texture format for render pipelines.
    /// If None, defaults to Bgra8UnormSrgb.
    fn create_renderer_internal(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        surface: Option<wgpu::Surface<'static>>,
        surface_format: Option<wgpu::TextureFormat>,
        width: u32,
        height: u32,
        scale_factor: f32,
        #[cfg(feature = "video")] video_generation: neomacs_video::GpuGeneration,
        #[cfg(feature = "video")] video_wake: neomacs_video::VideoWake,
    ) -> Self {
        // Create uniform buffer with logical size so vertex positions from Emacs map correctly
        let logical_w = width as f32 / scale_factor;
        let logical_h = height as f32 / scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Create bind group
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Load rect shader
        let rect_shader_source = include_str!("../shaders/rect.wgsl");
        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(rect_shader_source.into()),
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rect Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // Determine the target format
        let target_format = surface_format.unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        // Create rect pipeline
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rect Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(RectVertex::desc())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            cache: None,
            multiview_mask: None,
        });

        // Load rounded rect shader (SDF-based rounded borders)
        let rounded_rect_shader_source = include_str!("../shaders/rounded_rect.wgsl");
        let rounded_rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rounded Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(rounded_rect_shader_source.into()),
        });

        let rounded_rect_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Rounded Rect Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &rounded_rect_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(RoundedRectVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &rounded_rect_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        // Corner mask pipeline: uses the same SDF rounded rect shader but with
        // a blend mode that multiplies the destination by the source alpha.
        // This clips window corners to a rounded shape.
        let corner_mask_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Corner Mask Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rounded_rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(RoundedRectVertex::desc())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rounded_rect_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState {
                        // dst = dst * src_alpha (mask mode)
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::SrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::SrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            cache: None,
            multiview_mask: None,
        });

        // Load glyph shader
        let glyph_shader_source = include_str!("../shaders/glyph.wgsl");
        let glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Glyph Shader"),
            source: wgpu::ShaderSource::Wgsl(glyph_shader_source.into()),
        });
        let subpixel_glyph_shader_source = include_str!("../shaders/glyph_subpixel.wgsl");
        let subpixel_glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Subpixel Glyph Shader"),
            source: wgpu::ShaderSource::Wgsl(subpixel_glyph_shader_source.into()),
        });

        // Glyph bind group layout (for per-glyph texture)
        let glyph_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Glyph Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Glyph pipeline layout (uniform + glyph texture)
        let glyph_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Glyph Pipeline Layout"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&glyph_bind_group_layout)],
                immediate_size: 0,
            });

        // Create glyph pipeline
        let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Glyph Pipeline"),
            layout: Some(&glyph_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &glyph_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(GlyphVertex::desc())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &glyph_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            cache: None,
            multiview_mask: None,
        });

        let subpixel_glyph_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Subpixel Glyph Pipeline"),
                layout: Some(&glyph_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &subpixel_glyph_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(SubpixelGlyphVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &subpixel_glyph_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        // Create image cache (also creates its bind group layout)
        let image_cache = ImageCache::new(&device);

        // Create video cache
        #[cfg(feature = "video")]
        let gpu_frame_timer = crate::gpu_frame_timing::GpuFrameTimer::new(&device, &queue);
        #[cfg(feature = "video")]
        let mut video_cache = VideoCache::new(
            &device,
            &queue,
            image_cache.bind_group_layout(),
            image_cache.sampler(),
            video_generation,
            video_wake,
        );
        #[cfg(feature = "video")]
        video_cache.set_gpu_timing_status(gpu_frame_timer.status());

        // Create the WebView texture cache.
        #[cfg(all(feature = "webview", target_os = "linux"))]
        let webview_cache = WgpuWebViewCache::new(&device);

        // Create shader-surface cache
        let shader_surface_cache = crate::shader_surface_cache::ShaderSurfaceCache::new(&device);

        // Load image shader
        let image_shader_source = include_str!("../shaders/image.wgsl");
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Image Shader"),
            source: wgpu::ShaderSource::Wgsl(image_shader_source.into()),
        });

        // Image pipeline layout (uniform + image texture)
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Image Pipeline Layout"),
                bind_group_layouts: &[
                    Some(&bind_group_layout),
                    Some(image_cache.bind_group_layout()),
                ],
                immediate_size: 0,
            });

        // Create image pipeline (similar to glyph but for RGBA textures)
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Image Pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(GlyphVertex::desc())], // Reuse glyph vertex format
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            cache: None,
            multiview_mask: None,
        });

        #[cfg(feature = "video")]
        let bi_planar_video_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bi-planar Video Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/video_biplanar.wgsl").into()),
        });
        #[cfg(feature = "video")]
        let bi_planar_video_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Bi-planar Video Pipeline Layout"),
                bind_group_layouts: &[
                    Some(&bind_group_layout),
                    Some(video_cache.bi_planar_bind_group_layout()),
                ],
                immediate_size: 0,
            });
        #[cfg(feature = "video")]
        let bi_planar_video_pipeline = create_bi_planar_video_pipeline(
            &device,
            &bi_planar_video_shader,
            &bi_planar_video_pipeline_layout,
            target_format,
            None,
            "Bi-planar Video Pipeline",
        );
        #[cfg(feature = "video")]
        let bi_planar_video_copy_pipeline = create_bi_planar_video_copy_pipeline(
            &device,
            &bi_planar_video_shader,
            &bi_planar_video_pipeline_layout,
            crate::video_cache::VIDEO_CHANNEL_FORMAT,
        );

        // Opaque image pipeline — for XRGB/BGRX DMA-BUF textures where alpha=0x00.
        // Uses fs_main_opaque which ignores texture alpha and uses vertex alpha instead.
        let opaque_image_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Opaque Image Pipeline"),
                layout: Some(&image_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &image_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(GlyphVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &image_shader,
                    entry_point: Some("fs_main_opaque"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        // --- Stencil texture for child frame rounded-corner clipping ---
        let (stencil_texture, stencil_view) =
            Self::create_stencil_texture_static(&device, width, height);

        // Stencil state for content pipelines: pass only where stencil==reference
        let stencil_read_state = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Stencil8,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                back: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                read_mask: 0xFF,
                write_mask: 0x00,
            },
            bias: wgpu::DepthBiasState::default(),
        };

        // Stencil-read rect pipeline
        let stencil_rect_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Stencil Rect Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &rect_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(RectVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &rect_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(stencil_read_state.clone()),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        // Stencil-read rounded rect pipeline
        let stencil_rounded_rect_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Stencil Rounded Rect Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &rounded_rect_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(RoundedRectVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &rounded_rect_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(stencil_read_state.clone()),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        // Stencil-read glyph pipeline
        let stencil_glyph_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Stencil Glyph Pipeline"),
                layout: Some(&glyph_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &glyph_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(GlyphVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &glyph_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(stencil_read_state.clone()),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        let stencil_subpixel_glyph_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Stencil Subpixel Glyph Pipeline"),
                layout: Some(&glyph_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &subpixel_glyph_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(SubpixelGlyphVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &subpixel_glyph_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(stencil_read_state.clone()),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        // Stencil-read image pipeline
        let stencil_image_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Stencil Image Pipeline"),
                layout: Some(&image_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &image_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(GlyphVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &image_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(stencil_read_state.clone()),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        #[cfg(feature = "video")]
        let stencil_bi_planar_video_pipeline = create_bi_planar_video_pipeline(
            &device,
            &bi_planar_video_shader,
            &bi_planar_video_pipeline_layout,
            target_format,
            Some(stencil_read_state.clone()),
            "Stencil Bi-planar Video Pipeline",
        );

        // Stencil-read opaque image pipeline
        let stencil_opaque_image_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Stencil Opaque Image Pipeline"),
                layout: Some(&image_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &image_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(GlyphVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &image_shader,
                    entry_point: Some("fs_main_opaque"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(stencil_read_state),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        // Stencil-write pipeline: writes shape to stencil, no color output
        let stencil_write_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Stencil Write Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &rounded_rect_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(RoundedRectVertex::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &rounded_rect_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::empty(),
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Stencil8,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState {
                        front: wgpu::StencilFaceState {
                            compare: wgpu::CompareFunction::Always,
                            fail_op: wgpu::StencilOperation::Keep,
                            depth_fail_op: wgpu::StencilOperation::Keep,
                            pass_op: wgpu::StencilOperation::Replace,
                        },
                        back: wgpu::StencilFaceState {
                            compare: wgpu::CompareFunction::Always,
                            fail_op: wgpu::StencilOperation::Keep,
                            depth_fail_op: wgpu::StencilOperation::Keep,
                            pass_op: wgpu::StencilOperation::Replace,
                        },
                        read_mask: 0xFF,
                        write_mask: 0xFF,
                    },
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                cache: None,
                multiview_mask: None,
            });

        // Create surface_config from format if we have a surface
        let surface_config = if let Some(ref s) = surface {
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: target_format,
                color_space: wgpu::SurfaceColorSpace::Auto,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo, // VSync
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            s.configure(&device, &config);
            Some(config)
        } else {
            None
        };

        Self {
            device,
            queue,
            surface,
            surface_config,
            surface_format: target_format,
            pipelines: Pipelines {
                rect: rect_pipeline,
                rounded_rect: rounded_rect_pipeline,
                corner_mask: corner_mask_pipeline,
                glyph: glyph_pipeline,
                subpixel_glyph: subpixel_glyph_pipeline,
                image: image_pipeline,
                #[cfg(feature = "video")]
                bi_planar_video: bi_planar_video_pipeline,
                #[cfg(feature = "video")]
                bi_planar_video_copy: bi_planar_video_copy_pipeline,
                opaque_image: opaque_image_pipeline,
                stencil_rect: stencil_rect_pipeline,
                stencil_rounded_rect: stencil_rounded_rect_pipeline,
                stencil_glyph: stencil_glyph_pipeline,
                stencil_subpixel_glyph: stencil_subpixel_glyph_pipeline,
                stencil_image: stencil_image_pipeline,
                #[cfg(feature = "video")]
                stencil_bi_planar_video: stencil_bi_planar_video_pipeline,
                stencil_opaque_image: stencil_opaque_image_pipeline,
                stencil_write: stencil_write_pipeline,
            },
            stencil: StencilTargets {
                texture: stencil_texture,
                view: stencil_view,
            },
            glyph_bind_group_layout,
            uniform_buffer,
            uniform_bind_group,
            caches: RenderCaches {
                image: image_cache,
                #[cfg(feature = "video")]
                video: video_cache,
                #[cfg(all(feature = "webview", target_os = "linux"))]
                webview: webview_cache,
                surface: shader_surface_cache,
            },
            #[cfg(feature = "video")]
            gpu_frame_timer,
            arenas: VertexArenas::new(),
            frame_post: None,
            media_budget: crate::media_budget::MediaBudget::new(),
            surface_recreatable: std::collections::HashMap::new(),
            width,
            height,
            scale_factor,
            uniform_screen_size: None,
            effects: crate::effect_config::EffectsConfig::default(),
            fx: EffectsState::default(),
            clocks: EffectClocks::default(),
            ambient: AmbientClocks::default(),
            row_reuse: row_reuse::RowReuseCache::default(),
            glyph_stats: GlyphRenderStats::new(),
            frame_seq: 0,
            frame_sample: neomacs_display_protocol::frame_time::FrameSample::new(
                neomacs_display_protocol::frame_time::observe_platform_now(),
                std::time::Duration::from_millis(16),
            ),
        }
    }

    async fn new_async(
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        // Create wgpu instance
        let instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        let instance = wgpu::Instance::new(instance_descriptor);

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: crate::gpu_power_preference(),
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| format!("Failed to find a suitable GPU adapter: {}", e))?;

        // Request device and queue
        let (device, queue) = crate::request_renderer_device(&adapter, "Neomacs Device").await?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Configure surface if provided and extract format
        let surface_format = surface.as_ref().map(|s| {
            let caps = s.get_capabilities(&adapter);
            caps.formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0])
        });

        // Use the internal helper for pipeline/buffer creation (1.0 scale for standalone usage)
        Ok(Self::create_renderer_internal(
            device,
            queue,
            surface,
            surface_format,
            width,
            height,
            1.0,
            #[cfg(feature = "video")]
            neomacs_video::GpuGeneration::INITIAL,
            #[cfg(feature = "video")]
            neomacs_video::VideoWake::noop(),
        ))
    }

    /// Resize the renderer's surface.
    /// Create a Stencil8 texture and view for rounded-corner clipping.
    fn create_stencil_texture_static(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let w = width.max(1);
        let h = height.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Stencil Texture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Stencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Resize the render target, reapplying only what the new geometry
    /// actually invalidates.
    ///
    /// One renderer is shared across every frame window, so each window render
    /// brackets itself with `resize(that window)` ... `resize(previous)` —
    /// meaning this runs twice per present, and on the common single-window
    /// path both calls pass the dimensions already in effect. Every step here
    /// is idempotent for unchanged inputs, so each is guarded by what it
    /// depends on: the swapchain configuration and stencil texture on geometry,
    /// the uniform buffer on geometry *and* scale (`set_scale_factor` only
    /// stores the scale; this is what uploads it). A surface is only ever
    /// installed by the constructor — device loss builds a whole new renderer —
    /// so skipping a redundant `configure` can never leave a fresh surface
    /// unconfigured.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        let geometry_changed = self.width != width || self.height != height;
        self.width = width;
        self.height = height;

        if geometry_changed {
            // Update surface configuration
            if let (Some(surface), Some(config)) = (&self.surface, &mut self.surface_config) {
                config.width = width;
                config.height = height;
                surface.configure(&self.device, config);
            }

            // Recreate stencil texture at new size
            let (stencil_texture, stencil_view) =
                Self::create_stencil_texture_static(&self.device, width, height);
            self.stencil.texture = stencil_texture;
            self.stencil.view = stencil_view;
        }

        // Update uniform buffer with logical size so vertex positions from Emacs map correctly
        let screen_size = [
            width as f32 / self.scale_factor,
            height as f32 / self.scale_factor,
        ];
        if self.uniform_screen_size != Some(screen_size) {
            self.uniform_screen_size = Some(screen_size);
            let uniforms = Uniforms {
                screen_size,
                time: 0.0,
                _padding: 0.0,
            };
            self.queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        }
    }

    /// Update the display scale factor (for multi-monitor DPI changes)
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        // Threshold above float noise but well below any real DPI step
        // (1.0 → 1.25 → 2.0 …); avoids needless texture recreation.
        let changed = (scale_factor - self.scale_factor).abs() > 0.001;
        self.scale_factor = scale_factor;
        if !changed {
            return;
        }
        // A monitor/DPI change: existing shader surfaces froze their physical
        // size at create, so resample them to the new scale to stay crisp,
        // then re-account MediaBudget for each surface whose byte cost changed
        // (image cache owns the composite layout/sampler these surfaces use).
        let rescaled = self.caches.surface.rescale(
            &self.device,
            self.caches.image.bind_group_layout(),
            self.caches.image.sampler(),
            self.surface_format,
            scale_factor,
        );
        for (id, width_px, height_px) in rescaled {
            let recreatable = self.surface_recreatable.get(&id).copied().unwrap_or(false);
            self.register_surface_bytes(id, width_px, height_px, recreatable);
        }
    }

    /// Set the absolute time this frame's animation samples target
    /// (the frame tick's target presentation time).
    /// The time sample this frame's effects must date themselves to.
    #[must_use]
    pub fn frame_sample(&self) -> neomacs_display_protocol::frame_time::FrameSample {
        self.frame_sample
    }

    pub fn set_frame_sample(&mut self, sample: neomacs_display_protocol::frame_time::FrameSample) {
        self.frame_sample = sample;
        // A new sample is a new frame, and `frame_seq` is what effect seeds
        // vary on from frame to frame (see the field's doc comment).
        self.frame_seq = self.frame_seq.wrapping_add(1);
    }

    /// Get the glyph bind group layout for creating glyph bind groups
    pub fn glyph_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.glyph_bind_group_layout
    }

    /// Render a scene to the configured surface.
    pub fn render(&mut self, scene: &Scene) {
        let surface = match &self.surface {
            Some(s) => s,
            None => return,
        };

        let output = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.resize(self.width, self.height);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                tracing::warn!("Surface validation error");
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.render_to_view(&view, scene);

        self.queue.present(output);
    }

    /// Render a scene to a texture view.
    pub fn render_to_view(&self, view: &wgpu::TextureView, scene: &Scene) {
        // Collect all rectangles to render
        let mut vertices: Vec<RectVertex> = Vec::new();

        // 1. Draw scene background
        self.add_rect(
            &mut vertices,
            0.0,
            0.0,
            scene.width,
            scene.height,
            &scene.background,
        );

        // 2. For each window: draw background, then cursor if visible
        for window in &scene.windows {
            // Window background
            self.add_rect(
                &mut vertices,
                window.bounds.x,
                window.bounds.y,
                window.bounds.width,
                window.bounds.height,
                &window.background,
            );

            // Cursor
            if let Some(cursor) = &window.cursor
                && cursor.visible
            {
                let cursor_x = window.bounds.x + cursor.x;
                let cursor_y = window.bounds.y + cursor.y;

                match cursor.style {
                    SceneCursorStyle::Box => {
                        // Filled box cursor
                        self.add_rect(
                            &mut vertices,
                            cursor_x,
                            cursor_y,
                            cursor.width,
                            cursor.height,
                            &cursor.color,
                        );
                    }
                    SceneCursorStyle::Bar => {
                        // Thin vertical bar
                        self.add_rect(
                            &mut vertices,
                            cursor_x,
                            cursor_y,
                            2.0, // Bar width
                            cursor.height,
                            &cursor.color,
                        );
                    }
                    SceneCursorStyle::Underline => {
                        // Horizontal line at bottom
                        self.add_rect(
                            &mut vertices,
                            cursor_x,
                            cursor_y + cursor.height - 2.0,
                            cursor.width,
                            2.0, // Underline thickness
                            &cursor.color,
                        );
                    }
                    SceneCursorStyle::Hollow => {
                        // Hollow box (4 lines forming a rectangle)
                        let thickness = 1.0;
                        // Top
                        self.add_rect(
                            &mut vertices,
                            cursor_x,
                            cursor_y,
                            cursor.width,
                            thickness,
                            &cursor.color,
                        );
                        // Bottom
                        self.add_rect(
                            &mut vertices,
                            cursor_x,
                            cursor_y + cursor.height - thickness,
                            cursor.width,
                            thickness,
                            &cursor.color,
                        );
                        // Left
                        self.add_rect(
                            &mut vertices,
                            cursor_x,
                            cursor_y,
                            thickness,
                            cursor.height,
                            &cursor.color,
                        );
                        // Right
                        self.add_rect(
                            &mut vertices,
                            cursor_x + cursor.width - thickness,
                            cursor_y,
                            thickness,
                            cursor.height,
                            &cursor.color,
                        );
                    }
                }
            }
        }

        // 3. Draw borders
        for border in &scene.borders {
            self.add_rect(
                &mut vertices,
                border.x,
                border.y,
                border.width,
                border.height,
                &border.color,
            );
        }

        // Skip rendering if there's nothing to draw
        if vertices.is_empty() {
            return;
        }

        // Create vertex buffer
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Rect Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        // Create command encoder and render pass
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Rect Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: scene.background.r as f64,
                            g: scene.background.g as f64,
                            b: scene.background.b as f64,
                            a: scene.background.a as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.pipelines.rect);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.draw(0..vertices.len() as u32, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a scene to an offscreen texture.
    pub fn render_to_texture(&self, scene: &Scene) -> wgpu::Texture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Texture"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.render_to_view(&view, scene);

        texture
    }

    /// Add a rectangle to the vertex list (6 vertices = 2 triangles).
    fn add_rect(
        &self,
        vertices: &mut Vec<RectVertex>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: &Color,
    ) {
        let color_arr = [color.r, color.g, color.b, color.a];

        let x0 = x;
        let y0 = y;
        let x1 = x + width;
        let y1 = y + height;

        // First triangle (top-left, top-right, bottom-left)
        vertices.push(RectVertex {
            position: [x0, y0],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [x1, y0],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [x0, y1],
            color: color_arr,
        });

        // Second triangle (top-right, bottom-right, bottom-left)
        vertices.push(RectVertex {
            position: [x1, y0],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [x1, y1],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [x0, y1],
            color: color_arr,
        });
    }

    /// Render a stipple pattern (XBM bitmap) tiled over a rectangular area.
    /// Uses run-length encoding: consecutive set bits in each row are merged
    /// into a single wider rect to reduce vertex count.
    fn render_stipple_pattern(
        &self,
        vertices: &mut Vec<RectVertex>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        fg: &Color,
        pattern: &StipplePattern,
    ) {
        if pattern.width == 0 || pattern.height == 0 {
            return;
        }
        let bytes_per_row = pattern.width.div_ceil(8) as usize;
        // Round the fractional cell size UP so the tiled fill covers the whole
        // domain: with a proportional font the per-glyph cell width is
        // fractional (e.g. 16.25px), and truncating left a ~1px unfilled column
        // at every glyph boundary. Any overdraw past the domain is clipped by
        // the paint clip and overpainted by the next glyph's own fill.
        let w_pixels = width.ceil() as u32;
        let h_pixels = height.ceil() as u32;

        // Tile the pattern over the area, merging horizontal runs
        for py in 0..h_pixels {
            let pat_y = py % pattern.height;
            let mut px = 0u32;
            while px < w_pixels {
                let pat_x = px % pattern.width;
                let byte_idx = pat_y as usize * bytes_per_row + (pat_x / 8) as usize;
                let bit_idx = pat_x % 8;
                let bit_set =
                    byte_idx < pattern.bits.len() && (pattern.bits[byte_idx] >> bit_idx) & 1 != 0;
                if !bit_set {
                    px += 1;
                    continue;
                }
                // Start of a run — find how far it extends
                let run_start = px;
                px += 1;
                while px < w_pixels {
                    let pat_x2 = px % pattern.width;
                    let bi2 = pat_y as usize * bytes_per_row + (pat_x2 / 8) as usize;
                    let bit2 = pat_x2 % 8;
                    let set2 = bi2 < pattern.bits.len() && (pattern.bits[bi2] >> bit2) & 1 != 0;
                    if !set2 {
                        break;
                    }
                    px += 1;
                }
                let run_len = px - run_start;
                self.add_rect(
                    vertices,
                    x + run_start as f32,
                    y + py as f32,
                    run_len as f32,
                    1.0,
                    fg,
                );
            }
        }
    }

    /// Render a monochrome fringe bitmap into a window's fringe column.
    ///
    /// Reuses the stipple bits-to-quads technique (no texture/atlas): each set
    /// bit becomes a `scale`-sized foreground quad, with horizontal runs merged.
    /// `scale` is one logical pixel (`1.0`); the logical→physical projection in
    /// the rect pipeline then scales it by the device pixel ratio, so arrows are
    /// full-size on hidpi without any per-bit `* scale_factor`.
    ///
    /// Bits are MSB-aligned `u16` (column `b` of row `r` is set when
    /// `(bits[r] >> (15 - b)) & 1 == 1`). `column_x`/`row_y` are the fringe
    /// column's top-left in logical (offset-applied) frame coordinates; the
    /// bitmap is centered horizontally and aligned vertically per `align`
    /// (0 = center, 1 = top, 2 = bottom).
    #[allow(clippy::too_many_arguments)]
    fn render_fringe_bitmap(
        &self,
        vertices: &mut Vec<RectVertex>,
        column_x: f32,
        row_y: f32,
        column_width: f32,
        row_height: f32,
        fg: &Color,
        bitmap: &FringeBitmapData,
    ) {
        if bitmap.width == 0 || bitmap.height == 0 || bitmap.bits.is_empty() {
            return;
        }
        let scale = 1.0_f32;
        let bmp_w = bitmap.width as f32 * scale;
        let period = bitmap.period as usize;
        // A periodic bitmap (e.g. `empty-line`, period 3) tiles its `period`-row
        // motif down the FULL row height — GNU `draw_fringe_bitmap_1` keys the
        // start row off the row's frame y (`p.dh = p.y % period`) so the dashed
        // pattern stays on a single global grid across every empty row, then
        // draws `height - dh` rows clipped to the row.  A non-periodic bitmap is
        // a fixed-height glyph aligned within the row.
        let bmp_h = if period > 0 {
            row_height
        } else {
            bitmap.height as f32 * scale
        };

        // Horizontal: center the bitmap in the fringe column (GNU left-justifies
        // standard bitmaps, but centering reads well for magit's narrow arrows
        // and never clips when width <= column_width).
        let x_off = ((column_width - bmp_w) * 0.5).max(0.0);
        // Vertical alignment within the row. Periodic bitmaps always tile from
        // the row top (GNU stores them ALIGN_BITMAP_TOP); the phase below keeps
        // them aligned to the global grid.
        let y_off = if period > 0 {
            0.0
        } else {
            match bitmap.align {
                1 => 0.0,                                   // TOP
                2 => (row_height - bmp_h).max(0.0),         // BOTTOM
                _ => ((row_height - bmp_h) * 0.5).max(0.0), // CENTER
            }
        };
        let origin_x = column_x + x_off;
        let origin_y = row_y + y_off;

        // GNU's per-row phase: `dh = y % period`. We key off the row's logical
        // frame y so consecutive empty rows continue the same dotted grid.
        let phase = if period > 0 {
            (row_y.max(0.0).round() as i64).rem_euclid(period as i64) as usize
        } else {
            0
        };

        let width_bits = bitmap.width.min(16) as u32;
        let bitmap_rows = bitmap.bits.len();
        // Number of device rows to draw: clipped to the row height, and never
        // past the stored bitmap (a non-periodic bitmap draws its own height).
        let device_rows = if period > 0 {
            bmp_h.max(0.0).round() as usize
        } else {
            bitmap_rows
        };
        for dr in 0..device_rows {
            let py = origin_y + dr as f32 * scale;
            if py >= row_y + row_height {
                break;
            }
            // Which stored bitmap row to sample. Periodic bitmaps wrap modulo
            // `period`, phase-shifted so the grid is continuous across rows; a
            // non-periodic bitmap maps device row -> bitmap row 1:1.
            let bitmap_row = if period > 0 {
                (phase + dr) % period
            } else {
                dr
            };
            let Some(&row_bits) = bitmap.bits.get(bitmap_row) else {
                continue;
            };
            let mut b = 0u32;
            while b < width_bits {
                let set = (row_bits >> (15 - b)) & 1 != 0;
                if !set {
                    b += 1;
                    continue;
                }
                let run_start = b;
                b += 1;
                while b < width_bits && (row_bits >> (15 - b)) & 1 != 0 {
                    b += 1;
                }
                let run_len = (b - run_start) as f32;
                self.add_rect(
                    vertices,
                    origin_x + run_start as f32 * scale,
                    py,
                    run_len * scale,
                    scale,
                    fg,
                );
            }
        }
    }

    /// Emit a solid rounded rectangle as one oversized quad.
    fn add_rounded_rect(
        &self,
        vertices: &mut Vec<RoundedRectVertex>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border_width: f32,
        corner_radius: f32,
        color: &Color,
    ) {
        self.add_rounded_rect_with_box_vertical_edges(
            vertices,
            x,
            y,
            width,
            height,
            border_width,
            corner_radius,
            color,
            BoxVerticalEdges::Both,
        );
    }

    /// Emit a solid rounded box while honoring GNU's independently owned
    /// left/right box-run sides.
    #[allow(clippy::too_many_arguments)]
    fn add_rounded_rect_with_box_vertical_edges(
        &self,
        vertices: &mut Vec<RoundedRectVertex>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border_width: f32,
        corner_radius: f32,
        color: &Color,
        box_vertical_edges: BoxVerticalEdges,
    ) {
        self.add_rounded_rect_styled_with_box_vertical_edges(
            vertices,
            x,
            y,
            width,
            height,
            border_width,
            corner_radius,
            color,
            0,
            1.0,
            &Color::TRANSPARENT,
            box_vertical_edges,
        );
    }

    /// Emit a styled rounded box with explicit vertical terminal ownership.
    ///
    /// An unowned side is represented by extending the SDF's logical box past
    /// that side while clipping the emitted quad at the layout boundary.  The
    /// result retains square-ended top/bottom rails and the background fill,
    /// but neither a vertical cap nor a rounded corner is rasterized there.
    #[allow(clippy::too_many_arguments)]
    fn add_rounded_rect_styled_with_box_vertical_edges(
        &self,
        vertices: &mut Vec<RoundedRectVertex>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border_width: f32,
        corner_radius: f32,
        color: &Color,
        style_id: u32,
        speed: f32,
        color2: &Color,
        box_vertical_edges: BoxVerticalEdges,
    ) {
        // Extra padding: glow/neon effects need more room for falloff
        let padding = match style_id {
            4 | 5 => 12.0,
            10 => border_width + 2.0, // heartbeat expands border
            _ => 1.0,
        };
        let open_extension = corner_radius + padding + 2.0;
        let rect_x = if box_vertical_edges.owns_left() {
            x
        } else {
            x - open_extension
        };
        let rect_right = if box_vertical_edges.owns_right() {
            x + width
        } else {
            x + width + open_extension
        };
        let x0 = if box_vertical_edges.owns_left() {
            x - padding
        } else {
            x
        };
        let y0 = y - padding;
        let x1 = if box_vertical_edges.owns_right() {
            x + width + padding
        } else {
            x + width
        };
        let y1 = y + height + padding;

        let rect_min = [rect_x, y];
        let rect_max = [rect_right, y + height];
        let params = [border_width, corner_radius];
        let color_arr = [color.r, color.g, color.b, color.a];
        let style_params = [style_id as f32, speed, 0.0, 0.0];
        let color2_arr = [color2.r, color2.g, color2.b, color2.a];

        let v = |px: f32, py: f32| RoundedRectVertex {
            position: [px, py],
            color: color_arr,
            rect_min,
            rect_max,
            params,
            style_params,
            color2: color2_arr,
        };

        // Two triangles forming the quad
        vertices.push(v(x0, y0));
        vertices.push(v(x1, y0));
        vertices.push(v(x0, y1));
        vertices.push(v(x1, y0));
        vertices.push(v(x1, y1));
        vertices.push(v(x0, y1));
    }

    /// Add an arbitrary quad (4 corners) to the vertex list (6 vertices = 2 triangles).
    /// Corners order: [TL, TR, BR, BL].
    fn add_quad(&self, vertices: &mut Vec<RectVertex>, corners: &[(f32, f32); 4], color: &Color) {
        let color_arr = [color.r, color.g, color.b, color.a];
        let [tl, tr, br, bl] = *corners;

        // Triangle 1: TL, TR, BL
        vertices.push(RectVertex {
            position: [tl.0, tl.1],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [tr.0, tr.1],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [bl.0, bl.1],
            color: color_arr,
        });

        // Triangle 2: TR, BR, BL
        vertices.push(RectVertex {
            position: [tr.0, tr.1],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [br.0, br.1],
            color: color_arr,
        });
        vertices.push(RectVertex {
            position: [bl.0, bl.1],
            color: color_arr,
        });
    }

    /// Get the wgpu device.
    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    /// Get the wgpu queue.
    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    /// Get the current width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get the current height.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Get the current display scale factor.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    // =========== Image Loading Methods ===========

    // =========== Video Loading Methods ===========

    // ========================================================================
    // Offscreen texture management (for transitions)
    // ========================================================================

    /// Get the surface format
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    /// Get the image bind group layout (for creating bind groups for offscreen textures)
    pub fn image_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.caches.image.bind_group_layout()
    }

    /// Get the image sampler (for creating bind groups for offscreen textures)
    pub fn image_sampler(&self) -> &wgpu::Sampler {
        self.caches.image.sampler()
    }

    /// Get the uniform bind group (needed for composite rendering)
    pub fn uniform_bind_group(&self) -> &wgpu::BindGroup {
        &self.uniform_bind_group
    }

    /// Get the image pipeline (needed for blit and scroll slide)
    pub fn image_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipelines.image
    }

    /// Create an offscreen texture suitable for rendering a full frame
    pub fn create_offscreen_texture(
        &self,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    /// Create a bind group for a texture view (usable with image_pipeline)
    pub fn create_texture_bind_group(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Offscreen Bind Group"),
            layout: self.caches.image.bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(self.caches.image.sampler()),
                },
            ],
        })
    }

    /// Blit a texture to a target view (fullscreen quad)
    pub fn blit_texture_to_view(
        &mut self,
        src_bind_group: &wgpu::BindGroup,
        dst_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        // Use logical dimensions for vertex positions since screen_size uniform is logical
        let w = width as f32 / self.scale_factor;
        let h = height as f32 / self.scale_factor;

        let vertices = [
            GlyphVertex {
                position: [0.0, 0.0],
                tex_coords: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [w, 0.0],
                tex_coords: [1.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [w, h],
                tex_coords: [1.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [0.0, 0.0],
                tex_coords: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [w, h],
                tex_coords: [1.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [0.0, h],
                tex_coords: [0.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
        ];

        let upload = self
            .arenas
            .image
            .upload(&self.device, &self.queue, &vertices);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if let Some(ref upload) = upload {
                render_pass.set_pipeline(&self.pipelines.image);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_bind_group(1, src_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..6, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    // ── Scroll Effect Implementations ─────────────────────────────────────
}
